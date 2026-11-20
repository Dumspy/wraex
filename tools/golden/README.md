# Golden fixture harness

Phase 1 (see [PLAN.md](../../PLAN.md)). Runs the pinned TypeScript reference apps and
records what they produce, so the Rust port has something exact to match.

Not implemented yet. This file records the shape the harness must have so phase 1
can be built against it.

## What it does

`regen.mjs` loads one game's reference checkout, drives it over a corpus of item
texts, and writes fixtures:

```
fixtures/<game>/<case>/input.txt     the clipboard text
fixtures/<game>/<case>/parsed.json   parseClipboard(text)
fixtures/<game>/<case>/query.json    createTradeRequest(createFilters(parsed), ...)
```

`<game>` is `poe1` or `poe2`; `<case>` is a stable slug. JSON is normalised (key
order, float formatting) so TS and Rust output compare as text.

## Reference entry points

PoE1 (`ref/apt`):

- `renderer/src/parser/Parser.ts` → `parseClipboard` (line 95)
- `renderer/src/web/price-check/filters/create-item-filters.ts` → `createFilters`
- `renderer/src/web/price-check/filters/create-presets.ts` → `createPresets`
- `renderer/src/web/price-check/trade/pathofexile-trade.ts` → `createTradeRequest`
- `renderer/src/web/price-check/trade/pathofexile-bulk.ts` → `createTradeRequest`
  (bulk/currency items)
- data: `renderer/src/assets/data/index.ts` (`fetch` must be replaced with `fs`)

PoE2 (`ref/ee2`): same layout. Seed the corpus from the raw item texts in
`renderer/specs/Parser/items.ts` and port its assertions from
`renderer/specs/**/*.test.ts`.

## Corpus

Copied in-game item text (Ctrl+Alt+C): normal, magic, rare, unique, unidentified;
corrupted, mirrored, influenced; gems, maps, currency, divination cards, cluster
jewels, heist and sanctum items, logbooks; every language upstream supports
(en, ru, ko, cmn-Hant). Own play, GitHub issues and community dumps are the sources.

## Rules

- Fixtures are committed. A diff means upstream changed behaviour; that diff is the
  to-do list.
- Never hand-edit a fixture to match Rust output.
- EE2 has 26 known failing tests at v0.16.3; those cases are recorded as known
  failures, not as expected behaviour.
