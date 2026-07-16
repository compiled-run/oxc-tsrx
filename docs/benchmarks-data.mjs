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
const median = (rawMs) => {
  const sorted = [...rawMs].sort((a, b) => a - b)
  return sorted[Math.floor(sorted.length / 2)]
}

const fmt = (value) => {
  if (typeof value !== 'number') return String(value)
  if (Number.isInteger(value)) return String(value)
  if (value >= 100) return value.toFixed(2)
  if (value >= 1) return value.toFixed(3)
  return value.toFixed(3)
}

async function latestReport(family) {
  const aggregate = await aggregateReport
  const file = aggregate.results?.[family]?.path
  if (!new RegExp(`^benchmarks/${family}/results-\\d+\\.json$`).test(file ?? '')) {
    throw new Error(`no aggregate-selected report for benchmarks/${family}`)
  }
  return {
    file,
    report: JSON.parse(await readFile(path.join(repoRoot, file), 'utf8')),
  }
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

// Normalized row: { label, observed, threshold, direction ('<='|'>='|'=='), unit, pass }
function arrayRows({ report }) {
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
    return {
      label: presentation.label,
      observed: assertion.observed,
      threshold: assertion.threshold,
      direction,
      unit: '',
      pass: assertion.pass,
      note: presentation.note,
    }
  })
}

function typeAwareRows({ report }) {
  const b = report.budgets
  const a = report.assertions
  return [
    { label: 'Default syntax lint p95', observed: p95(report.defaultSyntax.rawMs), threshold: b.defaultSyntaxP95MsMax, direction: '<=', unit: 'ms', pass: a.defaultSyntaxP95 },
    { label: 'Single-file type-aware p95', observed: p95(report.singleTypeAware.rawMs), threshold: b.singleTypeAwareP95MsMax, direction: '<=', unit: 'ms', pass: a.singleTypeAwareP95 },
    { label: 'Two-file project type-aware p95', observed: p95(report.projectTypeAware.rawMs), threshold: b.projectTypeAwareP95MsMax, direction: '<=', unit: 'ms', pass: a.projectTypeAwareP95 },
    { label: 'Single-file type-aware cold start', observed: report.singleTypeAware.coldMs, threshold: b.singleTypeAwareColdMsMax, direction: '<=', unit: 'ms', pass: a.singleTypeAwareCold },
    { label: 'Two-file project cold start', observed: report.projectTypeAware.coldMs, threshold: b.projectTypeAwareColdMsMax, direction: '<=', unit: 'ms', pass: a.projectTypeAwareCold },
    { label: 'Type-aware vs default p95 ratio', observed: report.ratios.singleTypeAwareVsDefaultP95, threshold: b.singleTypeAwareVsDefaultP95RatioMax, direction: '<=', unit: '×', pass: a.singleTypeAwareRatio },
    { label: 'Type processes per batch', observed: report.invariants.singleTypeAwareProcesses, threshold: b.typeAwareProcessesPerBatch, direction: '==', unit: '', pass: a.oneTypeProcessPerBatch },
    { label: 'Default path parse count per file', observed: report.invariants.defaultParseCount, threshold: b.defaultParseCountPerFile, direction: '==', unit: '', pass: a.defaultPathUnchanged },
  ]
}

function viteRows({ report }) {
  const b = report.budgets
  const a = report.assertions
  return [
    { label: 'Mixed companion lint p95', observed: report.directMixedLint.p95Ms ?? p95(report.directMixedLint.rawMs), threshold: b.directLintP95MsMax, direction: '<=', unit: 'ms', pass: a.directLintP95 },
    { label: 'Mixed lint vs canonical p95 ratio', observed: report.ratios.directLintVsCanonicalP95, threshold: b.directLintVsCanonicalP95RatioMax, direction: '<=', unit: '×', pass: a.directLintRatio },
    { label: 'Mixed companion format-check p95', observed: report.directMixedFormat.p95Ms ?? p95(report.directMixedFormat.rawMs), threshold: b.directFormatP95MsMax, direction: '<=', unit: 'ms', pass: a.directFormatP95 },
    { label: 'Mixed format vs canonical p95 ratio', observed: report.ratios.directFormatVsCanonicalP95, threshold: b.directFormatVsCanonicalP95RatioMax, direction: '<=', unit: '×', pass: a.directFormatRatio },
    { label: 'Vite+ 0.2.4 mixed lint p95', observed: report.vitePlusCurrentMixedLint.p95Ms ?? p95(report.vitePlusCurrentMixedLint.rawMs), threshold: b.vitePlusCurrentLintP95MsMax, direction: '<=', unit: 'ms', pass: a.vitePlusLintP95 },
    { label: 'Native TSRX parses per file', observed: report.invariants?.nativeTsrxParseCount ?? b.nativeTsrxParseCountPerFile, threshold: b.nativeTsrxParseCountPerFile, direction: '==', unit: '', pass: a.oneNativeParse },
  ]
}

function editorRows({ report }) {
  const b = report.budgets
  const a = report.assertions
  return [
    { label: 'Server start to first diagnostics', observed: report.initialOpenMs, threshold: b.initialOpenMsMax, direction: '<=', unit: 'ms', pass: a.initialOpen },
    { label: 'Edit-to-diagnostics p95', observed: p95(report.editDiagnostics.rawMs), threshold: b.editDiagnosticsP95MsMax, direction: '<=', unit: 'ms', pass: a.editDiagnosticsP95 },
    { label: 'Formatting p95', observed: p95(report.formatting.rawMs), threshold: b.formatP95MsMax, direction: '<=', unit: 'ms', pass: a.formatP95 },
    { label: 'Safe code-action p95', observed: p95(report.codeActions.rawMs), threshold: b.codeActionP95MsMax, direction: '<=', unit: 'ms', pass: a.codeActionP95 },
    { label: 'RSS after 1,000-edit soak', observed: report.memory.rssAfterSoakMiB, threshold: b.residentMemoryMiBMax, direction: '<=', unit: 'MiB', pass: a.residentMemory },
    { label: 'RSS growth through soak', observed: report.memory.growthMiB, threshold: b.editSoakGrowthMiBMax, direction: '<=', unit: 'MiB', pass: a.editSoakGrowth },
  ]
}


function comparativeRows({ report }) {
  const b = report.budgets
  const a = report.assertions
  return [
    { label: 'OXC for TSRX / official Oxlint (matched TSX lane)', observed: report.ratios.oxcTsrxVsOxlint, threshold: b.oxcTsrxVsOxlintMax, direction: '<=', unit: '×', pass: a.nearOxlintParity },
    { label: 'ESLint / OXC for TSRX (matched TSX lane)', observed: report.ratios.eslintVsOxcTsrx, threshold: b.eslintVsOxcTsrxMin, direction: '>=', unit: '×', pass: a.fasterThanEslint },
    { label: 'Paired 20% TSRX / all-TSX product workload', observed: report.ratios.mixedVsTsx, threshold: b.mixedVsTsxMax, direction: '<=', unit: '×', pass: a.mixedNoBlowup },
  ]
}

const FAMILIES = [
  { family: 'comparative', title: 'Matched 1,000-file CLI comparison', rows: comparativeRows, note: 'ESLint, official Oxlint, and OXC for TSRX lint the same byte-identical TSX files with one no-debugger rule, the same explicit file list, zero-diagnostic default output, 5 warmups, and 20 measured processes. The separate 20% TSRX row is a paired internal workload ratio, not a cross-tool comparison.' },
  { family: 'native-lint', title: 'Native lint', rows: arrayRows, note: 'Frozen release gate: throughput, same-build ordinary-path overhead, cold start, memory, and batch invariants. The npm-launcher ratio is a diagnostic boundary, not a speed claim.' },
  { family: 'native-format', title: 'Native format', rows: arrayRows, note: 'Frozen release gate: formatter throughput, convergence scaling, memory, and batch invariants. The historical 16.6 MiB/s value is an absolute cross-corpus-derived floor, not a speedup claim.' },
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

function chartSvg(title, rows) {
  const numeric = rows.filter((row) => row.direction !== '==')
  const rowHeight = 30
  const labelWidth = 260
  const trackWidth = 320
  const height = numeric.length * rowHeight + 8
  const bars = numeric
    .map((row, index) => {
      const used = row.direction === '<=' ? row.observed / row.threshold : row.threshold / row.observed
      const pct = Math.max(Math.min(used, 1.15), 0.005)
      const y = index * rowHeight + 6
      const pctLabel = `${(used * 100).toFixed(used < 0.1 ? 1 : 0)}% of budget`
      const result = `${fmt(row.observed)}${row.unit ? ` ${row.unit}` : ''}`
      const budget = `${row.direction === '<=' ? '≤' : '≥'} ${fmt(row.threshold)}${row.unit ? ` ${row.unit}` : ''}`
      const summary = `${row.label}: ${result}, budget ${budget}, ${pctLabel}, ${row.pass ? 'pass' : 'FAIL'}${row.note ? `. ${row.note}` : ''}`
      return `
    <g class="bench-row" tabindex="0" role="img" aria-label="${escapeHtml(summary)}"
       data-label="${escapeDatasetHtml(row.label)}" data-result="${escapeDatasetHtml(result)}"
       data-budget="${escapeDatasetHtml(budget)}" data-pct="${escapeDatasetHtml(pctLabel)}"
       data-pass="${row.pass}"${row.note ? ` data-note="${escapeDatasetHtml(row.note)}"` : ''}>
      <rect x="2" y="${y - 3}" width="696" height="${rowHeight - 4}" rx="6" class="bench-hit"/>
      <text x="${labelWidth - 8}" y="${y + 13}" text-anchor="end" class="bench-label">${escapeHtml(row.label)}</text>
      <rect x="${labelWidth}" y="${y}" width="${trackWidth}" height="18" rx="4" class="bench-track"/>
      <rect x="${labelWidth}" y="${y}" width="${(pct * trackWidth).toFixed(1)}" height="18" rx="4" class="bench-bar ${row.pass ? 'pass' : 'fail'}"/>
      <text x="${labelWidth + trackWidth + 8}" y="${y + 13}" class="bench-value">${escapeHtml(result)}</text>
    </g>`
    })
    .join('')
  const budgetX = labelWidth + trackWidth
  return `<svg class="bench-chart" viewBox="0 0 700 ${height}" role="group" aria-label="${escapeHtml(
    title,
  )}: each bar shows the measured result as a percentage of its frozen budget; under 100% passes. Hover or focus a row for details.">
  <line x1="${budgetX}" y1="0" x2="${budgetX}" y2="${height}" class="bench-budget-line"/>
  ${bars}
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

export const benchmarkHeadings = FAMILIES.map(({ family, title }) => ({
  depth: 2,
  id: family,
  text: title,
}))

export async function benchmarksSectionsHtml() {
  const sections = []
  for (const { family, title, rows: extract, note } of FAMILIES) {
    const { file, report } = await latestReport(family)
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
${tableHtml(rows)}`)
  }
  return sections.join('\n')
}

// Headline rows for the home page chart, picked from the same normalized
// rows. Labels are plain language, the visible number is the real measured
// value, and each note explains the metric in the hover/focus tooltip.
const HOME_PICKS = [
  {
    family: 'native-lint',
    match: /median standard-path latency ratio/i,
    label: 'Ordinary-file lint overhead',
    unit: '×',
    note: 'Median ordinary-TSX latency through the product direct path versus the same-build canonical OXC adapter control. 1.00× is parity; this is not the stock Oxlint npm/CLI boundary.',
  },
  {
    family: 'native-format',
    match: /historical incumbent derived floor/i,
    label: 'Sequential formatting',
    unit: 'MiB/s',
    note: 'Sequential formatter throughput. The ≥16.6 MiB/s gate is an absolute regression floor derived from a non-comparable historical result; it is not a speedup comparison.',
  },
  {
    family: 'native-lint',
    match: /fresh-process TSRX p95 latency/i,
    label: 'Cold start: lint a file from scratch',
    unit: 'ms',
    note: 'Launching a brand-new process and linting a TSRX file end to end, 95th percentile. There is no warmup to wait for.',
  },
  {
    family: 'editor',
    match: /edit-to-diagnostics p95/i,
    label: 'Native editor diagnostics',
    unit: 'ms',
    note: 'Direct local stdio language-server edit-to-diagnostics round trip, excluding VS Code rendering.',
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
      unit: row.unit || pick.unit,
      note: typeof pick.note === 'function' ? pick.note(row) : pick.note,
    })
  }
  return chartSvg('Headline performance gates', picked)
}


export async function comparativeChartHtml() {
  const { file, report } = await latestReport('comparative')
  const assertions = report.assertions
  const budgets = report.budgets
  const bars = [
    {
      key: 'eslint',
      name: 'ESLint + typescript-eslint · matched TSX',
      ms: report.tools.eslint.medianMs,
      cls: 'other',
      pass: assertions.fasterThanEslint,
      gate: `ESLint / OXC for TSRX ≥ ${budgets.eslintVsOxcTsrxMin}×`,
    },
    {
      key: 'oxlint',
      name: 'official Oxlint · matched TSX',
      ms: report.tools.oxlint.medianMs,
      cls: 'other',
      pass: assertions.nearOxlintParity,
      gate: `OXC for TSRX / official Oxlint ≤ ${budgets.oxcTsrxVsOxlintMax}×`,
    },
    {
      key: 'oxcTsrx',
      name: 'OXC for TSRX · matched TSX',
      ms: report.tools.oxcTsrx.medianMs,
      cls: 'ours',
      pass: assertions.nearOxlintParity && assertions.fasterThanEslint,
      gate: `matched ratios ≤ ${budgets.oxcTsrxVsOxlintMax}× Oxlint and ≥ ${budgets.eslintVsOxcTsrxMin}× over ESLint`,
    },
    {
      key: 'oxcTsrxMixed',
      name: 'OXC for TSRX · paired 20% TSRX',
      ms: report.tools.oxcTsrxMixed.medianMs,
      cls: 'ours',
      pass: assertions.mixedNoBlowup,
      gate: `paired mixed / all-TSX ≤ ${budgets.mixedVsTsxMax}×`,
    },
  ]
  const max = Math.max(...bars.map((bar) => bar.ms))
  const rowH = 34
  const labelW = 250
  const trackW = 330
  const rows = bars
    .map((bar, index) => {
      const y = index * rowH + 6
      const width = Math.max((bar.ms / max) * trackW, 3)
      const label = `${bar.ms >= 100 ? bar.ms.toFixed(0) : bar.ms.toFixed(1)} ms`
      const mixed = bar.key === 'oxcTsrxMixed'
      const comparison = mixed
        ? `${report.ratios.mixedVsTsx.toFixed(3)}× versus the paired all-TSX product workload`
        : `${(report.tools.eslint.medianMs / bar.ms).toFixed(1)}× relative to the matched ESLint lane`
      const note = mixed
        ? `Median wall-clock time for the paired ${report.corpus.files}-component workload with ${Math.round(report.corpus.tsrxShare * 100)}% TSRX. This is an internal product workload ratio, not a cross-tool speed comparison.`
        : `Median wall-clock time for the same byte-identical ${report.corpus.files}-file TSX corpus, one no-debugger rule, one explicit file list, and zero-diagnostic default output.`
      return `
    <g class="bench-row" tabindex="0" role="img" aria-label="${escapeHtml(`${bar.name}: ${label} median`)}"
       data-label="${escapeDatasetHtml(bar.name)}" data-result="${escapeDatasetHtml(label + ' median')}"
       data-budget="${escapeDatasetHtml(bar.gate)}" data-pct="${escapeDatasetHtml(comparison)}" data-pass="${bar.pass}" data-note="${escapeDatasetHtml(note)}">
      <rect x="0" y="${y - 4}" width="700" height="${rowH - 2}" fill="transparent"/>
      <text x="${labelW - 8}" y="${y + 14}" text-anchor="end" class="bench-label">${escapeHtml(bar.name)}</text>
      <rect x="${labelW}" y="${y}" width="${width.toFixed(1)}" height="20" rx="4" class="bench-bar comp-${bar.cls}"/>
      <text x="${labelW + width + 8}" y="${y + 14}" class="bench-value">${label}</text>
    </g>`
    })
    .join('')
  const when = (report.timestamp ?? '').slice(0, 10)
  return `<svg class="bench-chart" viewBox="0 0 700 ${bars.length * rowH + 10}" role="group" aria-label="Median CLI lint time on one matched ${report.corpus.files}-file TSX corpus plus a separate paired TSRX workload.">${rows}</svg>
<p class="home-bench-caption">These bar lengths show absolute median wall-clock time, not percentage-of-budget. Each tooltip names the applicable frozen ratio gate. Matched lane: the same ${report.corpus.files} explicit TSX files (${(report.corpus.tsxBytes / 1024).toFixed(0)} KiB), one <code>no-debugger</code> rule, and zero-diagnostic default output on one machine. Results are medians after ${report.samplePolicy.warmups} warmups and ${report.samplePolicy.measured} measured processes. ESLint ${escapeHtml(report.versions.eslint)} + typescript-eslint ${escapeHtml(report.versions.typescriptEslint)}; official Oxlint ${escapeHtml(report.versions.oxlint.replace('Version: ', ''))}. The 20% TSRX row is a separately labeled paired internal workload. Report <code>${escapeHtml(file)}</code> (${when}).</p>`
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
