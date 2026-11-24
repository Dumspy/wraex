// The ORIGINAL app's overlay widgets (the widget menu, the stopwatch, the image strip
// and the delve grid), rendered with the rest of the reference renderer in headless
// Chromium.
//
// They are overlay widgets, not windows: each is a card positioned by its `anchor`
// inside the 1600x900 overlay page, hidden until `wmWants` says show. The element to
// screenshot is `#widget-<wmId> > div` (the positioned `.widget` card), the same
// element the item-check cases crop to, not the page around it.
//
// How each one is opened:
//
// - `menu`: visible on load (`wmWants: 'show'` in `defaultConfig()`).
// - `stopwatch`: not in `defaultConfig()` (it is an `instances: 'multi'` widget). The
//   menu's `fa-ellipsis-h` popover adds one (`wm.create('timer')`), which then shows
//   itself (`WidgetTimer.vue`'s `uninitialized` branch); the new widget's id is the
//   config's largest + 1.
// - `image-strip`: in the config, hidden; its button in the widget menu (`wm.show`)
//   shows it. The menu lists it under its title ("Cheat sheets").
// - `item-search`: in the config, hidden; its button in the widget menu (the magnifier,
//   `wmTitle: '{icon=fa-search}'`). The case also presses a `typeFilter` tab and types a
//   query, as a user would: APT opens on the gem tab, whose name list is empty at this pin
//   (its data ships no alt-quality gems), so the default case searches replicas instead.
// - `stash-search`: two instances in the config, both hidden; the menu lists them under
//   their titles ("Map rolling", "Dump sorting"). A case picks one by `wmTitle`.
// - `delve-grid`: in the config, hidden, and `menu::skip`, so the menu has no button
//   for it. Its only opener in Electron is the `MAIN->CLIENT::widget-action` event,
//   which `WidgetDelveGrid.vue` ignores outside Electron (`!Host.isElectron` returns),
//   so a browser cannot reach it. The host serves the same config with this one
//   widget's `wmWants` set to `show` and the widget menu's to `hide` (the menu sits at
//   the page's top left, over the grid's corner, and would be captured in the crop);
//   the grid card is upstream's own.
//
// The output is `target/screens/<game>/<name>.png` (+ `.txt` with its innerText), the
// names the Rune scenes `widget-menu`, `widget-stopwatch`, `widget-image-strip` and
// `widget-delve-grid` render. A footer logs the card's size, which is what a scene
// window is sized to.
import fs from 'node:fs'
import path from 'node:path'
import { REPO_ROOT, overlayScreenName } from './cases.mjs'
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

/** `#widget-<id>`'s positioned card (the id sits on the component root, the card inside it). */
function card (page, id) {
  return page.locator(`#widget-${id} > div`)
}

/** The `wmId` of the widget with `wmType` and, for the multi-instance kinds, a `wmTitle`. */
function widgetIdBy (config, wmType, wmTitle) {
  const widget = config.widgets.find(w => w.wmType === wmType && (wmTitle === undefined || w.wmTitle === wmTitle))
  if (!widget) throw new Error(`the config has no ${wmType} widget${wmTitle ? ` titled ${wmTitle}` : ''}`)
  return widget.wmId
}

/**
 * Opens one overlay widget and screenshots it.
 *
 * @param {object} options
 * @param {import('playwright').Browser} options.browser
 * @param {object} options.host          the fake main process (`host.mjs`)
 * @param {string} options.game          `poe1` or `poe2`
 * @param {string} options.name          `menu`, `stopwatch`, `image-strip` or `delve-grid`
 * @param {object} options.config        the decoded config the host serves
 * @param {Date} options.clock           the frozen page clock
 * @param {boolean} options.record       `--record` for item art
 * @param {boolean} options.debug        print the page's console and errors
 * @param {string} options.userAgent     the User-Agent a recording sends
 */
export async function shootWidget ({ browser, host, game, name, testCase = {}, config, clock, record, debug, userAgent }) {
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

  // A delve grid has no in-page opener; the host hands the next page the same config
  // with the widget shown and the menu hidden. Everything else serves the config as the
  // other cases do.
  const restore = name === 'delve-grid' ? showOnly(host, config, 'delve-grid') : null

  const clientReady = host.nextClient()
  const leaguesReady = host.nextProxyRequest('leagues')
  const pricesReady = host.nextProxyRequest(game === 'poe1' ? 'dense/overviews' : 'overviewData.json')

  try {
    await page.clock.setFixedTime(clock)
    await page.goto(host.url, { waitUntil: 'domcontentloaded' })
    await withTimeout(clientReady, 60_000, 'websocket connection to /events')
    await page.waitForSelector('#price-window', { state: 'attached', timeout: 120_000 })
    for (const [ready, what] of [
      [leaguesReady, 'leagues proxy request'],
      [pricesReady, 'poe.ninja price database']
    ]) {
      try {
        await withTimeout(ready, 60_000, what)
      } catch (err) {
        console.error(`[screens] warning (${game}/${name}): ${err.message}`)
      }
    }
    await host.settled({ quietMs: 800 })

    const menuId = widgetId(config, 'menu')
    let id
    if (name === 'menu') {
      id = menuId
    } else if (name === 'stopwatch') {
      // The menu's add popover lists every `instances: 'multi'` widget by its translated
      // name; tippy opens on hover and its content is a real button.
      const ellipsis = page.locator(`#widget-${menuId} button:has(i.fa-ellipsis-h)`)
      if (!(await ellipsis.count())) throw new Error(`no add-widget button in the menu (${game})`)
      await ellipsis.hover()
      const entry = page.getByRole('button', { name: 'Stopwatch', exact: true })
      await entry.waitFor({ state: 'visible', timeout: 10_000 })
      await entry.click()
      id = Math.max(0, ...config.widgets.map(w => w.wmId)) + 1
    } else if (name === 'image-strip') {
      id = widgetId(config, 'image-strip')
      const button = page.locator(`#widget-${menuId} button:has-text("Cheat sheets")`)
      if (!(await button.count())) throw new Error(`no image-strip button in the menu (${game})`)
      await button.click()
    } else if (name === 'delve-grid') {
      id = widgetId(config, 'delve-grid')
    } else if (name === 'item-search') {
      id = widgetId(config, 'item-search')
      // `wmTitle: '{icon=fa-search}'` is an icon-only button in the menu.
      const button = page.locator(`#widget-${menuId} button:has(i.fa-search)`)
      if (!(await button.count())) throw new Error(`no item-search button in the menu (${game})`)
      await button.click()
      // The wrapper (`#widget-<id>`) is a zero-size positioning box; its card is what shows.
      const root = page.locator(`#widget-${id}`)
      await card(page, id).waitFor({ state: 'visible', timeout: 30_000 })
      const { tab, query } = testCase
      if (!tab || !query) throw new Error(`${game}/item-search needs a \`tab\` and a \`query\``)
      // The tabs are `typeFilter` buttons; pressing one recomputes `results`. APT's replica tab
      // also lists "Base items" struck through, so the name is a prefix, not the whole label.
      await root.getByRole('button', { name: tab }).first().click()
      await root.locator('input[type=text]').fill(query)
    } else if (name === 'stash-search') {
      id = widgetIdBy(config, 'stash-search', testCase.wmTitle)
      const title = testCase.wmTitle
      if (!title) throw new Error(`${game}/stash-search needs a \`wmTitle\``)
      const button = page.locator(`#widget-${menuId} button:has-text("${title}")`)
      if (!(await button.count())) throw new Error(`no ${title} button in the menu (${game})`)
      await button.click()
    } else {
      throw new Error(`unknown overlay widget ${name}`)
    }

    const widget = card(page, id)
    await widget.waitFor({ state: 'visible', timeout: 30_000 })
    await host.settled({ quietMs: 500 })
    await waitForImages(page, `#widget-${id}`)
    await page.waitForTimeout(300)

    const box = await widget.boundingBox()
    const base = overlayScreenName(game, testCase.name ?? name)
    fs.mkdirSync(path.dirname(base), { recursive: true })
    await widget.screenshot({ path: `${base}.png`, animations: 'disabled', scale: 'css' })
    fs.writeFileSync(`${base}.txt`, await widget.innerText())

    console.log(
      `[screens] ${game}/${path.basename(base)} -> ${path.relative(REPO_ROOT, base)}.png ` +
      `(${box ? `${Math.round(box.width)}x${Math.round(box.height)}` : 'no box'}; ` +
      `${images.stats.replayed} images replayed, ${images.stats.recorded} recorded` +
      `${images.stats.missed.length ? `, ${images.stats.missed.length} NOT RECORDED` : ''})`
    )
  } catch (err) {
    if (!debug) {
      console.error(`[screens] page log for ${game}/${name}:`)
      for (const line of logs) console.error(`  ${line}`)
    }
    throw err
  } finally {
    if (restore) restore()
    await context.close()
  }
}

/**
 * Shows `wmType` alone in the config every later page load reads (`host.setConfig`), and
 * returns the undo. The widgets that normally start shown (the widget menu) are hidden, so
 * the crop holds only the widget being photographed.
 */
function showOnly (host, config, wmType) {
  const shown = config.widgets.find(w => w.wmType === wmType)
  if (!shown) throw new Error(`the config has no ${wmType} widget`)
  const before = config.widgets.map(widget => widget.wmWants)
  for (const widget of config.widgets) widget.wmWants = 'hide'
  shown.wmWants = 'show'
  host.setConfig(config)
  return () => {
    config.widgets.forEach((widget, index) => { widget.wmWants = before[index] })
    host.setConfig(config)
  }
}
