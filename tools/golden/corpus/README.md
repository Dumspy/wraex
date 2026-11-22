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
- `adv-*`: hand-written in the advanced format, aimed at parser branches the
  real copies miss: magic names, legacy rolls (volatile, reflecting), negated
  rolls, catalyst quality, veiled and synthesised mods, enchant and scourge
  lines, stat groups, weapons, "cannot use" items, split and twice corrupted.
  Not real copies; the stat text is real, the combinations may not be.
- `error-*`: texts the parser rejects (`item.parse_error`, `item.unknown`).
- Everything else: hand-written in the in-game clipboard format (Ctrl+C, not the
  advanced Ctrl+Alt+C), checked only in that the reference parses each one with no
  unknown modifiers. They exercise the pipeline per item class (currency, cards,
  gems, maps, flasks, cluster jewels, heist, uniques, rares, influence/mirrored,
  unidentified) but are not real copies. Replace them with real ones from play,
  upstream issues or community dumps as those arrive, and add the advanced-format
  and non-English (ru, ko, cmn-Hant) cases the parser also handles.
