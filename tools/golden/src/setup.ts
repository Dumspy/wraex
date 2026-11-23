// Mocks the browser/Electron surface the reference pipeline touches, modelled on
// ref/ee2/renderer/specs/vitest.setup.ts. Everything is served from disk: the
// harness never talks to the network or the GGG trade API.
import fs from 'node:fs'
import path from 'node:path'
import { vi } from 'vitest'

const REF = process.env.GOLDEN_REF!

// The config each case runs with; set by the runner before init().
declare global {
  var __GOLDEN_CONFIG__: { language: string, widgets: Array<{ wmType: string }> } & Record<string, unknown>
  // What the item check's actions open (window.open) or send (Host.sendEvent).
  var __GOLDEN_OUTPUT__: unknown[]
}
globalThis.__GOLDEN_OUTPUT__ = []

// Replaced whole, like EE2's setup: the real module imports every widget's .vue file.
vi.mock('@/web/Config', () => ({
  TipsFrequency: { Always: 1 },
  AppConfig: (type?: string) => {
    const config = globalThis.__GOLDEN_CONFIG__
    return type ? config.widgets.find(w => w.wmType === type) : config
  }
}))

vi.mock('@/web/background/IPC', () => ({
  Host: {
    // EE2 loads /api/trade2/data/{stats,items} through the proxy; serve its own
    // test snapshots of those endpoints.
    proxy: async (url: string) => {
      const m = /api\/trade2?\/data\/(stats|items)$/.exec(url)
      if (!m) throw new Error(`golden: unexpected network request ${url}`)
      return new Response(fs.readFileSync(path.join(REF, 'specs/data', `${m[1]}.json`)))
    },
    onEvent: () => new AbortController(),
    sendEvent: (event: unknown) => { globalThis.__GOLDEN_OUTPUT__.push(event) },
    getConfig: async () => null,
    logs: { value: '' },
    version: { value: '0.0.00000' },
    updateInfo: { value: { state: 'initial' } },
    isElectron: true
  }
}))

globalThis.fetch = async (input: string | URL | Request) => {
  const url = String(input)
  if (!url.startsWith('/')) throw new Error(`golden: unexpected network request ${url}`)
  const file = path.join(REF, 'public', url.slice(1))
  if (!fs.existsSync(file)) return new Response(`not found: ${file}`, { status: 404 })
  return new Response(fs.readFileSync(file))
}
