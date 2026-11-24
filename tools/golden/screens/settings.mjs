// The settings window of the ORIGINAL app, rendered with the rest of the reference
// renderer in headless Chromium.
//
// The settings window is a widget like any other: it is in the overlay from the start
// with `wmWants: 'hide'`, and the widget menu's cog button shows it (`wm.show`). Its
// `.window` is `position: absolute; max-width: 50rem; max-height: 38rem`, so the
// element to screenshot is that inner window, not the whole 1600x900 page the overlay
// draws over.
//
// A page is reached by pressing its button in the window's menu (`t(component.name)`:
// "Hotkeys", "Price check", ...). The output is
// `target/screens/<game>/settings-<page>.png` (+ `.txt` with its innerText), the same
// slugs as the Rune scenes (`settings-<game>-<page>` in `rust/crates/app/src/lib.rs`).
import fs from 'node:fs'
import path from 'node:path'
import { REPO_ROOT, SETTINGS_LABELS, settingsScreenName } from './cases.mjs'
import { installImages, waitForImages } from './images.mjs'

function withTimeout (promise, ms, label) {
  let timer
  const timeout = new Promise((_, reject) => {
    timer = setTimeout(() => reject(new Error(`timed out after ${ms}ms waiting for ${label}`)), ms)
  })
  return Promise.race([promise, timeout]).finally(() => clearTimeout(timer))
}

/** A widget's `wmId` in the config the host serves, by `wmType`. */
function widgetId (config, wmType) {
  const widget = config.widgets.find(w => w.wmType === wmType)
  if (!widget) throw new Error(`the config has no ${wmType} widget`)
  return widget.wmId
}

/** `#widget-<id>`'s `.window` (the CSS module hashes the class: `_window_ab12cd`). */
function settingsWindow (page, id) {
  return page.locator(`#widget-${id} > div[class*="window"]`)
}

/**
 * Opens the settings window at `pageSlug` and screenshots it.
 *
 * @param {object} options
 * @param {import('playwright').Browser} options.browser
 * @param {object} options.host          the fake main process (`host.mjs`)
 * @param {string} options.game          `poe1` or `poe2`
 * @param {string} options.pageSlug      `hotkeys`, `price-check`, ...
 * @param {object} options.config        the decoded config the host serves
 * @param {Date} options.clock           the frozen page clock
 * @param {boolean} options.record       `--record` for item art on the pages
 * @param {boolean} options.debug        print the page's console and errors
 * @param {string} options.userAgent     the User-Agent a recording sends
 */
export async function shootSettings ({ browser, host, game, pageSlug, config, clock, record, debug, userAgent }) {
  const labels = SETTINGS_LABELS[game] ?? {}
  const label = labels[pageSlug]
  if (!label) throw new Error(`${game} has no settings page ${pageSlug} (known: ${Object.keys(labels).join(', ')})`)
  const slug = `settings-${pageSlug}`

  const context = await browser.newContext({
    viewport: { width: 1600, height: 900 },
    deviceScaleFactor: 1,
    locale: 'en-US',
    timezoneId: 'UTC'
  })
  const page = await context.newPage()
  const images = await installImages(page, { game, record, userAgent })

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
  // The settings pages read the price database (the price check page's league list, the
  // item check's CoE row); wait for the same answers the price-check cases wait for.
  const pricesReady = host.nextProxyRequest(game === 'poe1' ? 'dense/overviews' : 'overviewData.json')

  try {
    await page.clock.setFixedTime(clock)
    await page.goto(host.url, { waitUntil: 'domcontentloaded' })
    await withTimeout(clientReady, 60_000, 'websocket connection to /events')
    // The menu widget is mounted with the rest of the overlay; the settings widget is in
    // the DOM from then on, hidden.
    await page.waitForSelector('#price-window', { state: 'attached', timeout: 120_000 })
    for (const [ready, what] of [
      [leaguesReady, 'leagues proxy request'],
      [pricesReady, 'poe.ninja price database']
    ]) {
      try {
        await withTimeout(ready, 60_000, what)
      } catch (err) {
        console.error(`[screens] warning (${game}/${slug}): ${err.message}`)
      }
    }
    await host.settled({ quietMs: 1000 })

    // The widget menu lists every hidden widget; the settings one shows as its cog only
    // (`wmTitle: '{icon=fa-cog}'`). Pressing it calls `wm.show(settings)`, which emits
    // `MAIN->CLIENT::config-changed` and the widget appears.
    const id = widgetId(config, 'settings')
    const widget = page.locator(`#widget-${id}`)
    if (!(await widget.isVisible().catch(() => false))) {
      const cog = page.locator(`#widget-${widgetId(config, 'menu')} button:has(i.fa-cog)`)
      if (!(await cog.count())) throw new Error(`no settings button in the widget menu (${game})`)
      await cog.first().click()
    }
    const window = settingsWindow(page, id)
    await window.waitFor({ state: 'visible', timeout: 30_000 })

    // The menu's default page is Hotkeys (`selectedComponent = SettingsHotkeys`).
    if (pageSlug !== 'hotkeys') {
      const button = window.getByRole('button', { name: label, exact: true })
      await button.click()
      await page.waitForTimeout(300)
    }
    await host.settled({ quietMs: 800 })
    await waitForImages(page, `#widget-${id}`)
    await page.waitForTimeout(300)

    const base = settingsScreenName(game, pageSlug)
    fs.mkdirSync(path.dirname(base), { recursive: true })
    await window.screenshot({ path: `${base}.png`, animations: 'disabled', scale: 'css' })
    fs.writeFileSync(`${base}.txt`, await window.innerText())

    console.log(
      `[screens] ${game}/${slug} -> ${path.relative(REPO_ROOT, base)}.png ` +
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
