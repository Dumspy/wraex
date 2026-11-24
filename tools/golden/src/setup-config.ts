// Mocks for the settings projects: the IPC host serves the config under test
// and records what the module sends back (saves, the host config).
import { vi } from 'vitest'

declare global {
  // The config file's text `Host.getConfig()` returns; `null` for none.
  var __GOLDEN_CONFIG_INPUT__: string | null
  // Events the module sent (`Host.sendEvent`).
  var __GOLDEN_HOST_EVENTS__: Array<{ name: string, payload: unknown }>
}
globalThis.__GOLDEN_CONFIG_INPUT__ = null
globalThis.__GOLDEN_HOST_EVENTS__ = []

vi.mock('@/web/background/IPC', () => ({
  Host: {
    getConfig: async () => globalThis.__GOLDEN_CONFIG_INPUT__,
    sendEvent: (event: { name: string, payload: unknown }) => { globalThis.__GOLDEN_HOST_EVENTS__.push(event) },
    onEvent: () => new AbortController(),
    proxy: async (url: string) => { throw new Error(`golden: unexpected network request ${url}`) },
    logs: { value: '' },
    version: { value: '0.0.00000' },
    updateInfo: { value: { state: 'initial' } },
    isElectron: true
  }
}))
