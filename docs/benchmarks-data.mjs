// Reads the committed benchmark reports and produces the tables and charts on
// the benchmarks page at build time, so the page can never go stale: rerun the
// benchmarks, rebuild, done.
import { readFile } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

// Charts are rendered at build time with ECharts in SSR mode: pure option
// objects (testable without a browser) turned into static SVG strings, one
// light and one dark variant per chart. No chart runtime ships to the client.
import * as echarts from 'echarts'

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
  // The policy itself is stated once on the page. Per family, all that is left
  // to say is which assertion triggered it and which reports were retained.
  return `<p class="bench-adjudication"><strong>Near-threshold adjudication.</strong> ${triggers} landed inside the ${band} band, so this family required ${adjudication.requiredReports - 1} additional fresh identity-matched reports and kept the median-pressure representative of the three. Retained: ${reports}.</p>`
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
  { family: 'comparative', title: 'Matched 1,000-file CLI comparison', rows: comparativeRows, note: 'ESLint, official Oxlint, and OXC for TSRX lint the same byte-identical TSX files, with one no-debugger rule, through each tool’s own npm entry point, over 5 warmups and 20 measured processes. The separate mixed-file-types row is a paired internal workload rather than a cross-tool comparison, and it is the only lane that enters the native TSRX path.' },
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

// ---------- chart rendering: one ECharts small-multiple chart per gate row ----------
const fractionOfBudget = (value, threshold, direction) =>
  direction === '<=' ? value / threshold : threshold / value

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

// Theme palettes reuse the site's tokens (docs/assets/style.css): brand violet
// as the single series accent, neutral ink for every piece of text, and the
// pass/fail state carried by words, never by color alone.
const CHART_FONT =
  "Inter, -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif"
const CHART_THEMES = {
  light: {
    ink: '#3c3c43',
    inkSoft: '#545560',
    inkFaint: '#6e6e76',
    axisLine: '#c9c9ce',
    gridLine: '#ececee',
    accent: '#6d28d9',
    dotOpacity: 0.35,
    dotOpacityFew: 0.85,
    budget: '#6e6e76',
    breach: 'rgba(220, 38, 38, 0.055)',
  },
  dark: {
    ink: '#dfdfd6',
    inkSoft: '#b5b5ad',
    inkFaint: '#98989f',
    axisLine: '#45454b',
    gridLine: '#2a2a2e',
    accent: '#c4b5fd',
    dotOpacity: 0.5,
    dotOpacityFew: 0.95,
    budget: '#98989f',
    breach: 'rgba(248, 113, 113, 0.08)',
  },
}

// Nice axis bounds so every sample and the budget line sit inside the plot
// with padding, and tick labels stay round numbers.
const NICE_MAX = [1, 1.2, 1.5, 2, 2.5, 3, 4, 5, 6, 8, 10]
const niceCeil = (x) => {
  if (!(x > 0)) return 1
  const exp = Math.floor(Math.log10(x))
  const base = x / 10 ** exp
  const m = NICE_MAX.find((n) => base <= n + 1e-9) ?? 10
  return m * 10 ** exp
}
const niceStep = (x) => {
  if (!(x > 0)) return 1
  const exp = Math.floor(Math.log10(x))
  const base = x / 10 ** exp
  const m = [1, 2, 2.5, 5, 10].find((n) => base <= n + 1e-9) ?? 10
  return m * 10 ** exp
}
// Zoomed axis for the sample strip: fitted to the retained values so spread
// stays visible even when the budget is 100x away. The tick labels make the
// zoom explicit.
function fittedAxis(values) {
  const lo = Math.min(...values)
  const hi = Math.max(...values)
  const span = hi - lo || Math.abs(hi) * 0.02 || 1
  const step = niceStep(span / 3)
  return {
    min: Math.max(0, Math.floor((lo - span * 0.1) / step) * step),
    max: Math.ceil((hi + span * 0.1) / step) * step,
    interval: step,
  }
}
const tickLabel = (value) => String(Number(Number(value).toFixed(6)))

// Every chart uses the same two-strip shape: the top strip places the gated
// value on a zero-based axis with the dashed frozen-budget line and a shaded
// fail region, and the bottom strip zooms into the retained samples (or, for
// ratio gates, the two raw runs the ratio divides).
const GRID = { left: 200, right: 64, gateTop: 52, stripHeight: 30, lowerTop: 118, ratioHeight: 58 }
const CHART_WIDTH = 700
export const chartHeightFor = (kind) => (kind === 'ratio' ? 200 : kind === 'samples' ? 176 : 108)

const budgetSymbol = (direction) => (direction === '<=' ? '≤' : '≥')

// The unit the gate strip is plotted in. RSS gates assert bytes but plot MiB
// (same fraction of budget, readable numbers), signaled by plotThreshold.
function gateGeometry(row) {
  const dist = row.dist ?? { kind: 'scalar' }
  const unit = dist.kind === 'ratio' ? '×' : dist.kind === 'samples' ? dist.unit : row.unit
  const value = dist.plotThreshold ? row.observed / MIB : row.observed
  const budget = dist.plotThreshold ?? row.threshold
  return { unit, value, budget }
}

export function benchmarkChartOption(row, themeName = 'light') {
  const t = CHART_THEMES[themeName]
  const dist = row.dist ?? { kind: 'scalar' }
  const kind = dist.kind
  const gate = gateGeometry(row)
  const gateMax = niceCeil(Math.max(gate.value, gate.budget) * 1.12)
  const status = row.pass ? 'pass' : 'FAIL'
  const failRegion =
    row.direction === '<='
      ? [{ xAxis: gate.budget }, { xAxis: gateMax }]
      : [{ xAxis: 0 }, { xAxis: gate.budget }]

  const axisCommon = {
    type: 'value',
    axisLine: { show: true, lineStyle: { color: t.axisLine } },
    axisTick: { show: false },
    splitLine: { show: true, lineStyle: { color: t.gridLine } },
    axisLabel: { color: t.inkFaint, fontSize: 11, fontFamily: CHART_FONT, formatter: tickLabel },
    nameTextStyle: { color: t.inkFaint, fontSize: 11, fontFamily: CHART_FONT },
    nameGap: 8,
  }
  const categoryAxis = (labels) => ({
    type: 'category',
    data: labels,
    axisLine: { show: false },
    axisTick: { show: false },
    axisLabel: {
      color: t.inkSoft,
      fontSize: 11,
      fontFamily: CHART_FONT,
      width: 184,
      overflow: 'truncate',
    },
  })

  const gateSeries = {
    name: 'gate',
    type: 'scatter',
    xAxisIndex: 0,
    yAxisIndex: 0,
    clip: false,
    symbol: 'diamond',
    symbolSize: 13,
    itemStyle: { color: t.accent },
    data: [[gate.value, 0]],
    markLine: {
      symbol: 'none',
      silent: true,
      animation: false,
      lineStyle: { color: t.budget, type: [6, 4], width: 1.5 },
      label: {
        show: true,
        formatter: 'budget',
        position: 'insideEndTop',
        rotate: 0,
        color: t.inkSoft,
        fontSize: 10,
        fontFamily: CHART_FONT,
      },
      data: [{ xAxis: gate.budget }],
    },
    markArea: { silent: true, itemStyle: { color: t.breach }, data: [failRegion] },
  }

  const budgetLabel = `budget ${budgetSymbol(row.direction)} ${valueWithUnit(gate.budget, gate.unit)}`
  const grids = [
    { left: GRID.left, right: GRID.right, top: GRID.gateTop, height: GRID.stripHeight },
  ]
  const xAxes = [{ ...axisCommon, gridIndex: 0, min: 0, max: gateMax, name: unitText(gate.unit).trim() }]
  const yAxes = [{ ...categoryAxis([budgetLabel]), gridIndex: 0 }]
  const series = [gateSeries]
  let subtext

  if (kind === 'samples') {
    const n = dist.values.length
    const axis = fittedAxis(dist.values)
    grids.push({ left: GRID.left, right: GRID.right, top: GRID.lowerTop, height: GRID.stripHeight })
    xAxes.push({ ...axisCommon, gridIndex: 1, ...axis, name: unitText(dist.unit).trim() })
    yAxes.push({ ...categoryAxis([`all ${n} samples`]), gridIndex: 1 })
    series.push({
      name: 'samples',
      type: 'scatter',
      xAxisIndex: 1,
      yAxisIndex: 1,
      clip: false,
      symbol: 'circle',
      symbolSize: n <= 5 ? 10 : n >= 60 ? 7 : 8,
      itemStyle: { color: t.accent, opacity: n <= 5 ? t.dotOpacityFew : t.dotOpacity },
      data: dist.values.map((value) => [value, 0]),
      markLine: {
        symbol: 'none',
        silent: true,
        animation: false,
        label: { show: false },
        data: [
          { xAxis: dist.median, lineStyle: { color: t.ink, type: 'solid', width: 2 } },
          { xAxis: dist.p95, lineStyle: { color: t.ink, type: [2, 3], width: 2 } },
        ],
      },
    })
    subtext = `measured ${valueWithUnit(gate.value, gate.unit)} · ${status} · below: ${n} samples, median ${valueWithUnit(dist.median, dist.unit)} (solid line), p95 ${valueWithUnit(dist.p95, dist.unit)} (dotted line)`
  } else if (kind === 'ratio') {
    const labels = [dist.den.label, dist.num.label]
    const localMax = niceCeil(Math.max(...dist.num.values, ...dist.den.values) * 1.08)
    grids.push({ left: GRID.left, right: GRID.right, top: GRID.lowerTop, height: GRID.ratioHeight })
    xAxes.push({ ...axisCommon, gridIndex: 1, min: 0, max: localMax, name: unitText(dist.unit).trim() })
    yAxes.push({ ...categoryAxis(labels), gridIndex: 1 })
    const sideSeries = (name, side) => ({
      name,
      type: 'scatter',
      xAxisIndex: 1,
      yAxisIndex: 1,
      clip: false,
      symbol: 'circle',
      symbolSize: 8,
      itemStyle: { color: t.accent, opacity: side.values.length <= 5 ? t.dotOpacityFew : t.dotOpacity },
      data: side.values.map((value) => [value, side.label]),
    })
    series.push(sideSeries('numerator samples', dist.num), sideSeries('denominator samples', dist.den))
    series.push({
      name: 'medians',
      type: 'scatter',
      xAxisIndex: 1,
      yAxisIndex: 1,
      clip: false,
      symbol: 'rect',
      symbolSize: [3, 22],
      itemStyle: { color: t.ink },
      data: [
        [dist.num.median, dist.num.label],
        [dist.den.median, dist.den.label],
      ],
    })
    subtext = `asserted ratio ${valueWithUnit(gate.value, '×')} · ${status} · below: the two runs it divides, sampled independently, so no per-sample ratios`
  } else {
    subtext = `single measurement per report: ${valueWithUnit(gate.value, gate.unit)} · ${status}`
  }

  return {
    animation: false,
    title: {
      text: row.label,
      subtext,
      top: 4,
      left: 2,
      itemGap: 3,
      textStyle: { color: t.ink, fontSize: 13, fontWeight: 600, fontFamily: CHART_FONT },
      subtextStyle: { color: t.inkSoft, fontSize: 11, fontFamily: CHART_FONT },
    },
    grid: grids,
    xAxis: xAxes,
    yAxis: yAxes,
    series,
  }
}

function renderChartSvg(option, width, height) {
  const chart = echarts.init(null, null, { renderer: 'svg', ssr: true, width, height })
  chart.setOption(option)
  const svg = chart.renderToSVGString()
  chart.dispose()
  return svg
}

// One figure per gate row: the light and dark renders side by side, with CSS
// showing exactly one depending on html.dark. Screen readers get the same
// summary the old chart exposed.
function chartFigure(row) {
  const kind = row.dist?.kind ?? 'scalar'
  const height = chartHeightFor(kind)
  const used = fractionOfBudget(row.observed, row.threshold, row.direction)
  const pctLabel = `${(used * 100).toFixed(used < 0.1 ? 1 : 0)}% of budget`
  const result = `${fmt(row.observed)}${row.unit ? ` ${row.unit}` : ''}`
  const budget = `${budgetSymbol(row.direction)} ${fmt(row.threshold)}${row.unit ? ` ${row.unit}` : ''}`
  const summary = `${row.label}: ${result}, budget ${budget}, ${pctLabel}, ${row.pass ? 'pass' : 'FAIL'}. ${distSummary(row.dist)}${row.note ? `. ${row.note}` : ''}`
  const light = renderChartSvg(benchmarkChartOption(row, 'light'), CHART_WIDTH, height)
  const dark = renderChartSvg(benchmarkChartOption(row, 'dark'), CHART_WIDTH, height)
  return `<figure class="bench-echart" role="img" aria-label="${escapeHtml(summary)}"><div class="bench-echart-light" aria-hidden="true">${light}</div><div class="bench-echart-dark" aria-hidden="true">${dark}</div></figure>`
}

function chartsHtml(rows) {
  const numeric = rows.filter((row) => row.direction !== '==')
  return `<div class="bench-chart">
${numeric.map((row) => chartFigure(row)).join('\n')}
</div>`
}

function tableHtml(rows) {
  const body = rows
    .map((row) => {
      const budget = row.direction === '==' ? `exactly ${fmt(row.threshold)}` : `${row.direction === '<=' ? '≤' : '≥'} ${fmt(row.threshold)}${row.unit ? ` ${row.unit}` : ''}`
      const cells = `<td>${escapeHtml(row.label)}</td><td class="num">${fmt(row.observed)}${row.unit ? ` ${row.unit}` : ''}</td><td class="num">${budget}</td><td class="${row.pass ? 'bench-pass' : 'bench-fail'}">${row.pass ? '✓ pass' : '✗ fail'}</td>`
      if (row.direction === '==') return `<tr>${cells}</tr>`
      // Numeric gate rows keep the hover/focus tooltip contract: dataset
      // values stay double-escaped for the innerHTML consumer in app.js.
      const used = fractionOfBudget(row.observed, row.threshold, row.direction)
      const pctLabel = `${(used * 100).toFixed(used < 0.1 ? 1 : 0)}% of budget`
      const result = `${fmt(row.observed)}${row.unit ? ` ${row.unit}` : ''}`
      return `<tr class="bench-row" tabindex="0" data-label="${escapeDatasetHtml(row.label)}" data-result="${escapeDatasetHtml(result)}" data-budget="${escapeDatasetHtml(budget)}" data-pct="${escapeDatasetHtml(pctLabel)}" data-pass="${row.pass}" data-samples="${escapeDatasetHtml(distSummary(row.dist))}"${row.note ? ` data-note="${escapeDatasetHtml(row.note)}"` : ''}>${cells}</tr>`
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

// Every gate keeps its chart, because the point of publishing them is that the
// retained samples are there to check. Read end to end, though, that is forty-odd
// charts and seventy table rows, which is a page nobody finishes. So the matched
// cross-tool comparison, the one result most readers came for, stays open, and
// each release-gate family collapses to a headline sentence with its numbers one
// click away.
export async function benchmarksSectionsHtml() {
  const sections = []
  for (const { family, title, rows: extract, note } of FAMILIES) {
    const { file, report, adjudication } = await latestReport(family)
    const rows = extract({ report })
    const when = report.generatedAtUnixMs
      ? new Date(report.generatedAtUnixMs).toISOString().slice(0, 10)
      : (report.timestamp ?? '').slice(0, 10)
    const allPass = rows.every((row) => row.pass)
    const verdict = allPass ? 'every budget passed' : 'BUDGET FAILURES PRESENT'
    const evidence = `${tableHtml(rows)}\n${chartsHtml(rows)}\n${adjudicationHtml(adjudication)}`
    const headline = `<p>${escapeHtml(note)} Report: <code>${escapeHtml(file)}</code> (${escapeHtml(
      when,
    )}), ${verdict}.</p>`
    sections.push(
      family === 'comparative'
        ? `
<h2 id="${family}">${escapeHtml(title)}</h2>
${headline}
${chartsHtml(rows)}
${tableHtml(rows)}
${adjudicationHtml(adjudication)}`
        : `
<h2 id="${family}">${escapeHtml(title)}</h2>
${headline}
<details class="bench-details">
<summary>${rows.length} ${rows.length === 1 ? 'budget' : 'budgets'}, ${verdict}. Show the numbers and the per-gate charts</summary>
${evidence}
</details>`,
    )
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
//
// House style for these notes: write for a developer who has never seen this
// repo. Say what was run and what the number means, spell out p95 rather than
// naming it, and keep every "this is not a comparison against X" disclaimer
// intact. Simpler is not allowed to become vaguer about what is being claimed.
//
// `aside` is the one thing a card says about itself WITHOUT hover, printed
// under the meter next to the gate. It exists for the parity card: a ratio
// under 1.00 sitting above a chart where official Oxlint is faster than us
// reads as a contradiction unless the card itself says what the two sides are.
//
// `ariaNote` is the same explanation written for someone listening rather than
// reading. `note` is a tooltip a sighted reader scans and can re-read at will;
// the screen-reader summary is spoken in one uninterrupted run, so a tighter
// sentence is the better experience there, not a lesser one. It also stops the
// page shipping the same 240 characters twice per card. Every disclaimer that
// the visible note carries has to survive in the spoken one, so where a note is
// already short enough to speak, `ariaNote` is left off and `note` is used.
const HOME_PICKS = [
  {
    family: 'native-lint',
    match: /median standard-path latency ratio/i,
    label: 'Ordinary files: parity with the OXC path we wrap',
    unit: '×',
    aside: '1.00× is the target. Internal same-build check, not official Oxlint.',
    note: 'Time to lint an ordinary TSX file with this project, divided by the time plain OXC takes on the same file from the same build. 1.00× means we add no cost. Both sides run in one process, so this is an internal check, not a race against the official Oxlint command from npm.',
    // The aside is read out immediately before this, and it already says
    // "1.00× is the target. Internal same-build check, not official Oxlint.",
    // so the spoken form does not repeat either point.
    ariaNote: 'Our lint time on an ordinary TSX file, divided by plain OXC on the same file from the same build.',
  },
  {
    family: 'native-format',
    match: /sequential throughput vs absolute/i,
    label: 'Sequential formatting',
    unit: 'MiB/s',
    note: 'How many megabytes of source the formatter gets through per second on one thread. The floor it clears is a line we refuse to drop below, taken from an earlier measurement of our own formatter. It is not a comparison against another tool.',
    ariaNote: 'Megabytes of source formatted per second on one thread. The floor comes from an earlier measurement of our own formatter, not from another tool.',
  },
  {
    family: 'native-lint',
    match: /fresh-process TSRX p95 latency/i,
    label: 'Cold start: lint a file from scratch',
    unit: 'ms',
    note: 'Time to start a brand-new process and lint one TSRX file, from launch to finished. There is no warmup to wait for. Timed at the slow end: 95 out of 100 runs are this fast or faster.',
    ariaNote: 'Time to start a brand-new process and lint one TSRX file, launch to finished, with no warmup. Slow end: 95 of 100 runs are this fast or faster.',
  },
  {
    family: 'editor',
    match: /edit-to-diagnostics p95/i,
    label: 'Native editor diagnostics',
    unit: 'ms',
    note: 'Time from your keystroke to the squiggly underline being ready, measured at the language server on your own machine. It does not count the time VS Code spends drawing it. Timed at the slow end: 95 out of 100 edits are this fast or faster.',
    ariaNote: 'Time from your keystroke to the squiggly underline being ready at the language server, not counting the time VS Code spends drawing it. Slow end: 95 of 100 edits are this fast or faster.',
  },
  {
    family: 'native-lint',
    match: /median scan\+copy\+parse throughput/i,
    label: 'Reading TSRX source',
    unit: 'MiB/s',
    note: 'How fast TSRX source moves through the extra work this project adds: scanning your file, building the in-memory TSX copy, and parsing it.',
  },
  {
    family: 'type-aware',
    match: /single-file type-aware p95/i,
    label: 'Type-aware lint (opt-in)',
    unit: 'ms',
    note: 'Linting one file with full TypeScript type information, the slower check that has to ask the type checker. Off unless you turn it on, so the everyday lint stays syntax only. Timed at the slow end: 95 out of 100 runs are this fast or faster.',
    ariaNote: 'Linting one file with full TypeScript type information, the slower check that asks the type checker. Off unless you turn it on. Slow end: 95 of 100 runs are this fast or faster.',
  },
]

// The normalized rows per family, exported so the chart parity test can check
// every plotted value against its retained sample array without scraping SVG.
export async function benchmarkRowsByFamily() {
  const rowsByFamily = {}
  for (const { family, rows: extract } of FAMILIES) {
    const { report } = await latestReport(family)
    rowsByFamily[family] = extract({ report })
  }
  return rowsByFamily
}

// What fraction of what this gate ALLOWS has been spent, which is the only
// thing the meter is drawing. The answer depends on where the gate's natural
// origin sits, and the two cases are genuinely different:
//
//   * A millisecond, MiB or MiB/s gate starts at zero. Spending none of a
//     50 ms budget means 0 ms, so observed / threshold is the whole answer.
//   * A RATIO gate starts at 1.00, not zero. Its budget is the overhead above
//     parity, so the fraction spent is (observed - 1) / (threshold - 1). The
//     zero-origin form drew the 0.988x parity card at 94% of budget running
//     hot magenta, when that result has in fact spent none of its allowed
//     overhead at all. That was the opposite of the honest picture.
//
// Nothing here changes a measured value, a threshold or a verdict. The card
// still prints 0.988x against a gate of <= 1.05x; only the meter moves.
// The unit is the discriminator: '×' is unique to ratio gates, and today only
// the parity pick resolves to it (the other five resolve to ms or MiB/s).
function budgetUsed(row) {
  if (row.unit === '×') {
    // Both directions measured from parity. A '<=' gate allows overhead above
    // 1.00; a '>=' ratio gate with a threshold under 1.00 allows shortfall
    // below it. A ratio gate pinned at exactly 1.00 allows neither and would
    // divide by zero, so it falls through to the zero-origin form.
    const allowed = row.direction === '<=' ? row.threshold - 1 : 1 - row.threshold
    const spent = row.direction === '<=' ? row.observed - 1 : 1 - row.observed
    // Clamped at zero so an at-or-better-than-parity result reads as empty
    // rather than as a negative fill.
    if (allowed > 0) return Math.max(spent / allowed, 0)
  }
  return row.direction === '<=' ? row.observed / row.threshold : row.threshold / row.observed
}

export async function homeBenchmarksHtml() {
  const rowsByFamily = await benchmarkRowsByFamily()
  const picked = []
  for (const pick of HOME_PICKS) {
    const row = rowsByFamily[pick.family]?.find((candidate) => pick.match.test(candidate.label))
    if (!row) continue
    picked.push({
      ...row,
      label: pick.label,
      unit: row.unit || pick.unit,
      aside: pick.aside ?? '',
      note: typeof pick.note === 'function' ? pick.note(row) : pick.note,
      ariaNote: pick.ariaNote ?? (typeof pick.note === 'function' ? pick.note(row) : pick.note),
    })
  }
  const cards = picked
    .map((row) => {
      const value = fmtHeadline(row.observed, row.unit)
      const result = withUnit(row.observed, row.unit)
      const gate = `${row.direction === '<=' ? '≤' : '≥'} ${withUnit(row.threshold, row.unit)}`
      const status = row.pass ? 'passing' : 'FAILING'
      // How close the result sits to its frozen budget: the meter fill runs
      // the full track width at the budget, so a longer fill is a tighter gate.
      // --gate-pct hands that same percentage to CSS so the fuel ramp can be
      // sized to the track rather than to the fill, which makes colour
      // temperature encode budget pressure instead of decorating it.
      // budgetUsed() is where the origin question is answered; see below.
      const used = budgetUsed(row)
      const usedPct = Math.max(Math.min(used, 1.1) * 100, 1).toFixed(1)
      // The tooltip prints this line verbatim next to the pass or fail glyph,
      // so it has to explain what a frozen budget buys the reader rather than
      // restating the verdict the glyph already carries.
      const budgetMeaning = 'Frozen budget: a build that breaks it fails the release'
      // The aside sits above the meter, not under the gate line. .gate-meter
      // carries margin-top:auto, so anything after it is pinned to the bottom
      // of the card and anything before it absorbs the slack. Putting the
      // caveat under the gate line therefore lifted this card's meter about
      // 35px clear of the other two in its row and broke the shared baseline
      // the six meters read as. Above the meter it qualifies the label, which
      // is what it is doing anyway, and every meter stays on one line.
      const aside = row.aside ? `\n    <span class="gate-aside">${escapeHtml(row.aside)}</span>` : ''
      // The aside is on the card, not just in the tooltip, so it belongs in the
      // screen-reader summary too. The explanation that follows it is the
      // spoken form, which is why this reads row.ariaNote and not row.note.
      const summary = `${row.label}: ${result}, release gate ${gate}, ${status}.${row.aside ? ` ${row.aside}` : ''} ${row.ariaNote}`
      return `
  <div class="bench-row gate-card" tabindex="0" role="img" aria-label="${escapeHtml(summary)}"
     data-label="${escapeDatasetHtml(row.label)}" data-result="${escapeDatasetHtml(result)}"
     data-budget="${escapeDatasetHtml(gate)}" data-pct="${escapeDatasetHtml(budgetMeaning)}"
     data-pass="${row.pass}" data-note="${escapeDatasetHtml(row.note)}">
    <span class="gate-value">${escapeHtml(value)}</span>
    <span class="gate-label">${escapeHtml(row.label)}</span>${aside}
    <span class="gate-meter" aria-hidden="true"><span class="gate-meter-fill${row.pass ? '' : ' fail'}" style="width:${usedPct}%;--gate-pct:${usedPct}"></span></span>
    <span class="gate-budget ${row.pass ? 'pass' : 'fail'}">${row.pass ? '' : '<span class="gate-status" aria-hidden="true">✗</span>'}gate ${escapeHtml(gate)}</span>
  </div>`
    })
    .join('')
  return `<div class="gate-grid" role="group" aria-label="Selected frozen release gates. Each card shows the measured value. Hover or focus a card for what it measures.">${cards}</div>`
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
      // Written for a first-time reader: what ran, against what, and what the
      // number is not. The exact route-evidence counts and the manifest-level
      // wording live in the methodology block below, which is where the
      // honesty tests match them; repeating that machinery in a hover made the
      // tooltips unreadable without making them more truthful.
      const files = report.corpus.files.toLocaleString('en-US')
      const comparison = mixed
        ? `${report.ratios.mixedVsTsx.toFixed(3)}× our own TSX-only run of the same command`
        : bar.key === 'eslint'
          ? 'The baseline the other two lanes are measured against'
          : `${(report.tools.eslint.medianMs / bar.ms).toFixed(1)}× the speed of the ESLint lane`
      let note = mixed
        ? `A different workload, not a rival tool: ${files} components where ${Math.round(report.corpus.tsrxShare * 100)}% are TSRX files, linted through the same oxlint-tsrx npm command. It is measured only against our own TSX-only run, to show that mixing file types does not blow the time up. It is not a comparison against ESLint or Oxlint.`
        : `Typical time to lint the same ${files} TSX files, byte for byte the same on every lane, with one rule turned on and nothing to report. Each tool is started through its own npm command, the way a project would run it.`
      if (bar.key === 'oxcTsrx') {
        note = `Typical time to lint those same ${files} TSX files through our oxlint-tsrx npm command. They are ordinary TSX, so it loads official Oxlint's own launcher and runs it in this very process. None of our TSRX handling is in the way.`
      }
      const badge = bar.badge ? `<span class="comp-badge">${escapeHtml(bar.badge)}</span>` : ''
      return `
  <div class="bench-row comp-row comp-${bar.cls.split(' ')[0]}" tabindex="0" role="img" aria-label="${escapeHtml(`${bar.name}: ${label} median`)}"
     data-key="${bar.key}" data-label="${escapeDatasetHtml(bar.name)}" data-result="${escapeDatasetHtml(label + ' median')}"
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
<p class="home-bench-caption">Shorter is faster. Every tool crosses the npm boundary: each time is the npm command a project actually runs, Node launcher included.</p>
<details class="bench-fine">
  <summary>Methodology, versions, and gates</summary>
  <ul>
    <li><strong>Corpus.</strong> Matched cross-tool bars use the same ${report.corpus.files} TSX files, byte for byte identical on every lane (${(report.corpus.tsxBytes / 1024).toFixed(0)} KiB), linted with the <code>no-debugger</code> rule and nothing to report, on one machine (${escapeHtml(report.host.cpu)}).</li>
    <li><strong>What each time includes.</strong> Bar lengths show absolute median wall-clock time, taken from ${report.samplePolicy.measured} measured processes after ${report.samplePolicy.warmups} warmups. Every lane runs through its own npm CLI entry point, so each bar carries that tool's Node launcher.</li>
    <li><strong>The product lane.</strong> On all-TSX input, <code>oxlint-tsrx</code> imports the exact manifest-declared official Oxlint launcher in the same Node process with zero TSRX dispatch.</li>
    <li><strong>The mixed bar.</strong> The separately patterned mixed bar is the paired internal workload of mixed file types (${Math.round(report.corpus.tsrxShare * 100)}% TSRX by file count), not a rival tool. It proves ${route.publicCanonicalNodeChildren} public canonical Node child, ${route.nativeTsrxChildren} native TSRX child, and ${route.privateInProcessAdapterChildren} private adapter children.</li>
    <li><strong>Gates.</strong> Hover any bar for its frozen ratio gate: the release fails if a future build breaks it.</li>
    <li><strong>Versions.</strong> ESLint ${escapeHtml(report.versions.eslint)} with typescript-eslint ${escapeHtml(report.versions.typescriptEslint)}; official Oxlint ${escapeHtml(report.versions.oxlint.replace('Version: ', ''))}. Report <code>${escapeHtml(file)}</code> (${when}).</li>
  </ul>
</details>`
}

// The home-page editor-replay demo quotes recorded latencies inside its prose
// rather than in a gate card, so it needs the same single source of truth as
// the tables: the aggregate-selected editor report. Exporting the scalars here
// keeps a refreshed benchmark run from leaving hand-typed numbers behind in
// docs/build.mjs.
export async function editorReplayLatencies() {
  const { report } = await latestReport('editor')
  const roundTrips = {
    editDiagnosticsP95Ms: report.editDiagnostics.p95Ms,
    formattingP95Ms: report.formatting.p95Ms,
    codeActionsP95Ms: report.codeActions.p95Ms,
  }
  return {
    cpu: report.host.cpu,
    initialOpenMedianMs: report.initialOpen.medianMs,
    initialOpenP95Ms: report.initialOpen.p95Ms,
    ...roundTrips,
    // One number can stand for "edit, format, and code action" only if it is
    // the slowest of the three, so the claim stays true whichever way a fresh
    // run reorders them.
    slowestRoundTripP95Ms: Math.max(...Object.values(roundTrips)),
  }
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
