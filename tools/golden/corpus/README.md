# Corpus

Clipboard texts the fixtures are generated from, one `<slug>.txt` per case. Keep
this list current when adding cases, so it is clear which ones are real copies.

## poe2

- `ee2-*`: the item texts of `ref/ee2/renderer/specs/Parser/items.ts` at v0.16.3,
  added with `npm run seed:poe2`. Real in-game copies (EE2's own test data).

- `error-*`: texts the parser rejects (`item.parse_error`, `item.unknown`, and
  `item.wrong_language` for a Russian copy). Hand-written.
- `unique-mageblood` (its own preset, after the example in EE2's
  `create-unique-filters.ts`) and `currency-djinn-barya` (trial filters):
  hand-written for filter branches EE2's items miss. Not real copies.

## poe1

- `real-*`: real in-game copies (advanced format, Ctrl+Alt+C), from the
  maintainer's own play: fractured and eldritch-implicit rares, a Foulborn unique
  a six-socket unique, and three maps (a rare T16, a foil Valdo map with its
  reward, a corrupted blighted map).
- `adv-*`: hand-written in the advanced format, aimed at parser branches the
  real copies miss: magic names, legacy rolls (volatile, reflecting), negated
  rolls, catalyst quality, veiled and synthesised mods, enchant and scourge
  lines, stat groups, weapons, "cannot use" items, split and twice corrupted.
  Not real copies; the stat text is real, the combinations may not be.
- `error-*`: texts the parser rejects (`item.parse_error`, `item.unknown`).
- Filter branches (`createPresets`), hand-written like `adv-*`:
  `adv-rare-amulet-anointed` (anointment oils), `adv-magic-flask-charge-recovery`
  (the flask "not increased effect" filter), `adv-rare-body-armour-attributes`
  (armour properties, attribute and life pseudo stats), `adv-unique-timeless-jewel`
  (keystone group), `chronicle-of-atzoatl` (open, obstructed and explosives rooms),
  `mirrored-tablet`, `expedition-logbook` (one preset per area), `heist-blueprint`,
  `mercenary-warrant` (skill groups, 6-link, `not` group) and `gem-transfigured`.
  Their layout follows the parser's section rules; they are not real copies.
- Everything else: hand-written in the in-game clipboard format (Ctrl+C, not the
  advanced Ctrl+Alt+C), checked only in that the reference parses each one with no
  unknown modifiers. They exercise the pipeline per item class (currency, cards,
  gems, maps, flasks, cluster jewels, heist, uniques, rares, influence/mirrored,
  unidentified) but are not real copies. Replace them with real ones from play,
  upstream issues or community dumps as those arrive, and add the advanced-format
  and non-English (ru, ko, cmn-Hant) cases the parser also handles.
