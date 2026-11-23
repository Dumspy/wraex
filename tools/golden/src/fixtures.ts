// Corpus loading and fixture writing, shared by both games. Layout and rules are
// in ../README.md.
import fs from 'node:fs'
import path from 'node:path'

export type Game = 'poe1' | 'poe2'

const ROOT = path.resolve(import.meta.dirname, '../../..')
const CORPUS = path.resolve(import.meta.dirname, '../corpus')

export interface Case<Options> {
  slug: string
  text: string
  /** Everything the output depends on; written verbatim to options.json. */
  inputs: {
    lang: string
    options: Options
    /** Currencies offered in bulk (exchange) requests; empty means the app default. */
    have: string[]
    config: Record<string, unknown>
  }
}

type Overrides<Options> = Partial<Omit<Case<Options>['inputs'], 'options' | 'config'>> & {
  options?: Partial<Options>
  config?: Record<string, unknown>
}

/**
 * Cases are `corpus/<game>/<slug>.txt`, with an optional `<slug>.json` holding
 * overrides of the defaults (`lang`, `options`, `have`, `config`).
 */
export function loadCases<Options> (game: Game, defaults: Case<Options>['inputs']): Array<Case<Options>> {
  const dir = path.join(CORPUS, game)
  return fs.readdirSync(dir)
    .filter(f => f.endsWith('.txt'))
    .sort()
    .map(file => {
      const slug = file.slice(0, -'.txt'.length)
      const overridesFile = path.join(dir, `${slug}.json`)
      const o: Overrides<Options> = fs.existsSync(overridesFile)
        ? JSON.parse(fs.readFileSync(overridesFile, 'utf8'))
        : {}
      return {
        slug,
        text: fs.readFileSync(path.join(dir, file), 'utf8'),
        inputs: {
          lang: o.lang ?? defaults.lang,
          options: { ...defaults.options, ...o.options },
          have: o.have ?? defaults.have,
          config: { ...defaults.config, ...o.config }
        }
      }
    })
}

/** Canonical JSON: sorted keys, two-space indent, trailing newline. */
export function canonical (value: unknown): string {
  return JSON.stringify(value, (_key, v) => {
    if (v && typeof v === 'object' && !Array.isArray(v)) {
      return Object.fromEntries(Object.keys(v).sort().map(k => [k, v[k]]))
    }
    return v
  }, 2) + '\n'
}

/** Replaces `fixtures/<game>/<slug>/` with exactly `files`. */
export function writeFixture (game: Game, slug: string, files: Record<string, string>) {
  const dir = path.join(ROOT, 'fixtures', game, slug)
  fs.rmSync(dir, { recursive: true, force: true })
  for (const [name, content] of Object.entries(files)) {
    const file = path.join(dir, name)
    fs.mkdirSync(path.dirname(file), { recursive: true })
    fs.writeFileSync(file, content)
  }
}

/** Removes fixtures whose corpus case no longer exists. */
export function pruneFixtures (game: Game, slugs: string[]) {
  const dir = path.join(ROOT, 'fixtures', game)
  if (!fs.existsSync(dir)) return
  const keep = new Set(slugs)
  for (const entry of fs.readdirSync(dir)) {
    if (!keep.has(entry)) fs.rmSync(path.join(dir, entry), { recursive: true })
  }
}

/** Groups cases by language, keeping corpus order, so `init(lang)` runs once each. */
export function byLang<T extends { inputs: { lang: string } }> (cases: T[]): Map<string, T[]> {
  const out = new Map<string, T[]>()
  for (const c of cases) {
    out.set(c.inputs.lang, [...(out.get(c.inputs.lang) ?? []), c])
  }
  return out
}

/**
 * What `action` opens (`window.open`) or sends (`Host.sendEvent`): the item
 * check's actions return nothing. `window` exists only while one runs; defined
 * for good, libraries take the harness for a browser and reach for `document`.
 */
export function captureOutput (action: () => void): unknown {
  globalThis.__GOLDEN_OUTPUT__ = []
  const g = globalThis as unknown as { window?: unknown }
  g.window = { open: (url: string) => { globalThis.__GOLDEN_OUTPUT__.push(url) } }
  try {
    action()
  } finally {
    delete g.window
  }
  const out = globalThis.__GOLDEN_OUTPUT__
  if (out.length > 1) throw new Error('golden: an action produced more than one output')
  return out[0]
}

/** Writes `fixtures/listings/<game>/<name>`: results of the trade site's answers. */
export function writeListingFixture (game: Game, name: string, content: string) {
  const file = path.join(ROOT, 'fixtures', 'listings', game, name)
  fs.mkdirSync(path.dirname(file), { recursive: true })
  fs.writeFileSync(file, content)
}
