// The settings pipeline both games share: the default config, and each corpus
// config file migrated (`initConfig`) with the host config derived from it.
import fs from 'node:fs'
import path from 'node:path'
import { it } from 'vitest'
import { canonical } from './fixtures'

const ROOT = path.resolve(import.meta.dirname, '../../..')

interface ConfigModule {
  defaultConfig: () => unknown
  initConfig: () => Promise<void>
  AppConfig: () => unknown
  pushHostConfig: () => void
}

function write (dir: string, files: Record<string, string>) {
  fs.rmSync(dir, { recursive: true, force: true })
  fs.mkdirSync(dir, { recursive: true })
  for (const [name, content] of Object.entries(files)) {
    fs.writeFileSync(path.join(dir, name), content)
  }
}

async function load (m: ConfigModule, input: string | null) {
  globalThis.__GOLDEN_CONFIG_INPUT__ = input
  globalThis.__GOLDEN_HOST_EVENTS__ = []
  await m.initConfig()
  const config = JSON.parse(JSON.stringify(m.AppConfig()))
  const saves = globalThis.__GOLDEN_HOST_EVENTS__.filter(e => e.name === 'CLIENT->MAIN::save-config')
  globalThis.__GOLDEN_HOST_EVENTS__ = []
  m.pushHostConfig()
  const host = globalThis.__GOLDEN_HOST_EVENTS__.find(e => e.name === 'CLIENT->MAIN::update-host-config')!.payload
  return { config, host, saves }
}

export function configCases (game: 'poe1' | 'poe2', m: ConfigModule) {
  const corpus = path.join(ROOT, 'tools/golden/corpus/config', game)
  const out = path.join(ROOT, 'fixtures/config', game)
  const slugs = fs.existsSync(corpus)
    ? fs.readdirSync(corpus).filter(f => f.endsWith('.json')).map(f => f.slice(0, -'.json'.length)).sort()
    : []

  it('records the default config', async () => {
    const { config, host } = await load(m, null)
    write(path.join(out, 'default'), {
      'config.json': canonical(m.defaultConfig()),
      'loaded.json': canonical(config),
      'host.json': canonical(host)
    })
  })

  for (const slug of slugs) {
    it(slug, async () => {
      const input = fs.readFileSync(path.join(corpus, `${slug}.json`), 'utf8')
      const { config, host, saves } = await load(m, input)
      write(path.join(out, slug), {
        // The file as read: not canonical, and not always JSON.
        'input.txt': input,
        'loaded.json': canonical(config),
        'host.json': canonical(host),
        // Unreadable files are replaced by the defaults and saved as `.tmp`.
        ...(saves.length ? { 'saves.json': canonical(saves) } : {})
      })
    })
  }

  it('prunes config fixtures without a corpus file', () => {
    if (!fs.existsSync(out)) return
    for (const entry of fs.readdirSync(out)) {
      if (entry !== 'default' && !slugs.includes(entry)) fs.rmSync(path.join(out, entry), { recursive: true })
    }
  })
}
