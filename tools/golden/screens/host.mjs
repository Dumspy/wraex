// A fake main process for a reference renderer running in a normal browser.
//
// The renderer was written for Electron but also supports "Open in Browser":
// it fetches `/config` (HostState JSON), connects a websocket to `/events`, and
// proxies every trade/poe.ninja request through `/proxy/<host>/<path>`. This
// module serves exactly those endpoints plus the built renderer from disk.
//
// Proxy answers are files under corpus/screens/<game>/proxy/, keyed by the
// URL after `/proxy/` (see `proxyFileName`). GET and POST are both supported.
//
// With `record: true` a request with no file is forwarded to the real host
// (only the ones upstream's proxy allows) and the answer is written next to the
// existing fixtures, so later runs replay it offline. A forwarded answer is
// stored as raw bytes in `<name>.<METHOD>.json`; a sibling
// `<name>.<METHOD>.meta.json` holds the status and the headers the page reads,
// as `{ "status": 200, "headers": { ... } }`. Files without a meta sibling
// replay as `200` with the default content type.
import fs from 'node:fs'
import https from 'node:https'
import { createServer } from 'node:http'
import { createHash } from 'node:crypto'
import path from 'node:path'
import zlib from 'node:zlib'
import { WebSocket, WebSocketServer } from 'ws'

// The hosts upstream's `/proxy/` forwards to: `PROXY_HOSTS` in
// `ref/apt/main/src/proxy.ts` (PoE1) and `ref/ee2/main/src/proxy.ts` (PoE2). A
// recording is only ever sent to these, with HTTPS.
export const PROXY_HOSTS = {
  poe1: [
    'www.pathofexile.com',
    'ru.pathofexile.com',
    'pathofexile.tw',
    'poe.kakaogames.com',
    'poe.ninja',
    'www.poeprices.info'
  ],
  poe2: [
    'www.pathofexile.com',
    'ru.pathofexile.com',
    'pathofexile.tw',
    'poe.kakaogames.com',
    'jp.pathofexile.com',
    'de.pathofexile.com',
    'es.pathofexile.com',
    'br.pathofexile.com',
    'fr.pathofexile.com',
    'poe.ninja',
    'www.poeprices.info',
    'api.exiledexchange2.dev'
  ]
}

// At most one forwarded request per second, whatever the page asks for. The
// page throttles itself like upstream, but a fresh install starts with no
// `X-Rate-Limit-*` state and fires its first requests at once.
const MIN_FORWARD_INTERVAL_MS = 1000

// The leagues answers are hand-written (a small, stable shape), so recording
// must never replace them; a miss for one stays a miss.
const NEVER_RECORD = [/\/api\/(?:trade2\/data\/)?leagues(?:\?|$)/]

// Response headers a replay keeps. The page only reads `x-rate-limit-*` (and
// `retry-after`); `content-type` decides whether the body is parsed as JSON.
const KEPT_HEADERS = [/^content-type$/, /^retry-after$/, /^x-rate-limit-/]

// Hop-by-hop headers, plus the ones we decode or recompute ourselves.
const DROPPED_HEADERS = new Set([
  'connection',
  'keep-alive',
  'transfer-encoding',
  'content-encoding',
  'content-length',
  'set-cookie'
])

const MIME = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.map': 'application/json; charset=utf-8',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.jpeg': 'image/jpeg',
  '.gif': 'image/gif',
  '.webp': 'image/webp',
  '.ico': 'image/x-icon',
  '.woff': 'font/woff',
  '.woff2': 'font/woff2',
  '.ttf': 'font/ttf',
  '.otf': 'font/otf',
  '.bin': 'application/octet-stream',
  '.txt': 'text/plain; charset=utf-8'
}

// `www.pathofexile.com/api/leagues?type=main&realm=pc`
//   -> `www.pathofexile.com__api__leagues--type_main-realm_pc`
// `/` -> `__`, `?` -> `--`, `&` -> `-`, `=` -> `_`, anything else outside
// [A-Za-z0-9._-] -> `_`. Query text is sanitized with the same rules.
//
// A request with a body (the trade search and exchange endpoints answer by body)
// gets `--body<8 hex of sha1(body)>` appended, so two searches of the same URL
// are two files. A name longer than 100 characters (a trade fetch URL lists the
// ids it wants) is cut to 64 and gets
// `--len<length>-<16 hex of sha1(name)>`: Windows limits a whole path to
// `MAX_PATH` (260 characters), not just one component to 255 bytes, so the
// prefix stays short; the name is a pure function of the URL and body, so
// recording and replay agree.
const MAX_NAME = 100
const CUT_NAME = 64

export function proxyFileName (rawUrl, { body } = {}) {
  const queryAt = rawUrl.indexOf('?')
  const pathname = queryAt === -1 ? rawUrl : rawUrl.slice(0, queryAt)
  const query = queryAt === -1 ? '' : rawUrl.slice(queryAt + 1)

  const sanitize = (text) => text
    .replace(/\//g, '__')
    .replace(/\?/g, '--')
    .replace(/&/g, '-')
    .replace(/=/g, '_')
    .replace(/[^A-Za-z0-9._-]+/g, '_')

  const hash = (value) => createHash('sha1').update(value).digest('hex')

  let name = sanitize(pathname)
  if (query) name += `--${sanitize(query)}`
  if (body?.length) name += `--body${hash(body).slice(0, 8)}`
  if (name.length > MAX_NAME) {
    name = `${name.slice(0, CUT_NAME)}--len${name.length}-${hash(name).slice(0, 16)}`
  }
  return name
}

function sendJson (res, status, body) {
  res.writeHead(status, { 'content-type': 'application/json; charset=utf-8' })
  res.end(JSON.stringify(body))
}

// The headers upstream's proxy keeps when it forwards a request: everything the
// page sent except the browser-only `sec-*` and the ones a server sets itself.
// `user-agent` is replaced with the app's (upstream sends Electron's),
// `referer` (a localhost URL) is dropped, and `accept-encoding: identity` asks
// for the plain body, so what is stored is the text the page parses.
function forwardHeaders (incoming, userAgent) {
  const headers = {}
  for (const [name, value] of Object.entries(incoming)) {
    const key = name.toLowerCase()
    if (key.startsWith('sec-')) continue
    if (['host', 'origin', 'content-length', 'connection', 'transfer-encoding',
      'accept-encoding', 'referer', 'user-agent'].includes(key)) continue
    headers[key] = value
  }
  headers['user-agent'] = userAgent
  headers['accept-encoding'] = 'identity'
  return headers
}

// What the page gets back: the host's headers minus the hop-by-hop ones, so a
// recorded answer and its replay hand the page the same thing.
function pageHeaders (headers) {
  const out = {}
  for (const [name, value] of Object.entries(headers)) {
    const key = name.toLowerCase()
    if (DROPPED_HEADERS.has(key)) continue
    out[key] = value
  }
  return out
}

function keptHeaders (headers) {
  const out = {}
  for (const [name, value] of Object.entries(headers)) {
    const key = name.toLowerCase()
    if (!KEPT_HEADERS.some(pattern => pattern.test(key))) continue
    out[key] = Array.isArray(value) ? value.join(', ') : String(value)
  }
  return out
}

function decodeBody (body, headers) {
  const encoding = String(headers['content-encoding'] ?? '').toLowerCase()
  if (!encoding || encoding === 'identity') return body
  if (encoding === 'gzip' || encoding === 'x-gzip') return zlib.gunzipSync(body)
  if (encoding === 'deflate') return zlib.inflateSync(body)
  if (encoding === 'br') return zlib.brotliDecompressSync(body)
  throw new Error(`unsupported content-encoding ${encoding}`)
}

function readBody (req) {
  return new Promise((resolve, reject) => {
    const chunks = []
    req.on('data', chunk => chunks.push(chunk))
    req.on('end', () => resolve(Buffer.concat(chunks)))
    req.on('error', reject)
  })
}

// One HTTPS request to the real host. Returns the decoded body, so the stored
// fixture is the same text the page would have parsed.
function forwardOnce (url, method, headers, body) {
  return new Promise((resolve, reject) => {
    const req = https.request(url, { method, headers }, (res) => {
      const chunks = []
      res.on('data', chunk => chunks.push(chunk))
      res.on('error', reject)
      res.on('end', () => {
        try {
          resolve({
            status: res.statusCode,
            headers: pageHeaders(res.headers),
            body: decodeBody(Buffer.concat(chunks), res.headers)
          })
        } catch (err) {
          reject(err)
        }
      })
    })
    req.on('error', reject)
    if (body?.length) req.write(body)
    req.end()
  })
}

const metaFileFor = (bodyFile) => bodyFile.replace(/\.json$/, '.meta.json')

function readMeta (bodyFile) {
  try {
    const meta = JSON.parse(fs.readFileSync(metaFileFor(bodyFile), 'utf8'))
    return {
      status: Number.isInteger(meta.status) ? meta.status : 200,
      headers: meta.headers ?? {}
    }
  } catch {
    return { status: 200, headers: {} }
  }
}

/**
 * @param {object} options
 * @param {string} options.buildDir     built renderer (has index.html)
 * @param {object} options.config      decoded Config; served as `contents`
 * @param {string} options.proxyDir    directory with proxy fixture files
 * @param {boolean} [options.verbose]  log every proxy request, not just misses
 * @param {boolean} [options.record]   forward proxy misses and save the answers
 * @param {string[]} [options.allowedHosts]  hosts `record` may forward to
 * @param {string} [options.userAgent]       User-Agent for recorded requests
 */
export async function startHost ({
  buildDir,
  config,
  proxyDir,
  verbose = false,
  record = false,
  allowedHosts = [],
  userAgent = ''
}) {
  const clients = new Set()
  const clientWaiters = []
  const requestWaiters = []
  let lastClient = null

  // Proxy bookkeeping: what a run did, and when the page was last asking.
  const stats = { replayed: 0, recorded: 0, forwarded: 0 }
  const failures = []
  const errors = []
  let lastProxyAt = Date.now()
  let forwarding = 0
  let stopped = false
  let lastForwardAt = 0
  let rateChain = Promise.resolve()

  // One forwarded request per MIN_FORWARD_INTERVAL_MS, however many the page
  // sends at once: each caller waits for the slot after the previous one.
  function throttle () {
    const slot = rateChain.then(async () => {
      const wait = Math.max(0, lastForwardAt + MIN_FORWARD_INTERVAL_MS - Date.now())
      if (wait > 0) await new Promise(resolve => setTimeout(resolve, wait))
      lastForwardAt = Date.now()
    })
    rateChain = slot.catch(() => {})
    return slot
  }

  const hostState = {
    version: '0.0.0',
    updater: { state: 'initial' },
    contents: JSON.stringify(config)
  }

  const wss = new WebSocketServer({ noServer: true })
  wss.on('connection', (ws) => {
    clients.add(ws)
    lastClient = ws
    ws.on('close', () => clients.delete(ws))
    for (const resolve of clientWaiters.splice(0)) resolve(ws)
    // The real server greets every client with its log history.
    ws.send(JSON.stringify({ name: 'MAIN->CLIENT::log-entry', payload: { message: '' } }))
  })

  function handleRequest (req, res) {
    const url = new URL(req.url, 'http://127.0.0.1')
    const pathname = decodeURIComponent(url.pathname)

    if (pathname === '/config') {
      sendJson(res, 200, hostState)
      return
    }

    if (pathname.startsWith('/proxy/')) {
      handleProxy(req, res).catch((err) => {
        console.error(`[screens] proxy ${req.method} ${req.url} failed: ${err.message}`)
        if (!res.headersSent) sendJson(res, 500, {})
        else res.destroy(err)
      })
      return
    }

    if (req.method !== 'GET' && req.method !== 'HEAD') {
      sendJson(res, 405, {})
      return
    }
    serveStatic(pathname, res, req.method === 'HEAD')
  }

  function replayFile (req, res, raw, file) {
    const { status, headers } = readMeta(file)
    stats.replayed++
    if (verbose) {
      console.error(`[screens] proxy ${req.method} ${raw} -> ${path.basename(file)} (${status})`)
    }
    res.writeHead(status, { 'content-type': 'application/json; charset=utf-8', ...headers })
    fs.createReadStream(file).pipe(res)
  }

  async function handleProxy (req, res) {
    const raw = req.url.replace(/^\/proxy\//, '')
    lastProxyAt = Date.now()
    for (let i = requestWaiters.length - 1; i >= 0; i--) {
      if (raw.includes(requestWaiters[i].match)) {
        const [{ resolve }] = requestWaiters.splice(i, 1)
        resolve(raw)
      }
    }

    // A POST body decides the answer (trade search/exchange), so the name has
    // to include it: read it before looking for a file.
    const body = req.method === 'GET' || req.method === 'HEAD' ? null : await readBody(req)
    const key = proxyFileName(raw, { body })
    const candidates = [`${key}.${req.method}.json`, `${key}.json`]
    const file = findFixture(candidates)

    if (file) {
      replayFile(req, res, raw, file)
      return
    }

    if (!record) {
      if (verbose) console.error(`[screens] proxy ${req.method} ${raw} -> 404 (no fixture)`)
      else console.error(`[screens] no proxy fixture for ${req.method} ${raw} (looked for ${candidates.join(', ')})`)
      sendJson(res, 404, {})
      return
    }

    await recordMiss(req, res, raw, key, body)
  }

  function findFixture (candidates) {
    return candidates
      .map(name => path.join(proxyDir, name))
      .find(candidate => fs.existsSync(candidate))
  }

  async function recordMiss (req, res, raw, key, body) {
    const slash = raw.indexOf('/')
    const host = slash === -1 ? raw : raw.slice(0, slash)

    if (stopped) {
      console.error(`[screens] not recording ${req.method} ${raw}: the run is stopped`)
      sendJson(res, 503, {})
      return
    }
    if (NEVER_RECORD.some(pattern => pattern.test(raw))) {
      console.error(`[screens] not recording ${req.method} ${raw}: that answer is hand-written`)
      sendJson(res, 404, {})
      return
    }
    if (!allowedHosts.includes(host)) {
      console.error(`[screens] not recording ${req.method} ${raw}: ${host} is not in upstream's proxy allowlist`)
      sendJson(res, 403, {})
      return
    }

    const headers = forwardHeaders(req.headers, userAgent)
    const bodyFile = path.join(proxyDir, `${key}.${req.method}.json`)
    let answer
    forwarding++
    try {
      await throttle()
      // The page often asks for the same URL twice at once (a widget and its
      // chart); the first request may have written the file while this one
      // waited for its slot, so replay it instead of asking again.
      if (fs.existsSync(bodyFile)) {
        replayFile(req, res, raw, bodyFile)
        return
      }
      stats.forwarded++
      answer = await forwardOnce(`https://${raw}`, req.method, headers, body)
    } catch (err) {
      failures.push(`${req.method} ${raw}: ${err.message}`)
      console.error(`[screens] recording ${req.method} ${raw} failed: ${err.message}`)
      sendJson(res, 502, {})
      return
    } finally {
      forwarding--
    }

    if (answer.status === 429) {
      // The page backs off on its own, but the run must not keep hammering the
      // real host: hand the answer over and stop.
      stats.recorded++
      stopped = true
      errors.push(`the real host answered 429 Too Many Requests to ${req.method} ${raw}`)
      console.error(`[screens] 429 from ${host} for ${req.method} ${raw}; stopping`)
      res.writeHead(429, { 'content-type': 'application/json; charset=utf-8', ...answer.headers })
      res.end(answer.body)
      return
    }

    if (answer.status >= 400) {
      // Not written: a replayed error would hide the problem forever.
      failures.push(`${req.method} ${raw}: HTTP ${answer.status}`)
      console.error(`[screens] recording ${req.method} ${raw} got HTTP ${answer.status} (not saved)`)
    } else {
      const meta = { status: answer.status, headers: keptHeaders(answer.headers) }
      fs.mkdirSync(proxyDir, { recursive: true })
      fs.writeFileSync(bodyFile, answer.body)
      fs.writeFileSync(metaFileFor(bodyFile), `${JSON.stringify(meta, null, 2)}\n`)
      stats.recorded++
      console.error(
        `[screens] recorded ${req.method} ${raw} -> ${path.basename(bodyFile)} ` +
        `(${answer.status}, ${answer.body.length} bytes)`
      )
    }

    res.writeHead(answer.status, { 'content-type': 'application/json; charset=utf-8', ...answer.headers })
    res.end(answer.body)
  }

  function serveStatic (pathname, res, headOnly) {
    const root = path.resolve(buildDir)
    let file = path.resolve(root, `.${pathname}`)
    if (file !== root && !file.startsWith(root + path.sep)) {
      sendJson(res, 403, {})
      return
    }

    let stat = null
    try {
      stat = fs.statSync(file)
    } catch {}

    if (!stat || stat.isDirectory()) {
      // SPA fallback; asset requests that are missing stay 404-ish by serving
      // the shell anyway, which vite's pages never do.
      file = path.join(root, 'index.html')
      try {
        stat = fs.statSync(file)
      } catch {
        sendJson(res, 404, {})
        return
      }
    }

    const type = MIME[path.extname(file).toLowerCase()] ?? 'application/octet-stream'
    res.writeHead(200, {
      'content-type': type,
      'content-length': stat.size,
      'cache-control': 'no-store'
    })
    if (headOnly) res.end()
    else fs.createReadStream(file).pipe(res)
  }

  const server = createServer(handleRequest)
  server.on('upgrade', (req, socket, head) => {
    const pathname = new URL(req.url, 'http://127.0.0.1').pathname
    if (pathname !== '/events') {
      socket.destroy()
      return
    }
    wss.handleUpgrade(req, socket, head, (ws) => wss.emit('connection', ws, req))
  })

  await new Promise((resolve, reject) => {
    server.once('error', reject)
    server.listen({ port: 0, host: '127.0.0.1' }, resolve)
  })
  const { port } = server.address()

  return {
    url: `http://127.0.0.1:${port}/`,

    /** Resolves on the next `/events` connection (register before navigating). */
    nextClient () {
      return new Promise(resolve => clientWaiters.push(resolve))
    },

    /** Resolves with the raw URL of the next proxied request containing `match`. */
    nextProxyRequest (match) {
      return new Promise(resolve => requestWaiters.push({ match, resolve }))
    },

    /**
     * Resolves once the page has not asked the proxy for `quietMs`, so a
     * throttled chain of recorded requests has finished. `true` on quiet, and
     * `false` when it timed out.
     */
    async settled ({ quietMs = 1500, timeoutMs = 120_000 } = {}) {
      const deadline = Date.now() + timeoutMs
      while (Date.now() < deadline) {
        if (forwarding === 0 && Date.now() - lastProxyAt >= quietMs) return true
        await new Promise(resolve => setTimeout(resolve, 50))
      }
      return false
    },

    /** What happened through `/proxy/` so far. */
    stats,
    /** Fatal recording problems (a 429 stops the run). */
    errors,
    /** Requests that could not be recorded, apart from 429. */
    failures,

    /**
     * Replaces the state `/config` serves, for a case that needs a config upstream's own
     * UI cannot reach in a browser (`screens/widgets.mjs`'s delve grid).
     */
    setConfig (next) {
      hostState.contents = JSON.stringify(next)
    },

    /** Sends one IPC event to the most recently connected client. */
    sendEvent (event) {
      if (!lastClient || lastClient.readyState !== WebSocket.OPEN) {
        throw new Error('no open websocket client')
      }
      lastClient.send(JSON.stringify(event))
    },

    async close () {
      for (const ws of wss.clients) ws.terminate()
      wss.close()
      server.closeAllConnections?.()
      await new Promise(resolve => server.close(resolve))
    }
  }
}
