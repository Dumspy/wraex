"""Check Rune's vendored patches against the workspace manifest and lockfile."""
import re
import tomllib
from pathlib import Path

rust = Path(__file__).resolve().parents[1] / "rust"
with (rust / "Cargo.toml").open("rb") as manifest:
    workspace = tomllib.load(manifest)
with (rust / "Cargo.lock").open("rb") as lock:
    resolved = tomllib.load(lock)

rune = workspace["workspace"]["dependencies"]["rune"]
if not re.fullmatch(r"[0-9a-f]{40}", rune["rev"]):
    raise SystemExit("Rune requires a full commit SHA")

patches = workspace["patch"]["crates-io"]
for name in ("winit", "sctk-adwaita", "parley_engine", "wgpu-hal", "accesskit_windows", "accesskit_macos"):
    patch = patches.get(name, {})
    if any(patch.get(key) != rune[key] for key in ("git", "rev")):
        raise SystemExit(f"{name} must use the same repository and revision as Rune")
if resolved.get("patch", {}).get("unused"):
    raise SystemExit("Cargo.lock contains unused patches")
