// PoE1 (ref/apt): the pipeline CheckedItem.vue runs for a price check.
import { beforeAll, describe, it } from 'vitest'
import { init } from '@/assets/data'
import { parseClipboard } from '@/parser'
import { createPresets } from '@/web/price-check/filters/create-presets'
import { apiToSatisfySearch } from '@/web/price-check/trade/common'
import { createTradeRequest } from '@/web/price-check/trade/pathofexile-trade'
import { createTradeRequest as createBulkRequest } from '@/web/price-check/trade/pathofexile-bulk'
import { byLang, canonical, loadCases, pruneFixtures, writeFixture } from './fixtures'

// TradeBulk.vue execSearch: the first exchange request a check sends.
function optimisticHave (refName: string): string[] {
  if (refName === 'Chaos Orb') return ['divine']
  if (refName === 'Divine Orb') return ['chaos']
  return ['divine', 'chaos']
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
