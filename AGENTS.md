# AGENTS.md

Wraex is a Rust rewrite of two TypeScript Electron apps: Awakened PoE Trade (PoE1)
and Exiled Exchange 2 (PoE2). Read [PLAN.md](PLAN.md) before starting; it explains
the phases and why they are ordered the way they are.

## Ground rules

- **The TypeScript code is the spec.** `ref/apt` and `ref/ee2` are pinned, shallow
  submodules. Never edit them, never copy them into the tree. Produce fixtures from
  them instead (`tools/golden/`), and port until the Rust output matches.
- **Fixtures are the definition of done.** A ported module is done when its fixture
  diff is empty, not when the code "looks right".
- **Keep Rune pinned.** `rust/Cargo.toml` holds the `rune` rev, and the same rev in
  the `[patch.crates-io]` table for the six crates Rune vendors. Bump all of them
  together in a dedicated commit and re-run the UI checks when you do; a mismatch
  does not compile.
- **Static game data stays** as shipped by upstream/EE2 for now. Access it through
  the data layer so it can be swapped later.
- Do not send PRs or issues to Exiled Exchange 2: its `AI_POLICY.md` forbids
  agent-generated contributions. Upstream Awakened PoE Trade takes normal PRs, but
  this repo does not contribute Rust code there.
- Game files, item text and the trade site belong to Grinding Gear Games. Keep API
  behaviour to what the originals do (rate limits, User-Agent, one action per
  keypress).

## Commands

```sh
devenv shell                       # Rust 1.99.1, Node 22, Python, cargo-xwin
devenv tasks run check:all         # fmt + clippy + cargo test
devenv tasks run ref:update        # move ref/apt and ref/ee2 to upstream HEAD

cd rust
cargo test --workspace
cargo run -p wraex-app
cargo run -p wraex-app -- --snap /tmp/wraex.png
```

Reference tests (only when touching the fixture harness):

```sh
cd ref/ee2/renderer && npm install && npx vitest run   # 501 pass / 26 fail at v0.16.3
```

`npm ci` fails on that revision: its lockfile is out of sync. The 26 failures are
known and documented in PLAN.md; do not turn them into expected Rust behaviour.

## Layout

- `rust/crates/core` — game-agnostic: item model, stats/filters, trade queries, API.
- `rust/crates/game` — the `Game` trait; only game-specific behaviour goes here.
- `rust/crates/poe1`, `rust/crates/poe2` — per-game parsers, data and filters.
- `rust/crates/platform` — window tracking, overlay, hotkeys, clipboard, logs.
- `rust/crates/app` — the Rune UI shell.
- `tools/golden` — Node harness that loads the TS code and writes `fixtures/`.

## Checks the agent can run itself

- `cargo fmt` / `cargo clippy -D warnings` / `cargo test`.
- Fixture diff: `node tools/golden/regen.mjs` then `git diff fixtures/` (phase 1+).
- UI: render headless with `--snap` and inspect the PNG and element tree (phase 5).
