import { spawn } from 'node:child_process'
import { mkdir, readFile, readdir, writeFile } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..')
const reportPath = path.join(root, 'docs', 'acceptance', 'performance-report.json')
const families = ['native-lint', 'native-format', 'type-aware', 'vite', 'editor', 'comparative']
const commands = []
const adjudicationBandFraction = 0.03
let failure = null
const adjudications = {}
let rssAdjudication = null
const selectedReports = {}

function tail(text, max = 12_000) {
  return text.length <= max ? text : text.slice(-max)
}

function run(executable, args, label, { allowFailure = false } = {}) {
  const began = performance.now()
  process.stdout.write(`\n[performance] ${label}\n`)
  return new Promise((resolve, reject) => {
    const child = spawn(executable, args, {
      cwd: root,
      env: { ...process.env, CI: '1', NO_COLOR: '1' },
      stdio: ['ignore', 'pipe', 'pipe'],
    })
    let stdout = ''
    let stderr = ''
    child.stdout.setEncoding('utf8')
    child.stderr.setEncoding('utf8')
    child.stdout.on('data', (chunk) => {
      stdout += chunk
      process.stdout.write(chunk)
    })
    child.stderr.on('data', (chunk) => {
      stderr += chunk
      process.stderr.write(chunk)
    })
    child.on('error', reject)
    child.on('close', (status, signal) => {
      commands.push({
        label,
        command: [executable, ...args].join(' '),
        status,
        signal,
        durationMs: Number((performance.now() - began).toFixed(1)),
      })
      if (status !== 0 && !allowFailure) {
        reject(new Error(`${label} exited ${status}\n${tail(stderr || stdout)}`))
      } else {
        resolve({ status, signal })
      }
    })
  })
}

async function latest(family) {
  const directory = path.join(root, 'benchmarks', family)
  const name = (await readdir(directory))
    .filter((candidate) => /^results-\d+\.json$/u.test(candidate))
    .sort()
    .at(-1)
  return {
    path: `benchmarks/${family}/${name}`,
    report: JSON.parse(await readFile(path.join(directory, name), 'utf8')),
  }
}

function assertionList(report) {
  if (Array.isArray(report.assertions)) return report.assertions
  return Object.entries(report.assertions ?? {}).map(([name, pass]) => ({ name, pass }))
}

function requireContract(family, report) {
  const fail = (message) => {
    throw new Error(`${family} performance contract: ${message}`)
  }
  if (family === 'native-format') {
    if (report.budgets?.generalizedControlWarmups < 5) fail('generalized warmups below 5')
    if (report.budgets?.generalizedControlSamples < 15) fail('generalized samples below 15')
    if (report.budgets?.batchWarmups < 5) fail('batch warmups below 5')
    if (report.budgets?.batchSamples < 15) fail('batch samples below 15')
    for (const key of [
      'candidateTsrxScanNs',
      'candidateTsrxProjectionNs',
      'candidateTsrxParseNs',
      'candidateTsrxFormatNs',
      'candidateTsrxLiftNs',
    ]) {
      if (report.rawSamples?.[key]?.length !== report.budgets?.samples) fail(`missing ${key}`)
    }
  }
  if (family === 'type-aware' && report.samplePolicy?.measured < 20) {
    fail('fewer than 20 measured fresh processes')
  }
  if (family === 'vite' && (report.samplePolicy?.warmups < 5 || report.samplePolicy?.measured < 15)) {
    fail('fewer than 5 warmups and 15 samples')
  }
  if (family === 'editor') {
    for (const key of ['editWarmups', 'formatWarmups', 'codeActionWarmups', 'initialOpenWarmups']) {
      if (report.samplePolicy?.[key] < 20) fail(`${key} below 20`)
    }
    for (const key of ['editSamples', 'formatSamples', 'codeActionSamples', 'initialOpenSamples']) {
      if (report.samplePolicy?.[key] < 100) fail(`${key} below 100`)
    }
    if (report.samplePolicy?.editSoak < 1_000) fail('edit soak below 1000')
  }
  if (family === 'comparative') {
    if (report.samplePolicy?.warmups < 5 || report.samplePolicy?.measured < 20) {
      fail('fewer than 5 warmups and 20 measured runs')
    }
    if (!/^[0-9a-f]{64}$/u.test(report.corpus?.tsxSha256 ?? '')) fail('missing TSX corpus hash')
    if (!/^[0-9a-f]{64}$/u.test(report.corpus?.mixedSha256 ?? '')) fail('missing mixed corpus hash')
    if (!/^[0-9a-f]{64}$/u.test(report.boundary?.configSha256 ?? '')) fail('missing config hash')
    if (report.boundary?.fileSelection !== 'same explicit file list') fail('unmatched file selection')
    if (report.boundary?.output !== 'zero-diagnostic default output') fail('unmatched output boundary')
    if (JSON.stringify(report.boundary?.rules) !== JSON.stringify(['no-debugger'])) {
      fail('unmatched rule boundary')
    }
    for (const lane of ['eslint', 'oxlint', 'oxcTsrx', 'oxcTsrxMixed']) {
      if (report.tools?.[lane]?.rawMs?.length !== report.samplePolicy.measured) {
        fail(`${lane} raw sample count mismatch`)
      }
      if (report.validation?.[lane]?.files !== report.corpus.files) fail(`${lane} file count mismatch`)
      if (report.validation?.[lane]?.diagnostics !== 0) fail(`${lane} diagnostic boundary mismatch`)
    }
  }
  if (['type-aware', 'vite', 'editor'].includes(family)) {
    if (!report.host?.cpu || report.host.cpu === 'recorded-by-host') fail('missing real CPU identity')
    if (!report.host?.osRelease) fail('missing OS release')
    if (report.build?.profile !== 'release') fail('missing release build identity')
    if (!/^[0-9a-f]{40}$/u.test(report.build?.oxcRevision ?? '')) fail('missing OXC revision')
    if (!/^[0-9a-f]{64}$/u.test(report.corpus?.sha256 ?? '')) fail('missing corpus hash')
  }
  if (family === 'comparative') {
    if (!report.host?.cpu || report.host.cpu === 'recorded-by-host') fail('missing real CPU identity')
    if (!report.host?.osRelease) fail('missing OS release')
    if (report.build?.profile !== 'release') fail('missing release build identity')
    if (!/^[0-9a-f]{40}$/u.test(report.build?.oxcRevision ?? '')) fail('missing OXC revision')
  }
}

function isInvariant(assertion) {
  return assertion.comparison === 'required boolean invariant' || assertion.comparison === '=='
}

function relativeMargin(assertion) {
  return Math.abs(assertion.threshold - assertion.observed) / Math.abs(assertion.threshold)
}

function arrayAdjudicationEntry(current, family) {
  const assertions = assertionList(current.report)
  if (assertions.length === 0) {
    throw new Error(`${family} performance contract: missing assertions`)
  }
  for (const assertion of assertions.filter((entry) => !isInvariant(entry))) {
    if (!Number.isFinite(assertion.observed) || !Number.isFinite(assertion.threshold)) {
      throw new Error(`${family} performance contract: non-numeric ${assertion.name}`)
    }
  }
  const corpusIdentity =
    family === 'native-format'
      ? `${current.report.corpus?.fnv1a64}:${current.report.generalizedControlCorpus?.fnv1a64}`
      : current.report.corpus?.fnv1a64
  return {
    path: current.path,
    assertions,
    reportPassed: assertions.every(({ pass }) => pass === true),
    invariantsPassed: assertions
      .filter(isInvariant)
      .every(({ pass }) => pass === true),
    oxcRevision: current.report.host?.oxcRevision,
    corpusIdentity,
    budgetsIdentity: JSON.stringify(current.report.budgets),
  }
}

async function runArrayAssertionFamilyWithAdjudication({ family, command, label }) {
  const currentReports = []
  await run(command[0], command.slice(1), label, {
    allowFailure: true,
  })
  currentReports.push(await latest(family))
  const first = arrayAdjudicationEntry(currentReports[0], family)
  if (!first.invariantsPassed) {
    throw new Error(`${family} correctness invariant failed; performance adjudication cannot override it`)
  }
  const triggeredBy = first.assertions
    .filter((assertion) => !isInvariant(assertion) && relativeMargin(assertion) <= adjudicationBandFraction)
    .map(({ name }) => name)
  const triggered = triggeredBy.length > 0
  const reports = [first]

  if (triggered) {
    for (let index = 2; index <= 3; index += 1) {
      await run(
        command[0],
        command.slice(1),
        `${label} confidence rerun ${index}/3`,
        { allowFailure: true },
      )
      currentReports.push(await latest(family))
      const current = arrayAdjudicationEntry(currentReports.at(-1), family)
      if (!current.invariantsPassed) {
        throw new Error(
          `${family} correctness invariant failed; performance adjudication cannot override it`,
        )
      }
      reports.push(current)
    }
  }

  if (
    reports.some(
      (entry) =>
        entry.threshold !== first.threshold ||
        entry.oxcRevision !== first.oxcRevision ||
        entry.corpusIdentity !== first.corpusIdentity ||
        entry.budgetsIdentity !== first.budgetsIdentity,
    )
  ) {
    throw new Error(`${family} confidence adjudication did not produce coherent reports`)
  }

  const assertionDecisions = first.assertions.map((firstAssertion) => {
    const samples = reports.map((report) => {
      const assertion = report.assertions.find(({ name }) => name === firstAssertion.name)
      if (!assertion || assertion.threshold !== firstAssertion.threshold) {
        throw new Error(`${family} confidence assertion drifted: ${firstAssertion.name}`)
      }
      return {
        path: report.path,
        observed: assertion.observed,
        threshold: assertion.threshold,
        pass: assertion.pass === true,
        relativeMargin: isInvariant(assertion) ? null : relativeMargin(assertion),
      }
    })
    const failures = samples.filter(({ pass }) => !pass)
    const definitiveFailure =
      !isInvariant(firstAssertion) &&
      failures.some((sample) => sample.relativeMargin > adjudicationBandFraction)
    const passed = isInvariant(firstAssertion)
      ? failures.length === 0
      : !definitiveFailure && failures.length < (triggered ? 2 : 1)
    return {
      name: firstAssertion.name,
      invariant: isInvariant(firstAssertion),
      triggered: samples.some(
        (sample) => sample.relativeMargin !== null && sample.relativeMargin <= adjudicationBandFraction,
      ),
      passCount: samples.length - failures.length,
      failCount: failures.length,
      definitiveFailure,
      decision: passed ? 'passed' : 'failed',
      samples,
    }
  })
  const decision = assertionDecisions.every((assertion) => assertion.decision === 'passed')
    ? 'passed'
    : 'failed'
  const selectedIndex = reports.map(({ reportPassed }) => reportPassed).lastIndexOf(true)
  selectedReports[family] = currentReports[selectedIndex >= 0 ? selectedIndex : currentReports.length - 1]
  const adjudication = {
    bandFraction: adjudicationBandFraction,
    triggered,
    triggeredBy,
    requiredReports: triggered ? 3 : 1,
    decision,
    selectedReport: selectedReports[family].path,
    reports: reports.map(({ path, reportPassed, oxcRevision, corpusIdentity }) => ({
      path,
      reportPassed,
      oxcRevision,
      corpusIdentity,
    })),
    assertionDecisions,
  }
  adjudications[family] = adjudication

  if (family === 'native-format') {
    const rssDecision = assertionDecisions.find(({ name }) => name === 'p07_rss_ratio')
    if (!rssDecision) throw new Error('native-format performance contract: missing p07 RSS assertion')
    rssAdjudication = {
      bandFraction: adjudicationBandFraction,
      relativeMargin: rssDecision.samples[0].relativeMargin,
      triggered: rssDecision.triggered,
      requiredReports: reports.length,
      passCount: rssDecision.passCount,
      failCount: rssDecision.failCount,
      decision: rssDecision.decision,
      reports: rssDecision.samples.map((sample, index) => ({
        path: sample.path,
        ratio: sample.observed,
        threshold: sample.threshold,
        pass: sample.pass,
        oxcRevision: reports[index].oxcRevision,
        corpusIdentity: reports[index].corpusIdentity,
      })),
    }
  }

  if (decision === 'failed') {
    throw new Error(`${family} budget failed its frozen two-of-three adjudication policy`)
  }
}

const startedAt = new Date().toISOString()
await mkdir(path.dirname(reportPath), { recursive: true })

try {
  await runArrayAssertionFamilyWithAdjudication({
    family: 'native-lint',
    command: ['npm', 'run', 'benchmark:native-lint'],
    label: 'fresh native lint performance gate',
  })
  await runArrayAssertionFamilyWithAdjudication({
    family: 'native-format',
    command: ['npm', 'run', 'benchmark:native-format'],
    label: 'fresh native format performance gate',
  })
  await run('npm', ['run', 'benchmark:type-aware'], 'fresh type-aware performance gate')
  await run(process.execPath, ['benchmarks/vite/run.mjs'], 'fresh Vite/Vite+ boundary gate')
  await run('npm', ['run', 'benchmark:editor'], 'fresh incremental editor performance gate')
  await run('npm', ['run', 'benchmark:comparative'], 'fresh like-for-like comparative gate')
} catch (error) {
  failure = error
}

const results = {}
for (const family of families) {
  const current = selectedReports[family] ?? (await latest(family))
  try {
    requireContract(family, current.report)
  } catch (error) {
    failure ??= error
  }
  const assertions = assertionList(current.report)
  const directAllPassed = assertions.length > 0 && assertions.every(({ pass }) => pass === true)
  const allPassed = adjudications[family]
    ? adjudications[family].decision === 'passed'
    : directAllPassed
  results[family] = {
    path: current.path,
    generatedAtUnixMs:
      current.report.generatedAtUnixMs ??
      (Number.isFinite(Date.parse(current.report.timestamp))
        ? Date.parse(current.report.timestamp)
        : null),
    assertions,
    allPassed,
    budgets: current.report.budgets,
  }
  if (adjudications[family]) results[family].adjudication = adjudications[family]
  if (family === 'native-format') {
    results[family].rssAdjudication = rssAdjudication
  }
  if (!results[family].allPassed) failure ??= new Error(`${family} has a red or empty assertion set`)
}

const report = {
  schemaVersion: 1,
  status: failure ? 'failed' : 'passed',
  startedAt,
  completedAt: new Date().toISOString(),
  commands,
  results,
  failure: failure ? { name: failure.name, message: failure.message } : null,
}
await writeFile(reportPath, `${JSON.stringify(report, null, 2)}\n`)

if (failure) {
  console.error(`\n[performance] FAILED: ${failure.message}`)
  process.exit(1)
}
console.log(`\n[performance] PASS: ${reportPath}`)
