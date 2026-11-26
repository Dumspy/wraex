#!/usr/bin/env python3
"""Build a runnable Wraex release: the binary plus the assets it reads at runtime.

Usage (from the repository root, ideally inside `devenv shell`):

    python3 tools/package/package.py                          # host target, version from rust/Cargo.toml
    python3 tools/package/package.py --target x86_64-pc-windows-msvc --archive zip
    python3 tools/package/package.py --target x86_64-pc-windows-msvc --archive zip --installer
    python3 tools/package/package.py --no-build --binary path/to/wraex --version 0.1.0

The output is `dist/wraex-<version>-<target>/`, laid out the way
`wraex_core::assets` resolves it:

    wraex[.exe]
    assets/poe1/data/<lang>/{client_strings.js,items.ndjson,stats.ndjson,app_i18n.json}
    assets/poe1/images/...           ref/apt/renderer/public/images
    assets/poe1/icon.png             ref/apt/renderer/public/icon.png
    assets/poe2/...                  the same from ref/ee2
    licences/                        Wraex's, both references' and Font Awesome's notices

The app finds `assets/` next to its executable (or in a macOS bundle's
`Contents/Resources/assets`), so the directory can be renamed or moved as a unit;
`WRAEX_ASSETS` overrides it.

`--installer` additionally builds `dist/wraex-<version>-<target>-setup.exe` from that
directory for Windows targets: the assisted, per-user NSIS install in
`tools/package/installer.nsi` (the same shape as upstream's electron-builder setup).
It needs `makensis` (NSIS 3) and ImageMagick, which the release workflow installs;
the packaged `assets/poe1/icon.png` is converted to the multi-size .ico NSIS takes.

The references' generated `*.index.bin` files are *not* shipped: they exist for the
TypeScript apps and for the Rust data layer's cross-check test
(`cargo test -p wraex-poe1 -p wraex-poe2 -- --ignored`). The Rust loader builds the
same hash tables in memory from `items.ndjson`/`stats.ndjson`, so a release needs no
generated data and this script runs `golden:prepare` for nothing.

Only the standard library is used: the release workflow runs this on the runner's
own Python, not inside devenv.
"""

from __future__ import annotations

import argparse
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
RUST = ROOT / "rust"

# What `Data::load` reads for one language (poe1::data / poe2::data): the client
# strings, the two ndjson tables, and the app's own UI messages. `th` has only the
# last one (PoE2 UI text), which `--languages all` skips because it has no game data.
DATA_FILES = ("client_strings.js", "items.ndjson", "stats.ndjson", "app_i18n.json")

# Each game's `renderer/public`, the source of the shipped files.
GAMES = {
    "poe1": ROOT / "ref" / "apt" / "renderer" / "public",
    "poe2": ROOT / "ref" / "ee2" / "renderer" / "public",
}

# Editor sources that sit in the images directories but are not images the app loads.
EDITOR_SOURCES = ("*.pdn", "*.psd", "*.xcf")

# The sizes the installer's .ico gets. The packaged PNG is 78x78, so these are the
# sizes that need no upscaling: Windows uses 16 for menus and the title bar, 32 for
# Apps & Features, 48/64 for Explorer and the Start menu.
ICON_SIZES = "64,48,32,24,16"


def workspace_version() -> str:
    """`[workspace.package] version` from `rust/Cargo.toml`."""
    text = (RUST / "Cargo.toml").read_text(encoding="utf-8")
    section = re.search(r"^\[workspace\.package\]\s*$(.*?)(?=^\[)", text, re.M | re.S)
    if section is None:
        raise SystemExit("rust/Cargo.toml has no [workspace.package] section")
    version = re.search(r'^version\s*=\s*"([^"]+)"', section.group(1), re.M)
    if version is None:
        raise SystemExit("rust/Cargo.toml's [workspace.package] has no version")
    return version.group(1)


def host_target() -> str:
    """`rustc -vV`'s `host:` line, the triple a plain `cargo build` produces."""
    out = subprocess.run(["rustc", "-vV"], capture_output=True, text=True, check=True).stdout
    match = re.search(r"^host:\s*(\S+)", out, re.M)
    if match is None:
        raise SystemExit("rustc -vV printed no host line")
    return match.group(1)


def git_commit(path: Path) -> str:
    """The reference's pinned commit, for the notices; `unknown` outside git."""
    try:
        out = subprocess.run(
            ["git", "-C", str(path), "rev-parse", "--short", "HEAD"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout.strip()
        return out or "unknown"
    except (OSError, subprocess.CalledProcessError):
        return "unknown"


def data_languages(public: Path, requested: str) -> list[str]:
    """The languages to ship: `all` for every directory that has game data, or a list."""
    available = sorted(
        entry.name for entry in (public / "data").iterdir() if entry.is_dir() and (entry / "items.ndjson").is_file()
    )
    if requested.strip() == "all":
        return available
    wanted = [lang.strip() for lang in requested.split(",") if lang.strip()]
    unknown = [lang for lang in wanted if lang not in available]
    if unknown:
        raise SystemExit(f"{public / 'data'}: no game data for {', '.join(unknown)} (have: {', '.join(available)})")
    return wanted


def size(path: Path) -> int:
    if path.is_file():
        return path.stat().st_size
    return sum(entry.stat().st_size for entry in path.rglob("*") if entry.is_file())


def human(count: int) -> str:
    value = float(count)
    for unit in ("B", "KiB", "MiB", "GiB"):
        if value < 1024 or unit == "GiB":
            return f"{value:.1f} {unit}" if unit != "B" else f"{int(value)} B"
        value /= 1024
    raise AssertionError("unreachable")


def copy(game: str, public: Path, stage: Path, languages: str) -> None:
    """One game's data, images and tray icon into `<stage>/assets/<game>`."""
    if not (public / "data").is_dir() or not (public / "images").is_dir():
        raise SystemExit(
            f"{public.relative_to(ROOT)} is missing.\n"
            "Are the submodules checked out?  git submodule update --init --recursive"
        )

    assets = stage / "assets" / game
    missing: list[Path] = []

    for lang in data_languages(public, languages):
        for name in DATA_FILES:
            source = public / "data" / lang / name
            if not source.is_file():
                missing.append(source)
                continue
            target = assets / "data" / lang / name
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, target)

    images = public / "images"
    if not images.is_dir():
        missing.append(images)
    else:
        shutil.copytree(images, assets / "images", ignore=shutil.ignore_patterns(*EDITOR_SOURCES))

    icon = public / "icon.png"
    if not icon.is_file():
        missing.append(icon)
    else:
        shutil.copy2(icon, assets / "icon.png")

    if missing:
        listed = "\n  ".join(str(path.relative_to(ROOT)) for path in missing)
        raise SystemExit(
            "missing reference files:\n  "
            + listed
            + "\nAre the submodules checked out?  git submodule update --init --recursive"
        )


def notices(version: str) -> str:
    """`licences/NOTICES.md`: what came from where, and the font question."""
    return f"""# Wraex {version}: bundled assets and their licences

Wraex's runtime assets live in `assets/` beside the executable
(`wraex_core::assets`):

- `assets/poe1/`: Path of Exile 1 game data, images and icon from Awakened PoE Trade
  (`ref/apt` @ `{git_commit(ROOT / "ref" / "apt")}`).
- `assets/poe2/`: Path of Exile 2 game data, images and icon from Exiled Exchange 2
  (`ref/ee2` @ `{git_commit(ROOT / "ref" / "ee2")}`).

Both references are MIT-licensed; their licence texts are next to this file as
`AWAKENED-POE-TRADE-LICENSE` and `EXILED-EXCHANGE-2-LICENSE`, and Wraex's own as
`WRAEX-LICENSE`.

## Fonts

The binary embeds the text faces `ref/apt/renderer/src/assets/font` ships and the
two Font Awesome Free faces vendored in `rust/crates/app/assets/fontawesome`:

- Fontin, Fontin SmallCaps: copyright (c) Jos Buivenga, 2004. All rights reserved.
- Friz Quadrata ITC: copyright (c) 1990-2005 ParaType Inc.; Friz Quadrata is a
  registered trademark of International Typeface Corporation.
- Exo 2: SIL Open Font License 1.1.
- Font Awesome Free 6.7.1: fonts SIL OFL 1.1, icons CC BY 4.0; see
  `FONT-AWESOME-LICENSE`.

Neither reference ships a separate licence file for the three text faces. Wraex
ships them as both references do (decided 2026-11-25); the redesign (#5) is
expected to replace Fontin and Friz Quadrata with faces of its own.

## Generated data

The references' `*.index.bin` files are not shipped. They are a TypeScript-side
artifact and a cross-check for the Rust data layer (its indexes are built in memory
from `items.ndjson`/`stats.ndjson`).
"""


def write_licences(stage: Path, version: str) -> None:
    licences = stage / "licences"
    licences.mkdir(parents=True)
    for source, name in [
        (ROOT / "LICENSE", "WRAEX-LICENSE"),
        (ROOT / "ref" / "apt" / "LICENSE", "AWAKENED-POE-TRADE-LICENSE"),
        (ROOT / "ref" / "ee2" / "LICENSE", "EXILED-EXCHANGE-2-LICENSE"),
        (ROOT / "rust" / "crates" / "app" / "assets" / "fontawesome" / "LICENSE.txt", "FONT-AWESOME-LICENSE"),
    ]:
        if not source.is_file():
            raise SystemExit(f"missing licence {source.relative_to(ROOT)}")
        shutil.copy2(source, licences / name)
    (licences / "NOTICES.md").write_text(notices(version), encoding="utf-8")


def build(target: str | None, explicit_target: bool) -> Path:
    """`cargo build --release`, returning the binary it wrote."""
    command = ["cargo", "build", "--release", "--locked", "-p", "wraex-app"]
    if explicit_target:
        command += ["--target", target]
    print("$", " ".join(command), f"(in {RUST.relative_to(ROOT)})", flush=True)
    subprocess.run(command, cwd=RUST, check=True)
    return binary_path(target, explicit_target)


def binary_name(target: str) -> str:
    """The packaged binary's name, which is also what the installer expects."""
    return "wraex.exe" if "windows" in target else "wraex"


def binary_path(target: str, explicit_target: bool) -> Path:
    directory = RUST / "target" / (target if explicit_target else "") / "release"
    return directory / binary_name(target)


def archive(stage: Path, kind: str) -> Path:
    """`zip` or `tar.gz` of the stage directory, beside it in `dist/`."""
    formats = {"zip": "zip", "tar.gz": "gztar"}
    created = shutil.make_archive(str(stage), formats[kind], root_dir=stage.parent, base_dir=stage.name)
    return Path(created)


def image_magick() -> str:
    """ImageMagick's command: `magick` (7) or `convert` (6)."""
    for name in ("magick", "convert"):
        found = shutil.which(name)
        if found:
            return found
    raise SystemExit(
        "building the installer needs ImageMagick for the .ico NSIS takes:\n"
        "  Debian/Ubuntu: apt-get install imagemagick;  macOS: brew install imagemagick"
    )


def setup_icon(stage: Path, target: Path) -> None:
    """The package's icon as the multi-size .ico the installer's Icon/MUI_ICON want."""
    png = stage / "assets" / "poe1" / "icon.png"
    if not png.is_file():
        raise SystemExit(f"{png} is missing; the setup's icon comes from the packaged assets")
    command = [
        image_magick(),
        str(png),
        "-background",
        "none",
        "-define",
        f"icon:auto-resize={ICON_SIZES}",
        str(target),
    ]
    subprocess.run(command, check=True)
    if not target.is_file():
        raise SystemExit(f"{command[0]} did not write {target}")


def windows_version(version: str) -> str:
    """`0.1.0` -> `0.1.0.0`: the four numbers VIProductVersion takes."""
    numbers = re.findall(r"\d+", re.split(r"[-+]", version, maxsplit=1)[0])[:4]
    return ".".join((numbers + ["0"] * 4)[:4])


def build_setup(stage: Path, version: str) -> Path:
    """`<stage>-setup.exe`: the NSIS install of `stage` (tools/package/installer.nsi)."""
    makensis = shutil.which("makensis")
    if makensis is None:
        raise SystemExit(
            "--installer needs makensis (NSIS 3):\n"
            "  Debian/Ubuntu: apt-get install nsis;  macOS: brew install makensis"
        )
    setup = stage.parent / f"{stage.name}-setup.exe"
    setup.unlink(missing_ok=True)
    with tempfile.TemporaryDirectory(prefix="wraex-setup-") as temporary:
        icon = Path(temporary) / "wraex.ico"
        setup_icon(stage, icon)
        # `-D` with the raw value: makensis reads `/D` as a path on POSIX, and keeps
        # quotes as characters. Paths may contain spaces.
        command = [
            makensis,
            f"-DVERSION={version}",
            f"-DVI_VERSION={windows_version(version)}",
            f"-DSTAGE_DIR={stage}",
            f"-DICON_FILE={icon}",
            f"-DOUT_FILE={setup}",
            str(ROOT / "tools" / "package" / "installer.nsi"),
        ]
        print("$", " ".join(command), flush=True)
        subprocess.run(command, check=True)
    if not setup.is_file():
        raise SystemExit(f"makensis did not write {setup}")
    return setup


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--version", default=None, help="release version (default: rust/Cargo.toml's workspace version)")
    parser.add_argument("--target", default=None, help="cargo target triple (default: the host's)")
    parser.add_argument("--out", default=str(ROOT / "dist"), help="where the release directory goes (default: dist/)")
    parser.add_argument("--binary", default=None, help="an already-built binary; skips `cargo build`")
    parser.add_argument("--no-build", action="store_true", help="do not run cargo; requires --binary")
    parser.add_argument(
        "--languages",
        default="en",
        help="game-data languages to ship: a comma-separated list or `all` (default: en)",
    )
    parser.add_argument("--archive", choices=["none", "zip", "tar.gz"], default="none", help="also write an archive")
    parser.add_argument(
        "--installer",
        action="store_true",
        help="also build <stage>-setup.exe with makensis (Windows targets; needs NSIS and ImageMagick)",
    )
    args = parser.parse_args()

    version = args.version or workspace_version()
    target = args.target or host_target()
    explicit_target = args.target is not None
    for field, value in (("version", version), ("target", target)):
        if not re.fullmatch(r"[a-zA-Z0-9][a-zA-Z0-9.+_-]*", value):
            parser.error(f"invalid {field}: expected a single package name component")
    output = Path(args.out).resolve()
    stage = output / f"wraex-{version}-{target}"
    if stage.is_symlink() or stage.resolve().parent != output:
        parser.error("staging directory must be a direct child of the output directory")


    if args.no_build and args.binary is None:
        parser.error("--no-build needs --binary")

    if args.installer and "windows" not in target:
        parser.error(f"--installer is for Windows targets, not {target} (there is no wraex.exe to install)")

    if args.binary is not None:
        binary = Path(args.binary).resolve()
        if not binary.is_file():
            raise SystemExit(f"no binary at {binary}")
    elif args.no_build:
        raise AssertionError("--no-build without --binary was rejected above")
    else:
        binary = build(target, explicit_target)

    if not binary.is_file():
        raise SystemExit(f"cargo did not write {binary} (is wraex-app's binary still named `wraex`?)")

    if stage.exists():
        shutil.rmtree(stage)
    stage.mkdir(parents=True)
    # Named after the target, not after --binary: the installer and the layout
    # `wraex_core::assets` documents expect `wraex.exe` beside `assets/`.
    staged = stage / binary_name(target)
    shutil.copy2(binary, staged)

    for game, public in GAMES.items():
        copy(game, public, stage, args.languages)
    write_licences(stage, version)

    print(f"\nstaged {stage}")
    parts = [(staged, "binary")]
    parts += [(stage / "assets" / game, f"{game} assets") for game in GAMES]
    parts += [(stage / "licences", "licences")]
    width = max(len(label) for _, label in parts)
    for path, label in parts:
        print(f"  {label.ljust(width)}  {human(size(path)):>10}  {path.relative_to(stage)}")
    print(f"  {'total'.ljust(width)}  {human(size(stage)):>10}  {stage.name}")
    print("\n  the references' generated *.index.bin files are not shipped: the Rust data layer builds")
    print("  the same hash tables in memory, so a release needs no generated data")
    if args.languages != "all":
        print(f"  game data languages: {args.languages} (`--languages all` ships every language)")

    if args.archive != "none":
        created = archive(stage, args.archive)
        print(f"  archive           {human(created.stat().st_size):>10}  {created.name}")

    if args.installer:
        setup = build_setup(stage, version)
        print(f"  setup             {human(setup.stat().st_size):>10}  {setup.name}")


if __name__ == "__main__":
    try:
        main()
    except subprocess.CalledProcessError as error:
        sys.exit(f"command failed: {' '.join(error.cmd)}")
