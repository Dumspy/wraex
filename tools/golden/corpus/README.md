# Corpus

Clipboard texts the fixtures are generated from, one `<slug>.txt` per case. Keep
this list current when adding cases, so it is clear which ones are real copies.

## item-search

`item-search/<game>.json`: the queries the item search widget is asked (see
[the harness README](../README.md#item-search)). Hand-picked from the English
data to cover the matcher's rules: a found name, more than `MAX_RESULTS`
matches, no match, the three-character rule, the longest-word prefix rule, case
and trimming (including the U+FEFF/U+0085 differences between JavaScript's and
Rust's whitespace), apostrophes, diacritics, every tab, and the `cmn-Hant`
exemption (as the config language, on the English data the milestone uses). The
`fuzzy` lines are OCR-like misspellings of gem names, plus one line too long for
any gem.

## poe2

- `ee2-*`: the item texts of `ref/ee2/renderer/specs/Parser/items.ts` at v0.16.3,
  added with `npm run seed:poe2`. Real in-game copies (EE2's own test data).

- `error-*`: texts the parser rejects (`item.parse_error`, `item.unknown`, and
  `item.wrong_language` for a Russian copy). Hand-written.
- `unique-mageblood` (its own preset, after the example in EE2's
  `create-unique-filters.ts`) and `currency-djinn-barya` (trial filters):
  hand-written for filter branches EE2's items miss. Not real copies.
- `note-rare-bow`: `ee2-rare-item` with a price note, for the item check's
  "same priced" stash search.

## poe1

- `real-*`: real in-game copies (advanced format, Ctrl+Alt+C), from the
  maintainer's own play: fractured and eldritch-implicit rares, a Foulborn unique
  a six-socket unique, and three maps (a rare T16, a foil Valdo map with its
  reward, a corrupted blighted map). From issue #11, also real copies: a unique
  and an enchanted magic flask, a corrupted magic jewel, a Foulborn unique jewel,
  a large cluster jewel, a rare fishing rod, a Foulborn unique body armour, rare
  maps (T16 with a Drox citadel, T16 Elder/Originator, T10 delirium mirror),
  two corrupted unique maps (Cortex, Altered Distant Memory), currency
  (Runegraft, Reflecting Mist, Allflame Ember, Essence of Delirium), a
  divination card, two fragments, an invitation, a skill gem and two support
  gems (one exceptional). Kept byte-exact, trailing spaces included. The fishing
  rod's lure and the unique maps' "Monster Level" mod are unknown to APT's stat
  data, which the fixtures record as upstream does.
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
