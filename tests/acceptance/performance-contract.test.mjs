import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { readFile, readdir } from 'node:fs/promises'
import { cpus } from 'node:os'
import path from 'node:path'
import test from 'node:test'

const root = path.resolve(import.meta.dirname, '..', '..')
const publicFamilies = ['comparative', 'editor', 'native-format', 'native-lint', 'type-aware', 'vite']
const frozenBudgetHashes = {
  comparative: 'e9bd056f5a8452b151684292554f887360bc3f3fc648cb9be513c465aba357d1',
  editor: '1fe5c78f1ac1543ca9a169c721d463ba6bc119631f678b896e572334d29592cd',
  'native-format': 'a2cf813a0ad2418df49af0862db3d04b789e1bf906ca760a872f5176c694e819',
  'native-lint': 'a8d95c3526dc7ebb20d9913622f63d45e6057200136877c30b5d8f760ad1c7b9',
  'type-aware': '799a15d0a986744f7e1d80688c35bc631697c32a926cde55595a7fbb591d0db4',
  vite: 'c0e22e8f3a47d3ba002819b732498aa4c822982f8281eaa168ef022145887807',
}

async function latest(family) {
  const directory = path.join(root, 'benchmarks', family)
  const name = (await readdir(directory))
    .filter((candidate) => /^results-\d+\.json$/u.test(candidate))
    .sort()
    .at(-1)
  assert.ok(name, `missing ${family} report`)
  return JSON.parse(await readFile(path.join(directory, name), 'utf8'))
}

function completeHost(report, family) {
  assert.equal(typeof report.host?.cpu, 'string', `${family}: cpu`)
  assert.notEqual(report.host.cpu, 'recorded-by-host', `${family}: placeholder cpu`)
  assert.ok(report.host.cpu.length >= 3, `${family}: empty cpu`)
  assert.equal(typeof report.host?.osRelease, 'string', `${family}: osRelease`)
  assert.ok(report.host.osRelease.length >= 1, `${family}: empty osRelease`)
  assert.equal(report.host.arch, process.arch, `${family}: architecture`)
  assert.equal(report.host.platform, process.platform, `${family}: platform`)
  assert.ok(cpus().some((cpu) => cpu.model === report.host.cpu), `${family}: actual host cpu`)
}

function completeNativeIdentity(report, family) {
  assert.equal(report.build?.profile, 'release', `${family}: release profile`)
  assert.match(report.build?.oxcRevision ?? '', /^[0-9a-f]{40}$/u, `${family}: OXC revision`)
  assert.match(report.corpus?.sha256 ?? '', /^[0-9a-f]{64}$/u, `${family}: corpus hash`)
  assert.ok(report.corpus?.bytes > 0, `${family}: corpus bytes`)
}

test('fresh performance evidence satisfies the frozen confidence and identity policy', async () => {
  const [format, typeAware, vite, editor] = await Promise.all([
    latest('native-format'),
    latest('type-aware'),
    latest('vite'),
    latest('editor'),
  ])

  assert.ok(format.budgets.generalizedControlWarmups >= 5)
  assert.ok(format.budgets.generalizedControlSamples >= 15)
  assert.ok(format.budgets.batchWarmups >= 5)
  assert.ok(format.budgets.batchSamples >= 15)
  assert.ok(format.budgets.coldProcessSamples >= 20)
  assert.ok(format.budgets.rssProcessSamples >= 5)
  for (const key of [
    'candidateTsrxScanNs',
    'candidateTsrxProjectionNs',
    'candidateTsrxParseNs',
    'candidateTsrxFormatNs',
    'candidateTsrxLiftNs',
  ]) {
    assert.equal(format.rawSamples?.[key]?.length, format.budgets.samples, key)
  }
  assert.equal('prettierSpeedup' in (format.p04 ?? {}), false)
  assert.equal(
    format.assertions.some((entry) => /prettier.*speedup/iu.test(entry.name)),
    false,
  )

  assert.ok(typeAware.samplePolicy?.warmupsAfterCold >= 5)
  assert.ok(typeAware.samplePolicy?.measured >= 20)
  completeHost(typeAware, 'type-aware')
  completeNativeIdentity(typeAware, 'type-aware')

  assert.ok(vite.samplePolicy?.warmups >= 5)
  assert.ok(vite.samplePolicy?.measured >= 15)
  completeHost(vite, 'vite')
  completeNativeIdentity(vite, 'vite')

  for (const field of ['editWarmups', 'formatWarmups', 'codeActionWarmups', 'initialOpenWarmups']) {
    assert.ok(editor.samplePolicy?.[field] >= 20, `editor: ${field}`)
  }
  for (const field of ['editSamples', 'formatSamples', 'codeActionSamples', 'initialOpenSamples']) {
    assert.ok(editor.samplePolicy?.[field] >= 100, `editor: ${field}`)
  }
  assert.ok(editor.samplePolicy?.editSoak >= 1_000)
  assert.equal(editor.initialOpen?.rawMs?.length, editor.samplePolicy.initialOpenSamples)
  const retainedEditorSource = await readFile(
    path.join(root, 'tests', 'fixtures', 'editor', 'markless-arm-try-events.tsrx'),
    'utf8',
  )
  const measuredEditorSource = retainedEditorSource
    .replace(
      'export function App() @{',
      'export function App() @{\nvar editorProbe=0;\nvoid editorProbe;\ndebugger;',
    )
    .replace("let saved = state('none');", "let saved=state('none');")
  assert.equal(
    editor.corpus.sha256,
    createHash('sha256').update(measuredEditorSource).digest('hex'),
    'editor: exact measured source hash',
  )
  assert.equal(
    editor.corpus.retainedFixtureSha256,
    createHash('sha256').update(retainedEditorSource).digest('hex'),
    'editor: retained fixture hash',
  )
  completeHost(editor, 'editor')
  completeNativeIdentity(editor, 'editor')
})

test('aggregate performance evidence records a real generation time for every lane', async () => {
  const report = JSON.parse(
    await readFile(path.join(root, 'docs', 'acceptance', 'performance-report.json'), 'utf8'),
  )
  for (const [family, result] of Object.entries(report.results)) {
    assert.ok(Number.isFinite(result.generatedAtUnixMs), `${family}: generatedAtUnixMs`)
    assert.ok(result.generatedAtUnixMs > 0, `${family}: generatedAtUnixMs`)
  }

  const adjudication = report.results['native-format'].rssAdjudication
  assert.equal(adjudication.bandFraction, 0.03)
  assert.equal(adjudication.triggered, adjudication.relativeMargin <= adjudication.bandFraction)
  assert.equal(adjudication.requiredReports, adjudication.triggered ? 3 : 1)
  assert.equal(adjudication.reports.length, adjudication.requiredReports)
  assert.equal(adjudication.decision, adjudication.failCount >= 2 ? 'failed' : 'passed')
  assert.equal(adjudication.passCount + adjudication.failCount, adjudication.reports.length)
  const [first, ...rest] = adjudication.reports
  assert.ok(adjudication.passCount >= (adjudication.triggered ? 2 : 1))
  assert.ok(rest.every((entry) => entry.threshold === first.threshold))
  assert.ok(rest.every((entry) => entry.oxcRevision === first.oxcRevision))
  assert.ok(rest.every((entry) => entry.corpusIdentity === first.corpusIdentity))

  const formatAdjudication = report.results['native-format'].adjudication
  assert.equal(formatAdjudication.bandFraction, 0.03)
  assert.equal(formatAdjudication.reports.length, formatAdjudication.requiredReports)
  assert.equal(formatAdjudication.decision, 'passed')
  assert.ok(formatAdjudication.assertionDecisions.every((entry) => entry.decision === 'passed'))
  assert.ok(
    formatAdjudication.assertionDecisions.every(
      (entry) => entry.passCount + entry.failCount === formatAdjudication.requiredReports,
    ),
  )

  const lintAdjudication = report.results['native-lint'].adjudication
  assert.equal(lintAdjudication.bandFraction, 0.03)
  assert.equal(lintAdjudication.reports.length, lintAdjudication.requiredReports)
  assert.equal(lintAdjudication.decision, 'passed')
  assert.ok(lintAdjudication.assertionDecisions.every((entry) => entry.decision === 'passed'))
})

test('every public performance lane is reproducible, structured, and budget-frozen', async () => {
  const aggregate = JSON.parse(
    await readFile(path.join(root, 'docs', 'acceptance', 'performance-report.json'), 'utf8'),
  )
  assert.equal(aggregate.status, 'passed')
  assert.equal(aggregate.failure, null)
  assert.deepEqual(Object.keys(aggregate.results).sort(), publicFamilies)
  assert.ok(aggregate.commands.length >= publicFamilies.length)
  assert.ok(
    aggregate.commands.every(
      (entry) =>
        entry &&
        typeof entry === 'object' &&
        typeof entry.label === 'string' &&
        typeof entry.command === 'string' &&
        entry.status === 0 &&
        Number.isFinite(entry.durationMs),
    ),
  )

  for (const family of publicFamilies) {
    const budgetPath = path.join(root, 'benchmarks', family, 'budgets.json')
    const budgetBytes = await readFile(budgetPath)
    assert.equal(
      createHash('sha256').update(budgetBytes).digest('hex'),
      frozenBudgetHashes[family],
      `${family}: frozen budget snapshot`,
    )
    const selected = aggregate.results[family]
    assert.match(selected.path, new RegExp(`^benchmarks/${family}/results-\\d+\\.json$`))
    assert.equal(selected.allPassed, true, `${family}: aggregate pass`)
    const raw = JSON.parse(await readFile(path.join(root, selected.path), 'utf8'))
    assert.deepEqual(selected.budgets, raw.budgets, `${family}: selected budgets`)
  }

  const comparativeResult = aggregate.results.comparative
  const comparative = JSON.parse(await readFile(path.join(root, comparativeResult.path), 'utf8'))
  assert.ok(comparative.samplePolicy.warmups >= 5)
  assert.ok(comparative.samplePolicy.measured >= 20)
  assert.match(comparative.corpus.tsxSha256, /^[0-9a-f]{64}$/u)
  assert.match(comparative.corpus.mixedSha256, /^[0-9a-f]{64}$/u)
  assert.match(comparative.boundary.configSha256, /^[0-9a-f]{64}$/u)
  assert.equal(comparative.boundary.fileSelection, 'same explicit file list')
  assert.equal(comparative.boundary.output, 'zero-diagnostic default output')
  assert.deepEqual(comparative.boundary.rules, ['no-debugger'])
  assert.equal(comparative.build.profile, 'release')
  assert.match(comparative.build.oxcRevision, /^[0-9a-f]{40}$/u)
  for (const lane of ['eslint', 'oxlint', 'oxcTsrx', 'oxcTsrxMixed']) {
    assert.equal(comparative.tools[lane].rawMs.length, comparative.samplePolicy.measured, lane)
    assert.equal(comparative.validation[lane].files, comparative.corpus.files, `${lane}: files`)
    assert.equal(comparative.validation[lane].diagnostics, 0, `${lane}: diagnostics`)
  }
})
