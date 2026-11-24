// A fake main process for a reference renderer running in a normal browser.
//
// The renderer was written for Electron but also supports "Open in Browser":
// it fetches `/config` (HostState JSON), connects a websocket to `/events`, and
// proxies every trade/poe.ninja request through `/proxy/<host>/<path>`. This
// module serves exactly those endpoints plus the built renderer from disk.
//
// Proxy answers are files under corpus/screens/<game>/proxy/, keyed by the
// URL after `/proxy/` (see `proxyFileName`). GET and POST are both supported.
import fs from 'node:fs'
import { createServer } from 'node:http'
import path from 'node:path'
import { WebSocket, WebSocketServer } from 'ws'

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
export function proxyFileName (rawUrl) {
  const queryAt = rawUrl.indexOf('?')
  const pathname = queryAt === -1 ? rawUrl : rawUrl.slice(0, queryAt)
  const query = queryAt === -1 ? '' : rawUrl.slice(queryAt + 1)

  const sanitize = (text) => text
    .replace(/\//g, '__')
    .replace(/\?/g, '--')
    .replace(/&/g, '-')
    .replace(/=/g, '_')
    .replace(/[^A-Za-z0-9._-]+/g, '_')

  const name = sanitize(pathname)
  return query ? `${name}--${sanitize(query)}` : name
}

function sendJson (res, status, body) {
  res.writeHead(status, { 'content-type': 'application/json; charset=utf-8' })
  res.end(JSON.stringify(body))
}

/**
 * @param {object} options
 * @param {string} options.buildDir     built renderer (has index.html)
 * @param {object} options.config      decoded Config; served as `contents`
 * @param {string} options.proxyDir    directory with proxy fixture files
 * @param {boolean} [options.verbose]  log every proxy request, not just misses
 */
export async function startHost ({ buildDir, config, proxyDir, verbose = false }) {
  const clients = new Set()
  const clientWaiters = []
  const requestWaiters = []
  let lastClient = null

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
      handleProxy(req, res)
      return
    }

    if (req.method !== 'GET' && req.method !== 'HEAD') {
      sendJson(res, 405, {})
      return
    }
    serveStatic(pathname, res, req.method === 'HEAD')
  }

  function handleProxy (req, res) {
    const raw = req.url.replace(/^\/proxy\//, '')
    for (let i = requestWaiters.length - 1; i >= 0; i--) {
      if (raw.includes(requestWaiters[i].match)) {
        const [{ resolve }] = requestWaiters.splice(i, 1)
        resolve(raw)
      }
    }

    const key = proxyFileName(raw)
    const candidates = [`${key}.${req.method}.json`, `${key}.json`]
    const file = candidates
      .map(name => path.join(proxyDir, name))
      .find(candidate => fs.existsSync(candidate))

    if (file) {
      if (verbose) console.error(`[screens] proxy ${req.method} ${raw} -> ${path.basename(file)}`)
      res.writeHead(200, { 'content-type': 'application/json; charset=utf-8' })
      fs.createReadStream(file).pipe(res)
      return
    }

    if (verbose) console.error(`[screens] proxy ${req.method} ${raw} -> 404 (no fixture)`)
    else console.error(`[screens] no proxy fixture for ${req.method} ${raw} (looked for ${candidates.join(', ')})`)
    sendJson(res, 404, {})
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
