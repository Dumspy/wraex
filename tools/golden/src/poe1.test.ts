// PoE1 (ref/apt): the pipeline CheckedItem.vue runs for a price check.
import { beforeAll, describe, expect, it } from 'vitest'
import { init, ALTQ_GEM_NAMES, REPLICA_UNIQUE_NAMES } from '@/assets/data'
import { ItemCategory, ItemRarity, parseClipboard, type ParsedItem } from '@/parser'
import { createPresets } from '@/web/price-check/filters/create-presets'
import { apiToSatisfySearch } from '@/web/price-check/trade/common'
import { createTradeRequest } from '@/web/price-check/trade/pathofexile-trade'
import { createTradeRequest as createBulkRequest } from '@/web/price-check/trade/pathofexile-bulk'
import { prepareMapStat } from '@/web/map-check/prepare-map-stats'
import { ModifierType } from '@/parser/modifiers'
import {
  byLang,
  canonical,
  captureOutput,
  loadCases,
  loadItemSearchCorpus,
  pruneFixtures,
  writeFixture,
  writeItemSearchFixture
} from './fixtures'
import * as itemCheck from '@/web/item-check/hotkeyable-actions'
// WidgetItemSearch.vue's private matchers, served by vitest.config.ts.
import { findItems, fuzzyFindHeistGem } from 'virtual:golden-item-search-poe1'

// TradeBulk.vue execSearch: the first exchange request a check sends.
function optimisticHave (refName: string): string[] {
  if (refName === 'Chaos Orb') return ['divine']
  if (refName === 'Divine Orb') return ['chaos']
  return ['divine', 'chaos']
}

// WidgetItemCheck.vue isMapLike: the items the item check shows MapCheck for.
function isMapLike (item: ParsedItem): boolean {
  const { category, rarity, info: { refName } } = item
  return (
    (category === ItemCategory.Map && rarity !== ItemRarity.Unique) ||
    category === ItemCategory.HeistContract ||
    category === ItemCategory.HeistBlueprint ||
    category === ItemCategory.Invitation ||
    category === ItemCategory.Chart ||
    refName === 'Expedition Logbook')
}

// The price-check widget defaults (PriceCheckWindow.vue) and AppConfig defaults.
const cases = loadCases('poe1', {
  lang: 'en',
  options: {
    league: 'Standard',
    merchantOnly: true,
    currency: null,
    collapseListings: 'api',
    activateStockFilter: false,
    searchStatRange: 10,
    useEn: false
  },
  // Empty: use the optimistic search's currencies (see optimisticHave).
  have: [],
  config: {
    language: 'en',
    realm: 'pc-ggg',
    useIntlSite: false,
    widgets: [{ wmType: 'price-check', apiLatencySeconds: 2 }]
  }
})

for (const [lang, group] of byLang(cases)) {
  describe(`poe1 ${lang}`, () => {
    beforeAll(async () => {
      globalThis.__GOLDEN_CONFIG__ = group[0].inputs.config as typeof globalThis.__GOLDEN_CONFIG__
      await init(lang)
    })

    for (const c of group) {
      it(c.slug, () => {
        globalThis.__GOLDEN_CONFIG__ = c.inputs.config as typeof globalThis.__GOLDEN_CONFIG__
        const files: Record<string, string> = {
          'input.txt': c.text,
          'options.json': canonical(c.inputs)
        }
        const parsed = parseClipboard(c.text)
        if (parsed.isErr()) {
          files['parsed.json'] = canonical({ error: parsed.error })
          writeFixture('poe1', c.slug, files)
          return
        }
        const item = parsed.value
        files['parsed.json'] = canonical(item)

        // The item check's hotkey actions: the URL each opens, the stash search it sends.
        files['item-check.json'] = canonical({
          wiki: captureOutput(() => itemCheck.openWiki(item)),
          poedb: captureOutput(() => itemCheck.openPoedb(item)),
          craftOfExile: captureOutput(() => itemCheck.openCoE(item)),
          similar: captureOutput(() => itemCheck.findSimilarItems(item))
        })

        // MapCheck.vue mapStats, before anything else sees the item.
        if (isMapLike(item)) {
          files['map-check.json'] = canonical(item.statsByType
            .filter(calc => calc.type === ModifierType.Explicit)
            .map(calc => prepareMapStat(calc)))
        }

        const presets = createPresets(item, c.inputs.options)
        // Serialised before the requests are built: like the app, they get the
        // live preset objects (EE2 wraps some trade ids in proxies, so no clone).
        files['presets.json'] = canonical(presets)

        for (const { id, filters, stats } of presets.presets) {
          const api = apiToSatisfySearch(item, stats, filters)
          const request = api === 'bulk'
            ? createBulkRequest(filters, item, c.inputs.have.length ? c.inputs.have : optimisticHave(item.info.refName))
            : createTradeRequest(filters, stats)
          files[`trade/${id}.json`] = canonical({ api, request })
        }
        writeFixture('poe1', c.slug, files)
      })
    }
  })
}

it('prunes fixtures without a corpus case', () => {
  pruneFixtures('poe1', cases.map(c => c.slug))
})

// The item search widget (`WidgetItemSearch.vue`): `findItems` per tab and the
// OCR fallback `fuzzyFindHeistGem`, for the queries in
// `corpus/item-search/poe1.json`. Both functions are private to the file's script
// block, which `vitest.config.ts` serves as a module so this runs the
// reference's own code.
describe('poe1 item search', () => {
  it('records the reference answers', async () => {
    const corpus = loadItemSearchCorpus('poe1')
    let loaded = ''
    // `init(lang)` once per data language, in corpus order.
    const useLang = async (lang: string) => {
      if (lang === loaded) return
      await init(lang)
      loaded = lang
    }

    const searches = []
    for (const entry of corpus.searches) {
      const lang = entry.lang ?? corpus.lang
      await useLang(lang)
      // `findItems` reads AppConfig().language for the cmn-Hant exemption.
      const language = entry.language ?? lang
      globalThis.__GOLDEN_CONFIG__ = { language, widgets: [] }
      let results
      if (entry.tab === 'gem') {
        results = findItems({ search: entry.query, namespace: 'GEM', itemNames: ALTQ_GEM_NAMES })
      } else if (entry.tab === 'replica') {
        results = findItems({ search: entry.query, namespace: 'UNIQUE', itemNames: REPLICA_UNIQUE_NAMES })
      } else {
        throw new Error(`golden: unknown poe1 item-search tab ${entry.tab}`)
      }
      searches.push({ lang, language, tab: entry.tab, query: entry.query, results })
    }

    const fuzzy = []
    for (const entry of corpus.fuzzy) {
      const lang = entry.lang ?? corpus.lang
      await useLang(lang)
      globalThis.__GOLDEN_CONFIG__ = { language: lang, widgets: [] }
      // The reference returns undefined when nothing is close enough.
      fuzzy.push({ lang, query: entry.query, result: fuzzyFindHeistGem(entry.query) ?? null })
    }

    expect(searches.length).toBeGreaterThan(0)
    expect(fuzzy.length).toBeGreaterThan(0)
    writeItemSearchFixture('poe1', { searches, fuzzy })
  })
})
