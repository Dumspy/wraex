// PoE2 (ref/ee2): the pipeline CheckedItem.vue runs for a price check.
import { beforeAll, describe, expect, it } from 'vitest'
import { init, GEM_NS_NAMES, ITEM_NS_NAMES, UNIQUE_NS_NAMES } from '@/assets/data'
import { ItemCategory, ItemRarity, parseClipboard, type ParsedItem } from '@/parser'
import { createPresets } from '@/web/price-check/filters/create-presets'
import { apiToSatisfySearch } from '@/web/price-check/trade/common'
import { createTradeRequest } from '@/web/price-check/trade/pathofexile-trade'
import { createTradeRequest as createBulkRequest } from '@/web/price-check/trade/pathofexile-bulk'
import { prepareMapStats } from '@/web/map-check/prepare-map-stats'
import fs from 'node:fs'
import path from 'node:path'
import {
  byLang,
  canonical,
  captureOutput,
  loadCases,
  loadItemSearchCorpus,
  pruneFixtures,
  writeFixture,
  writeItemSearchFixture,
  writeListingFixture
} from './fixtures'
import { __testExports as tradeTestExports } from '@/web/price-check/trade/pathofexile-trade'
import * as itemCheck from '@/web/item-check/hotkeyable-actions'
// WidgetItemSearch.vue's private matchers, served by vitest.config.ts.
import { findItems, fuzzyFindHeistGem } from 'virtual:golden-item-search-poe2'

// bulk-api.ts search: the first exchange request a check sends.
function optimisticHave (refName: string): string[] {
  if (refName === 'Exalted Orb') return ['divine', 'chaos']
  if (refName === 'Chaos Orb') return ['divine', 'exalted']
  if (refName === 'Divine Orb') return ['exalted', 'chaos']
  return ['divine', 'exalted', 'chaos']
}

// WidgetItemCheck.vue isMapLike: the items the item check shows MapCheck for.
function isMapLike (item: ParsedItem): boolean {
  const { category, rarity, info: { refName } } = item
  return (
    (category === ItemCategory.Map && rarity !== ItemRarity.Unique) ||
    category === ItemCategory.HeistContract ||
    category === ItemCategory.HeistBlueprint ||
    category === ItemCategory.Invitation ||
    refName === 'Expedition Logbook')
}

// The price-check widget defaults (PriceCheckWindow.vue) and AppConfig defaults.
// `useEn` follows CheckedItem.vue: cmn-Hant on pc-ggg, or preferredTradeSite "www".
const cases = loadCases('poe2', {
  lang: 'en',
  options: {
    league: 'Standard',
    currency: undefined,
    listingType: undefined,
    collapseListings: 'api',
    activateStockFilter: false,
    searchStatRange: 10,
    useEn: false,
    defaultAllSelected: false
  },
  // Empty: use the optimistic search's currencies (see optimisticHave).
  have: [],
  config: {
    language: 'en',
    realm: 'pc-ggg',
    preferredTradeSite: 'default',
    widgets: [{
      wmType: 'price-check',
      apiLatencySeconds: 2,
      searchStatRange: 10,
      alwaysShowTier: false,
      coreCurrency: 'exalted',
      savedAugments: {}
    }, {
      // WidgetItemCheck.vue's default.
      wmType: 'item-check',
      samePricedType: 'both'
    }]
  }
})

for (const [lang, group] of byLang(cases)) {
  describe(`poe2 ${lang}`, () => {
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
          writeFixture('poe2', c.slug, files)
          return
        }
        const item = parsed.value
        files['parsed.json'] = canonical(item)

        // The item check's hotkey actions: the URL each opens, the stash search it sends.
        files['item-check.json'] = canonical({
          wiki: captureOutput(() => itemCheck.openWiki(item)),
          poedb: captureOutput(() => itemCheck.openPoedb(item)),
          craftOfExile: captureOutput(() => itemCheck.openCoE(item)),
          similar: captureOutput(() => itemCheck.findSimilarItems(item)),
            samePriced: captureOutput(() => itemCheck.findSamePricedItems(item))
        })

        // MapCheck.vue mapStats, before anything else sees the item.
        if (isMapLike(item)) {
          files['map-check.json'] = canonical(prepareMapStats(item))
        }

        const presets = createPresets(item, c.inputs.options)
        // Serialised before the requests are built: like the app, they get the
        // live preset objects (EE2 wraps some trade ids in proxies, so no clone).
        files['presets.json'] = canonical(presets)

        for (const { id, filters, stats } of presets.presets) {
          const api = apiToSatisfySearch(item, stats, filters)
          let request
          if (api === 'bulk') {
            // CheckedItem.vue forces this before handing the filters to TradeBulk.
            filters.trade.listingType = 'online'
            request = createBulkRequest(filters, item, c.inputs.have.length ? c.inputs.have : optimisticHave(item.info.refName))
          } else {
            request = createTradeRequest(filters, stats, item)
          }
          files[`trade/${id}.json`] = canonical({ api, request })
        }
        writeFixture('poe2', c.slug, files)
      })
    }
  })
}

it('prunes fixtures without a corpus case', () => {
  pruneFixtures('poe2', cases.map(c => c.slug))
})

// The item search widget (`WidgetItemSearch.vue`): `findItems` per tab and the
// OCR fallback `fuzzyFindHeistGem`, for the queries in
// `corpus/item-search/poe2.json`. Both functions are private to the file's script
// block, which `vitest.config.ts` serves as a module so this runs the
// reference's own code.
describe('poe2 item search', () => {
  it('records the reference answers', async () => {
    const corpus = loadItemSearchCorpus('poe2')
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
        results = findItems({ search: entry.query, namespace: 'GEM', itemNames: GEM_NS_NAMES })
      } else if (entry.tab === 'unique') {
        results = findItems({ search: entry.query, namespace: 'UNIQUE', itemNames: UNIQUE_NS_NAMES })
      } else if (entry.tab === 'base_item') {
        results = findItems({ search: entry.query, namespace: 'ITEM', itemNames: ITEM_NS_NAMES })
      } else {
        throw new Error(`golden: unknown poe2 item-search tab ${entry.tab}`)
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
    writeItemSearchFixture('poe2', { searches, fuzzy })
  })
})

// The trade site's fetch answer EE2 keeps for its own tests: the display item
// parseFetchResult builds for each listing (CheckedItem's result rows).
describe('poe2 listings', () => {
  beforeAll(async () => {
    globalThis.__GOLDEN_CONFIG__ = cases[0].inputs.config as typeof globalThis.__GOLDEN_CONFIG__
    await init('en')
  })

  it('records display items', () => {
    const file = path.join(process.env.GOLDEN_REF!, 'specs/docs/fetchResponses.json')
    const data = JSON.parse(fs.readFileSync(file, 'utf8')) as { result: Array<{ id: string }> }
    const items = data.result.map(result => {
      try {
        return { id: result.id, displayItem: tradeTestExports.parseFetchResult(result as never) }
      } catch (e) {
        return { id: result.id, error: (e as Error).message }
      }
    })
    writeListingFixture('poe2', 'display-items.json', canonical(items))
  })
})
