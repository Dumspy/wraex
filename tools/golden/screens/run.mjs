// Screenshots of the ORIGINAL price-check window, produced by running the
// reference renderer (ref/apt or ref/ee2) in a normal browser against a fake
// main process. Phase 5 uses these to compare the Rune port against upstream.
//
//   node tools/golden/screens/run.mjs [--game poe1|poe2] [--case <slug>]... [--rebuild]
//
// Output goes to target/screens/<game>/<case>.png (+ .txt with innerText); it is
// not committed and only meant for visual comparison.
import fs from 'node:fs'
import path from 'node:path'
import { chromium } from 'playwright'
import { ensureBuild } from './build.mjs'
import { DEFAULT_CASES, REPO_ROOT, corpusFile } from './cases.mjs'
import { startHost } from './host.mjs'

// The fixture harness records `defaultConfig()` without a league; `Host.getConfig`
// hands the renderer the same JSON. The only edit is leagueId, so the leagues
// request fixture matches what the window shows in its title bar.
const LEAGUE_ID = 'Standard'

function loadConfig (game) {
  const file = path.join(REPO_ROOT, 'fixtures/config', game, 'default/config.json')
  const config = JSON.parse(fs.readFileSync(file, 'utf8'))
  config.leagueId = LEAGUE_ID
  return config
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
  --rebuild           rebuild the renderer even if .screens-build/<game> exists
  --debug             print the page's console and errors while it runs
  -h, --help

Cases are tools/golden/corpus/<game>/<slug>.txt. Output: target/screens/<game>/<slug>.png
and .txt.`)
}

function parseArgs (argv) {
  const opts = { games: [], cases: [], rebuild: false, debug: false }
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

async function shoot ({ browser, host, game, slug, debug }) {
  const text = fs.readFileSync(corpusFile(game, slug), 'utf8')
  const context = await browser.newContext({
    viewport: { width: 1600, height: 900 },
    deviceScaleFactor: 1
  })
  const page = await context.newPage()

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

  try {
    await page.clock.setFixedTime(new Date('2026-01-01T12:00:00Z'))
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

    host.sendEvent({
      name: 'MAIN->CLIENT::item-text',
      payload: {
        target: 'price-check',
        clipboard: text,
        position: { x: 800, y: 450 },
        focusOverlay: false
      }
    })

    await page.waitForSelector('#price-window', { state: 'visible', timeout: 60_000 })
    // Trade/poe.ninja answers are deliberately absent (404), so let any retry
    // or spinner settle briefly; do not wait for it indefinitely.
    await page.waitForLoadState('networkidle', { timeout: 20_000 }).catch(() => {})
    await page.waitForTimeout(500)

    const outDir = path.join(REPO_ROOT, 'target/screens', game)
    fs.mkdirSync(outDir, { recursive: true })
    const element = page.locator('#price-window')
    await element.screenshot({
      path: path.join(outDir, `${slug}.png`),
      animations: 'disabled',
      scale: 'css'
    })
    fs.writeFileSync(path.join(outDir, `${slug}.txt`), await element.innerText())

    console.log(`[screens] ${game}/${slug} -> target/screens/${game}/${slug}.png`)
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

async function main () {
  const opts = parseArgs(process.argv.slice(2))
  const casesByGame = opts.cases.length
    ? { [opts.games[0]]: opts.cases }
    : DEFAULT_CASES

  const browser = await chromium.launch()
  try {
    for (const game of opts.games) {
      const buildDir = ensureBuild(game, { rebuild: opts.rebuild })
      const proxyDir = path.join(REPO_ROOT, 'tools/golden/corpus/screens', game, 'proxy')
      const host = await startHost({
        buildDir,
        config: loadConfig(game),
        proxyDir,
        verbose: opts.debug
      })
      console.log(`[screens] ${game}: hosting ${path.relative(REPO_ROOT, buildDir)} at ${host.url}`)
      try {
        for (const slug of casesByGame[game] ?? []) {
          await shoot({ browser, host, game, slug, debug: opts.debug })
        }
      } finally {
        await host.close()
      }
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
