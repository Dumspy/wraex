{ pkgs, config, lib, ... }:

let
  root = config.devenv.root;
  # The Rune workspace lives beside the tooling, not at the repo root, so the
  # reference checkouts in ref/ and the golden harness stay first-class.
  # (`cargo fmt` has no --manifest-path, so run everything from rust/.)
  cargo = "cd ${root}/rust && cargo";
  # The git hooks run from the repo root and can run outside this shell (a GUI
  # git client, say), so they put the pinned toolchain on PATH themselves.
  # Shared with the check: tasks below, so a hook runs exactly what CI runs.
  inRust = script: ''
    set -eu
    export PATH=${config.languages.rust.toolchainPackage}/bin:$PATH
    cd ${root}/rust
    ${script}
  '';
  fmtCheck = pkgs.writeShellScript "cargo-fmt-check" (inRust ''
    if ! cargo fmt --all --check; then
      echo "rust/: formatting differs; run 'cargo fmt --all' in rust/ and commit the result" >&2
      exit 1
    fi
  '');
  clippyCheck = pkgs.writeShellScript "cargo-clippy" (inRust ''
    exec cargo clippy --workspace --all-targets -- -D warnings
  '');
  # `rune` and every vendor patch must move together; a stale patch rev can
  # still compile when the vendored crate's version did not change.
  runePin = ''
    cd ${root}
    revs=$(grep -oE 'rev = "[0-9a-f]{40}"' rust/Cargo.toml | sort -u | wc -l)
    if [ "$revs" -ne 1 ]; then
      echo "rust/Cargo.toml has $revs different Rune revs (rune + [patch.crates-io])"
      exit 1
    fi
    if grep -q 'patch.unused' rust/Cargo.lock; then
      echo "rust/Cargo.lock has unused patches: bump the patch revs with rune"
      exit 1
    fi
  '';
  runePinCheck = pkgs.writeShellScript "rune-pin" runePin;
  # `act` needs Docker, and some actions expect a GITHUB_TOKEN. The `gh` one is
  # passed when it is logged in; checkout of the submodules is local, so the
  # token is optional. `set --` builds act's extra arguments, for any shell.
  actToken = ''
    set --
    token=$(gh auth token 2>/dev/null || true)
    if [ -n "$token" ]; then
      set -- -s "GITHUB_TOKEN=$token"
    fi
  '';
in
{
  # The Rust channel is pinned by rust-toolchain.toml (same as Rune's `dev`),
  # including the Windows target used by cargo-xwin.
  languages.rust = {
    enable = true;
    toolchainFile = ./rust-toolchain.toml;
  };

  packages = with pkgs; [
    git
    # Match the Node EE2's CI uses: it runs npm ci and the vitest suite on 24.
    nodejs_24
    # EE2's data pipeline (ref/ee2/dataParser) needs these; it is only used when
    # regenerating game data, not for the test suite or the fixtures.
    (python3.withPackages (ps: [ ps.numpy ps.requests ps.tqdm ]))
    # `cargo xwin build` for the Windows build from this machine. cargo-xwin needs
    # clang-cl, lld-link and llvm-lib; the unwrapped clang is the working clang-cl.
    cargo-xwin
    llvmPackages.clang-unwrapped
    lld
    llvm
    # Inspecting fixture diffs.
    jq
    # Run the GitHub Actions workflows locally (ci:local, ci:release:local): it
    # reads .actrc, which maps ubuntu-latest to an amd64 image.
    act
  ] ++ lib.optionals pkgs.stdenv.isLinux [
    # Rune links libxkbcommon and libwayland-client; window runs also need the
    # X11/XKB libraries and, without a GPU, a software Vulkan driver.
    pkg-config
    libxkbcommon
    wayland
    vulkan-loader
    xorg.libX11
    xorg.libXcursor
    xorg.libXi
    # The tray icon's AppIndicator backend (tray-icon README): GTK 3 to build the
    # menu, libayatana-appindicator3 loaded at runtime.
    gtk3
    libayatana-appindicator
  ];

  env = lib.mkIf pkgs.stdenv.isLinux {
    LD_LIBRARY_PATH = lib.makeLibraryPath [
      pkgs.vulkan-loader
      pkgs.libxkbcommon
      pkgs.wayland
      pkgs.xorg.libX11
      pkgs.xorg.libXcursor
      pkgs.xorg.libXi
      pkgs.gtk3
      pkgs.libayatana-appindicator
    ];
  };

  # One entrypoint for local checks: `devenv tasks run check:all`.
  tasks = {
    "check:fmt".exec = "${fmtCheck}";
    "check:clippy".exec = "${clippyCheck}";
    "check:test".exec = "${cargo} test --workspace";
    "check:rune-pin".exec = runePin;
    # Golden fixtures (tools/golden). `golden:prepare` once per reference bump:
    # it installs the references' npm deps and builds their index files.
    "golden:prepare".exec = "cd ${root}/tools/golden && npm ci --no-audit --no-fund && npm run prepare-refs";
    "golden:regen".exec = "cd ${root}/tools/golden && npm run regen";
    # Reference price-check screenshots (phase 5), built and rendered on demand.
    # Output is target/screens/, never committed; `--rebuild` after a ref bump.
    "golden:screens".exec = "cd ${root}/tools/golden && npm run screens";
    # Not part of check:all: it needs golden:prepare first. CI runs it.
    "check:fixtures" = {
      after = [ "golden:regen" ];
      exec = ''
        # The Rust data indexes against the `*.index.bin` files golden:prepare built.
        ${cargo} test -p wraex-poe1 -p wraex-poe2 -- --ignored
        cd ${root}
        # Against the index, so staged fixtures count as expected.
        if ! git diff --quiet -- fixtures || [ -n "$(git ls-files --others --exclude-standard -- fixtures)" ]; then
          git status --short -- fixtures
          echo "fixtures differ from what the references produce (see git diff fixtures/)"
          exit 1
        fi
      '';
    };
    "check:all" = {
      after = [ "check:fmt" "check:clippy" "check:test" "check:rune-pin" ];
      exec = "echo 'All checks passed'";
    };
    # The GitHub Actions workflows locally (nektos/act + Docker, against .actrc).
    # amd64 containers are emulated on Apple Silicon: slow, but end to end. A manual
    # release run only uploads artifacts (the draft release waits for a v* tag), and
    # one matrix leg can be run directly, e.g. the Windows cross-build:
    #   act workflow_dispatch -W .github/workflows/release.yml -j package --matrix name:windows
    "ci:local".exec = ''
      cd ${root}
      ${actToken}
      act "$@" pull_request -W .github/workflows/ci.yml
    '';
    "ci:fixtures:local".exec = ''
      cd ${root}
      ${actToken}
      act "$@" pull_request -W .github/workflows/fixtures.yml
    '';
    "ci:release:local".exec = ''
      cd ${root}
      ${actToken}
      act "$@" workflow_dispatch -W .github/workflows/release.yml -j package
    '';
    # Reference tracking. Apt ships from master; EE2 ships releases, so pick the
    # tag explicitly:  EE2_TAG=v0.16.3 devenv tasks run ref:ee2:release
    "ref:apt:latest".exec = ''
      git -C ${root}/ref/apt fetch --depth 1 origin master
      git -C ${root}/ref/apt checkout --detach FETCH_HEAD
    '';
    "ref:ee2:release".exec = ''
      test -n "$EE2_TAG" || { echo "set EE2_TAG (e.g. EE2_TAG=v0.16.3)"; exit 1; }
      git -C ${root}/ref/ee2 fetch --depth 1 origin "tag $EE2_TAG"
      git -C ${root}/ref/ee2 checkout --detach FETCH_HEAD
    '';
  };

  # Cheap Rust checks as git hooks: fmt on commit, the Rune pin and clippy on
  # push. These replace CI's former fmt and rune-pin jobs, which CI keeps only
  # as seconds-long steps in the test job because a hook can be skipped; in an
  # emergency, `git push --no-verify` (or `git commit --no-verify`) skips them.
  git-hooks.hooks = {
    fmt = {
      enable = true;
      name = "cargo fmt";
      entry = "${fmtCheck}";
      files = "\\.rs$";
      pass_filenames = false;
      stages = [ "pre-commit" ];
    };
    rune-pin = {
      enable = true;
      name = "Rune pin";
      entry = "${runePinCheck}";
      pass_filenames = false;
      always_run = true;
      stages = [ "pre-push" ];
    };
    # Not the attribute name `clippy`: git-hooks.nix ships a built-in hook of
    # that name whose defaults (and packages) assume Cargo.toml at the root.
    rust-clippy = {
      id = "clippy";
      enable = true;
      name = "cargo clippy";
      entry = "${clippyCheck}";
      pass_filenames = false;
      always_run = true;
      stages = [ "pre-push" ];
    };
  };

  enterShell = ''
    echo "wraex dev shell: $(rustc --version), node $(node --version)"
    echo "reference: ref/apt (PoE1) @ $(git -C ${root}/ref/apt rev-parse --short HEAD 2>/dev/null || echo 'not checked out')"
    echo "           ref/ee2 (PoE2) @ $(git -C ${root}/ref/ee2 rev-parse --short HEAD 2>/dev/null || echo 'not checked out')"
  '';
}
