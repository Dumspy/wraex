# Golden fixture harness

Phase 1 (see [PLAN.md](../../PLAN.md)). Runs the pinned TypeScript reference apps and
records what they produce, so the Rust port has something exact to match.

This file is the contract; the layout and entry points below are the single source
of truth (README and PLAN point here). The Rust side is `rust/crates/golden`.

## Usage

```sh
devenv tasks run golden:prepare   # once, and after every reference bump
devenv tasks run golden:regen     # rewrite fixtures/ from corpus/
devenv tasks run check:fixtures   # regen, then fail if fixtures/ changed (CI does this)

cd tools/golden
npm run regen:poe1                # one game only
npm run seed:poe2                 # add EE2's specs/Parser/items.ts texts to corpus/poe2
```

A case is `corpus/<game>/<slug>.txt`, plus an optional `<slug>.json` overriding the
defaults in `src/<game>.test.ts`: `{ "lang": "ru", "options": { ... }, "have": [...],
"config": { ... } }`. Fixtures for deleted cases are pruned on regen.

## Fixture layout

```
fixtures/<poe1|poe2>/<case>/
  input.txt        the clipboard text
  options.json     the pinned inputs (see below)
  parsed.json      parseClipboard(text)
  presets.json     createPresets(parsed, options) -> { presets, active }
  trade/<id>.json  { api, request } for each preset id
  map-check.json   the map check's prepared stats (map-like items only)
  item-check.json  what the item check's hotkey actions open or send
```

If `parseClipboard` fails, `parsed.json` is `{ "error": "<key>" }` and there are no
presets or requests.

JSON is `JSON.stringify` with recursively sorted keys, two-space indent and a
trailing newline; numbers are JavaScript's shortest round-trip form. Rust reproduces
it with `wraex_golden::canonical`, and a test round-trips every fixture through it,
so the two compare as text. Parse fixture JSON with `serde_json`'s
`float_roundtrip` (on in the workspace): the default parser can land on a
neighbouring double. `case` is a stable slug (`unique-headhunter-corrupted`).

## Pipeline

The reference code runs these stages:

1. `init(lang)` (`renderer/src/assets/data/index.ts`) after the index files exist.
2. `parseClipboard(text)` (`renderer/src/parser/Parser.ts`).
3. `createPresets(item, opts)` (`renderer/src/web/price-check/filters/create-presets.ts`)
   returns `{ presets, active }`, and each preset carries both `filters` and the
   `stats` list. `createTradeRequest(filters, stats)` needs that pair; calling
   `createFilters` alone is not enough.
4. Per preset, `apiToSatisfySearch(item, stats, filters)` (`.../trade/common.ts`)
   picks the API, as `CheckedItem.vue` does, and is recorded as `api`:
   - `trade`: `createTradeRequest(filters, stats)` in PoE1,
     `createTradeRequest(filters, stats, item)` in PoE2 (`.../trade/pathofexile-trade.ts`);
   - `bulk`: `createTradeRequest(filters, item, have)` (`.../trade/pathofexile-bulk.ts`).
     PoE2 first sets `filters.trade.listingType = "online"`, as `CheckedItem.vue` does.
     `have` is the first (optimistic) exchange search the app sends, unless the case
     sets it: PoE1 `TradeBulk.vue` (`['divine', 'chaos']`, minus the item itself),
     PoE2 `bulk-api.ts` (`['divine', 'exalted', 'chaos']`, minus the item itself).

5. The item check, on the parsed item before the presets see it:
   - `map-check.json`, only where `WidgetItemCheck.vue` `isMapLike` holds:
     PoE1 `MapCheck.vue` `mapStats` (the explicit stats through
     `prepareMapStat`), PoE2 `prepareMapStats(item)`. `isMapLike` and PoE1's
     filter live in `.vue` files, so the harness repeats them.
   - `item-check.json`: `openWiki`, `openPoedb`, `openCoE`, `findSimilarItems`
     and (PoE2) `findSamePricedItems` from `web/item-check/hotkeyable-actions.ts`.
     They return nothing; the harness records the URL each passes to
     `window.open` (defined only while one runs) or the event it sends through
     `Host.sendEvent`. PoE2's config gets `WidgetItemCheck.vue`'s default
     item-check widget (`samePricedType: "both"`).

Record the `active` preset and all presets, not just the active one.

## Settings

The `poe1-config` and `poe2-config` projects load the real `web/Config.ts`
(the item pipeline mocks it), with each reference's Vue plugin for the widget
`.vue` files it imports and `happy-dom` as their browser. The widget registry
is imported first, as the app's `App.vue` does (EE2's widgets read Config's
exports while loading). The IPC host serves the input file and records what is
sent back. Per game they write `fixtures/config/<game>/`:

```
default/config.json   defaultConfig()
default/loaded.json   initConfig() without a file
default/host.json     getConfigForHost() of that
<slug>/input.txt      corpus/config/<game>/<slug>.json, as read
<slug>/loaded.json    initConfig() of it (upgradeConfig, or the defaults)
<slug>/host.json      getConfigForHost() of it
<slug>/saves.json     save events, when initConfig saves (unreadable files)
```

The corpus files are derived from the recorded defaults by
`scripts/make-config-corpus.py` (older versions and field names set by hand);
real config files from players should replace them. `presets.json`
is written before any request is built, and the requests get the live preset objects
like the app does (EE2 wraps some trade ids in proxies, which cannot be cloned).

## Options are part of the fixture

The two apps take different option objects, and results depend on them. Write the
exact object used into `options.json`.

- PoE1 (`ref/apt`) `CreateOptions` (`filters/create-item-filters.ts`): `league`,
  `merchantOnly`, `currency`, `collapseListings`, `activateStockFilter`, `exact`,
  `useEn`; `createPresets` additionally takes `searchStatRange`.
- PoE2 (`ref/ee2`): `league`, `currency`, `listingType`, `collapseListings`,
  `activateStockFilter`, `searchStatRange`, `useEn`, `defaultAllSelected`.

Defaults are the apps' own (`PriceCheckWindow.vue` widget defaults, `Config.ts`), with
league `Standard`. `options.json` also records the `AppConfig()` the case ran with
(`config`): EE2 reads `language`, the price-check widget's `searchStatRange`,
`savedAugments` and more from it during parsing and filter creation.

## Running the TypeScript

Plain `node` cannot load the sources: they use `@/` path aliases,
`import.meta.env.BASE_URL`, Vue, `@/web/Config` (`AppConfig`) and `@/web/background/IPC`.
The harness is a vitest project with one project per game (`vitest.config.ts`). Each
maps the reference's aliases (`@/`, `@ipc/`, and `/data/` for the dynamic
`client_strings.js` import) onto its checkout, so the sources run unmodified and
resolve their dependencies from the reference's own `node_modules`. `src/setup.ts`
holds the mocks, modelled on `ref/ee2/renderer/specs/vitest.setup.ts`: `fetch` and
the IPC host's `proxy` read from disk (EE2's `/api/trade2/data/*` from its
`specs/data/` snapshots), and `@/web/Config` is replaced whole (the real module
imports every widget's `.vue` file). Any other network request throws.

The data indexes are generated, not committed, and must be built first:

- PoE2: `cd ref/ee2/renderer && npm ci && npm run make-index-files`
- PoE1: `cd ref/apt/renderer && npm ci && node src/assets/make-index-files.mjs` (no
  npm script, but the module runs itself when executed directly)

`npm run prepare-refs` (`golden:prepare`) does both.

`*.index.bin` and `node_modules` are gitignored in both references, so generating
them does not dirty the submodules. Use `npm ci`, never `npm install`: the latter
rewrites `package-lock.json` inside a submodule.

Run the harness offline, with no network and no calls to the GGG trade API.

## Corpus

`corpus/README.md` records where each group of cases came from. Target: copied
in-game item text (Ctrl+Alt+C): normal, magic, rare, unique, unidentified;
corrupted, mirrored, influenced; gems, maps, currency, divination cards, cluster
jewels, heist and sanctum items, logbooks; every language upstream supports
(en, ru, ko, cmn-Hant). Seed PoE2 with the item texts in
`ref/ee2/renderer/specs/Parser/items.ts` and port its assertions from
`renderer/specs/**/*.test.ts`. Own play, GitHub issues and community dumps fill the
rest.

## Screenshots (phase 5)

`screens/` renders the *original* price-check window in headless Chromium and writes
a PNG plus the element's `innerText`, so the Rune port can be compared with upstream
before Rune's own widget exists. It is not a fixture: nothing it produces is
committed or compared automatically.

```sh
devenv tasks run golden:screens                    # both games, default cases
cd tools/golden
npm run screens -- --game poe1                     # one game, default cases
npm run screens -- --game poe2 --case ee2-rare-map # one case
npm run screens -- --rebuild                       # after bumping a reference
```

It runs the reference's own `vite build` once per game, with cwd
`ref/<game>/renderer` and the output in `tools/golden/.screens-build/<game>/`
(gitignored; the build is skipped while `index.html` exists). That needs
`golden:prepare` once: its `npm ci` installs `vite`, and it builds the data index
files. PoE1's vite plugin regenerates those itself on `buildStart`.

Per case it starts a fake main process (`screens/host.mjs`), opens the built page in
Playwright (`playwright` 1.65.0, Chromium revision 1254) at 1600x900 / DPR 1, freezes
the clock at 2026-01-01T12:00Z, waits for the `/events` websocket, sends

```json
{ "name": "MAIN->CLIENT::item-text",
  "payload": { "target": "price-check", "clipboard": "<corpus text>",
               "position": { "x": 800, "y": 450 }, "focusOverlay": false } }
```

and screenshots `#price-window` to `target/screens/<game>/<case>.png` (repo root,
gitignored), with `target/screens/<game>/<case>.txt` next to it. The default cases
are in `screens/cases.mjs`; item text comes from `corpus/<game>/<slug>.txt`.

The host serves `/config` as `HostState`
(`{ version, updater, contents: JSON.stringify(config) }`). `config` is
`fixtures/config/<game>/default/config.json` with one change: `leagueId` is set to
`Standard`, because the recorded default omits it and the leagues fixture only
offers `Standard`/`Hardcore`. Everything else stays as recorded.

### Proxy fixtures

Trade and poe.ninja requests must go through `/proxy/<host>/<path>`, so the host
answers them from `corpus/screens/<game>/proxy/`. A file is named after the URL
after `/proxy/`, with `/` -> `__`, `?` -> `--`, `&` -> `-`, `=` -> `_` and any
other character outside `[A-Za-z0-9._-]` -> `_`:

```
www.pathofexile.com/api/leagues?type=main&realm=pc
  -> www.pathofexile.com__api__leagues--type_main-realm_pc.json
www.pathofexile.com/api/trade2/data/leagues
  -> www.pathofexile.com__api__trade2__data__leagues.json
```

Lookup order is `<name>.<METHOD>.json`, then `<name>.json`, so a POST and a GET to
the same URL can be answered differently. Missing URLs are logged to stderr and
answered `404 {}`, so the page never hangs. Only the leagues answers are committed
today (PoE1 `[{ id, realm, event, rules }]`, PoE2 `{ result: [{ id, text }] }`).
Because the trade endpoints are unanswered, the results area shows "Trade site
request failed" (and PoE2 marks stats it would need `/api/trade2/data/stats` for as
unrecognized). That is expected: the item header and the filters above it are what
these screenshots are for. Add proxy fixture files, not code, to answer more.

Screenshots are for visual comparison only: fonts and rendering differ by machine
and browser, so do not pixel-diff them (the `.txt` dump helps with text). They are
not committed.

## Rules

- Fixtures are committed. Any diff after bumping a reference is the to-do list.
- Never hand-edit a fixture to match Rust output.
- A reference test that fails is only recorded as a known failure after the
  reference repo's own CI steps reproduce it (index files built, `npm ci`,
  Node 24). Otherwise the harness is wrong, not the app.
