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

The reference code has three stages, not two:

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

Record the `active` preset and all presets, not just the active one. `presets.json`
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

## Rules

- Fixtures are committed. Any diff after bumping a reference is the to-do list.
- Never hand-edit a fixture to match Rust output.
- A reference test that fails is only recorded as a known failure after the
  reference repo's own CI steps reproduce it (index files built, `npm ci`,
  Node 24). Otherwise the harness is wrong, not the app.
