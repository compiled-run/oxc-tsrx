import { execFile } from 'node:child_process'
import { createReadStream, existsSync, statSync } from 'node:fs'
import { readFile } from 'node:fs/promises'
import http from 'node:http'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.join(path.dirname(fileURLToPath(import.meta.url)), '..', '..')
const dist = path.join(root, 'docs', 'dist')
const { default: siteConfig } = await import(
  new URL('../../docs/site.config.mjs', import.meta.url)
)
// URL prefix the site lives under ('' means no path prefix). The build nests
// its output to match, so URL paths map straight into dist.
const baseSegments = (siteConfig.base ?? '/').split('/').filter(Boolean)
const basePath = baseSegments.length > 0 ? `/${baseSegments.join('/')}` : ''
const siteDist = path.join(dist, ...baseSegments)
const requireWasm = process.argv.slice(2).includes('--require-wasm')
const unsupported = process.argv.slice(2).filter((argument) => argument !== '--require-wasm')
if (unsupported.length > 0) throw new Error(`unsupported option(s): ${unsupported.join(', ')}`)

const types = {
  '.css': 'text/css; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.wasm': 'application/wasm',
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.map': 'application/json; charset=utf-8',
  '.md': 'text/markdown; charset=utf-8',
  '.png': 'image/png',
  '.svg': 'image/svg+xml',
  '.txt': 'text/plain; charset=utf-8',
  '.woff2': 'font/woff2',
  '.xml': 'application/xml; charset=utf-8',
}

function run(executable, args, options = {}) {
  return new Promise((resolve, reject) => {
    execFile(
      executable,
      args,
      {
        cwd: root,
        env: options.env ?? process.env,
        maxBuffer: 32 * 1024 * 1024,
      },
      (error, stdout, stderr) => {
        if (error) reject(new Error(stderr || stdout, { cause: error }))
        else resolve({ stdout, stderr })
      },
    )
  })
}

await run(process.execPath, ['docs/build.mjs'], {
  env: {
    ...process.env,
    ...(requireWasm ? { OXC_TSRX_REQUIRE_WASM: '1' } : {}),
  },
})

const server = http.createServer((request, response) => {
  // Mirror the deployed host: cross-origin isolation for the wasm engine.
  response.setHeader('Cross-Origin-Opener-Policy', 'same-origin')
  response.setHeader('Cross-Origin-Embedder-Policy', 'require-corp')
  const url = new URL(request.url, 'http://localhost')
  const publicPath = decodeURIComponent(url.pathname || '/')
  let file = path.join(dist, path.normalize(publicPath))
  if (!file.startsWith(dist)) {
    response.writeHead(403, { 'Content-Type': 'text/plain' }).end('Forbidden')
    return
  }
  if (existsSync(file) && statSync(file).isDirectory()) file = path.join(file, 'index.html')
  if (!existsSync(file) && !path.extname(file) && existsSync(`${file}.html`)) file = `${file}.html`
  if (!existsSync(file)) {
    response.writeHead(404, { 'Content-Type': 'text/plain' }).end('Not found')
    return
  }
  response.writeHead(200, {
    'Content-Type': types[path.extname(file)] ?? 'application/octet-stream',
    'Cache-Control': 'no-cache',
  })
  createReadStream(file).pipe(response)
})

await new Promise((resolve, reject) => {
  server.once('error', reject)
  server.listen(0, '127.0.0.1', resolve)
})

try {
  const address = server.address()
  const baseUrl = `http://127.0.0.1:${address.port}${basePath}`
  // The built site self-describes its demo mode: 'wasm' when the in-browser
  // engine was bundled (npm run docs:wasm), 'static' otherwise.
  const capabilities = JSON.parse(
    await readFile(path.join(siteDist, 'demo-capabilities.json'), 'utf8'),
  )
  if (requireWasm && capabilities.mode !== 'wasm') {
    throw new Error(`required wasm verification built ${capabilities.mode} mode`)
  }
  const { stdout, stderr } = await run(process.execPath, [
    'docs/verify.mjs',
    baseUrl,
    `--mode=${capabilities.mode === 'wasm' ? 'wasm' : 'static'}`,
  ])
  process.stdout.write(stdout)
  process.stderr.write(stderr)
} finally {
  await new Promise((resolve, reject) =>
    server.close((error) => (error ? reject(error) : resolve())),
  )
}
