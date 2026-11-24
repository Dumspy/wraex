// Which corpus cases the screenshot tool runs by default, per game.
// A case is `corpus/<game>/<slug>.txt`, the same file the fixture harness uses.
//
// A case is a slug, or `{ slug, search: true }`. `search` clicks the window's
// "Search" button, which upstream only renders when it did not search on its
// own (a rare, a map); uniques and bulk currency search as soon as the item
// arrives. Set it wherever the file's `.txt` dump ends with `Search`, so a
// recording includes the trade request.
import path from 'node:path'

export const REPO_ROOT = path.resolve(import.meta.dirname, '../../..')

// Chosen to cover the window's main shapes: a stackable currency item, a rare
// with advanced (catalyst/mirrored) mods, a unique with variants, and a map
// with map-check stats. PoE2: currency, rare, unique, waystone.
export const DEFAULT_CASES = {
  poe1: [
    // bulk currency: searches on its own
    'currency-divine-orb',
    // rare, unique, map: the Search button starts the trade request
    { slug: 'adv-rare-ring-mirrored-catalyst', search: true },
    'adv-unique-timeless-jewel',
    { slug: 'real-map-rare-t16-max-res', search: true }
  ],
  poe2: [
    // bulk currency: searches on its own
    'currency-djinn-barya',
    // rare, unique, waystone: the Search button starts the trade request
    { slug: 'ee2-rare-item', search: true },
    'ee2-unique-item',
    { slug: 'ee2-rare-map', search: true }
  ]
}

/** A case entry as `{ slug, search }`, from a corpus slug or a cases.mjs row. */
export function normalizeCase (entry) {
  if (typeof entry === 'string') return { slug: entry, search: false }
  return { slug: entry.slug, search: Boolean(entry.search) }
}

export function corpusFile (game, slug) {
  return path.join(REPO_ROOT, 'tools/golden/corpus', game, `${slug}.txt`)
}
