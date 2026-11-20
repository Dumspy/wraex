{ pkgs, ... }:

let
  # The Rune workspace lives beside the tooling, not at the repo root, so the
  # reference checkouts in ref/ and the golden harness stay first-class.
  # (`cargo fmt` has no --manifest-path, so run everything from rust/.)
  cargo = "cd rust && cargo";
in
{
  # The Rust channel is pinned by rust-toolchain.toml (same as Rune's `dev`).
  languages.rust = {
    enable = true;
    toolchainFile = ./rust-toolchain.toml;
  };

  packages = with pkgs; [
    git
    # Runs the TypeScript reference code in ref/apt and ref/ee2, and the golden
    # harness in tools/golden/ that turns it into fixtures.
    nodejs_22
    # EE2's data pipeline (ref/ee2/dataParser).
    python3
    # `cargo xwin build` for the Windows build from this machine.
    cargo-xwin
    # Inspecting fixture diffs.
    jq
  ];

  # One entrypoint for local dev and CI: `devenv tasks run check:all`.
  tasks = {
    "check:fmt".exec = "${cargo} fmt --all --check";
    "check:clippy".exec = "${cargo} clippy --workspace --all-targets -- -D warnings";
    "check:test".exec = "${cargo} test --workspace";
    "check:all" = {
      after = [ "check:fmt" "check:clippy" "check:test" ];
      exec = "echo 'All checks passed'";
    };
    # Moves both reference checkouts to their latest upstream commits. Re-run
    # `golden regen` afterwards (tools/golden/, phase 1) to see what changed.
    "ref:update".exec = "git submodule update --remote --depth 1 ref/apt ref/ee2";
  };

  enterShell = ''
    echo "wraex dev shell: $(rustc --version), node $(node --version)"
    echo "reference: ref/apt (PoE1) @ $(git -C ref/apt rev-parse --short HEAD 2>/dev/null || echo 'not checked out')"
    echo "           ref/ee2 (PoE2) @ $(git -C ref/ee2 rev-parse --short HEAD 2>/dev/null || echo 'not checked out')"
  '';
}
