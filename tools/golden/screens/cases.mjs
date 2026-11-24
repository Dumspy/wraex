// Which corpus cases the screenshot tool runs by default, per game.
// A case is `corpus/<game>/<slug>.txt`, the same file the fixture harness uses.
//
// A case is a slug, or `{ slug, search: true, widget: 'map-check' }`:
//
// - `search` clicks the price check's "Search" button, which upstream only renders
//   when it did not search on its own (a rare, a map); uniques and bulk currency
//   search as soon as the item arrives. Set it wherever the file's `.txt` dump ends
//   with `Search`, so a recording includes the trade request.
// - `widget` picks which overlay widget the item text opens: `price-check` (the
//   default, `MAIN->CLIENT::item-text` with `target: 'price-check'`), or
//   `map-check` / `item-check`, which both open `item-check`'s widget — it shows
//   the map check for a map-like item and the item's info otherwise. Their output
//   is `target/screens/<game>/<widget>-<slug>.png`, next to the price check's
//   `<slug>.png`.
import path from 'node:path'

export const REPO_ROOT = path.resolve(import.meta.dirname, '../../..')

// Chosen to cover the windows' main shapes. Price check: a stackable currency
// item, a rare with advanced (catalyst/mirrored) mods, a unique with variants,
// and a map with map-check stats. Map check: every PoE1 map case (rows, no rows,
// the maven reminder, a map screenshot) and PoE2's two waystones. Item check: a
// rare, a unique, a gem and a weapon (the DPS grid). PoE2 price check: currency,
// rare, unique, waystone.
export const DEFAULT_CASES = {
  poe1: [
    // bulk currency: searches on its own
    'currency-divine-orb',
    // rare, unique, map: the Search button starts the trade request
    { slug: 'adv-rare-ring-mirrored-catalyst', search: true },
    'adv-unique-timeless-jewel',
    { slug: 'real-map-rare-t16-max-res', search: true },
    // map check: `real-map-*`, `map-*`
    { slug: 'real-map-rare-t16-max-res', widget: 'map-check' },
    { slug: 'real-map-blighted-corrupted', widget: 'map-check' },
    { slug: 'real-map-valdo-foil', widget: 'map-check' },
    { slug: 'map-normal-t16', widget: 'map-check' },
    { slug: 'map-rare-t16-corrupted', widget: 'map-check' },
    // item check: a rare (mirrored, so no CoE), a unique, a gem, a weapon (DPS
    // grid and CoE)
    { slug: 'adv-rare-ring-mirrored-catalyst', widget: 'item-check' },
    { slug: 'adv-unique-timeless-jewel', widget: 'item-check' },
    { slug: 'gem-support-empower', widget: 'item-check' },
    { slug: 'adv-rare-sword-cannot-use', widget: 'item-check' }
  ],
  poe2: [
    // bulk currency: searches on its own
    'currency-djinn-barya',
    // rare, unique, waystone: the Search button starts the trade request
    { slug: 'ee2-rare-item', search: true },
    'ee2-unique-item',
    { slug: 'ee2-rare-map', search: true },
    // map check: the waystones
    { slug: 'ee2-rare-map', widget: 'map-check' },
    { slug: 'ee2-rare-map-fake-all-props', widget: 'map-check' },
    // item check: a rare with a price note (the DPS grid and the price button), a
    // unique, an uncut gem
    { slug: 'note-rare-bow', widget: 'item-check' },
    { slug: 'ee2-unique-item', widget: 'item-check' },
    { slug: 'ee2-uncut-skill-gem', widget: 'item-check' }
  ]
}

/** A case entry as `{ slug, search, widget }`, from a corpus slug or a cases.mjs row. */
export function normalizeCase (entry) {
  if (typeof entry === 'string') return { slug: entry, search: false, widget: 'price-check' }
  return {
    slug: entry.slug,
    search: Boolean(entry.search),
    widget: entry.widget ?? 'price-check'
  }
}

/** The element id the item check's widget is drawn with: `widget-<wmId>`. */
export function widgetId (config) {
  const widget = config.widgets.find(w => w.wmType === 'item-check')
  if (!widget) throw new Error('the config has no item-check widget')
  return widget.wmId
}

/** `target/screens/<game>/<name>`, without the extension. */
export function screenName (game, testCase) {
  const { slug, widget } = testCase
  return path.join(REPO_ROOT, 'target/screens', game, widget === 'price-check' ? slug : `${widget}-${slug}`)
}

export function corpusFile (game, slug) {
  return path.join(REPO_ROOT, 'tools/golden/corpus', game, `${slug}.txt`)
}

// The settings window's pages, per game: the slug (`settings-<page>`, the same suffix as
// the Rune scenes in `rust/crates/app/src/lib.rs`) and the menu button that opens it
// (`t(component.name)` in `SettingsWindow.vue`). EE2's Help and Leveling pages are left
// out: the port has no scene for them yet (`rust/crates/app/src/settings/mod.rs`).
export const SETTINGS_LABELS = {
  poe1: {
    hotkeys: 'Hotkeys',
    chat: 'Chat',
    general: 'General',
    'price-check': 'Price check',
    maps: 'Map check',
    'item-check': 'Item info',
    debug: 'Debug',
    about: 'About'
  },
  poe2: {
    hotkeys: 'Hotkeys',
    chat: 'Chat',
    general: 'General',
    'price-check': 'Price check',
    maps: 'Map check',
    'item-check': 'Item info',
    debug: 'Debug',
    about: 'About'
  }
}

// The settings window's pages, rendered after the item cases. A settings case is
// `{ settings: '<page>' }`; it needs no corpus file, because the window opens on the
// default config (`fixtures/config/<game>/default/config.json`, served by the host).
// The output is `target/screens/<game>/settings-<page>.png`, the name the Rune scene
// `settings-<game>-<page>` renders.
export const DEFAULT_SETTINGS_CASES = {
  poe1: [
    { settings: 'hotkeys' },
    { settings: 'chat' },
    { settings: 'general' },
    { settings: 'price-check' },
    { settings: 'maps' },
    { settings: 'item-check' },
    { settings: 'debug' },
    { settings: 'about' }
  ],
  poe2: [
    { settings: 'hotkeys' },
    { settings: 'chat' },
    { settings: 'general' },
    { settings: 'price-check' },
    { settings: 'maps' },
    { settings: 'item-check' },
    { settings: 'debug' },
    { settings: 'about' }
  ]
}

/** A settings case as `{ settings }`, from a cases.mjs row. */
export function normalizeSettingsCase (entry) {
  if (typeof entry === 'string') return { settings: entry }
  return { settings: entry.settings }
}

/** `target/screens/<game>/settings-<page>`, without the extension. */
export function settingsScreenName (game, page) {
  return path.join(REPO_ROOT, 'target/screens', game, `settings-${page}`)
}

// The overlay widgets the screenshot tool renders, per game: the widget menu, the
// stopwatch, the image strip, the delve grid, the item search and the stash search
// (`screens/widgets.mjs` says how each is opened). A widget case is
// `{ overlay: '<name>' }`; like the settings cases it needs no corpus file, because the
// host serves `fixtures/config/<game>/default/config.json`. The output is
// `target/screens/<game>/<name>.png`, the names of the Rune scenes `widget-<name>`
// (`rust/crates/app/src/lib.rs`).
//
// The two that are config lists rather than chrome take more:
//
// - `item-search`: `tab` (the `typeFilter` button to press: APT's "Skill Gem" or
//   "Replicas", EE2's "Skill Gems", "Uniques" or "Base Items") and `query` (typed into the
//   search field). APT's pinned data ships no alt-quality gems, so its gem tab lists
//   nothing and a default case searches for replicas instead.
// - `stash-search`: `wmTitle` picks the config instance the menu button opens ("Map
//   rolling"; "Dump sorting" is the same widget with another list), and an optional `name`
//   renames the output.
export const WIDGET_NAMES = ['menu', 'stopwatch', 'image-strip', 'delve-grid', 'item-search', 'stash-search']

export const DEFAULT_WIDGET_CASES = {
  poe1: [
    ...['menu', 'stopwatch', 'image-strip', 'delve-grid'].map(overlay => ({ overlay })),
    { overlay: 'item-search', tab: 'Replicas', query: 'headhunter' },
    { overlay: 'stash-search', wmTitle: 'Map rolling' }
  ],
  poe2: [
    ...['menu', 'stopwatch', 'image-strip', 'delve-grid'].map(overlay => ({ overlay })),
    { overlay: 'item-search', tab: 'Uniques', query: 'Mageblood' },
    { overlay: 'stash-search', wmTitle: 'Map rolling' }
  ]
}

/** A widget case as `{ overlay, tab, query, wmTitle, name }`, from a cases.mjs row. */
export function normalizeWidgetCase (entry) {
  if (typeof entry === 'string') return { overlay: entry }
  const { overlay } = entry
  return {
    overlay,
    tab: entry.tab,
    query: entry.query,
    wmTitle: entry.wmTitle,
    name: entry.name ?? overlay
  }
}

/** `target/screens/<game>/<name>`, without the extension. */
export function overlayScreenName (game, name) {
  return path.join(REPO_ROOT, 'target/screens', game, name)
}
