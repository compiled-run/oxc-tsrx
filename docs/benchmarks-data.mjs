// Reads the committed benchmark reports and produces the tables and charts on
// the benchmarks page at build time, so the page can never go stale: rerun the
// benchmarks, rebuild, done.
import { readFile } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const repoRoot = path.join(path.dirname(fileURLToPath(import.meta.url)), '..')
const aggregateReport = readFile(
  path.join(repoRoot, 'docs', 'acceptance', 'performance-report.json'),
  'utf8',
).then(JSON.parse)

const p95 = (rawMs) => {
  const sorted = [...rawMs].sort((a, b) => a - b)
  return sorted[Math.min(sorted.length - 1, Math.ceil(sorted.length * 0.95) - 1)]
}
// Lower-middle median, matching the benchmark harnesses exactly: for every
// retained array in every selected report, sorted[floor((n - 1) / 2)]
// reproduces the reported p50 to the last bit (verified by the chart parity
// test), so median ticks land exactly on the asserted values.
const median = (rawMs) => {
  const sorted = [...rawMs].sort((a, b) => a - b)
  return sorted[Math.floor((sorted.length - 1) / 2)]
}

// ---------- distribution mapping: assertion row -> retained sample array ----------
// Every converted number below is derived only from fields inside the selected
// report: nanosecond arrays to milliseconds, nanosecond arrays plus in-report
// corpus byte counts to MiB/s, and RSS byte arrays to MiB. Nothing is
// hand-entered.
const MIB = 1048576
const nsToMs = (ns) => ns / 1e6
const mibPerSecond = (ns, bytes) => bytes / MIB / (ns / 1e9)

const latencyDist = (nsSamples) => ({
  kind: 'samples',
  unit: 'ms',
  values: nsSamples.map(nsToMs),
  median: nsToMs(median(nsSamples)),
  p95: nsToMs(p95(nsSamples)),
})
const throughputDist = (nsSamples, corpusBytes) => ({
  kind: 'samples',
  unit: 'MiB/s',
  values: nsSamples.map((ns) => mibPerSecond(ns, corpusBytes)),
  median: mibPerSecond(median(nsSamples), corpusBytes),
  p95: mibPerSecond(p95(nsSamples), corpusBytes),
})
const msDist = (msSamples) => ({
  kind: 'samples',
  unit: 'ms',
  values: [...msSamples],
  median: median(msSamples),
  p95: p95(msSamples),
})
// RSS arrays hold bytes; plot them in MiB against the byte budget converted
// with the same constant, so the fraction of budget is unchanged.
const rssDist = (byteSamples, allowedBytes) => ({
  kind: 'samples',
  unit: 'MiB',
  values: byteSamples.map((bytes) => bytes / MIB),
  median: median(byteSamples) / MIB,
  p95: p95(byteSamples) / MIB,
  plotThreshold: allowedBytes / MIB,
})
const scalarDist = () => ({ kind: 'scalar' })
// Ratio gates never derive per-sample ratios: the two runs are sampled
// independently, so index-pairing them would fabricate data. The gate marker
// uses the asserted ratio; the two raw arrays are shown as labeled sub-strips.
const strip = (label, values) => ({ label, values: [...values], median: median(values) })
const msStrip = (label, nsSamples) => strip(label, nsSamples.map(nsToMs))
const ratioDist = (unit, num, den) => ({ kind: 'ratio', unit, num, den })

function nativeLintDistributions(report) {
  const raw = report.rawSamples
  const corpusBytes = report.corpus.bytes
  const control = [...raw.controlBeforeNs, ...raw.controlAfterNs]
  // The three phase arrays come from the same 30 measured iterations, so the
  // per-iteration sum is the retained scan+copy+parse timing.
  const scanCopyParse = raw.candidateTsrxScanNs.map(
    (scan, index) => scan + raw.candidateTsrxProjectionNs[index] + raw.candidateTsrxParseNs[index],
  )
  return {
    'P01 median standard-path latency ratio': ratioDist(
      'ms',
      msStrip('product standard path', raw.candidateStandardTotalNs),
      msStrip('same-build canonical control', control),
    ),
    'P01 p95 standard-path latency ratio': ratioDist(
      'ms',
      msStrip('product standard path', raw.candidateStandardTotalNs),
      msStrip('same-build canonical control', control),
    ),
    'P02 median scan+copy+parse throughput': throughputDist(scanCopyParse, corpusBytes),
    'P02 p95 scan+copy+parse throughput': throughputDist(scanCopyParse, corpusBytes),
    'P02 equivalent-TSX throughput ratio': ratioDist(
      'ms',
      msStrip('TSRX scan, copy, and parse', scanCopyParse),
      msStrip('equivalent TSX parse', raw.candidateStandardParseNs),
    ),
    'P02 warm 10 KiB p95 scan+copy+parse latency': latencyDist(raw.warm10kScanCopyParseNs),
    'P03 in-process one-thread lint throughput': throughputDist(raw.candidateTsrxTotalNs, corpusBytes),
    'P03 CLI one-thread lint throughput': throughputDist(raw.candidateTsrxCliNs, corpusBytes),
    'P03 end-to-end CLI latency ratio': ratioDist(
      'ms',
      msStrip('TSRX CLI lint', raw.candidateTsrxCliNs),
      msStrip('equivalent TSX CLI lint', raw.candidateStandardCliNs),
    ),
    'P05 fresh-process TSRX p95 latency': latencyDist(raw.candidateColdCliNs),
    'P05 fresh-process upstream latency ratio': ratioDist(
      'ms',
      msStrip('direct Rust TSRX process', raw.candidateColdCliNs),
      msStrip('official Oxlint npm launcher', raw.stockColdCliNs),
    ),
    'P07 TSRX peak RSS': rssDist(
      report.summaries.p07.candidateTsrxRssBytes,
      report.summaries.p07.allowedRssBytes,
    ),
  }
}

function nativeFormatDistributions(report) {
  const raw = report.rawSamples
  const corpusBytes = report.corpus.bytes
  const generalizedBytes = report.generalizedControlCorpus.bytes
  const control = [...raw.directControlBeforeNs, ...raw.directControlAfterNs]
  // The batch corpus byte count is not stored directly, but the report keeps
  // both the batch latency percentiles' throughputs and the raw latency
  // array, so the byte count is exact algebra on in-report fields. The two
  // independent derivations must agree, or we refuse to plot.
  const batch = report.p04.candidateDefaultThreadBatch
  const batchBytesFromMedian = batch.medianMibPerSecond * (median(raw.candidateBatchNs) / 1e9) * MIB
  const batchBytesFromP95 = batch.p95MibPerSecond * (p95(raw.candidateBatchNs) / 1e9) * MIB
  if (Math.abs(batchBytesFromMedian - batchBytesFromP95) > 0.5) {
    throw new Error('native-format batch corpus byte derivation is inconsistent; refusing to plot')
  }
  const batchBytes = Math.round(batchBytesFromP95)
  const mibStrip = (label, byteSamples) => strip(label, byteSamples.map((bytes) => bytes / MIB))
  return {
    p04_direct_median_ratio: ratioDist(
      'ms',
      msStrip('product standard path', raw.candidateStandardNs),
      msStrip('canonical formatter control', control),
    ),
    p04_direct_p95_ratio: ratioDist(
      'ms',
      msStrip('product standard path', raw.candidateStandardNs),
      msStrip('canonical formatter control', control),
    ),
    p04_sequential_median_mib_s: throughputDist(raw.candidateTsrxSequentialNs, corpusBytes),
    p04_sequential_p95_mib_s: throughputDist(raw.candidateTsrxSequentialNs, corpusBytes),
    p04_historical_incumbent_derived_floor_mib_s: throughputDist(
      raw.candidateTsrxSequentialNs,
      corpusBytes,
    ),
    p04_default_thread_mib_s: throughputDist(raw.candidateBatchNs, batchBytes),
    p04_generalized_control_median_mib_s: throughputDist(
      raw.candidateGeneralizedControlNs,
      generalizedBytes,
    ),
    p04_generalized_control_p95_mib_s: throughputDist(
      raw.candidateGeneralizedControlNs,
      generalizedBytes,
    ),
    p04_generalized_control_linear_scaling: ratioDist(
      'ms',
      msStrip('full generalized corpus', raw.candidateGeneralizedControlNs),
      msStrip('half-size corpus', raw.candidateGeneralizedControlHalfNs),
    ),
    p05_stdin_p95_ms: latencyDist(raw.candidateStdinNs),
    p05_upstream_ratio: ratioDist(
      'ms',
      msStrip('direct Rust TSRX stdin', raw.candidateStdinNs),
      msStrip('official Oxfmt npm launcher', raw.stockStdinNs),
    ),
    p07_rss_ratio: ratioDist(
      'MiB',
      mibStrip('TSRX formatter RSS', report.p07.candidateTsrxRssBytes),
      mibStrip('canonical TSX RSS', report.p07.canonicalTsxRssBytes),
    ),
  }
}

const fmt = (value) => {
  if (typeof value !== 'number') return String(value)
  if (Number.isInteger(value)) return String(value)
  if (value >= 100) return value.toFixed(2)
  if (value >= 1) return value.toFixed(3)
  return value.toFixed(3)
}

// Short display forms for the home-page stat cards: trim trailing zeros and
// attach the ratio sign directly, so gates read "≤ 1.05×" instead of "≤ 1.050 ×".
const trimZeros = (text) => (text.includes('.') ? text.replace(/\.?0+$/, '') : text)
const withUnit = (value, unit) =>
  `${trimZeros(fmt(value))}${unit === '×' ? '×' : unit ? ` ${unit}` : ''}`
const fmtHeadline = (value, unit) => {
  if (unit === '×') return `${trimZeros(value.toFixed(3))}×`
  if (unit === 'MiB/s') return `${Math.round(value)} MiB/s`
  if (unit === 'ms') {
    if (value >= 100) return `${Math.round(value)} ms`
    if (value >= 10) return `${value.toFixed(1)} ms`
    if (value >= 1) return `${value.toFixed(1)} ms`
    return `${value.toFixed(2)} ms`
  }
  return `${fmt(value)}${unit ? ` ${unit}` : ''}`
}

async function latestReport(family) {
  const aggregate = await aggregateReport
  const selected = aggregate.results?.[family]
  const file = selected?.path
  if (!new RegExp(`^benchmarks/${family}/results-\\d+\\.json$`).test(file ?? '')) {
    throw new Error(`no aggregate-selected report for benchmarks/${family}`)
  }
  return {
    file,
    report: JSON.parse(await readFile(path.join(repoRoot, file), 'utf8')),
    adjudication: selected.adjudication ?? null,
  }
}

function adjudicationHtml(adjudication) {
  if (!adjudication?.triggered) return ''
  const band = `${(adjudication.bandFraction * 100).toFixed(0)}%`
  const triggers = adjudication.triggeredBy.map((name) => `<code>${escapeHtml(name)}</code>`).join(', ')
  const reports = adjudication.reports
    .map(({ path: reportPath }) => `<code>${escapeHtml(reportPath)}</code>`)
    .join(', ')
  return `<p class="bench-adjudication"><strong>Near-threshold adjudication.</strong> ${triggers} entered the unchanged ${band} band, so the aggregate required exactly ${adjudication.requiredReports - 1} additional fresh identity-matched reports. Only triggering assertions receive two-of-three tolerance; every other assertion and invariant must pass in every report. The representative is selected by median normalized budget pressure with a stable report-path tie-break, never by choosing the fastest passing sample. Any failure more than ${band} beyond its threshold, or a red selected representative, fails the aggregate. Retained reports: ${reports}.</p>`
}

function assertionPresentation(assertion) {
  if (assertion.name === 'P05 fresh-process upstream latency ratio') {
    return {
      label: 'Direct Rust / official Oxlint npm-launcher p95 ratio',
      note: 'Diagnostic launcher-boundary guardrail, not a cross-tool speed claim.',
    }
  }
  if (assertion.name === 'p05_upstream_ratio') {
    return {
      label: 'Direct Rust / official Oxfmt npm-launcher p95 ratio',
      note: 'Diagnostic launcher-boundary guardrail, not a cross-tool speed claim.',
    }
  }
  if (assertion.name === 'p04_historical_incumbent_derived_floor_mib_s') {
    return {
      label: 'Sequential throughput vs absolute 16.6 MiB/s floor',
      note: 'The floor is derived from a non-comparable historical corpus and is not a Prettier speedup claim.',
    }
  }
  return { label: assertion.name.replace(/_/g, ' '), note: null }
}

// Normalized row: { label, observed, threshold, direction ('<='|'>='|'=='),
// unit, pass, dist }. `dist` maps the row to its retained sample array so the
// chart can draw the whole distribution, never just the summary number.
function arrayRows({ report }, distributions) {
  return report.assertions.map((assertion) => {
    const presentation = assertionPresentation(assertion)
    let direction = assertion.comparison
    if (direction !== '<=' && direction !== '>=' && direction !== '==') {
      // native-format uses descriptive comparison strings; infer from outcome.
      if (assertion.observed === assertion.threshold) direction = '=='
      else if (assertion.pass) {
        direction = assertion.observed <= assertion.threshold ? '<=' : '>='
      } else {
        direction = assertion.observed > assertion.threshold ? '<=' : '>='
      }
    }
    const dist = direction === '==' ? null : distributions[assertion.name]
    if (direction !== '==' && !dist) {
      // Fail closed: a gate without an explicit sample-array mapping must not
      // silently render as if it had one measurement.
      throw new Error(`no retained-sample mapping for assertion "${assertion.name}"`)
    }
    return {
      label: presentation.label,
      observed: assertion.observed,
      threshold: assertion.threshold,
      direction,
      unit: '',
      pass: assertion.pass,
      note: presentation.note,
      dist,
    }
  })
}

function typeAwareRows({ report }) {
  const b = report.budgets
  const a = report.assertions
  return [
    { label: 'Default syntax lint p95', observed: p95(report.defaultSyntax.rawMs), threshold: b.defaultSyntaxP95MsMax, direction: '<=', unit: 'ms', pass: a.defaultSyntaxP95, dist: msDist(report.defaultSyntax.rawMs) },
    { label: 'Single-file type-aware p95', observed: p95(report.singleTypeAware.rawMs), threshold: b.singleTypeAwareP95MsMax, direction: '<=', unit: 'ms', pass: a.singleTypeAwareP95, dist: msDist(report.singleTypeAware.rawMs) },
    { label: 'Two-file project type-aware p95', observed: p95(report.projectTypeAware.rawMs), threshold: b.projectTypeAwareP95MsMax, direction: '<=', unit: 'ms', pass: a.projectTypeAwareP95, dist: msDist(report.projectTypeAware.rawMs) },
    { label: 'Single-file type-aware cold start', observed: report.singleTypeAware.coldMs, threshold: b.singleTypeAwareColdMsMax, direction: '<=', unit: 'ms', pass: a.singleTypeAwareCold, dist: scalarDist() },
    { label: 'Two-file project cold start', observed: report.projectTypeAware.coldMs, threshold: b.projectTypeAwareColdMsMax, direction: '<=', unit: 'ms', pass: a.projectTypeAwareCold, dist: scalarDist() },
    { label: 'Type-aware vs default p95 ratio', observed: report.ratios.singleTypeAwareVsDefaultP95, threshold: b.singleTypeAwareVsDefaultP95RatioMax, direction: '<=', unit: '×', pass: a.singleTypeAwareRatio, dist: ratioDist('ms', strip('single-file type-aware lane', report.singleTypeAware.rawMs), strip('default syntax lane', report.defaultSyntax.rawMs)) },
    { label: 'Type processes per batch', observed: report.invariants.singleTypeAwareProcesses, threshold: b.typeAwareProcessesPerBatch, direction: '==', unit: '', pass: a.oneTypeProcessPerBatch },
    { label: 'Default path parse count per file', observed: report.invariants.defaultParseCount, threshold: b.defaultParseCountPerFile, direction: '==', unit: '', pass: a.defaultPathUnchanged },
  ]
}

function viteRows({ report }) {
  const b = report.budgets
  const a = report.assertions
  return [
    { label: 'Mixed companion lint p95', observed: report.directMixedLint.p95Ms ?? p95(report.directMixedLint.rawMs), threshold: b.directLintP95MsMax, direction: '<=', unit: 'ms', pass: a.directLintP95, dist: msDist(report.directMixedLint.rawMs) },
    { label: 'Mixed lint vs canonical p95 ratio', observed: report.ratios.directLintVsCanonicalP95, threshold: b.directLintVsCanonicalP95RatioMax, direction: '<=', unit: '×', pass: a.directLintRatio, dist: ratioDist('ms', strip('mixed companion lint', report.directMixedLint.rawMs), strip('canonical lint', report.canonicalLint.rawMs)) },
    { label: 'Mixed companion format-check p95', observed: report.directMixedFormat.p95Ms ?? p95(report.directMixedFormat.rawMs), threshold: b.directFormatP95MsMax, direction: '<=', unit: 'ms', pass: a.directFormatP95, dist: msDist(report.directMixedFormat.rawMs) },
    { label: 'Mixed format vs canonical p95 ratio', observed: report.ratios.directFormatVsCanonicalP95, threshold: b.directFormatVsCanonicalP95RatioMax, direction: '<=', unit: '×', pass: a.directFormatRatio, dist: ratioDist('ms', strip('mixed companion format', report.directMixedFormat.rawMs), strip('canonical format', report.canonicalFormat.rawMs)) },
    { label: 'Ordinary npm formatter p95', observed: report.directOrdinaryFormat.p95Ms ?? p95(report.directOrdinaryFormat.rawMs), threshold: b.directOrdinaryFormatP95MsMax, direction: '<=', unit: 'ms', pass: a.directOrdinaryFormatP95, dist: msDist(report.directOrdinaryFormat.rawMs) },
    { label: 'Ordinary npm formatter vs canonical p95 ratio', observed: report.ratios.directOrdinaryFormatVsCanonicalP95, threshold: b.directOrdinaryFormatVsCanonicalP95RatioMax, direction: '<=', unit: '×', pass: a.directOrdinaryFormatRatio, dist: ratioDist('ms', strip('ordinary npm formatter', report.directOrdinaryFormat.rawMs), strip('canonical format', report.canonicalFormat.rawMs)) },
    { label: 'Vite+ 0.2.4 mixed lint p95', observed: report.vitePlusCurrentMixedLint.p95Ms ?? p95(report.vitePlusCurrentMixedLint.rawMs), threshold: b.vitePlusCurrentLintP95MsMax, direction: '<=', unit: 'ms', pass: a.vitePlusLintP95, dist: msDist(report.vitePlusCurrentMixedLint.rawMs) },
    { label: 'Native TSRX parses per file', observed: report.invariants?.nativeTsrxParseCount ?? b.nativeTsrxParseCountPerFile, threshold: b.nativeTsrxParseCountPerFile, direction: '==', unit: '', pass: a.oneNativeParse },
  ]
}

function editorRows({ report }) {
  const b = report.budgets
  const a = report.assertions
  return [
    // The release gate asserts the scalar `initialOpenMs`, so this row stays a
    // single-value bar even though the report also retains initialOpen.rawMs.
    { label: 'Server start to first diagnostics', observed: report.initialOpenMs, threshold: b.initialOpenMsMax, direction: '<=', unit: 'ms', pass: a.initialOpen, dist: scalarDist() },
    { label: 'Edit-to-diagnostics p95', observed: p95(report.editDiagnostics.rawMs), threshold: b.editDiagnosticsP95MsMax, direction: '<=', unit: 'ms', pass: a.editDiagnosticsP95, dist: msDist(report.editDiagnostics.rawMs) },
    { label: 'Formatting p95', observed: p95(report.formatting.rawMs), threshold: b.formatP95MsMax, direction: '<=', unit: 'ms', pass: a.formatP95, dist: msDist(report.formatting.rawMs) },
    { label: 'Safe code-action p95', observed: p95(report.codeActions.rawMs), threshold: b.codeActionP95MsMax, direction: '<=', unit: 'ms', pass: a.codeActionP95, dist: msDist(report.codeActions.rawMs) },
    { label: 'RSS after 1,000-edit soak', observed: report.memory.rssAfterSoakMiB, threshold: b.residentMemoryMiBMax, direction: '<=', unit: 'MiB', pass: a.residentMemory, dist: scalarDist() },
    { label: 'RSS growth through soak', observed: report.memory.growthMiB, threshold: b.editSoakGrowthMiBMax, direction: '<=', unit: 'MiB', pass: a.editSoakGrowth, dist: scalarDist() },
  ]
}


function comparativeRows({ report }) {
  const b = report.budgets
  const a = report.assertions
  return [
    { label: 'OXC for TSRX / official Oxlint (matched TSX lane)', observed: report.ratios.oxcTsrxVsOxlint, threshold: b.oxcTsrxVsOxlintMax, direction: '<=', unit: '×', pass: a.nearOxlintParity, dist: ratioDist('ms', strip('OXC for TSRX, all-TSX lane', report.tools.oxcTsrx.rawMs), strip('official Oxlint', report.tools.oxlint.rawMs)) },
    { label: 'ESLint / OXC for TSRX (matched TSX lane)', observed: report.ratios.eslintVsOxcTsrx, threshold: b.eslintVsOxcTsrxMin, direction: '>=', unit: '×', pass: a.fasterThanEslint, dist: ratioDist('ms', strip('ESLint + typescript-eslint', report.tools.eslint.rawMs), strip('OXC for TSRX, all-TSX lane', report.tools.oxcTsrx.rawMs)) },
    { label: 'Paired mixed-file-types / all-TSX product workload', observed: report.ratios.mixedVsTsx, threshold: b.mixedVsTsxMax, direction: '<=', unit: '×', pass: a.mixedNoBlowup, dist: ratioDist('ms', strip('mixed file types workload', report.tools.oxcTsrxMixed.rawMs), strip('all-TSX product workload', report.tools.oxcTsrx.rawMs)) },
  ]
}

const FAMILIES = [
  { family: 'comparative', title: 'Matched 1,000-file CLI comparison', rows: comparativeRows, note: 'ESLint, official Oxlint, and OXC for TSRX lint the same byte-identical TSX files with one no-debugger rule, the same explicit file list, zero-diagnostic default output, 5 warmups, and 20 measured processes. Every lane runs through its npm CLI entry point. The ordinary oxlint-tsrx lane imports the exact declared official Oxlint launcher in the same Node process; only the separate mixed-file-types lane enters the native TSRX path. That mixed row is a paired internal workload ratio, not a cross-tool comparison.' },
  { family: 'native-lint', title: 'Native lint', rows: (context) => arrayRows(context, nativeLintDistributions(context.report)), note: 'Frozen release gate: throughput, same-build ordinary-path overhead, cold start, memory, and batch invariants. The npm-launcher ratio is a diagnostic boundary, not a speed claim.' },
  { family: 'native-format', title: 'Native format', rows: (context) => arrayRows(context, nativeFormatDistributions(context.report)), note: 'Frozen release gate: formatter throughput, convergence scaling, memory, and batch invariants. The historical 16.6 MiB/s value is an absolute cross-corpus-derived floor, not a speedup claim.' },
  { family: 'type-aware', title: 'Opt-in type-aware lint', rows: typeAwareRows, note: 'The opt-in TypeScript-Go lane. The default lane stays syntax-only with zero type processes.' },
  { family: 'vite', title: 'Vite/Vite+ command boundary', rows: viteRows, note: 'Fresh companion processes at the ecosystem seam. Vite build and HMR carry zero OXC for TSRX transforms.' },
  { family: 'editor', title: 'Native editor server', rows: editorRows, note: 'Syntax-only local language-server round trips on the retained Markless fixture.' },
]

const escapeHtml = (text) =>
  String(text)
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;')

// Chart tooltips currently consume data-* values as HTML. Encode one layer for
// the attribute parser and a second layer for that HTML consumer so report text
// can never become markup after `dataset` decodes the outer layer.
export const escapeDatasetHtml = (text) => escapeHtml(escapeHtml(text))

// ---------- chart rendering: distribution rows on a shared percent-of-budget axis ----------
const CHART = { width: 700, labelWidth: 260, trackWidth: 320 }
const OVERFLOW_FRACTION = 1.15

const fractionOfBudget = (value, threshold, direction) =>
  direction === '<=' ? value / threshold : threshold / value
const clampFraction = (fraction) => Math.max(Math.min(fraction, OVERFLOW_FRACTION), 0.005)
const fractionX = (fraction) => CHART.labelWidth + clampFraction(fraction) * CHART.trackWidth

const unitText = (unit) => (unit === '×' ? '×' : unit ? ` ${unit}` : '')
const valueWithUnit = (value, unit) => `${fmt(value)}${unitText(unit)}`

// Tooltip line stating sample provenance: count plus median and p95 for dot
// strips, both sub-strip medians for ratio rows, and an explicit single-value
// statement for scalar gates.
function distSummary(dist) {
  if (!dist || dist.kind === 'scalar') return 'single measurement per report'
  if (dist.kind === 'ratio') {
    const side = (stripData) =>
      `${stripData.label}: ${stripData.values.length} samples, median ${valueWithUnit(stripData.median, dist.unit)}`
    return `${side(dist.num)} · ${side(dist.den)} · runs are sampled independently, so no per-sample ratios`
  }
  const count = dist.values.length
  const shown = count <= 5 ? `${count} samples (all shown)` : `${count} samples`
  return `${shown} · median ${valueWithUnit(dist.median, dist.unit)} · p95 ${valueWithUnit(dist.p95, dist.unit)}`
}

const overflowMarker = (y, statusClass) =>
  `<path class="bench-overflow ${statusClass}" d="M ${(CHART.labelWidth + OVERFLOW_FRACTION * CHART.trackWidth + 2).toFixed(1)} ${y + 4} l 6 5 l -6 5"/>`

// One dot per retained sample; the solid full-height tick is the median and
// the hollow diamond is the p95, distinguished by shape, not color.
function samplesBandSvg(row, bandY) {
  const dist = row.dist
  const statusClass = row.pass ? 'pass' : 'fail'
  const count = dist.values.length
  const sizeClass = count <= 5 ? 'bench-dot-lg' : count < 30 ? 'bench-dot-md' : 'bench-dot-sm'
  const radius = count <= 5 ? 4 : count < 30 ? 3.2 : 2.4
  const threshold = dist.plotThreshold ?? row.threshold
  const cy = bandY + 9
  let overflow = false
  const dots = dist.values
    .map((value) => {
      const fraction = fractionOfBudget(value, threshold, row.direction)
      if (fraction > OVERFLOW_FRACTION) overflow = true
      return `<circle class="bench-dot ${sizeClass} ${statusClass}" cx="${fractionX(fraction).toFixed(1)}" cy="${cy}" r="${radius}"/>`
    })
    .join('')
  const p95X = fractionX(fractionOfBudget(dist.p95, threshold, row.direction))
  const medianX = fractionX(fractionOfBudget(dist.median, threshold, row.direction))
  return `<rect x="${CHART.labelWidth}" y="${bandY}" width="${CHART.trackWidth}" height="18" rx="4" class="bench-track"/>${dots}<path class="bench-p95 ${statusClass}" d="M ${p95X.toFixed(1)} ${bandY + 2} L ${(p95X + 5).toFixed(1)} ${cy} L ${p95X.toFixed(1)} ${bandY + 16} L ${(p95X - 5).toFixed(1)} ${cy} Z"/><rect class="bench-median ${statusClass}" x="${(medianX - 1).toFixed(1)}" y="${bandY}" width="2" height="18"/>${overflow ? overflowMarker(bandY, statusClass) : ''}`
}

// Scalar gates keep the original filled budget bar.
function scalarBandSvg(row, bandY) {
  const fraction = fractionOfBudget(row.observed, row.threshold, row.direction)
  return `<rect x="${CHART.labelWidth}" y="${bandY}" width="${CHART.trackWidth}" height="18" rx="4" class="bench-track"/><rect x="${CHART.labelWidth}" y="${bandY}" width="${(clampFraction(fraction) * CHART.trackWidth).toFixed(1)}" height="18" rx="4" class="bench-bar ${row.pass ? 'pass' : 'fail'}"/>${fraction > OVERFLOW_FRACTION ? overflowMarker(bandY, row.pass ? 'pass' : 'fail') : ''}`
}

function ratioStripSvg(stripData, stripY, localMax, unit) {
  const stripX = (value) => CHART.labelWidth + (value / localMax) * CHART.trackWidth
  const cy = stripY + 5
  const dots = stripData.values
    .map((value) => `<circle class="bench-subdot" cx="${stripX(value).toFixed(1)}" cy="${cy}" r="2"/>`)
    .join('')
  return `<text x="${CHART.labelWidth - 8}" y="${stripY + 8}" text-anchor="end" class="bench-sublabel">${escapeHtml(stripData.label)}</text><rect x="${CHART.labelWidth}" y="${stripY}" width="${CHART.trackWidth}" height="10" rx="3" class="bench-subtrack"/>${dots}<rect class="bench-submedian" x="${(stripX(stripData.median) - 1).toFixed(1)}" y="${stripY}" width="2" height="10"/><text x="${CHART.labelWidth + CHART.trackWidth + 8}" y="${stripY + 8}" class="bench-subvalue">med ${escapeHtml(valueWithUnit(stripData.median, unit))}</text>`
}

// Ratio gates: a solid marker for the asserted ratio on the budget axis, then
// the two raw sample arrays it divides as labeled sub-strips on one shared
// local scale. Per-sample ratios are never derived.
function ratioRowSvg(row, rowTop) {
  const bandY = rowTop + 6
  const statusClass = row.pass ? 'pass' : 'fail'
  const fraction = fractionOfBudget(row.observed, row.threshold, row.direction)
  const gateX = fractionX(fraction)
  const localMax = Math.max(...row.dist.num.values, ...row.dist.den.values)
  return `<rect x="${CHART.labelWidth}" y="${bandY}" width="${CHART.trackWidth}" height="18" rx="4" class="bench-track"/><rect class="bench-gate ${statusClass}" x="${(gateX - 1.5).toFixed(1)}" y="${bandY}" width="3" height="18" rx="1"/>${fraction > OVERFLOW_FRACTION ? overflowMarker(bandY, statusClass) : ''}${ratioStripSvg(row.dist.num, rowTop + 32, localMax, row.dist.unit)}${ratioStripSvg(row.dist.den, rowTop + 46, localMax, row.dist.unit)}<text x="${CHART.labelWidth}" y="${rowTop + 68}" class="bench-subscale">both strips share one scale, 0 to ${escapeHtml(valueWithUnit(localMax, row.dist.unit))}</text>`
}

function chartSvg(title, rows) {
  const numeric = rows.filter((row) => row.direction !== '==')
  const parts = []
  const budgetSegments = []
  let y = 4
  for (const row of numeric) {
    const kind = row.dist?.kind ?? 'scalar'
    const rowHeight = kind === 'ratio' ? 74 : 30
    const bandY = y + 6
    const used = fractionOfBudget(row.observed, row.threshold, row.direction)
    const pctLabel = `${(used * 100).toFixed(used < 0.1 ? 1 : 0)}% of budget`
    const result = `${fmt(row.observed)}${row.unit ? ` ${row.unit}` : ''}`
    const budget = `${row.direction === '<=' ? '≤' : '≥'} ${fmt(row.threshold)}${row.unit ? ` ${row.unit}` : ''}`
    const samples = distSummary(row.dist)
    const summary = `${row.label}: ${result}, budget ${budget}, ${pctLabel}, ${row.pass ? 'pass' : 'FAIL'}. ${samples}${row.note ? `. ${row.note}` : ''}`
    const band =
      kind === 'ratio'
        ? ratioRowSvg(row, y)
        : kind === 'samples'
          ? samplesBandSvg(row, bandY)
          : scalarBandSvg(row, bandY)
    // The dashed budget line only spans gate bands: ratio sub-strips use their
    // own local scale, where the budget position would be meaningless.
    budgetSegments.push(`<line x1="${CHART.labelWidth + CHART.trackWidth}" y1="${y}" x2="${CHART.labelWidth + CHART.trackWidth}" y2="${y + 30}" class="bench-budget-line"/>`)
    parts.push(`
    <g class="bench-row" tabindex="0" role="img" aria-label="${escapeHtml(summary)}"
       data-label="${escapeDatasetHtml(row.label)}" data-result="${escapeDatasetHtml(result)}"
       data-budget="${escapeDatasetHtml(budget)}" data-pct="${escapeDatasetHtml(pctLabel)}"
       data-pass="${row.pass}" data-samples="${escapeDatasetHtml(samples)}"${row.note ? ` data-note="${escapeDatasetHtml(row.note)}"` : ''}>
      <rect x="2" y="${y + 3}" width="696" height="${rowHeight - 6}" rx="6" class="bench-hit"/>
      <text x="${CHART.labelWidth - 8}" y="${bandY + 13}" text-anchor="end" class="bench-label">${escapeHtml(row.label)}</text>
      ${band}
      <text x="${CHART.labelWidth + CHART.trackWidth + 8}" y="${bandY + 13}" class="bench-value">${escapeHtml(result)}</text>
    </g>`)
    y += rowHeight
  }
  const height = y + 4
  return `<svg class="bench-chart" viewBox="0 0 700 ${height}" role="group" aria-label="${escapeHtml(
    title,
  )}: each row places every retained sample as a dot at its share of the frozen budget; the dashed line is the budget, the solid tick is the median, and the hollow diamond is the p95. Hover or focus a row for details.">
  ${budgetSegments.join('')}
  ${parts.join('')}
</svg>`
}

function tableHtml(rows) {
  const body = rows
    .map((row) => {
      const budget = row.direction === '==' ? `exactly ${fmt(row.threshold)}` : `${row.direction === '<=' ? '≤' : '≥'} ${fmt(row.threshold)}${row.unit ? ` ${row.unit}` : ''}`
      return `<tr><td>${escapeHtml(row.label)}</td><td class="num">${fmt(row.observed)}${row.unit ? ` ${row.unit}` : ''}</td><td class="num">${budget}</td><td class="${row.pass ? 'bench-pass' : 'bench-fail'}">${row.pass ? '✓ pass' : '✗ fail'}</td></tr>`
    })
    .join('\n')
  return `<div class="table-wrap"><table>
<thead><tr><th>Boundary</th><th class="num">Result</th><th class="num">Budget</th><th>Status</th></tr></thead>
<tbody>${body}</tbody>
</table></div>`
}

function tableMarkdown(rows) {
  const body = rows
    .map((row) => {
      const budget = row.direction === '=='
        ? `exactly ${fmt(row.threshold)}`
        : `${row.direction === '<=' ? '≤' : '≥'} ${fmt(row.threshold)}${row.unit ? ` ${row.unit}` : ''}`
      return `| ${row.label} | ${fmt(row.observed)}${row.unit ? ` ${row.unit}` : ''} | ${budget} | ${row.pass ? 'pass' : 'fail'} |`
    })
    .join('\n')
  return `| Boundary | Result | Budget | Status |
| --- | ---: | ---: | --- |
${body}`
}

function adjudicationMarkdown(adjudication) {
  if (!adjudication?.triggered) return ''
  const band = `${(adjudication.bandFraction * 100).toFixed(0)}%`
  const triggers = adjudication.triggeredBy.map((name) => `\`${name}\``).join(', ')
  const reports = adjudication.reports.map(({ path: reportPath }) => `\`${reportPath}\``).join(', ')
  return `**Near-threshold adjudication.** ${triggers} entered the unchanged ${band} band, so the aggregate required exactly ${adjudication.requiredReports - 1} additional fresh identity-matched reports. Only triggering assertions receive two-of-three tolerance; every other assertion and invariant must pass in every report. The representative is selected by median normalized budget pressure with a stable report-path tie-break, never by choosing the fastest passing sample. Any failure more than ${band} beyond its threshold, or a red selected representative, fails the aggregate. Retained reports: ${reports}.`
}

export const benchmarkHeadings = FAMILIES.map(({ family, title }) => ({
  depth: 2,
  id: family,
  text: title,
}))

export async function benchmarksSectionsHtml() {
  const sections = []
  for (const { family, title, rows: extract, note } of FAMILIES) {
    const { file, report, adjudication } = await latestReport(family)
    const rows = extract({ report })
    const when = report.generatedAtUnixMs
      ? new Date(report.generatedAtUnixMs).toISOString().slice(0, 10)
      : (report.timestamp ?? '').slice(0, 10)
    const allPass = rows.every((row) => row.pass)
    sections.push(`
<h2 id="${family}">${escapeHtml(title)}</h2>
<p>${escapeHtml(note)} Report: <code>${escapeHtml(file)}</code> (${escapeHtml(when)}), ${
      allPass ? 'every budget passed' : 'BUDGET FAILURES PRESENT'
    }.</p>
${chartSvg(title, rows)}
${tableHtml(rows)}
${adjudicationHtml(adjudication)}`)
  }
  return sections.join('\n')
}

export async function benchmarksSectionsMarkdown() {
  const sections = []
  for (const { family, title, rows: extract, note } of FAMILIES) {
    const { file, report, adjudication } = await latestReport(family)
    const rows = extract({ report })
    const when = report.generatedAtUnixMs
      ? new Date(report.generatedAtUnixMs).toISOString().slice(0, 10)
      : (report.timestamp ?? '').slice(0, 10)
    const allPass = rows.every((row) => row.pass)
    sections.push(`## ${title}

${note} Report: \`${file}\` (${when}), ${allPass ? 'every budget passed' : 'BUDGET FAILURES PRESENT'}.

${tableMarkdown(rows)}${adjudication ? `\n\n${adjudicationMarkdown(adjudication)}` : ''}`)
  }
  return sections.join('\n\n')
}

// Headline rows for the home page chart, picked from the same normalized
// rows. Labels are plain language, the visible number is the real measured
// value, and each note explains the metric in the hover/focus tooltip.
const HOME_PICKS = [
  {
    family: 'native-lint',
    match: /median standard-path latency ratio/i,
    label: 'Ordinary-file lint overhead',
    emoji: '⚖️',
    hue: 'violet',
    unit: '×',
    note: 'Median ordinary-TSX latency through the product direct path versus the same-build canonical OXC adapter control. 1.00× is parity; this is not the stock Oxlint npm/CLI boundary.',
  },
  {
    family: 'native-format',
    match: /sequential throughput vs absolute/i,
    label: 'Sequential formatting',
    emoji: '✨',
    hue: 'orange',
    unit: 'MiB/s',
    note: 'Sequential formatter throughput. The ≥16.6 MiB/s gate is an absolute regression floor derived from a non-comparable historical result; it is not a speedup comparison.',
  },
  {
    family: 'native-lint',
    match: /fresh-process TSRX p95 latency/i,
    label: 'Cold start: lint a file from scratch',
    emoji: '❄️',
    hue: 'aqua',
    unit: 'ms',
    note: 'Launching a brand-new process and linting a TSRX file end to end, 95th percentile. There is no warmup to wait for.',
  },
  {
    family: 'editor',
    match: /edit-to-diagnostics p95/i,
    label: 'Native editor diagnostics',
    emoji: '💡',
    hue: 'yellow',
    unit: 'ms',
    note: 'Direct local stdio language-server edit-to-diagnostics round trip, excluding VS Code rendering.',
  },
  {
    family: 'native-lint',
    match: /median scan\+copy\+parse throughput/i,
    label: 'Reading TSRX source',
    emoji: '📖',
    hue: 'magenta',
    unit: 'MiB/s',
    note: 'How fast TSRX source moves through the extra work this project adds: scanning your file, building the in-memory TSX copy, and parsing it.',
  },
  {
    family: 'type-aware',
    match: /single-file type-aware p95/i,
    label: 'Type-aware lint (opt-in)',
    emoji: '🔬',
    hue: 'blue',
    unit: 'ms',
    note: 'A single-file lint with full TypeScript type information, 95th percentile. Off by default; the standard lane stays syntax-only.',
  },
]

export async function homeBenchmarksHtml() {
  const rowsByFamily = {}
  for (const { family, rows: extract } of FAMILIES) {
    const { report } = await latestReport(family)
    rowsByFamily[family] = extract({ report })
  }
  const picked = []
  for (const pick of HOME_PICKS) {
    const row = rowsByFamily[pick.family]?.find((candidate) => pick.match.test(candidate.label))
    if (!row) continue
    picked.push({
      ...row,
      label: pick.label,
      emoji: pick.emoji,
      hue: pick.hue,
      unit: row.unit || pick.unit,
      note: typeof pick.note === 'function' ? pick.note(row) : pick.note,
    })
  }
  const cards = picked
    .map((row) => {
      const value = fmtHeadline(row.observed, row.unit)
      const result = withUnit(row.observed, row.unit)
      const gate = `${row.direction === '<=' ? '≤' : '≥'} ${withUnit(row.threshold, row.unit)}`
      const status = row.pass ? 'passing' : 'FAILING'
      // How close the result sits to its frozen budget: the meter fill, with
      // the dashed line at the right edge marking the budget itself.
      const used = row.direction === '<=' ? row.observed / row.threshold : row.threshold / row.observed
      const usedPct = Math.max(Math.min(used, 1.1) * 100, 1).toFixed(1)
      const summary = `${row.label}: ${result}, release gate ${gate}, ${status}. ${row.note}`
      return `
  <div class="bench-row gate-card gate-hue-${row.hue}" tabindex="0" role="img" aria-label="${escapeHtml(summary)}"
     data-label="${escapeDatasetHtml(row.label)}" data-result="${escapeDatasetHtml(result)}"
     data-budget="${escapeDatasetHtml(gate)}" data-pct="${escapeDatasetHtml(`frozen release gate ${status}`)}"
     data-pass="${row.pass}" data-note="${escapeDatasetHtml(row.note)}">
    <span class="gate-value">${escapeHtml(value)}</span>
    <span class="gate-label"><span class="gate-emoji" aria-hidden="true">${row.emoji}</span>${escapeHtml(row.label)}</span>
    <span class="gate-meter" aria-hidden="true"><span class="gate-meter-fill${row.pass ? '' : ' fail'}" style="width:${usedPct}%"></span></span>
    <span class="gate-budget ${row.pass ? 'pass' : 'fail'}"><span class="gate-status" aria-hidden="true">${row.pass ? '✓' : '✗'}</span>gate ${escapeHtml(gate)}</span>
  </div>`
    })
    .join('')
  return `<div class="gate-grid" role="group" aria-label="Selected frozen release gates. Each card shows the measured value, and a thin meter shows how close it sits to the frozen budget marked by the dashed line. Hover or focus a card for what it measures.">${cards}</div>
<p class="home-bench-caption">Each card is one release gate: the big number is the measured value, and the thin bar shows how close it sits to its frozen budget (the dashed line). If a result ever crosses its budget, the release fails. Hover or focus a card for what exactly is measured.</p>`
}


export async function comparativeChartHtml() {
  const { file, report } = await latestReport('comparative')
  const assertions = report.assertions
  const budgets = report.budgets
  const bars = [
    {
      key: 'eslint',
      name: 'ESLint + typescript-eslint',
      ms: report.tools.eslint.medianMs,
      cls: 'other',
      pass: assertions.fasterThanEslint,
      gate: `ESLint / OXC for TSRX ≥ ${budgets.eslintVsOxcTsrxMin}×`,
    },
    {
      key: 'oxlint',
      name: 'official Oxlint',
      ms: report.tools.oxlint.medianMs,
      cls: 'other',
      pass: assertions.nearOxlintParity,
      gate: `OXC for TSRX / official Oxlint ≤ ${budgets.oxcTsrxVsOxlintMax}×`,
    },
    {
      key: 'oxcTsrx',
      name: 'OXC for TSRX (oxlint-tsrx)',
      ms: report.tools.oxcTsrx.medianMs,
      cls: 'ours',
      pass: assertions.nearOxlintParity && assertions.fasterThanEslint,
      gate: `matched ratios ≤ ${budgets.oxcTsrxVsOxlintMax}× Oxlint and ≥ ${budgets.eslintVsOxcTsrxMin}× over ESLint`,
      badge: 'same-process official Oxlint route',
    },
    {
      key: 'oxcTsrxMixed',
      name: 'OXC for TSRX · mixed file types',
      ms: report.tools.oxcTsrxMixed.medianMs,
      cls: 'ours mixed',
      pass: assertions.mixedNoBlowup,
      gate: `paired mixed / all-TSX ≤ ${budgets.mixedVsTsxMax}×`,
    },
  ]
  const max = Math.max(...bars.map((bar) => bar.ms))
  const rows = bars
    .map((bar) => {
      const widthPct = Math.max((bar.ms / max) * 100, 0.8)
      const label = `${bar.ms >= 100 ? bar.ms.toFixed(0) : bar.ms.toFixed(1)} ms`
      const mixed = bar.key === 'oxcTsrxMixed'
      const comparison = mixed
        ? `${report.ratios.mixedVsTsx.toFixed(3)}× versus the paired all-TSX product workload`
        : `${(report.tools.eslint.medianMs / bar.ms).toFixed(1)}× relative to the matched ESLint lane`
      let note = mixed
        ? `Median wall-clock time for the paired mixed-file-types workload (${report.corpus.files} components, ${Math.round(report.corpus.tsrxShare * 100)}% TSRX), through the oxlint-tsrx npm command. This is an internal product workload ratio, not a cross-tool speed comparison.`
        : `Median wall-clock time for the same byte-identical ${report.corpus.files}-file TSX corpus, one no-debugger rule, one explicit file list, and zero-diagnostic default output. Every tool is launched through its npm command.`
      if (bar.key === 'oxcTsrx') {
        note = `Median wall-clock time for the same corpus through the oxlint-tsrx npm command. It imports the exact manifest-declared official Oxlint launcher in the same Node process with zero TSRX dispatch.`
      } else if (bar.key === 'oxcTsrxMixed') {
        const route = report.validation.routeEvidence
        note += ` The measured route proves ${route.publicCanonicalNodeChildren} public canonical Node child, ${route.nativeTsrxChildren} native TSRX child, and ${route.privateInProcessAdapterChildren} private adapter children.`
      }
      const badge = bar.badge ? `<span class="comp-badge">${escapeHtml(bar.badge)}</span>` : ''
      return `
  <div class="bench-row comp-row comp-${bar.cls.split(' ')[0]}" tabindex="0" role="img" aria-label="${escapeHtml(`${bar.name}: ${label} median`)}"
     data-label="${escapeDatasetHtml(bar.name)}" data-result="${escapeDatasetHtml(label + ' median')}"
     data-budget="${escapeDatasetHtml(bar.gate)}" data-pct="${escapeDatasetHtml(comparison)}" data-pass="${bar.pass}" data-note="${escapeDatasetHtml(note)}">
    <span class="comp-head"><span class="comp-name">${escapeHtml(bar.name)}${badge}</span><span class="comp-time">${label}</span></span>
    <span class="comp-track"><span class="bench-bar comp-fill${bar.cls.includes('mixed') ? ' comp-mixed' : ` comp-${bar.cls}`}" style="width:${widthPct.toFixed(1)}%"></span></span>
  </div>`
    })
    .join('')
  const when = (report.timestamp ?? '').slice(0, 10)
  const route = report.validation.routeEvidence
  return `<div class="comp-chart" role="group" aria-label="Median CLI lint time on one matched ${report.corpus.files}-file TSX corpus plus a separate paired mixed-file-types workload. Shorter bars are faster.">${rows}
</div>
<p class="home-bench-caption">Bar lengths show absolute median wall-clock time. Matched cross-tool bars use the same ${report.corpus.files} TSX files with one rule; the separately patterned mixed bar is the paired internal workload of mixed file types (TSX plus TSRX). Every tool crosses the npm boundary: each time is the npm command a project actually runs, Node launcher included. Shorter is faster.</p>
<details class="bench-fine">
  <summary>Methodology, versions, and gates</summary>
  <p>All three tools lint the same explicit list of ${report.corpus.files} byte-identical TSX files (${(report.corpus.tsxBytes / 1024).toFixed(0)} KiB) with the <code>no-debugger</code> rule and zero-diagnostic default output, on the same machine (${escapeHtml(report.host.cpu)}). Every lane is measured through its npm CLI entry point, so each time includes that tool's own Node launcher. For the all-TSX product lane, <code>oxlint-tsrx</code> imports the exact manifest-declared official Oxlint launcher in the same Node process with zero TSRX dispatch. The separate mixed-file-types lane (${Math.round(report.corpus.tsrxShare * 100)}% TSRX by file count) proves ${route.publicCanonicalNodeChildren} public canonical Node child, ${route.nativeTsrxChildren} native TSRX child, and ${route.privateInProcessAdapterChildren} private adapter children; it is a paired internal workload, not a cross-tool comparison. Each time is the median of ${report.samplePolicy.measured} measured processes after ${report.samplePolicy.warmups} warmups. Hover any bar for its frozen ratio gate: the release fails if a future build breaks it. Versions: ESLint ${escapeHtml(report.versions.eslint)} with typescript-eslint ${escapeHtml(report.versions.typescriptEslint)}; official Oxlint ${escapeHtml(report.versions.oxlint.replace('Version: ', ''))}. Report <code>${escapeHtml(file)}</code> (${when}).</p>
</details>`
}

export async function latestReportDates() {
  const dates = []
  for (const { family } of FAMILIES) {
    const { report } = await latestReport(family)
    const when = report.generatedAtUnixMs
      ? new Date(report.generatedAtUnixMs)
      : new Date(report.timestamp)
    dates.push(when)
  }
  return dates.sort((a, b) => b - a)[0]
}
