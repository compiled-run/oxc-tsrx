// Like-for-like ecosystem benchmark: lint one deterministic 1,000-file TSX
// application with ESLint, official Oxlint, and OXC for TSRX. All three use
// the same explicit file list, the same no-debugger rule, zero-diagnostic
// default output, and the same launch boundary: every lane runs through its
// npm CLI entry point, exactly as a project would invoke it. The OXC for TSRX
// lane is the oxc-tsrx `oxlint` launcher. Proven ordinary-only lists enter the exact
// binary declared by oxlint-current in the launcher process; mixed lists use
// official Oxlint plus the native TSRX lane. A separate paired workload
// measures the product's own all-TSX versus 20%-TSRX overhead; it is not a
// cross-tool speed comparison.
//
// Run: node benchmarks/comparative/run.mjs [--assert]
import { execFileSync, spawnSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { cpus, release as osRelease } from 'node:os'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const here = path.dirname(fileURLToPath(import.meta.url))
const repoRoot = path.join(here, '..', '..')
const budgetsPath = path.join(here, 'budgets.json')
const budgets = JSON.parse(readFileSync(budgetsPath, 'utf8'))
const FILES = budgets.files
const TSRX_SHARE = budgets.tsrxShare
const RULES = ['no-debugger']
const OXC_REVISION = readFileSync(path.join(repoRoot, 'crates', 'oxc_adapter', 'Cargo.toml'), 'utf8')
  .match(/rev = "([0-9a-f]{40})"/u)?.[1]
if (!OXC_REVISION) throw new Error('cannot resolve the canonical OXC revision')

const samplePolicy = {
  warmups: budgets.warmups,
  measured: budgets.samples,
  percentile: 'nearest-rank',
}
if (samplePolicy.warmups < 5 || samplePolicy.measured < 20) {
  throw new Error('comparative sampling weakens the frozen 5-warmup/20-measurement policy')
}

const NOUNS = ['User', 'Cart', 'Order', 'Task', 'Post', 'Invoice', 'Report', 'Team', 'Session', 'Widget']
const FIELDS = ['id', 'name', 'title', 'count', 'status', 'ready', 'items', 'tags', 'total', 'owner']

function specification(index) {
  return {
    index,
    noun: NOUNS[index % NOUNS.length],
    field: FIELDS[(index * 7 + 3) % FIELDS.length],
    minimumLength: 1 + (index % 4),
    classIndex: index % 7,
  }
}

function componentTsx(spec) {
  return `export type ${spec.noun}${spec.index}Props = {
  title: string;
  items: string[];
  ready: boolean;
  ${spec.field}?: number;
};

export function ${spec.noun}Card${spec.index}({ title, items, ready }: ${spec.noun}${spec.index}Props) {
  const visible = items.filter((item) => item.length > ${spec.minimumLength});
  return (
    <section className="card-${spec.classIndex}">
      <h2>{title}</h2>
      {ready ? (
        <ul>
          {visible.map((label, position) => (
            <li key={position}>{label}</li>
          ))}
        </ul>
      ) : (
        <p>Loading {title}…</p>
      )}
    </section>
  );
}
`
}

function componentTsrx(spec) {
  return `export type ${spec.noun}${spec.index}Props = {
  title: string;
  items: string[];
  ready: boolean;
  ${spec.field}?: number;
};

export function ${spec.noun}Card${spec.index}({ title, items, ready }: ${spec.noun}${spec.index}Props) @{
  const visible = items.filter((item) => item.length > ${spec.minimumLength});
  <section class="card-${spec.classIndex}">
    <h2>{title}</h2>
    @if (ready) {
      <ul>
        @for (const label of visible; index position; key label) {
          <li>{label}</li>;
        }
      </ul>;
    } @else {
      <p>Loading {title}…</p>;
    }
  </section>;
}
`
}

function digestFiles(entries) {
  const hash = createHash('sha256')
  for (const { name, content } of entries) hash.update(name).update('\0').update(content).update('\0')
  return hash.digest('hex')
}

function writeCorpus(directory, mixed) {
  rmSync(directory, { recursive: true, force: true })
  mkdirSync(directory, { recursive: true })
  const entries = []
  for (let index = 0; index < FILES; index += 1) {
    const spec = specification(index)
    const useTsrx = mixed && index % Math.round(1 / TSRX_SHARE) === 0
    const name = `component-${String(index).padStart(4, '0')}.${useTsrx ? 'tsrx' : 'tsx'}`
    const content = useTsrx ? componentTsrx(spec) : componentTsx(spec)
    writeFileSync(path.join(directory, name), content)
    entries.push({ name, content })
  }
  return {
    files: entries.map(({ name }) => name),
    bytes: entries.reduce((total, { content }) => total + Buffer.byteLength(content), 0),
    sha256: digestFiles(entries),
  }
}

function percentile(values, quantile) {
  const sorted = [...values].sort((left, right) => left - right)
  return sorted[Math.min(sorted.length - 1, Math.ceil(sorted.length * quantile) - 1)]
}

function runProcess(label, command, args, cwd, stdio = 'ignore', extraEnv = {}) {
  const result = spawnSync(command, args, {
    cwd,
    encoding: stdio === 'pipe' ? 'utf8' : undefined,
    stdio,
    maxBuffer: 64 * 1024 * 1024,
    env: { ...process.env, CI: '1', NO_COLOR: '1', ...extraEnv },
  })
  if (result.error) throw new Error(`${label}: ${result.error.message}`)
  if (result.status !== 0) {
    throw new Error(`${label}: exit ${result.status}${result.stderr ? `\n${result.stderr.slice(-4000)}` : ''}`)
  }
  return result
}

function measure(label, command, args, cwd, extraEnv = {}) {
  const warmupMs = []
  const rawMs = []
  for (let attempt = 0; attempt < samplePolicy.warmups + samplePolicy.measured; attempt += 1) {
    const started = process.hrtime.bigint()
    runProcess(label, command, args, cwd, 'ignore', extraEnv)
    const elapsedMs = Number(process.hrtime.bigint() - started) / 1e6
    ;(attempt < samplePolicy.warmups ? warmupMs : rawMs).push(elapsedMs)
  }
  const summary = {
    warmupMs,
    rawMs,
    medianMs: percentile(rawMs, 0.5),
    p95Ms: percentile(rawMs, 0.95),
  }
  console.log(
    `${label.padEnd(34)} median ${summary.medianMs.toFixed(1)} ms · p95 ${summary.p95Ms.toFixed(1)} ms`,
  )
  return summary
}

function version(binary, flag = '--version') {
  try {
    return execFileSync(binary, [flag], { encoding: 'utf8' }).trim().split('\n')[0]
  } catch {
    return 'unknown'
  }
}

function parseOxcValidation(label, result) {
  const parsed = JSON.parse(result.stdout)
  return {
    files: parsed.number_of_files,
    diagnostics: parsed.diagnostics?.length ?? 0,
  }
}

function readTrace(pathname) {
  if (!existsSync(pathname)) return []
  return readFileSync(pathname, 'utf8')
    .trim()
    .split('\n')
    .filter(Boolean)
    .map((line) => JSON.parse(line))
}

function validateLanes({ eslintBin, oxlintBin, oxcTsrxBin, nativeLintBin, oxcTsrxEnv, tsxDir, mixedDir, tsxFiles, mixedFiles }) {
  const ordinaryTrace = path.join(tsxDir, '.oxc-tsrx-route.jsonl')
  const mixedTrace = path.join(mixedDir, '.oxc-tsrx-route.jsonl')
  const eslintResult = runProcess(
    'ESLint validation',
    eslintBin,
    ['--config', 'eslint.config.mjs', '--no-ignore', '--no-warn-ignored', '--format', 'json', ...tsxFiles],
    tsxDir,
    'pipe',
  )
  const eslintJson = JSON.parse(eslintResult.stdout)
  const validation = {
    eslint: {
      files: eslintJson.length,
      diagnostics: eslintJson.reduce((total, entry) => total + entry.messages.length, 0),
    },
    oxlint: parseOxcValidation(
      'Oxlint validation',
      runProcess(
        'Oxlint validation',
        oxlintBin,
        ['--allow', 'all', '--deny', RULES[0], '--no-ignore', '--format', 'json', ...tsxFiles],
        tsxDir,
        'pipe',
      ),
    ),
    oxcTsrx: parseOxcValidation(
      'OXC for TSRX validation',
      runProcess(
        'OXC for TSRX validation',
        oxcTsrxBin,
        ['--allow', 'all', '--deny', RULES[0], '--no-ignore', '--format=json', ...tsxFiles],
        tsxDir,
        'pipe',
        { ...oxcTsrxEnv, OXC_TSRX_TRACE_FILE: ordinaryTrace },
      ),
    ),
    oxcTsrxMixed: parseOxcValidation(
      'OXC for TSRX mixed validation',
      runProcess(
        'OXC for TSRX mixed validation',
        oxcTsrxBin,
        ['--allow', 'all', '--deny', RULES[0], '--no-ignore', '--format=json', ...mixedFiles],
        mixedDir,
        'pipe',
        { ...oxcTsrxEnv, OXC_TSRX_TRACE_FILE: mixedTrace },
      ),
    ),
  }
  for (const [lane, observed] of Object.entries(validation)) {
    if (observed.files !== FILES || observed.diagnostics !== 0) {
      throw new Error(`${lane}: expected ${FILES} files and zero diagnostics, got ${JSON.stringify(observed)}`)
    }
  }
  const ordinaryEvents = readTrace(ordinaryTrace)
  const mixedStarts = readTrace(mixedTrace).filter((event) => event.event === 'start')
  const publicCanonicalChildren = mixedStarts.filter(
    (event) =>
      event.executable === process.execPath &&
      path.resolve(event.args?.[0] ?? '') === path.resolve(oxlintBin),
  )
  const nativeTsrxChildren = mixedStarts.filter(
    (event) => path.resolve(event.executable) === path.resolve(nativeLintBin),
  )
  const privateAdapterChildren = mixedStarts.filter((event) =>
    String(event.executable).startsWith('in-process:'),
  )
  if (ordinaryEvents.length !== 0) {
    throw new Error('ordinary npm validation entered the TSRX process-dispatch layer')
  }
  if (
    mixedStarts.length !== 2 ||
    publicCanonicalChildren.length !== 1 ||
    nativeTsrxChildren.length !== 1 ||
    privateAdapterChildren.length !== 0
  ) {
    throw new Error('mixed validation did not prove the public canonical child plus native TSRX route')
  }
  validation.routeEvidence = {
    ordinaryDispatchEvents: ordinaryEvents.length,
    mixedDispatchEvents: mixedStarts.length,
    publicCanonicalNodeChildren: publicCanonicalChildren.length,
    nativeTsrxChildren: nativeTsrxChildren.length,
    privateInProcessAdapterChildren: privateAdapterChildren.length,
    mixedUsesPublicCanonicalNodeChild: publicCanonicalChildren.length === 1,
    mixedUsesNativeTsrxChild: nativeTsrxChildren.length === 1,
    mixedUsesPrivateInProcessAdapter: privateAdapterChildren.length !== 0,
  }
  return validation
}

const tsxDir = path.join(here, '.corpus-tsx')
const mixedDir = path.join(here, '.corpus-mixed')
let report
try {
  const tsx = writeCorpus(tsxDir, false)
  const mixed = writeCorpus(mixedDir, true)
  const specsSha256 = createHash('sha256')
    .update(JSON.stringify(Array.from({ length: FILES }, (_, index) => specification(index))))
    .digest('hex')
  const eslintConfig = `import tseslint from 'typescript-eslint'
export default [{
  files: ['**/*.tsx'],
  languageOptions: { parser: tseslint.parser, parserOptions: { ecmaFeatures: { jsx: true } } },
  rules: { 'no-debugger': 'error' },
}]
`
  writeFileSync(path.join(tsxDir, 'eslint.config.mjs'), eslintConfig)

  const eslintBin = path.join(repoRoot, 'node_modules', '.bin', 'eslint')
  const oxlintBin = path.join(repoRoot, 'node_modules', 'oxlint-current', 'bin', 'oxlint')
  const nativeLintBin = path.join(repoRoot, 'target', 'release', 'oxc-tsrx')
  // The product lane crosses the same boundary users do: the oxc-tsrx npm
  // launcher, pinned to the release binary under test. The all-TSX route
  // imports oxlint-current's declared CLI directly; only the mixed route also
  // launches the native TSRX binary.
  const oxcTsrxBin = path.join(repoRoot, 'node_modules', 'oxc-tsrx', 'bin', 'oxlint')
  const oxcTsrxEnv = { OXC_TSRX_LINT_BIN: nativeLintBin }
  const validation = validateLanes({
    eslintBin,
    oxlintBin,
    oxcTsrxBin,
    nativeLintBin,
    oxcTsrxEnv,
    tsxDir,
    mixedDir,
    tsxFiles: tsx.files,
    mixedFiles: mixed.files,
  })

  const eslintArgs = [
    '--config',
    'eslint.config.mjs',
    '--no-ignore',
    '--no-warn-ignored',
    ...tsx.files,
  ]
  const oxlintArgs = ['--allow', 'all', '--deny', RULES[0], '--no-ignore', ...tsx.files]
  const oxcTsrxArgs = ['--allow', 'all', '--deny', RULES[0], '--no-ignore', ...tsx.files]
  const oxcTsrxMixedArgs = ['--allow', 'all', '--deny', RULES[0], '--no-ignore', ...mixed.files]
  console.log(`corpus: ${FILES} paired components, ${(tsx.bytes / 1024).toFixed(0)} KiB TSX\n`)
  const eslint = measure('ESLint + typescript-eslint', eslintBin, eslintArgs, tsxDir)
  const oxlint = measure('official Oxlint', oxlintBin, oxlintArgs, tsxDir)
  const oxcTsrx = measure('OXC for TSRX npm CLI · TSX', oxcTsrxBin, oxcTsrxArgs, tsxDir, oxcTsrxEnv)
  const oxcTsrxMixed = measure('OXC for TSRX npm CLI · mixed file types', oxcTsrxBin, oxcTsrxMixedArgs, mixedDir, oxcTsrxEnv)

  const ratios = {
    oxcTsrxVsOxlint: oxcTsrx.medianMs / oxlint.medianMs,
    eslintVsOxcTsrx: eslint.medianMs / oxcTsrx.medianMs,
    mixedVsTsx: oxcTsrxMixed.medianMs / oxcTsrx.medianMs,
  }
  const assertions = {
    nearOxlintParity: ratios.oxcTsrxVsOxlint <= budgets.oxcTsrxVsOxlintMax,
    fasterThanEslint: ratios.eslintVsOxcTsrx >= budgets.eslintVsOxcTsrxMin,
    mixedNoBlowup: ratios.mixedVsTsx <= budgets.mixedVsTsxMax,
  }
  const assertionDetails = [
    {
      name: 'nearOxlintParity',
      observed: ratios.oxcTsrxVsOxlint,
      threshold: budgets.oxcTsrxVsOxlintMax,
      comparison: '<=',
      pass: assertions.nearOxlintParity,
    },
    {
      name: 'fasterThanEslint',
      observed: ratios.eslintVsOxcTsrx,
      threshold: budgets.eslintVsOxcTsrxMin,
      comparison: '>=',
      pass: assertions.fasterThanEslint,
    },
    {
      name: 'mixedNoBlowup',
      observed: ratios.mixedVsTsx,
      threshold: budgets.mixedVsTsxMax,
      comparison: '<=',
      pass: assertions.mixedNoBlowup,
    },
  ]
  const boundary = {
    rules: RULES,
    fileSelection: 'same explicit file list',
    output: 'zero-diagnostic default output',
    crossToolCorpus: 'byte-identical TSX files',
    launch: 'every lane measured through its npm CLI entry point, Node launcher included',
    compileCache: 'oxc-tsrx enables the Node module compile cache before routing; five fresh-process warmups precede measurements',
    productRouting: {
      allTsx: 'oxc-tsrx npm launcher -> oxlint-current declared npm binary in the same Node process',
      mixed: 'oxc-tsrx npm launcher -> public oxlint-current declared npm binary in a Node subprocess plus target/release/oxc-tsrx; the canonical child starts while the bridge loads',
    },
    executables: {
      eslint: 'node_modules/.bin/eslint',
      oxlint: 'node_modules/oxlint-current/bin/oxlint',
      oxcTsrx: 'node_modules/oxc-tsrx/bin/oxlint',
      oxcTsrxMixed: 'node_modules/oxc-tsrx/bin/oxlint',
    },
    argumentShape: {
      eslint: '--config eslint.config.mjs --no-ignore --no-warn-ignored <1,000 explicit TSX files>',
      oxlint: '--allow all --deny no-debugger --no-ignore <1,000 explicit TSX files>',
      oxcTsrx: '--allow all --deny no-debugger --no-ignore <1,000 explicit TSX files>',
      oxcTsrxMixed: '--allow all --deny no-debugger --no-ignore <1,000 explicit paired TSX/TSRX files>',
    },
    mixedComparison: 'paired generated component specifications; internal OXC for TSRX workload ratio only',
    configSha256: createHash('sha256')
      .update(eslintConfig)
      .update(JSON.stringify({ rules: RULES, selection: 'explicit', output: 'zero-diagnostic default' }))
      .digest('hex'),
  }
  report = {
    schemaVersion: 3,
    generatedAtUnixMs: Date.now(),
    timestamp: new Date().toISOString(),
    host: {
      platform: process.platform,
      arch: process.arch,
      cpu: cpus()[0]?.model ?? 'unknown',
      cores: cpus().length,
      osRelease: osRelease(),
      node: process.version,
    },
    build: {
      profile: 'release',
      binary: 'node_modules/oxc-tsrx/bin/oxlint (all TSX -> in-process oxlint-current declared bin; mixed -> public oxlint-current Node child + target/release/oxc-tsrx)',
      oxcRevision: OXC_REVISION,
    },
    corpus: {
      files: FILES,
      tsrxShare: TSRX_SHARE,
      tsxBytes: tsx.bytes,
      mixedBytes: mixed.bytes,
      tsxSha256: tsx.sha256,
      mixedSha256: mixed.sha256,
      pairedSpecificationSha256: specsSha256,
      generator: 'benchmarks/comparative/run.mjs schema 3',
    },
    boundary,
    samplePolicy,
    versions: {
      eslint: version(eslintBin),
      typescriptEslint: JSON.parse(
        readFileSync(path.join(repoRoot, 'node_modules', 'typescript-eslint', 'package.json'), 'utf8'),
      ).version,
      oxlint: version(oxlintBin),
      oxcTsrx: version(nativeLintBin, '--version'),
      oxcTsrxLauncher: `oxc-tsrx ${
        JSON.parse(readFileSync(path.join(repoRoot, 'packages', 'toolchain', 'package.json'), 'utf8')).version
      }`,
    },
    validation,
    tools: { eslint, oxlint, oxcTsrx, oxcTsrxMixed },
    ratios,
    budgets,
    assertions,
    assertionDetails,
    passed: Object.values(assertions).every(Boolean),
  }
  const output = path.join(here, `results-${report.generatedAtUnixMs}.json`)
  writeFileSync(output, `${JSON.stringify(report, null, 2)}\n`)
  console.log(
    `\nratios: OXC for TSRX / Oxlint ${ratios.oxcTsrxVsOxlint.toFixed(3)}× · ESLint / OXC for TSRX ${ratios.eslintVsOxcTsrx.toFixed(1)}× · mixed / TSX ${ratios.mixedVsTsx.toFixed(3)}×`,
  )
  console.log(`passed: ${report.passed} -> ${path.relative(repoRoot, output)}`)
} finally {
  rmSync(tsxDir, { recursive: true, force: true })
  rmSync(mixedDir, { recursive: true, force: true })
}

if (process.argv.includes('--assert') && !report?.passed) process.exit(1)
