// Which corpus cases the screenshot tool runs by default, per game.
// A case is `corpus/<game>/<slug>.txt`, the same file the fixture harness uses.
import path from 'node:path'

export const REPO_ROOT = path.resolve(import.meta.dirname, '../../..')

// Chosen to cover the window's main shapes: a stackable currency item, a rare
// with advanced (catalyst/mirrored) mods, a unique with variants, and a map
// with map-check stats. PoE2: currency, rare, unique, waystone.
export const DEFAULT_CASES = {
  poe1: [
    'currency-divine-orb',
    'adv-rare-ring-mirrored-catalyst',
    'adv-unique-timeless-jewel',
    'real-map-rare-t16-max-res'
  ],
  poe2: [
    'currency-djinn-barya',
    'ee2-rare-item',
    'ee2-unique-item',
    'ee2-rare-map'
  ]
}

export function corpusFile (game, slug) {
  return path.join(REPO_ROOT, 'tools/golden/corpus', game, `${slug}.txt`)
}
