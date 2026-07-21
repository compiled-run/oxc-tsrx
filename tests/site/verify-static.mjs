import { execFile } from 'node:child_process'
import { createReadStream, existsSync, statSync } from 'node:fs'
import http from 'node:http'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.join(path.dirname(fileURLToPath(import.meta.url)), '..', '..')
const dist = path.join(root, 'docs', 'dist')
// The site is served at the root ('' means no path prefix).
const basePath = ''

const types = {
  '.css': 'text/css; charset=utf-8',
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

function run(executable, args) {
  return new Promise((resolve, reject) => {
    execFile(
      executable,
      args,
      { cwd: root, maxBuffer: 32 * 1024 * 1024 },
      (error, stdout, stderr) => {
        if (error) reject(new Error(stderr || stdout, { cause: error }))
        else resolve({ stdout, stderr })
      },
    )
  })
}

await run(process.execPath, ['docs/build.mjs'])

const server = http.createServer((request, response) => {
  const url = new URL(request.url, 'http://localhost')
  if (basePath && url.pathname === '/') {
    response.writeHead(302, { Location: `${basePath}/` }).end()
    return
  }
  if (basePath && url.pathname !== basePath && !url.pathname.startsWith(`${basePath}/`)) {
    response.writeHead(404, { 'Content-Type': 'text/plain' }).end('Not found')
    return
  }
  const publicPath = decodeURIComponent(url.pathname.slice(basePath.length) || '/')
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
  const { stdout, stderr } = await run(process.execPath, [
    'docs/verify.mjs',
    baseUrl,
    '--mode=static',
  ])
  process.stdout.write(stdout)
  process.stderr.write(stderr)
} finally {
  await new Promise((resolve, reject) =>
    server.close((error) => (error ? reject(error) : resolve())),
  )
}
