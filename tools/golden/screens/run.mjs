// Screenshots of the ORIGINAL price-check window, produced by running the
// reference renderer (ref/apt or ref/ee2) in a normal browser against a fake
// main process. Phase 5 uses these to compare the Rune port against upstream.
//
//   node tools/golden/screens/run.mjs [--game poe1|poe2] [--case <slug>]... [--record] [--rebuild]
//
// Output goes to target/screens/<game>/<case>.png (+ .txt with innerText); it is
// not committed and only meant for visual comparison. `--record` forwards proxy
// misses to the real trade site / poe.ninja and saves the answers under
// corpus/screens/<game>/proxy/, so later runs replay them offline.
import fs from 'node:fs'
import path from 'node:path'
import { chromium } from 'playwright'
import { ensureBuild } from './build.mjs'
import { DEFAULT_CASES, REPO_ROOT, corpusFile, normalizeCase } from './cases.mjs'
import { PROXY_HOSTS, startHost } from './host.mjs'
import { imageDir, installImages, recordImage, waitForImages } from './images.mjs'

// The fixture harness records `defaultConfig()` without a league; `Host.getConfig`
// hands the renderer the same JSON. The only edit is leagueId, so the leagues
// request fixture matches what the window shows in its title bar.
const LEAGUE_ID = 'Standard'

// Used when a game has no `recorded-at.txt`: a fixed date, so replayed files
// always render the same relative listing times.
const DEFAULT_CLOCK = new Date('2026-01-01T12:00:00Z')

function loadConfig (game) {
  const file = path.join(REPO_ROOT, 'fixtures/config', game, 'default/config.json')
  const config = JSON.parse(fs.readFileSync(file, 'utf8'))
  config.leagueId = LEAGUE_ID
  return config
}

// `[workspace.package] version` from rust/Cargo.toml, so a recorded request
// identifies the app the way `wraex_core::trade::api::USER_AGENT` does
// (`wraex/<version> (+https://github.com/Dumspy/wraex)`).
function workspaceVersion () {
  const toml = fs.readFileSync(path.join(REPO_ROOT, 'rust/Cargo.toml'), 'utf8')
  const section = toml.split(/^\[/m).find(chunk => chunk.startsWith('workspace.package]'))
  const found = /^version\s*=\s*"([^"]+)"/m.exec(section ?? '')
  if (!found) throw new Error('no [workspace.package] version in rust/Cargo.toml')
  return found[1]
}

function userAgent () {
  return `wraex/${workspaceVersion()} (+https://github.com/Dumspy/wraex)`
}

function recordedAtFile (game) {
  return path.join(REPO_ROOT, 'tools/golden/corpus/screens', game, 'recorded-at.txt')
}

// The date the page's clock is frozen at. Recording writes it once (the
// listings it saves must not look like they are in the future); later runs read
// it, so replay is stable. Delete the file to re-freeze at the next recording.
function pageClock (game, { record }) {
  const file = recordedAtFile(game)
  if (record && !fs.existsSync(file)) {
    fs.mkdirSync(path.dirname(file), { recursive: true })
    fs.writeFileSync(file, `${new Date().toISOString()}\n`)
    console.log(`[screens] ${game}: freezing the clock at now (${path.relative(REPO_ROOT, file)})`)
  }
  if (!fs.existsSync(file)) return DEFAULT_CLOCK

  const text = fs.readFileSync(file, 'utf8').trim()
  const date = new Date(text)
  if (Number.isNaN(date.getTime())) {
    throw new Error(`${path.relative(REPO_ROOT, file)} is not an ISO date: ${JSON.stringify(text)}`)
  }
  return date
}

function withTimeout (promise, ms, label) {
  let timer
  const timeout = new Promise((_, reject) => {
    timer = setTimeout(() => reject(new Error(`timed out after ${ms}ms waiting for ${label}`)), ms)
  })
  return Promise.race([promise, timeout]).finally(() => clearTimeout(timer))
}

function usage () {
  console.log(`Usage: node tools/golden/screens/run.mjs [options]

  --game <poe1|poe2>  game to render (repeatable; default: both)
  --case <slug>       corpus case to render (repeatable; needs exactly one --game)
  --record            fetch proxy misses from the real trade site / poe.ninja and
                      save them under corpus/screens/<game>/proxy/ (one request
                      per second; needs network)
  --image <url>       record one more item image (repeatable; needs --game and
                      network), for art only a state no case reaches, e.g. the
                      anointment oils of a hidden filter row: --game poe1 --image
                      https://web.poecdn.com/gen/image/.../GoldenOil.png
  --rebuild           rebuild the renderer even if .screens-build/<game> exists
  --debug             print the page's console and errors while it runs
  -h, --help

Cases are tools/golden/corpus/<game>/<slug>.txt. Output: target/screens/<game>/<slug>.png
and .txt.`)
}

function parseArgs (argv) {
  const opts = { games: [], cases: [], rebuild: false, debug: false, record: false, images: [] }
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i]
    if (arg === '--game') {
      const game = argv[++i]
      if (!['poe1', 'poe2'].includes(game)) throw new Error(`--game must be poe1 or poe2, got ${game}`)
      if (!opts.games.includes(game)) opts.games.push(game)
    } else if (arg === '--case') {
      const slug = argv[++i]
      if (!slug) throw new Error('--case needs a corpus slug')
      opts.cases.push(slug)
    } else if (arg === '--record') {
      opts.record = true
    } else if (arg === '--image') {
      const url = argv[++i]
      if (!url) throw new Error('--image needs a URL')
      opts.images.push(url)
    } else if (arg === '--rebuild') {
      opts.rebuild = true
    } else if (arg === '--debug') {
      opts.debug = true
    } else if (arg === '-h' || arg === '--help') {
      usage()
      process.exit(0)
    } else {
      throw new Error(`unknown argument ${arg}`)
    }
  }

  if (!opts.games.length) opts.games = ['poe1', 'poe2']
  if (opts.images.length && opts.games.length !== 1) throw new Error('--image needs exactly one --game')
  if (opts.cases.length) {
    if (opts.games.length !== 1) throw new Error('--case needs exactly one --game')
    for (const slug of opts.cases) {
      if (!fs.existsSync(corpusFile(opts.games[0], slug))) {
        throw new Error(`no corpus file for ${opts.games[0]} case ${slug}`)
      }
    }
  }
  return opts
}

// Clicks the window's "Search" button until it sticks. CheckedItem resets
// `doSearch` when the active preset's stats change (a poe.ninja answer landing
// after the item did, say), which can undo a click made right after the window
// appears; so keep clicking while the button is still rendered.
async function startSearch (page, { game, slug }) {
  const button = page.locator('#price-window').getByRole('button', { name: 'Search', exact: true })
  for (let attempt = 0; attempt < 4; attempt++) {
    if (!(await button.isVisible().catch(() => false))) return
    await button.click()
    await page.waitForTimeout(800)
  }
  if (await button.isVisible().catch(() => false)) {
    console.error(`[screens] warning (${game}/${slug}): the Search button would not go away`)
  }
}

async function shoot ({ browser, host, game, testCase, clock, record, debug }) {
  const { slug, search } = testCase
  const text = fs.readFileSync(corpusFile(game, slug), 'utf8')
  // The machine's own locale would reach the page (`toLocaleString` writes `1,2` under a
  // da-DK format where en-US writes `1.2`), and the screenshots are compared with the
  // port's en-US output. UTC keeps listing ages off the runner's time zone too.
  const context = await browser.newContext({
    viewport: { width: 1600, height: 900 },
    deviceScaleFactor: 1,
    locale: 'en-US',
    timezoneId: 'UTC'
  })
  const page = await context.newPage()
  // Item art is the game CDN's, which the page fetches itself; serve it from (or record it
  // to) `corpus/screens/<game>/images/`.
  const images = await installImages(page, { game, record, userAgent: record ? userAgent() : '' })

  const logs = []
  page.on('console', (msg) => {
    const line = `[console.${msg.type()}] ${msg.text()}`
    logs.push(line)
    if (debug) console.error(`  ${line}`)
  })
  page.on('pageerror', (err) => {
    const line = `[pageerror] ${err.stack ?? err.message}`
    logs.push(line)
    if (debug) console.error(`  ${line}`)
  })
  page.on('requestfailed', (req) => {
    const line = `[requestfailed] ${req.method()} ${req.url()} ${req.failure()?.errorText ?? ''}`
    logs.push(line)
    if (debug) console.error(`  ${line}`)
  })

  // Registered before navigation, so no event can slip past.
  const clientReady = host.nextClient()
  const leaguesReady = host.nextProxyRequest('leagues')
  // PoE1 reads `poe.ninja/poe1/.../dense/overviews`, PoE2 `api.exiledexchange2.dev/...`
  // `overviewData.json`; either way it is the answer that fills the price database.
  const pricesReady = host.nextProxyRequest(game === 'poe1' ? 'dense/overviews' : 'overviewData.json')

  try {
    await page.clock.setFixedTime(clock)
    await page.goto(host.url, { waitUntil: 'domcontentloaded' })

    await withTimeout(clientReady, 60_000, 'websocket connection to /events')
    // The app mounts after config, i18n and the data indexes load; the widget is
    // in the DOM from then on but stays `display: none` until it is shown.
    await page.waitForSelector('#price-window', { state: 'attached', timeout: 120_000 })

    // CheckedItem only renders once a league is selected, which needs the
    // leagues answer. Without that fixture the window shows an error instead,
    // so this is a warning rather than a failure.
    try {
      await withTimeout(leaguesReady, 60_000, 'leagues proxy request')
    } catch (err) {
      console.error(`[screens] warning (${game}/${slug}): ${err.message}`)
    }

    // Selecting the league also asks poe.ninja for its price database (`Prices.ts`'s
    // `watch(leagues.selectedId)`, `load(true)`), and the window must have it before the
    // item arrives: `PriceTrend.vue` computes its row with `findPriceByQuery` and only
    // re-runs when `xchgRate` changes, so an item sent first stays at `?` for good.
    // Waiting here is what a person sees, whose price database has long been warm.
    try {
      await withTimeout(pricesReady, 60_000, 'poe.ninja price database')
    } catch (err) {
      console.error(`[screens] warning (${game}/${slug}): ${err.message}`)
    }
    await host.settled({ quietMs: 1000 })
    await page.waitForTimeout(300)

    const itemText = {
      name: 'MAIN->CLIENT::item-text',
      payload: {
        target: 'price-check',
        clipboard: text,
        position: { x: 800, y: 450 },
        focusOverlay: false
      }
    }
    host.sendEvent(itemText)

    await page.waitForSelector('#price-window', { state: 'visible', timeout: 60_000 })
    // Let the item's own requests (poe.ninja, in-flight stats) land first.
    await host.settled({ quietMs: 1200 })
    await page.waitForTimeout(500)

    // Upstream only searches on its own for uniques and bulk currency; for
    // everything else the "Search" button (the same text the window renders)
    // starts the trade request. Clicking is the one place the harness acts.
    if (search) await startSearch(page, { game, slug })

    // The page asks the proxy for a chain of answers (search, then fetch); a
    // recorded chain is throttled to one request per second, so wait for the
    // traffic to stop rather than for a fixed time.
    const quiet = await host.settled({ quietMs: record ? 3000 : 1500 })
    if (!quiet) console.error(`[screens] warning (${game}/${slug}): proxy traffic never went quiet`)

    const listed = await page.waitForFunction(() => {
      const win = document.querySelector('#price-window')
      if (!win || win.querySelector('ui-error-box')) return false
      return Boolean(win.querySelector('tbody tr'))
    }, { timeout: 15_000 }).then(() => true).catch(() => false)
    if (!listed) console.error(`[screens] warning (${game}/${slug}): no listings in the window`)
    // The window's pictures are served from disk, so they settle almost at once; waiting
    // keeps the screenshot from catching a half-loaded `<img>`.
    await waitForImages(page)
    await page.waitForTimeout(300)

    const outDir = path.join(REPO_ROOT, 'target/screens', game)
    fs.mkdirSync(outDir, { recursive: true })
    const element = page.locator('#price-window')
    await element.screenshot({
      path: path.join(outDir, `${slug}.png`),
      animations: 'disabled',
      scale: 'css'
    })
    fs.writeFileSync(path.join(outDir, `${slug}.txt`), await element.innerText())

    console.log(
      `[screens] ${game}/${slug} -> target/screens/${game}/${slug}.png ` +
      `(${images.stats.replayed} images replayed, ${images.stats.recorded} recorded` +
      `${images.stats.missed.length ? `, ${images.stats.missed.length} NOT RECORDED` : ''})`
    )
  } catch (err) {
    if (!debug) {
      console.error(`[screens] page log for ${game}/${slug}:`)
      for (const line of logs) console.error(`  ${line}`)
    }
    throw err
  } finally {
    await context.close()
  }
}

// `--case` keeps the options a default case has (its `search` flag), so
// rendering one case behaves like the full run.
function caseEntry (game, slug) {
  const known = (DEFAULT_CASES[game] ?? [])
    .map(normalizeCase)
    .find(entry => entry.slug === slug)
  return known ?? { slug, search: false }
}

async function main () {
  const opts = parseArgs(process.argv.slice(2))
  const casesByGame = opts.cases.length
    ? { [opts.games[0]]: opts.cases.map(slug => caseEntry(opts.games[0], slug)) }
    : Object.fromEntries(
      Object.entries(DEFAULT_CASES).map(([game, cases]) => [game, cases.map(normalizeCase)])
    )

  if (opts.images.length) {
    const game = opts.games[0]
    for (const url of opts.images) {
      const { file, bytes } = await recordImage(url, { dir: imageDir(game), userAgent: userAgent() })
      console.log(`[screens] recorded ${path.relative(REPO_ROOT, file)} (${bytes} bytes)`)
    }
    if (!opts.cases.length) return
  }

  const agent = opts.record ? userAgent() : ''
  const browser = await chromium.launch()
  try {
    for (const game of opts.games) {
      const buildDir = ensureBuild(game, { rebuild: opts.rebuild })
      const proxyDir = path.join(REPO_ROOT, 'tools/golden/corpus/screens', game, 'proxy')
      const clock = pageClock(game, opts)
      const host = await startHost({
        buildDir,
        config: loadConfig(game),
        proxyDir,
        verbose: opts.debug,
        record: opts.record,
        allowedHosts: PROXY_HOSTS[game],
        userAgent: agent
      })
      console.log(`[screens] ${game}: hosting ${path.relative(REPO_ROOT, buildDir)} at ${host.url}`)
      if (opts.record) console.log(`[screens] ${game}: recording as ${agent}, one request per second`)
      try {
        for (const testCase of casesByGame[game] ?? []) {
          await shoot({ browser, host, game, testCase, clock, record: opts.record, debug: opts.debug })
          if (host.errors.length) throw new Error(host.errors[0])
        }
      } finally {
        await host.close()
        const { replayed, recorded, forwarded } = host.stats
        console.log(`[screens] ${game}: ${replayed} replayed, ${forwarded} forwarded, ${recorded} recorded`)
        for (const failure of host.failures) console.error(`[screens] warning (${game}): not recorded ${failure}`)
      }
      if (host.errors.length) throw new Error(host.errors[0])
    }
  } finally {
    await browser.close()
  }
}

main().catch((err) => {
  console.error(`[screens] ${err.message}`)
  if (process.env.DEBUG) console.error(err)
  process.exitCode = 1
})
