# Corpus

Clipboard texts the fixtures are generated from, one `<slug>.txt` per case. Keep
this list current when adding cases, so it is clear which ones are real copies.

## poe2

- `ee2-*`: the item texts of `ref/ee2/renderer/specs/Parser/items.ts` at v0.16.3,
  added with `npm run seed:poe2`. Real in-game copies (EE2's own test data).

## poe1

- `real-*`: real in-game copies (advanced format, Ctrl+Alt+C), from the
  maintainer's own play: fractured and eldritch-implicit rares, a Foulborn unique
  and a six-socket unique.
- Everything else: hand-written in the in-game clipboard format (Ctrl+C, not the
  advanced Ctrl+Alt+C), checked only in that the reference parses each one with no
  unknown modifiers. They exercise the pipeline per item class (currency, cards,
  gems, maps, flasks, cluster jewels, heist, uniques, rares, influence/mirrored,
  unidentified) but are not real copies. Replace them with real ones from play,
  upstream issues or community dumps as those arrive, and add the advanced-format
  and non-English (ru, ko, cmn-Hant) cases the parser also handles.
