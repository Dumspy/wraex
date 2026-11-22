// PoE2 (ref/ee2): the pipeline CheckedItem.vue runs for a price check.
import { beforeAll, describe, it } from 'vitest'
import { init } from '@/assets/data'
import { parseClipboard } from '@/parser'
import { createPresets } from '@/web/price-check/filters/create-presets'
import { apiToSatisfySearch } from '@/web/price-check/trade/common'
import { createTradeRequest } from '@/web/price-check/trade/pathofexile-trade'
import { createTradeRequest as createBulkRequest } from '@/web/price-check/trade/pathofexile-bulk'
import { byLang, canonical, loadCases, pruneFixtures, writeFixture } from './fixtures'

// bulk-api.ts search: the first exchange request a check sends.
function optimisticHave (refName: string): string[] {
  if (refName === 'Exalted Orb') return ['divine', 'chaos']
  if (refName === 'Chaos Orb') return ['divine', 'exalted']
  if (refName === 'Divine Orb') return ['exalted', 'chaos']
  return ['divine', 'exalted', 'chaos']
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
