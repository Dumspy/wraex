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
  together in a dedicated commit and re-run the UI checks when you do. A stale
  patch rev can still compile when the vendored crate's version did not change, so
  `devenv tasks run check:rune-pin` and CI check that the revs match and that
  `Cargo.lock` has no unused patches.
- **Rune problems are documented here, not filed elsewhere.** Add each one (symptom, cause,
  workaround and where it lives) to [docs/rune-issues.md](docs/rune-issues.md), and re-check
  the list when Rune is bumped.
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
devenv shell                       # Rust 1.99.1, Node 24, Python, cargo-xwin
devenv tasks run check:all         # fmt + clippy + cargo test + Rune pin check
devenv tasks run ref:apt:latest    # move the PoE1 reference to upstream master
EE2_TAG=v0.16.3 devenv tasks run ref:ee2:release   # move the PoE2 reference to a tag

cd rust
cargo test --workspace
cargo run -p wraex-app
cargo run -p wraex-app -- --snap /tmp/wraex.png
```

Reference tests (only when touching the fixture harness):

```sh
cd ref/ee2/renderer
npm ci && npm run make-index-files && npx vitest run   # 527 pass / 2 skip at v0.16.3
```

The index files are generated, not committed; without `make-index-files` the suite
reports dozens of bogus failures. Use `npm ci` (never `npm install`, which rewrites
the reference's lockfile), and Node 24, matching EE2's own CI. PoE1 has no tests
beyond `main/build/apprun-patch.test.mjs`, and no npm script for its indexes
(`node src/assets/make-index-files.mjs` builds them; `golden:prepare` does both).

## Layout

- `rust/crates/core` — game-agnostic: item model, stats/filters, trade queries, API.
- `rust/crates/game` — the `Game` trait; only game-specific behaviour goes here.
- `rust/crates/poe1`, `rust/crates/poe2` — per-game parsers, data and filters.
- `rust/crates/platform` — window tracking, overlay, hotkeys, clipboard, logs.
- `rust/crates/app` — the Rune UI shell.
- `rust/crates/golden` — loads fixtures and compares Rust output with them.
- `tools/golden` — the fixture harness (vitest, runs the references); its README is
  the contract. Corpus in `tools/golden/corpus/`, fixtures in `fixtures/<game>/<case>/`.
- `tools/package` — the release packager (Python): `cargo build --release` plus
  `dist/wraex-<version>-<target>/` with the binary, `assets/` and `licences/`.
  `rust/crates/core/src/assets.rs` is where the app finds that directory at runtime.

Item names and serialisation follow upstream's `ParsedItem` (`rawText`, `"Rare"`).
Add fields only with the module that produces them; do not guess at the shape.

## Checks the agent can run itself

- `cargo fmt` / `cargo clippy -D warnings` / `cargo test`.
- Rune pin: `devenv tasks run check:rune-pin`.
- Fixture diff: `devenv tasks run golden:prepare` once, then
  `devenv tasks run check:fixtures` (regenerates, fails on any change in `fixtures/`).
  Never edit a fixture by hand; add corpus cases instead.
- UI: render headless with `--snap` and inspect the PNG and element tree (phase 5).
