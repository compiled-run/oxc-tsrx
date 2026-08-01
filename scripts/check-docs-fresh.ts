import { execFile } from 'node:child_process'
import { createHash } from 'node:crypto'
import { mkdtemp, readFile, readdir, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.join(path.dirname(fileURLToPath(import.meta.url)), '..')
const committed = path.join(root, 'docs', 'dist')
const temporary = await mkdtemp(path.join(tmpdir(), 'oxc-tsrx-docs-fresh-'))

function runBuild(outDir) {
  return new Promise((resolve, reject) => {
    execFile(
      process.execPath,
      ['docs/build.mjs'],
      {
        cwd: root,
        env: { ...process.env, OXC_TSRX_DOCS_OUT_DIR: outDir },
        maxBuffer: 32 * 1024 * 1024,
      },
      (error, stdout, stderr) => {
        if (error) reject(new Error(stderr || stdout, { cause: error }))
        else resolve(stdout)
      },
    )
  })
}

async function inventory(directory) {
  const entries = new Map()
  async function visit(current) {
    for (const entry of await readdir(current, { withFileTypes: true })) {
      const absolute = path.join(current, entry.name)
      if (entry.isDirectory()) {
        await visit(absolute)
        continue
      }
      const relative = path.relative(directory, absolute).split(path.sep).join('/')
      const contents = await readFile(absolute)
      entries.set(relative, createHash('sha256').update(contents).digest('hex'))
    }
  }
  await visit(directory)
  return entries
}

try {
  await runBuild(temporary)
  const [expected, actual] = await Promise.all([inventory(temporary), inventory(committed)])
  const names = [...new Set([...expected.keys(), ...actual.keys()])].sort()
  const drift = names.filter((name) => expected.get(name) !== actual.get(name))
  if (drift.length > 0) {
    console.error('docs/dist is stale; run pnpm run docs:build')
    for (const name of drift) {
      const status = !actual.has(name) ? 'missing' : !expected.has(name) ? 'unexpected' : 'changed'
      console.error(`- ${status}: ${name}`)
    }
    process.exitCode = 1
  } else {
    console.log(`verified ${actual.size} generated documentation files`)
  }
} finally {
  await rm(temporary, { recursive: true, force: true })
}
