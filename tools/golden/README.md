# Golden fixture harness

Phase 1 (see [PLAN.md](../../PLAN.md)). Runs the pinned TypeScript reference apps and
records what they produce, so the Rust port has something exact to match.

Not implemented yet. This file is the spec the harness must satisfy; the layout and
entry points below are the single source of truth (README and PLAN point here).

## Fixture layout

```
fixtures/<poe1|poe2>/<case>/
  input.txt        the clipboard text
  options.json     the pinned inputs (see below)
  parsed.json      parseClipboard(text)
  presets.json     createPresets(parsed, options) -> { presets, active }
  trade/<id>.json  createTradeRequest for each preset id
```

JSON is normalised (sorted keys, fixed float formatting) so TypeScript and Rust
output compare as text. `case` is a stable slug (`unique-headhunter-corrupted`).

## Pipeline

The reference code has three stages, not two:

1. `init(lang)` (`renderer/src/assets/data/index.ts`) after the index files exist.
2. `parseClipboard(text)` (`renderer/src/parser/Parser.ts`).
3. `createPresets(item, opts)` (`renderer/src/web/price-check/filters/create-presets.ts`)
   returns `{ presets, active }`, and each preset carries both `filters` and the
   `stats` list. `createTradeRequest(filters, stats)` needs that pair; calling
   `createFilters` alone is not enough.
4. Per preset: `createTradeRequest(preset.filters, preset.stats)`
   (`.../trade/pathofexile-trade.ts`). Bulk/currency items use the other overload,
   `createTradeRequest(filters, item, have)` (`.../trade/pathofexile-bulk.ts`),
   where `have` comes from `options.json`.

Record the `active` preset and all presets, not just the active one.

## Options are part of the fixture

The two apps take different option objects, and results depend on them. Write the
exact object used into `options.json`.

- PoE1 (`ref/apt`) `CreateOptions` (`filters/create-item-filters.ts`): `league`,
  `merchantOnly`, `currency`, `collapseListings`, `activateStockFilter`, `exact`,
  `useEn`; `createPresets` additionally takes `searchStatRange`.
- PoE2 (`ref/ee2`): `league`, `currency`, `listingType`, `collapseListings`,
  `activateStockFilter`, `searchStatRange`, `useEn`, `defaultAllSelected`.

Pick one league string (e.g. `Standard`) and keep currency/`have` fixed per case.

## Running the TypeScript

Plain `node` cannot load the sources: they use `@/` path aliases,
`import.meta.env.BASE_URL`, Vue, `@/web/Config` (`AppConfig`) and `@/web/background/IPC`.
Run the harness under **vitest or vite-node** with the reference repo's own config,
mocking what the tests mock. `ref/ee2/renderer/specs/vitest.setup.ts` is the model:
`loadClientStrings`, `fetch` reading from disk, `AppConfig`, and the IPC host.

The data indexes are generated, not committed, and must be built first:

- PoE2: `cd ref/ee2/renderer && npm ci && npm run make-index-files`
- PoE1: no npm script; `renderer/src/assets/make-index-files.mjs` is driven by
  `renderer/src/assets/vite-plugin-make-indexes.ts`, so run it through that plugin
  (or call the module the same way the plugin does).

`*.index.bin` and `node_modules` are gitignored in both references, so generating
them does not dirty the submodules. Use `npm ci`, never `npm install`: the latter
rewrites `package-lock.json` inside a submodule.

Run the harness offline, with no network and no calls to the GGG trade API.

## Corpus

Copied in-game item text (Ctrl+Alt+C): normal, magic, rare, unique, unidentified;
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
