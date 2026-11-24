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

- **It has a test suite**: vitest, 23 test files, 529 test cases, ~7k lines in
  `renderer/specs/`. Run on 2026-11-21 (master @ v0.16.3, Node 24 via Nix, its own
  CI steps: `npm ci` then `npm run make-index-files`): **527 passed, 2 skipped, 0
  failed**. The earlier 501/26/2 result was the harness's fault, not EE2's: the
  `*.index.bin` files are generated, not committed, and without them the fetch mock
  returns 404s and stat lookups come back `undefined`. `npm ci` works; the lockfile
  is in sync. Covers the
  parser (integration with ~28 raw clipboard items in `specs/Parser/items.ts`,
  modifiers, stat translation, maps, gems, magic names), filter creation, pseudo
  stats, trade request building, client log parsing, prices, trade API.
  Tests load real data via `init("en")` + `specs/vitest.setup.ts`.
- Its parser has diverged a lot from upstream (`Parser.ts` 2269 vs 1342 lines,
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
- EE2's suite is the PoE2 reference and passes at the pinned revision, so it can be
  ported to Rust as-is alongside the golden fixtures.

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
- `devenv.nix`: Rust toolchain, Node 24 (EE2's CI version), Python with the
  `dataParser` dependencies, `cargo-xwin` (with the Windows target and LLVM tools).
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
- **Node harness** (`tools/golden/`, see its README for the contract; done: 16 PoE1
  and 27 PoE2 cases, CI regenerates and diffs them): runs under vitest with the
  reference repo's aliases and mocks, runs:
  1. `init(lang)` → `parseClipboard(text)` → `ParsedItem` JSON
  2. `createPresets` (`web/price-check/filters/create-presets.ts`) →
     `{ presets, active }`, then per preset `createTradeRequest` on the preset's
     `filters` + `stats` (`web/price-check/trade/pathofexile-trade.ts`, and
     `pathofexile-bulk.ts` for bulk items) → trade API request JSON
  3. map-check / item-check derived data
  Writes `fixtures/<game>/<case>/{input.txt,options.json,parsed.json,presets.json,trade/<id>.json}`.
  The `CreateOptions` used are recorded per case; they differ between the games.
- Normalize JSON (key order, floats) so Rust and TS output compare as text.
- Script: `golden regen` (TS, `devenv tasks run golden:regen`) and `golden check`
  (Rust, `wraex_golden::{cases, diff}` in each ported module's tests) → diff report.
- Still open: map-check / item-check data (step 3 above), a real PoE1 corpus
  (the current one is hand-written, see `tools/golden/corpus/README.md`),
  non-English cases, and porting EE2's vitest assertions.

## Phase 2: platform spike on Windows (≈ 1–2 weeks, go/no-go)

A throwaway Rune app proving everything that `electron-overlay-window` and
`uiohook-napi` do today:

| Need | Upstream | Rust plan |
|---|---|---|
| Overlay over the game window | `electron-overlay-window` | Rune `WindowOptions::overlay` + raw window handle; our own Win32 code to find the PoE window, follow move/resize/focus (`SetWinEventHook`), hide when the game loses focus |
| Click-through on/off (clickable when a widget is open) | `OverlayController` (`main/src/windowing/OverlayWindow.ts`) | `WindowHandle` click-through toggle |
| System-wide hotkeys while PoE is focused | `uiohook-napi`, `globalShortcut` | `rdev` or a low-level keyboard hook, or `global-hotkey` |
| Send Ctrl+C / Ctrl+Alt+C to copy item, type in chat, stash search | uiohook key taps | `enigo` or `SendInput` |
| Read the clipboard + restore it afterwards | `main/src/shortcuts/HostClipboard.ts` | `arboard` or Rune `Tx::copy` (read side to verify) |
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

### Shared vs per-game split (decided 2026-11-22)

Method: both `renderer/src` trees normalised with the same prettier settings (EE2
reformatted everything, so raw diffs are noise), then compared function by
function. Result: the two have diverged far more than "much of filter and query
building" suggested. Outside the data-access mechanics and a few helpers, no
function body is identical: in `Parser.ts` only `markupConditionParser` matches,
and `modifiers.ts`, `calc-q20.ts`, `magic-name.ts` and `pathofexile-trade.ts`
share none. Merging diverged logic behind flags would make every reference bump
hard to follow, so the rule is: **core holds mechanisms and byte-identical
helpers; anything that reads or writes a game's item, stat or filter shapes is
per game, one Rust module per TS file**, so a reference diff maps onto one module.

| TS module (`renderer/src/…`) | Where | Notes |
|---|---|---|
| `assets/data/index.ts`: fnv1a 32, index build (`make-index-files.mjs`), binary search, ndjson line lookup and substring iterator | `core::data` | Same algorithm in both. The game supplies which keys a line indexes: PoE1 stat lines may be groups and index `advanced ?? string`, PoE2 lines index `string` and `advanced` |
| Data files: where `items/stats.ndjson`, `client_strings.js`, `item-drop.json` come from | `core::data::DataSource` trait | Directory source now (`ref/*/renderer/public/data`), swappable later (PLAN: less static data) |
| `client_strings.js` | `core::data` reader, per-game key structs | Every file is one literal per line (string, regex, array); a strict reader, no JS engine. Keys differ a lot between games |
| `assets/data/interfaces.ts` (`BaseType`, `Stat`, `StatGroup`, `TranslationDict`) | per game | Schemas diverged (PoE1 stat groups, areas, mercenaries; PoE2 augments, stat tiers) |
| PoE2-only data: `TradeData` (`/api/trade2/data`), augment and catalyst tables | `poe2::data` | Tests read the trade data from `ref/ee2/renderer/specs/data`, as the harness does |
| `parser/*` (`Parser.ts`, `ParsedItem.ts`, `meta.ts`, `modifiers.ts`, `stat-translations.ts`, `advanced-mod-desc.ts`, `calc-q20.ts`, `magic-name.ts`; EE2's `augment-builder.ts`, `calc-base.ts`, `utils.ts`) | per game | `ParsedItem` differs (`isCorrupted` is `{twice?}` vs `boolean`, PoE2 augments/requires/waystones), so `Item` moves out of core. Section pipeline and section order differ |
| Byte-identical helpers: `advanced-mod-desc` `applyIncr`/`groupLinesByMod`/`incrRoll`/`isModInfoLine`, `linesToStatStrings`, `markupConditionParser`, `filters/util.ts`, `ceil/floorToBracket` | `core` | Only while they stay identical in both references and touch no game types; if one side changes, the copy moves to that game |
| `ItemRarity` | `core` | Same enum in both |
| `web/price-check/filters/*` (`create-item-filters`, `create-stat-filters`, `create-presets`, `interfaces`, `pseudo/*`; EE2's `common`, `create-unique-filters`, `fill-augments`) | per game | 30–100 % of lines differ; PoE1 has stat-filter groups, PoE2 augments and unique presets |
| `trade/pathofexile-trade.ts` (`createTradeRequest`) | per game | 961 vs 2103 lines, nothing shared |
| `trade/common.ts` `apiToSatisfySearch` | per game | Small, differs (PoE2 stack-size rule, PoE1 filter groups) |
| `trade/pathofexile-bulk.ts` | `core`, parameterised | Differs in endpoint (`trade`/`trade2`), listing option, result count and default currency |
| `trade/RateLimiter.ts`, `artificial-slowdown.ts`, `Cache.ts`, request sending in `common.ts` | `core` (phase 4) | Identical apart from `Cache.purgeIfDifferentCurrency` (PoE2) |

The `Game` trait gets associated types (`type Item: Serialize`, later filters and
presets) instead of the shared `core::Item`; the app selects a game through an enum
over the two implementations, so the trait need not be object-safe.

Languages: English only for now (decided 2026-11-22). Every fixture is `en`;
non-English clients are not a goal of this milestone. The data layer still loads
every language the references ship (PoE1 `ru`, `ko`, `cmn-Hant`; PoE2 also `ja`,
`de`, `es`, `pt`, `fr`; `th` is UI text only), so they stay possible later.

In order, each step done when its fixtures pass (for each game):

1. `apt-data`: items/stats ndjson + index loading, per language. **Done**
   (`core::data`, `poe1::data`, `poe2::data`): every language loads, every
   ndjson line round-trips through the typed schema, and the in-memory indexes
   hold the same rows as the generated `*.index.bin` (`--ignored` tests, run by
   `check:fixtures` and CI). Deferred to the modules that need them:
   `item-drop.json`, `patrons.json`.
2. `parser/modifiers.ts`, `stat-translations.ts`, `advanced-mod-desc.ts`
3. `parser/Parser.ts` (`parseClipboard`), `calc-q20.ts`, `magic-name.ts`.
   **PoE1 done** (`poe1::parser`, with `core::js` and `core::js_regex` for
   JavaScript number and regex semantics): `parsed.json` matches for every
   PoE1 case. **PoE2 done** (`poe2::parser`, incl. `augment-builder.ts` and
   rune detection): every PoE2 case matches. Shared helpers moved to
   `core::parse`. Not yet: filling empty sockets from `savedAugments`
   (`useAugment`, needs `calc-base.ts`), which the harness's default config
   never reaches. Steps 2 and 3 are checked together, since the fixtures record
   whole items. English only (see Languages above).
4. `price-check/filters` (incl. `pseudo`) → default filter generation.
   **PoE1 done** (`poe1::filters`, helpers shared with PoE2 in `core::filters`):
   `presets.json` matches for all 40 parsed PoE1 cases, ten of them added for
   filter branches (logbooks, mercenaries, Atzoatl, Mirrored Tablet, timeless
   jewels, anointments, ...). Where the reference throws or asserts a missing
   field with `!`, `createPresets` returns a `FilterError`.
   **PoE2 done** (`poe2::filters`, incl. `common.ts`, Mageblood's
   `create-unique-filters.ts` and `pseudo/missing-fractured-rules.ts`):
   `presets.json` matches for all 29 parsed PoE2 cases, two of them added for
   filter branches. Not ported: `fill-augments.ts` and the `calc-base.ts`
   behind it, which only the item editor and settings page call.
5. `price-check/trade` → trade API request building (and bulk exchange).
   **Done for both games**: `poe1::trade` and `poe2::trade` port
   `apiToSatisfySearch` and `createTradeRequest`; the bulk exchange request
   and `dot-prop`'s `setProperty` are in `core::trade`, the bulk status chosen
   per game. Every `trade/<id>.json` matches (47 PoE1, 40 PoE2 requests).
   Sending, rate limits and result parsing are phase 4.
6. Map-check and item-check logic. **Done for both games**: the harness also
   records `map-check.json` (prepared map stats, for map-like items) and
   `item-check.json` (the URLs and stash searches of the item check's hotkey
   actions); `poe1`/`poe2::map_check` (with `common.ts`' per-profile
   decisions) and `::item_check` match them. More real PoE2 waystones are
   wanted: https://github.com/Dumspy/wraex/issues/2

Agent loop per step: port module → `golden check` → fix diffs → repeat.

## Phase 4: services (≈ 1–2 weeks)

- Trade API client with GGG's rate-limit headers and required User-Agent
  (`main/src/proxy.ts` today), leagues, price data (`background/Prices.ts`).
  **Client done** (2026-11-23): `core::trade::rate_limit` ports `RateLimiter.ts`,
  `adjustRateLimits` and `preventQueueCreation` as clock-free state;
  `core::trade::api::TradeClient` sends search, fetch and exchange requests
  within those limits and caches results for 5 minutes, over a `Transport`
  (`wraex_platform::http`, blocking `ureq`, upstream's proxy host allowlist,
  User-Agent `wraex/<version> (+repo URL)`). Leagues (`core::leagues`, both
  games), PoE1 result rows and bulk groups (`poe1::results`, with Luxon's
  short relative dates in `core::relative_time`), EE2's listing display items
  (`poe2::results`, matched against `fixtures/listings/poe2/`, recorded from
  EE2's own `specs/docs/fetchResponses.json`) and PoE1's poe.ninja prices
  (`poe1::prices`) are ported, and so are EE2's prices (`poe2::prices`) and
  result rows. EE2 downloads its price data from its own server
  (`api.exiledexchange2.dev`); Wraex asks poe.ninja's PoE2 exchange overviews
  directly instead (decided 2026-11-24), one request per category, and looks
  items up by name. Other sources (EE2's server, by agreement) can come later. Not yet: the session cookies upstream's proxy
  forwards (Cloudflare, a logged-in POESESSID).
- `GameConfig.ts` (reads PoE's keybinds from `production_Config.ini`),
  `GameLogWatcher.ts` (Client.txt for zone/chat events).
  **Done** (`core::game_files`, `core::keys`): default locations per game and
  OS, the one-readable-candidate rule, the "show advanced item descriptions"
  hotkey from the INI, and `Client.txt` tailing (EE2's re-read included).
- Settings storage (`ConfigStore.ts`): read upstream's config format so users
  can switch over without losing settings.
  **PoE1 done** (`core::config_store`, `poe1::config`): the store (with
  upstream's path to import from), `defaultConfig`, every `upgradeConfig` step,
  `initConfig` and `getConfigForHost`, matched against `fixtures/config/poe1/`.
  PoE2 next.
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

Checked 2026-11-21 against `ref/apt` @ `2498730` (master) and `ref/ee2` @ `cca3066`
(v0.16.3). Everything below `renderer/src/` that the phases above call
`price-check/...` or `trade/...` actually sits under `renderer/src/web/`:

| What | `ref/apt` (PoE1) | `ref/ee2` (PoE2) |
|---|---|---|
| Clipboard entry point | `renderer/src/parser/Parser.ts:95` `parseClipboard` | `renderer/src/parser/Parser.ts:189` |
| Parser modules | `renderer/src/parser/{modifiers,stat-translations,advanced-mod-desc,calc-q20,magic-name}.ts` | same names, plus `augment-builder.ts`, `calc-base.ts`, `utils.ts` |
| Data loading | `renderer/src/assets/data/index.ts:192` `init` | `renderer/src/assets/data/index.ts:286` `init` |
| Index generation | `renderer/src/assets/make-index-files.mjs` via `vite-plugin-make-indexes.ts` (no npm script) | `npm run make-index-files` |
| Default filters | `.../filters/create-item-filters.ts:23` `createFilters` | `.../create-item-filters.ts:40` |
| Stat filters | `.../filters/create-stat-filters.ts:140` `initUiModFilters` | `.../create-stat-filters.ts` |
| Presets | `.../filters/create-presets.ts:14` `createPresets` → `{presets, active}` | same shape |
| Trade query | `.../trade/pathofexile-trade.ts:270` + `pathofexile-bulk.ts:119` (`createTradeRequest`) | `.../pathofexile-trade.ts:567` + `pathofexile-bulk.ts:139` |
| Electron main | `main/src/{proxy,server}.ts`, `main/src/shortcuts/`, `main/src/windowing/`, `main/src/vision/` | `main/` + `ipc/` |
| Tests | only `main/build/apprun-patch.test.mjs` | 23 `renderer/specs/**/*.test.ts` + `specs/vitest.setup.ts` |

EE2's spec files use `*.test.ts` (not `*.spec.ts`) and load the real data through
`init("en")` in `renderer/specs/vitest.setup.ts`. Both apps need their generated
`*.index.bin` files before the data loads; EE2's CI runs `npm ci` then
`npm run make-index-files` on Node 24 (527 passed, 2 skipped at this revision).
