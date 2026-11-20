# Awakened PoE Trade → Rust + Rune: initial plan

Fork of `SnosMe/awakened-poe-trade` (MIT; keep the copyright notice). Upstream is
~20.6k lines of TypeScript/Vue: an Electron main process (~2k lines) and a Vue 3
renderer (parser ~2.5k lines, the rest is trade logic and widget UI).

Goal: one native Rust app with a Rune UI that supports **both PoE1 and PoE2**,
behaving the same as the two TS apps (upstream for PoE1, Exiled Exchange 2 for
PoE2), and that can keep pulling in changes from both.

Scope decisions (2026-11-18):
- First milestone: a **game-agnostic core** shared by both games (platform,
  overlay, hotkeys, trade client, UI shell), with per-game parser and data.
- **Static game data stays** as-is for now (upstream's and EE2's generated data
  files). Reducing how much we depend on static data that must be regenerated on
  every game patch is a far-future goal; keep the data layer behind a trait so it
  can be swapped later.
- Toolchain via Nix/devenv (Node for running the TS reference code, Rust for the port).

## Exiled Exchange 2 (PoE2 fork)

`Kvan7/Exiled-Exchange-2` is a GitHub fork of upstream for Path of Exile 2 (MIT,
same copyright line, ~1.2k stars, v0.16.3 on 2026-09-06). ~39.7k lines TS/Vue.

- **It has a test suite**: vitest, 23 spec files, 529 test cases, ~7k lines in
  `renderer/specs/`. Run on 2026-11-18 (master @ v0.16.3, Node 22 via Nix):
  **501 passed, 26 failed, 2 skipped**. Failures are in augment, magic-name,
  modifiersParsers, dataLoader and pseudo-filter specs; most are stat lookups
  returning `undefined` (`calc.stat` / `.matchers`) and missing zh-Hant item
  names, which looks like test data out of date with the data files (not yet
  confirmed). Its `package-lock.json` is out of sync, so `npm ci` fails and
  `npm install` is needed. Covers the
  parser (integration with ~28 raw clipboard items in `specs/Parser/items.ts`,
  modifiers, stat translation, maps, gems, magic names), filter creation, pseudo
  stats, trade request building, client log parsing, prices, trade API.
  Tests load real data via `init("en")` + `specs/vitest.setup.ts`.
- Its parser has diverged a lot from upstream (`Parser.ts` 2269 vs 1319 lines,
  extra `augment-builder.ts`, `calc-base.ts`, `utils.ts`), so its tests are for
  PoE2 items and won't run against PoE1 code as-is.
- It has its own data pipeline (`dataParser/`, Python) instead of relying on
  upstream's data files.
- `AI_POLICY.md`: contributions there may use at most end-of-line completion,
  no agents. That doesn't restrict an MIT fork, but it means AI-ported work can't
  be contributed back to EE2, and we shouldn't send them agent-written PRs/issues.

Use for us:
- The **structure of its specs** (raw item text → expected fields, per module) is a
  template for our Phase 1 fixtures; its item texts seed the PoE2 corpus.
- If the Rust port targets PoE2 (or both games), EE2's suite is a ready-made
  first set of checks: port the tests to Rust alongside the golden fixtures.
- EE2's suite is the PoE2 reference. Its 26 failures get recorded as known
  failures (not copied into Rust as expected behaviour) until we understand them.

## Guiding rules

1. **The JavaScript version is the spec.** Every logic module gets ported against
   golden fixtures produced by running upstream's TS code. A port is done when the
   output is identical.
2. **Prove the platform before porting features.** The overlay, hotkeys and
   clipboard are what would kill the project, so they get tested first, on Windows.
3. **Small, ordered steps an agent can verify.** One module or widget per step,
   each with a check the agent can run itself (fixture diff, headless Rune PNG,
   element tree).
4. **Track upstream from day one.** Regenerate fixtures from each upstream release;
   any fixture that changes is the to-do list.

## Phase 0: setup (≈ days)

- Fork the repo. Add `rust/` as a Cargo workspace beside the TS code, so upstream
  merges stay simple and the TS stays runnable for producing fixtures.
- Reference code as git submodules (or subtrees): `ref/apt` (upstream) and
  `ref/ee2` (Exiled Exchange 2), so fixtures can be regenerated from both.
- `devenv.nix`: Rust toolchain, Node 22, Python (EE2's `dataParser`), `cargo-xwin`.
- Crates:
  - `core`: game-agnostic types and logic: item model, stat/filter model, trade
    API client (rate limits, User-Agent, leagues), trade query building, prices
  - `game`: the trait each game implements: data loading, `parse_clipboard`,
    default filters, game window/process names, config and log file paths
  - `poe1`, `poe2`: per-game parser, data loader (upstream's / EE2's data files),
    game-specific filters and widgets
  - `platform`: game window tracking, hotkeys, input simulation, clipboard,
    log/config file watching
  - `app`: the Rune UI; picks a game at runtime
- Pin a Rune `dev` rev in `Cargo.toml`. Bump it on purpose, never by drift.
- CI: Windows + Linux build, `cargo test`, fixture diff, headless UI snapshots.

## Phase 1: golden fixture harness (≈ 1 week)

Upstream has no logic tests (only `main/build/apprun-patch.test.mjs`), so we make
them. One harness, run against both `ref/apt` (PoE1) and `ref/ee2` (PoE2), writing
to `fixtures/poe1/` and `fixtures/poe2/`. For PoE2, seed the corpus with the item
texts from EE2's `specs/Parser/items.ts` and port its vitest assertions too.

- **Item corpus**: clipboard texts (Ctrl+Alt+C in game) covering normal, magic, rare,
  unique and unidentified items; corrupted, mirrored and influenced items; gems,
  maps, currency, divination cards, cluster jewels, heist items, sanctum items,
  logbooks; plus every supported language (en, ru, ko, cmn-Hant). Sources: own
  play, GitHub issues, community dumps.
- **Node harness** (`tools/golden/`): loads the data files the way
  `renderer/src/assets/data/index.ts` does (replace `fetch` with fs), runs:
  1. `parseClipboard(text)` → `ParsedItem` JSON
  2. `createFilters` / `createPresets` (`price-check/filters/`) → default filters, then
     `createTradeRequest` (`trade/pathofexile-trade.ts`, and `trade/pathofexile-bulk.ts`
     for bulk items) → trade API request JSON
  3. map-check / item-check derived data
  Writes `fixtures/<case>.{input.txt,parsed.json,query.json}`.
- Normalize JSON (key order, floats) so Rust and TS output compare as text.
- Script: `golden regen` (TS) and `golden check` (Rust) → diff report.

## Phase 2: platform spike on Windows (≈ 1–2 weeks, go/no-go)

A throwaway Rune app proving everything that `electron-overlay-window` and
`uiohook-napi` do today:

| Need | Upstream | Rust plan |
|---|---|---|
| Overlay over the game window | `electron-overlay-window` | Rune `WindowOptions::overlay` + raw window handle; our own Win32 code to find the PoE window, follow move/resize/focus (`SetWinEventHook`), hide when the game loses focus |
| Click-through on/off (clickable when a widget is open) | `OverlayController` | `WindowHandle` click-through toggle |
| System-wide hotkeys while PoE is focused | `uiohook-napi`, `globalShortcut` | `rdev` or a low-level keyboard hook, or `global-hotkey` |
| Send Ctrl+C / Ctrl+Alt+C to copy item, type in chat, stash search | uiohook key taps | `enigo` or `SendInput` |
| Read the clipboard + restore it afterwards | `HostClipboard.ts` | `arboard` or Rune `Tx::copy` (read side to verify) |
| Scroll wheel actions (stash tab scrolling) | uiohook wheel | low-level mouse hook |

**Exit criteria**: on Windows with PoE (or a dummy fullscreen-windowed app), the
hotkey copies an item, the overlay shows a clickable panel with the text at the
cursor position, focus returns to the game, and nothing breaks in borderless
windowed mode. Also try X11 and Wayland (Wayland: Rune's interactive overlays are
unsupported there, so note what's lost).

If this fails: report issues to Rune, or fall back to a different native UI
framework for the overlay. Phases 1 and 3 are not affected either way.

## Phase 3: port core logic (≈ 2–4 weeks per game)

Before porting, diff upstream and EE2 module by module to decide what goes in
`core` (same in both, e.g. trade API client, rate limiting, much of filter and
query building) and what is per game (parser sections, item classes, PoE2
augments/spirit, PoE1 influences/heist/etc.). Port the shared parts once, then
each game's parser behind the `game` trait.

In order, each step done when its fixtures pass (for each game):

1. `apt-data`: items/stats ndjson + index loading, per language
2. `parser/modifiers.ts`, `stat-translations.ts`, `advanced-mod-desc.ts`
3. `parser/Parser.ts` (`parseClipboard`), `calc-q20.ts`, `magic-name.ts`
4. `price-check/filters` (incl. `pseudo`) → default filter generation
5. `price-check/trade` → trade API request building (and bulk exchange)
6. Map-check and item-check logic

Agent loop per step: port module → `golden check` → fix diffs → repeat.

## Phase 4: services (≈ 1–2 weeks)

- Trade API client with GGG's rate-limit headers and required User-Agent
  (`main/src/proxy.ts` today), leagues, price data (`background/Prices.ts`).
- `GameConfig.ts` (reads PoE's keybinds from `production_Config.ini`),
  `GameLogWatcher.ts` (Client.txt for zone/chat events).
- Settings storage (`ConfigStore.ts`): read upstream's config format so users
  can switch over without losing settings.
- Tray icon, auto-updater, logging.
- Later / optional: OCR (`main/src/vision`, Heist gem finder).

## Phase 5: UI in Rune (≈ 4–8 weeks)

Widget by widget, most used first:

1. Price check window (item header, filters, results table, trends, price prediction)
2. Settings window (general, hotkeys incl. hotkey recorder, price check, chat, maps)
3. Item check / map check
4. Widget menu, stash search, item search, stopwatch, image strip, delve grid
5. i18n (reuse upstream's Crowdin translation files)

Verification per widget: run the Electron widget with fixture state and take a
screenshot; render the Rune widget headlessly with the same state; the agent
compares the two and checks the element tree for required elements and labels.
Visual similarity, not pixel identity.

## Phase 6: release + upstream tracking

- Windows installer + portable build, Linux AppImage.
- Per upstream or EE2 release: bump the `ref/` submodule → `golden regen` → diff
  fixtures → port changed logic → update UI where their UI changed → release.
- Corpus keeps growing: every bug report with item text becomes a fixture.

## Far future: less static data

Both TS apps ship generated data (items, stats, translations, indexes) that has
to be rebuilt on every game patch. Later options to explore, behind the data
trait from Phase 0: fetching data at runtime from the trade API's
`/data/items`, `/data/stats` endpoints, deriving matchers from those, and keeping
only small hand-maintained overrides. Not part of the first milestone.

## Main risks

- **Rune is pre-release** (created Oct 2026): API churn, Windows overlays only
  cross-compile-checked upstream. Mitigation: pinned rev, phase 2 spike, report issues early.
- **Game window tracking** has no ready-made Rust crate equal to
  `electron-overlay-window`; we write it.
- **Anti-cheat / GGG ToS**: the same input simulation upstream does is accepted
  today; don't add anything beyond one action per keypress.
- **Upstream drift**: each league changes the parser and data; the fixture
  harness is what keeps this manageable.
- **Item corpus** quality limits how much the fixtures prove.

## First concrete steps

1. Fork + `rust/` workspace skeleton.
2. Phase 2 spike (Windows overlay + hotkey + clipboard), in parallel with
3. Phase 1 harness on 20–50 item texts to prove the fixture pipeline end to end.

## Appendix: verified paths at the pinned revisions

Checked 2026-11-20 against `ref/apt` @ `2498730` (master) and `ref/ee2` @ `cca3066`
(v0.16.3). Everything below `renderer/src/` that the phases above call
`price-check/...` or `trade/...` actually sits under `renderer/src/web/`:

| What | `ref/apt` (PoE1) | `ref/ee2` (PoE2) |
|---|---|---|
| Clipboard entry point | `renderer/src/parser/Parser.ts:95` `parseClipboard` | `renderer/src/parser/Parser.ts` |
| Parser modules | `renderer/src/parser/{modifiers,stat-translations,advanced-mod-desc,calc-q20,magic-name}.ts` | same names, plus `augment-builder.ts`, `calc-base.ts`, `utils.ts` |
| Data loading | `renderer/src/assets/data/index.ts` | `renderer/src/assets/data/index.ts` |
| Default filters | `renderer/src/web/price-check/filters/create-item-filters.ts` `createFilters` | same |
| Presets | `renderer/src/web/price-check/filters/create-presets.ts` `createPresets` | same |
| Trade query | `renderer/src/web/price-check/trade/pathofexile-trade.ts:270` + `pathofexile-bulk.ts:119` (`createTradeRequest`) | same layout |
| Electron main | `main/src/{proxy,server}.ts`, `main/src/host-files/`, `main/src/shortcuts/`, `main/src/windowing/` | `main/` + `ipc/` |
| Tests | only `main/build/apprun-patch.test.mjs` | `renderer/specs/**/*.test.ts` + `specs/vitest.setup.ts` |

EE2's spec files use `*.test.ts` (not `*.spec.ts`) and load the real data through
`init("en")` in `renderer/specs/vitest.setup.ts`.
