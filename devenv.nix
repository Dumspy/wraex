{ pkgs, config, lib, ... }:

let
  root = config.devenv.root;
  # The Rune workspace lives beside the tooling, not at the repo root, so the
  # reference checkouts in ref/ and the golden harness stay first-class.
  # (`cargo fmt` has no --manifest-path, so run everything from rust/.)
  cargo = "cd ${root}/rust && cargo";
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
  ];

  env = lib.mkIf pkgs.stdenv.isLinux {
    LD_LIBRARY_PATH = lib.makeLibraryPath [
      pkgs.vulkan-loader
      pkgs.libxkbcommon
      pkgs.wayland
      pkgs.xorg.libX11
      pkgs.xorg.libXcursor
      pkgs.xorg.libXi
    ];
  };

  # One entrypoint for local checks: `devenv tasks run check:all`.
  tasks = {
    "check:fmt".exec = "${cargo} fmt --all --check";
    "check:clippy".exec = "${cargo} clippy --workspace --all-targets -- -D warnings";
    "check:test".exec = "${cargo} test --workspace";
    # `rune` and every vendor patch must move together; a stale patch rev can
    # still compile when the vendored crate's version did not change.
    "check:rune-pin".exec = ''
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
    # Golden fixtures (tools/golden). `golden:prepare` once per reference bump:
    # it installs the references' npm deps and builds their index files.
    "golden:prepare".exec = "cd ${root}/tools/golden && npm ci --no-audit --no-fund && npm run prepare-refs";
    "golden:regen".exec = "cd ${root}/tools/golden && npm run regen";
    # Not part of check:all: it needs golden:prepare first. CI runs it.
    "check:fixtures" = {
      after = [ "golden:regen" ];
      exec = ''
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

  enterShell = ''
    echo "wraex dev shell: $(rustc --version), node $(node --version)"
    echo "reference: ref/apt (PoE1) @ $(git -C ${root}/ref/apt rev-parse --short HEAD 2>/dev/null || echo 'not checked out')"
    echo "           ref/ee2 (PoE2) @ $(git -C ${root}/ref/ee2 rev-parse --short HEAD 2>/dev/null || echo 'not checked out')"
  '';
}
