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
npm run screens -- --game poe2 --record            # fetch + save new answers (network)
npm run screens -- --game poe1 --image <url>       # record one item image on its own
```

It runs the reference's own `vite build` once per game, with cwd
`ref/<game>/renderer` and the output in `tools/golden/.screens-build/<game>/`
(gitignored; the build is skipped while `index.html` exists). That needs
`golden:prepare` once: its `npm ci` installs `vite`, and it builds the data index
files. PoE1's vite plugin regenerates those itself on `buildStart`.

Per case it starts a fake main process (`screens/host.mjs`), opens the built page in
Playwright (`playwright` 1.65.0, Chromium revision 1254) at 1600x900 / DPR 1, freezes
the clock, waits for the `/events` websocket, sends

```json
{ "name": "MAIN->CLIENT::item-text",
  "payload": { "target": "price-check", "clipboard": "<corpus text>",
               "position": { "x": 800, "y": 450 }, "focusOverlay": false } }
```

and screenshots `#price-window` to `target/screens/<game>/<case>.png` (repo root,
gitignored), with `target/screens/<game>/<case>.txt` next to it. The default cases
are in `screens/cases.mjs`; item text comes from `corpus/<game>/<slug>.txt`.

A case is a slug or `{ slug, search: true }`. Upstream searches by itself only for
uniques and bulk currency (see `CheckedItem.vue`); for a rare or a map the window
renders a "Search" button instead, so a case that should show listings needs
`search: true` and the harness clicks that button. A case that needs it but lacks it
prints `no listings in the window` and its screenshot shows the pre-search window.

The page clock is frozen so listing ages and poe.ninja windows are stable. The date
comes from `corpus/screens/<game>/recorded-at.txt` (ISO 8601, written by `--record`
when the file is missing) and falls back to `2026-01-01T12:00:00Z`. Re-recording
keeps the existing file: deleting it re-freezes the clock at the next recording.

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

Two suffixes keep the name unambiguous:

- a request with a body (the search and exchange endpoints answer by body) appends
  `--body<8 hex of sha1(body)>`, so two searches of the same URL are two files. A
  POST never falls back to a body-less name.
- a name longer than 200 characters (a trade fetch lists the ids it wants) is cut to
  160 and gets `--len<length>-<16 hex of sha1(name)>`: one path component cannot
  exceed 255 bytes. Both are pure functions of the URL and body, so recording and
  replay agree.

Lookup order is `<name>.<METHOD>.json`, then `<name>.json`, so a POST and a GET to
the same URL can be answered differently. Missing URLs are logged to stderr and
answered `404 {}` (or the answer fails and the page shows "Trade site request
failed"), so the page never hangs. Add fixture files, not code, to answer more.

The committed answers are recorded from the live sites (`--record`): leagues are
hand-written, everything else is a real GGG / poe.ninja answer from 2026-11-24. The
two big ones are `poe.ninja/poe1/.../dense/overviews` (3.4 MB, every item's price
dense blob) and `/api/trade2/data/stats` (855 KB, every modifier); the rest is a few
KB each. They are kept because the results and price areas need them; re-recording
is manual, and stale answers are fine (the screenshots are a label, not a market
feed).

### Item images

An item's picture is not proxied: the page's `<img>` loads it straight from the game's
CDN (`https://web.poecdn.com/gen/image/...`), which comes from the game data's `icon`
fields and from trade listings. `screens/images.mjs` routes those requests with
`page.route`, so a run stays offline:

```
web.poecdn.com/gen/image/WzI1LDE0.../40b81baee3/LakeRing1.png
  -> corpus/screens/<game>/images/1f462dd1-LakeRing1.png
```

The name is the first eight hex digits of the URL's SHA-1, then the URL's own file name
(anything outside `[A-Za-z0-9._-]` becomes `_`). Both are pure functions of the URL, and
`rust/crates/app/src/ui/art.rs` (`recorded_name`) rebuilds the same name, so the Rune
replay mode reads what the harness recorded. With `--record` a miss is fetched once, at
most one request per second, with Wraex's User-Agent (the proxy recordings' rules), and
saved as the exact bytes; without it a miss is a `404` and a line on stderr. `--image
<url>` records one URL without rendering a case: it is for art only a state the harness
does not click reaches, such as the anointment oils of a filter row behind the Hidden
toggle (all sixteen oils are recorded that way). The small art the page serves itself
(`/images/chaos.png`, `stash.png`, `dust.png`, the influence badges) is the reference's
own file and needs no fixture: the port reads it from
`ref/<game>/renderer/public/images`.

A context pins `locale: 'en-US'` and `timezoneId: 'UTC'`: the runner's own locale would
otherwise reach the page, which formats `1.2` as `1,2` under a da-DK format and dates in
its own time zone, and the port's output is en-US.

### Recording answers (`--record`)

With `--record`, a proxy miss is forwarded to the real host over HTTPS and the answer
is saved next to the fixtures, so later runs replay it offline:

- only hosts upstream's `/proxy/` allows (`PROXY_HOSTS` in `ref/apt/main/src/proxy.ts`
  and `ref/ee2/main/src/proxy.ts`, mirrored in `screens/host.mjs`); anything else is
  refused, as upstream destroys such a request;
- the request's method, body and headers, minus the browser-only `sec-*`, `host`,
  `origin`, `content-length`, `connection`, `transfer-encoding` and `referer`, with
  `User-Agent: wraex/<workspace.package.version> (+https://github.com/Dumspy/wraex)`
  (the format of `wraex_core::trade::api::USER_AGENT`) and
  `accept-encoding: identity`, because the saved body is the decoded text (as
  upstream's Electron client sees it) and a replayed answer must be byte-identical;
- at most one forwarded request per second, whatever the page asks for, and the run
  stops with an error on a `429`;
- the body is stored as the exact bytes received in `<name>.<METHOD>.json`. A
  sibling `<name>.<METHOD>.meta.json` holds `{ "status": 200, "headers": { ... } }`
  with `content-type`, `retry-after` and the `x-rate-limit-*` headers, which are
  also handed back to the page on every replay. Without a meta file a fixture
  replays as `200` with the default content type, which is how the hand-written
  leagues files work. A `4xx`/`5xx` answer is not saved;
- the leagues answers are never recorded (a miss for one stays a miss), and an
  existing file is never overwritten, so deleting one is how you re-record just it.

Screenshots are for visual comparison only: fonts and rendering differ by machine
and browser, so do not pixel-diff them (the `.txt` dump helps with text). They are
not committed.

## Rules

- Fixtures are committed. Any diff after bumping a reference is the to-do list.
- Never hand-edit a fixture to match Rust output.
- A reference test that fails is only recorded as a known failure after the
  reference repo's own CI steps reproduce it (index files built, `npm ci`,
  Node 24). Otherwise the harness is wrong, not the app.
