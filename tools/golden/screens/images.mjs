// Item and map art for the reference screenshots.
//
// The reference renderer loads an item's picture straight from the game's CDN
// (`https://web.poecdn.com/gen/image/...`) and a map's screenshot from imgur
// (`https://i.imgur.com/...`, the `area.screenshot` of the game data), not through
// `/proxy`, so a screenshot run has to answer those requests itself. Playwright
// routes them here:
//
// - the file is `corpus/screens/<game>/images/<8 hex of sha1(url)>-<the file name>`,
//   the name `rust/crates/app/src/ui/art.rs` (`recorded_name`) rebuilds, so the Rune
//   port's replay mode reads the very same files;
// - an existing file is served back, so replay needs no network at all;
// - with `--record` a miss is fetched once (one request per second, Wraex's
//   User-Agent) and saved, then served;
// - without `--record` a miss is a 404 with a line on stderr, so a scene that needs an
//   image the recording lacks is visible instead of silently broken.
import fs from 'node:fs'
import path from 'node:path'
import https from 'node:https'
import { createHash } from 'node:crypto'
import { REPO_ROOT } from './cases.mjs'

/** The hosts the reference's `<img>` tags load art from: the item CDN and imgur (map screenshots). */
export const IMAGE_HOSTS = ['web.poecdn.com', 'i.imgur.com']
/** The item CDN (`wraex_core::images::CDN_HOST`). */
export const IMAGE_HOST = IMAGE_HOSTS[0]

// At most one fetched image per second, like the proxy recordings.
const MIN_FETCH_INTERVAL_MS = 1000

const CONTENT_TYPES = {
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.jpeg': 'image/jpeg',
  '.gif': 'image/gif',
  '.webp': 'image/webp',
  '.svg': 'image/svg+xml',
  '.avif': 'image/avif'
}

/** `corpus/screens/<game>/images`: the recorded item art of one game. */
export function imageDir (game) {
  return path.join(REPO_ROOT, 'tools/golden/corpus/screens', game, 'images')
}

/**
 * `<8 hex of sha1(url)>-<the file's own name>`: `art.rs`'s `recorded_name`, so a recording
 * and the port's lookup agree. The hash keeps two paths that end in the same file name
 * (every skin has its own `BodyStr02.png`) apart; the file name keeps it readable.
 */
export function imageFileName (url) {
  const hash = createHash('sha1').update(url).digest('hex').slice(0, 8)
  const file = url.slice(url.lastIndexOf('/') + 1)
  const name = (file || 'image.png').replace(/[^A-Za-z0-9._-]/g, '_')
  return `${hash}-${name}`
}

function contentTypeFor (file) {
  return CONTENT_TYPES[path.extname(file).toLowerCase()] ?? 'application/octet-stream'
}

// One fetch at a time, a second apart, whatever asked for the images (a page's `<img>`
// tags or `--image`).
let chain = Promise.resolve()
let lastFetchAt = 0
function throttle () {
  const slot = chain.then(async () => {
    const wait = Math.max(0, lastFetchAt + MIN_FETCH_INTERVAL_MS - Date.now())
    if (wait > 0) await new Promise(resolve => setTimeout(resolve, wait))
    lastFetchAt = Date.now()
  })
  chain = slot.catch(() => {})
  return slot
}

// One HTTPS GET for an image. `accept-encoding: identity` asks for the bytes as they
// are, so what is saved is what the page would have received.
function fetchImageOnce (url, userAgent) {
  return new Promise((resolve, reject) => {
    const request = https.get(url, {
      headers: {
        'user-agent': userAgent,
        accept: 'image/avif,image/webp,image/png,image/*;q=0.8',
        'accept-encoding': 'identity'
      }
    }, (response) => {
      const chunks = []
      response.on('data', chunk => chunks.push(chunk))
      response.on('error', reject)
      response.on('end', () => resolve({
        status: response.statusCode,
        headers: response.headers,
        body: Buffer.concat(chunks)
      }))
    })
    request.on('error', reject)
  })
}

/**
 * Record one image, wherever its URL came from: the screenshot cases' misses, and
 * `--image` for art only a state the harness does not click reaches (the anointment oils
 * of a hidden filter row, a map screenshot, say). Returns the file's path.
 *
 * @param {string} url
 * @param {{dir: string, userAgent: string}} options
 */
export async function recordImage (url, { dir, userAgent }) {
  const allowed = IMAGE_HOSTS.some(host => url.startsWith(`https://${host}/`))
  if (!allowed) {
    throw new Error(`${url} is not an image on ${IMAGE_HOSTS.join(' or ')}`)
  }
  await throttle()
  const answer = await fetchImageOnce(url, userAgent)
  if (answer.status !== 200 || !answer.body.length) {
    throw new Error(`could not fetch ${url}: HTTP ${answer.status}`)
  }
  const file = path.join(dir, imageFileName(url))
  fs.mkdirSync(dir, { recursive: true })
  fs.writeFileSync(file, answer.body)
  return { file, bytes: answer.body.length }
}

/**
 * Route the reference's image hosts on `page` to the recordings in `imageDir(game)`.
 *
 * @param {import('playwright').Page} page
 * @param {object} options
 * @param {'poe1'|'poe2'} options.game
 * @param {boolean} [options.record]  fetch misses and save them
 * @param {string} [options.userAgent] User-Agent for a recorded request
 * @param {boolean} [options.verbose] log every served image
 * @returns {Promise<{stats: {replayed: number, recorded: number, missed: string[]}}>}
 */
export async function installImages (page, { game, record = false, userAgent = '', verbose = false }) {
  const dir = imageDir(game)
  fs.mkdirSync(dir, { recursive: true })
  const stats = { replayed: 0, recorded: 0, missed: [] }

  async function fulfilFile (route, file) {
    stats.replayed++
    if (verbose) console.error(`[screens] image ${route.request().url()} -> ${path.basename(file)}`)
    await route.fulfill({ status: 200, contentType: contentTypeFor(file), body: fs.readFileSync(file) })
  }

  async function miss (route, url) {
    stats.missed.push(url)
    const file = path.join(dir, imageFileName(url))
    console.error(
      `[screens] no recorded image for ${url} (looked for ${path.relative(REPO_ROOT, file)}); ` +
      'run with --record to fetch it'
    )
    await route.fulfill({ status: 404, contentType: 'text/plain', body: 'no recorded image' })
  }

  await page.route(url => IMAGE_HOSTS.includes(url.hostname), async (route) => {
    const url = route.request().url()
    const file = path.join(dir, imageFileName(url))
    if (fs.existsSync(file)) return fulfilFile(route, file)
    if (!record) return miss(route, url)

    try {
      const recorded = await recordImage(url, { dir, userAgent })
      stats.recorded++
      console.log(`[screens] recorded ${path.relative(REPO_ROOT, recorded.file)} (${recorded.bytes} bytes)`)
      await route.fulfill({
        status: 200,
        contentType: contentTypeFor(recorded.file),
        body: fs.readFileSync(recorded.file)
      })
    } catch (err) {
      console.error(`[screens] could not record ${url}: ${err.message}`)
      await route.fulfill({ status: 404, contentType: 'text/plain', body: 'not recorded' })
    }
  })

  return { stats }
}

/** Wait until the window's `<img>` elements have all loaded (they are served from disk). */
export async function waitForImages (page, root = '#price-window') {
  await page.waitForFunction((selector) => {
    const images = [...document.querySelectorAll(`${selector} img`)]
    return images.every(image => image.complete)
  }, root, { timeout: 5_000 }).catch(() => {})
}
