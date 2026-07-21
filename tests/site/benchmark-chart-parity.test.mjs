// Chart parity: every value the ECharts benchmark charts plot must be
// recomputable from the raw sample arrays kept in the aggregate-selected
// reports. This file rebuilds the expectations from the reports independently
// of docs/benchmarks-data.mjs, then checks the pure chart-option builders the
// page renders from, so a drift between the charts and the retained data
// fails loudly.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import test from "node:test";

import { benchmarkChartOption, benchmarkRowsByFamily } from "../../docs/benchmarks-data.mjs";

const root = resolve(import.meta.dirname, "../..");
const MIB = 1048576;

// ---------- independent statistics (must match the harness conventions) ----------
const sortAsc = (values) => [...values].sort((a, b) => a - b);
// Lower-middle median: sorted[floor((n - 1) / 2)].
const med = (values) => sortAsc(values)[Math.floor((values.length - 1) / 2)];
// Nearest-rank p95: sorted[min(n - 1, ceil(n * 0.95) - 1)].
const p95 = (values) => {
  const sorted = sortAsc(values);
  return sorted[Math.min(sorted.length - 1, Math.ceil(sorted.length * 0.95) - 1)];
};
const nsToMs = (ns) => ns / 1e6;
const thpt = (ns, bytes) => bytes / MIB / (ns / 1e9);

// ---------- expected rows, recomputed from the selected reports ----------
const families = ["comparative", "native-lint", "native-format", "type-aware", "vite", "editor"];

async function loadReports() {
  const aggregate = JSON.parse(
    await readFile(join(root, "docs/acceptance/performance-report.json"), "utf8"),
  );
  const reports = new Map();
  for (const family of families) {
    const selected = aggregate.results?.[family]?.path;
    assert.match(selected ?? "", new RegExp(`^benchmarks/${family}/results-\\d+\\.json$`));
    reports.set(family, {
      report: JSON.parse(await readFile(join(root, selected), "utf8")),
    });
  }
  return reports;
}

const samplesRow = (values, unit, extra = {}) => ({
  kind: "samples",
  values,
  unit,
  median: med(values),
  p95: p95(values),
  ...extra,
});
// Throughput ticks follow the harness: take median/p95 on the latency array,
// then convert. The decreasing ns-to-MiB/s conversion does not commute with
// order statistics, so computing them on converted values would be wrong.
const throughputRow = (nsValues, bytes) =>
  samplesRow(nsValues.map((ns) => thpt(ns, bytes)), "MiB/s", {
    median: thpt(med(nsValues), bytes),
    p95: thpt(p95(nsValues), bytes),
  });
const ratioRow = (unit, numLabel, numValues, denLabel, denValues) => ({
  kind: "ratio",
  unit,
  num: { label: numLabel, values: numValues },
  den: { label: denLabel, values: denValues },
});
const scalarRow = () => ({ kind: "scalar" });

// Each builder returns [rowExpectation, exactObserved] pairs keyed in chart
// order, where exactObserved must equal the report's asserted observed value
// bit for bit; that is the trace from chart to retained array.
function expectNativeLint(report) {
  const raw = report.rawSamples;
  const bytes = report.corpus.bytes;
  const control = [...raw.controlBeforeNs, ...raw.controlAfterNs];
  const scp = raw.candidateTsrxScanNs.map(
    (scan, i) => scan + raw.candidateTsrxProjectionNs[i] + raw.candidateTsrxParseNs[i],
  );
  const rss = report.summaries.p07.candidateTsrxRssBytes;
  const ms = (nsValues) => nsValues.map(nsToMs);
  return new Map([
    ["P01 median standard-path latency ratio", {
      row: ratioRow("ms", "product standard path", ms(raw.candidateStandardTotalNs), "same-build canonical control", ms(control)),
      observed: med(raw.candidateStandardTotalNs) / med(control),
    }],
    ["P01 p95 standard-path latency ratio", {
      row: ratioRow("ms", "product standard path", ms(raw.candidateStandardTotalNs), "same-build canonical control", ms(control)),
      observed: p95(raw.candidateStandardTotalNs) / p95(control),
    }],
    ["P02 median scan+copy+parse throughput", {
      row: throughputRow(scp, bytes),
      observed: thpt(med(scp), bytes),
    }],
    ["P02 p95 scan+copy+parse throughput", {
      row: throughputRow(scp, bytes),
      observed: thpt(p95(scp), bytes),
    }],
    ["P02 equivalent-TSX throughput ratio", {
      row: ratioRow("ms", "TSRX scan, copy, and parse", ms(scp), "equivalent TSX parse", ms(raw.candidateStandardParseNs)),
      observed: thpt(med(scp), bytes) / thpt(med(raw.candidateStandardParseNs), bytes),
    }],
    ["P02 warm 10 KiB p95 scan+copy+parse latency", {
      row: samplesRow(ms(raw.warm10kScanCopyParseNs), "ms"),
      observed: nsToMs(p95(raw.warm10kScanCopyParseNs)),
    }],
    ["P03 in-process one-thread lint throughput", {
      row: throughputRow(raw.candidateTsrxTotalNs, bytes),
      observed: thpt(med(raw.candidateTsrxTotalNs), bytes),
    }],
    ["P03 CLI one-thread lint throughput", {
      row: throughputRow(raw.candidateTsrxCliNs, bytes),
      observed: thpt(med(raw.candidateTsrxCliNs), bytes),
    }],
    ["P03 end-to-end CLI latency ratio", {
      row: ratioRow("ms", "TSRX CLI lint", ms(raw.candidateTsrxCliNs), "equivalent TSX CLI lint", ms(raw.candidateStandardCliNs)),
      observed: med(raw.candidateTsrxCliNs) / med(raw.candidateStandardCliNs),
    }],
    ["P05 fresh-process TSRX p95 latency", {
      row: samplesRow(ms(raw.candidateColdCliNs), "ms"),
      observed: nsToMs(p95(raw.candidateColdCliNs)),
    }],
    ["P05 fresh-process upstream latency ratio", {
      row: ratioRow("ms", "direct Rust TSRX process", ms(raw.candidateColdCliNs), "official Oxlint npm launcher", ms(raw.stockColdCliNs)),
      observed: p95(raw.candidateColdCliNs) / p95(raw.stockColdCliNs),
    }],
    ["P07 TSRX peak RSS", {
      row: samplesRow(rss.map((b) => b / MIB), "MiB", {
        plotThreshold: report.summaries.p07.allowedRssBytes / MIB,
      }),
      observed: med(rss),
    }],
  ]);
}

function expectNativeFormat(report) {
  const raw = report.rawSamples;
  const bytes = report.corpus.bytes;
  const genBytes = report.generalizedControlCorpus.bytes;
  const genHalfBytes = report.generalizedControlCorpus.halfBytes;
  const control = [...raw.directControlBeforeNs, ...raw.directControlAfterNs];
  // The batch corpus byte count is pure algebra on in-report fields; the two
  // independent derivations must agree on the same integer.
  const batch = report.p04.candidateDefaultThreadBatch;
  const bytesFromMedian = batch.medianMibPerSecond * (med(raw.candidateBatchNs) / 1e9) * MIB;
  const bytesFromP95 = batch.p95MibPerSecond * (p95(raw.candidateBatchNs) / 1e9) * MIB;
  assert.ok(
    Math.abs(bytesFromMedian - bytesFromP95) <= 0.5,
    "batch corpus byte derivations disagree",
  );
  const batchBytes = Math.round(bytesFromP95);
  const ms = (nsValues) => nsValues.map(nsToMs);
  const seq = raw.candidateTsrxSequentialNs;
  const gen = raw.candidateGeneralizedControlNs;
  return new Map([
    ["p04_direct_median_ratio", {
      row: ratioRow("ms", "product standard path", ms(raw.candidateStandardNs), "canonical formatter control", ms(control)),
      observed: med(raw.candidateStandardNs) / med(control),
    }],
    ["p04_direct_p95_ratio", {
      row: ratioRow("ms", "product standard path", ms(raw.candidateStandardNs), "canonical formatter control", ms(control)),
      observed: p95(raw.candidateStandardNs) / p95(control),
    }],
    ["p04_sequential_median_mib_s", {
      row: throughputRow(seq, bytes),
      observed: thpt(med(seq), bytes),
    }],
    ["p04_sequential_p95_mib_s", {
      row: throughputRow(seq, bytes),
      observed: thpt(p95(seq), bytes),
    }],
    ["p04_historical_incumbent_derived_floor_mib_s", {
      row: throughputRow(seq, bytes),
      observed: thpt(med(seq), bytes),
    }],
    ["p04_default_thread_mib_s", {
      row: throughputRow(raw.candidateBatchNs, batchBytes),
      observed: thpt(p95(raw.candidateBatchNs), batchBytes),
    }],
    ["p04_generalized_control_median_mib_s", {
      row: throughputRow(gen, genBytes),
      observed: thpt(med(gen), genBytes),
    }],
    ["p04_generalized_control_p95_mib_s", {
      row: throughputRow(gen, genBytes),
      observed: thpt(p95(gen), genBytes),
    }],
    ["p04_generalized_control_linear_scaling", {
      row: ratioRow("ms", "full generalized corpus", ms(gen), "half-size corpus", ms(raw.candidateGeneralizedControlHalfNs)),
      observed:
        med(gen) / med(raw.candidateGeneralizedControlHalfNs) / (genBytes / genHalfBytes),
    }],
    ["p05_stdin_p95_ms", {
      row: samplesRow(ms(raw.candidateStdinNs), "ms"),
      observed: nsToMs(p95(raw.candidateStdinNs)),
    }],
    ["p05_upstream_ratio", {
      row: ratioRow("ms", "direct Rust TSRX stdin", ms(raw.candidateStdinNs), "official Oxfmt npm launcher", ms(raw.stockStdinNs)),
      observed: p95(raw.candidateStdinNs) / p95(raw.stockStdinNs),
    }],
    ["p07_rss_ratio", {
      row: ratioRow(
        "MiB",
        "TSRX formatter RSS",
        report.p07.candidateTsrxRssBytes.map((b) => b / MIB),
        "canonical TSX RSS",
        report.p07.canonicalTsxRssBytes.map((b) => b / MIB),
      ),
      observed: med(report.p07.candidateTsrxRssBytes) / med(report.p07.canonicalTsxRssBytes),
    }],
  ]);
}

function expectTypeAware(report) {
  return [
    { label: "Default syntax lint p95", row: samplesRow(report.defaultSyntax.rawMs, "ms"), observed: p95(report.defaultSyntax.rawMs), reported: report.defaultSyntax.p95Ms, threshold: report.budgets.defaultSyntaxP95MsMax, direction: "<=" },
    { label: "Single-file type-aware p95", row: samplesRow(report.singleTypeAware.rawMs, "ms"), observed: p95(report.singleTypeAware.rawMs), reported: report.singleTypeAware.p95Ms, threshold: report.budgets.singleTypeAwareP95MsMax, direction: "<=" },
    { label: "Two-file project type-aware p95", row: samplesRow(report.projectTypeAware.rawMs, "ms"), observed: p95(report.projectTypeAware.rawMs), reported: report.projectTypeAware.p95Ms, threshold: report.budgets.projectTypeAwareP95MsMax, direction: "<=" },
    { label: "Single-file type-aware cold start", row: scalarRow(), observed: report.singleTypeAware.coldMs, threshold: report.budgets.singleTypeAwareColdMsMax, direction: "<=" },
    { label: "Two-file project cold start", row: scalarRow(), observed: report.projectTypeAware.coldMs, threshold: report.budgets.projectTypeAwareColdMsMax, direction: "<=" },
    { label: "Type-aware vs default p95 ratio", row: ratioRow("ms", "single-file type-aware lane", report.singleTypeAware.rawMs, "default syntax lane", report.defaultSyntax.rawMs), observed: p95(report.singleTypeAware.rawMs) / p95(report.defaultSyntax.rawMs), reported: report.ratios.singleTypeAwareVsDefaultP95, threshold: report.budgets.singleTypeAwareVsDefaultP95RatioMax, direction: "<=" },
  ];
}

function expectVite(report) {
  const b = report.budgets;
  return [
    { label: "Mixed companion lint p95", row: samplesRow(report.directMixedLint.rawMs, "ms"), observed: p95(report.directMixedLint.rawMs), reported: report.directMixedLint.p95Ms, threshold: b.directLintP95MsMax, direction: "<=" },
    { label: "Mixed lint vs canonical p95 ratio", row: ratioRow("ms", "mixed companion lint", report.directMixedLint.rawMs, "canonical lint", report.canonicalLint.rawMs), observed: p95(report.directMixedLint.rawMs) / p95(report.canonicalLint.rawMs), reported: report.ratios.directLintVsCanonicalP95, threshold: b.directLintVsCanonicalP95RatioMax, direction: "<=" },
    { label: "Mixed companion format-check p95", row: samplesRow(report.directMixedFormat.rawMs, "ms"), observed: p95(report.directMixedFormat.rawMs), reported: report.directMixedFormat.p95Ms, threshold: b.directFormatP95MsMax, direction: "<=" },
    { label: "Mixed format vs canonical p95 ratio", row: ratioRow("ms", "mixed companion format", report.directMixedFormat.rawMs, "canonical format", report.canonicalFormat.rawMs), observed: p95(report.directMixedFormat.rawMs) / p95(report.canonicalFormat.rawMs), reported: report.ratios.directFormatVsCanonicalP95, threshold: b.directFormatVsCanonicalP95RatioMax, direction: "<=" },
    { label: "Ordinary npm formatter p95", row: samplesRow(report.directOrdinaryFormat.rawMs, "ms"), observed: p95(report.directOrdinaryFormat.rawMs), reported: report.directOrdinaryFormat.p95Ms, threshold: b.directOrdinaryFormatP95MsMax, direction: "<=" },
    { label: "Ordinary npm formatter vs canonical p95 ratio", row: ratioRow("ms", "ordinary npm formatter", report.directOrdinaryFormat.rawMs, "canonical format", report.canonicalFormat.rawMs), observed: p95(report.directOrdinaryFormat.rawMs) / p95(report.canonicalFormat.rawMs), reported: report.ratios.directOrdinaryFormatVsCanonicalP95, threshold: b.directOrdinaryFormatVsCanonicalP95RatioMax, direction: "<=" },
    { label: "Vite+ 0.2.4 mixed lint p95", row: samplesRow(report.vitePlusCurrentMixedLint.rawMs, "ms"), observed: p95(report.vitePlusCurrentMixedLint.rawMs), reported: report.vitePlusCurrentMixedLint.p95Ms, threshold: b.vitePlusCurrentLintP95MsMax, direction: "<=" },
  ];
}

function expectEditor(report) {
  const b = report.budgets;
  return [
    { label: "Server start to first diagnostics", row: scalarRow(), observed: report.initialOpenMs, threshold: b.initialOpenMsMax, direction: "<=" },
    { label: "Edit-to-diagnostics p95", row: samplesRow(report.editDiagnostics.rawMs, "ms"), observed: p95(report.editDiagnostics.rawMs), reported: report.editDiagnostics.p95Ms, threshold: b.editDiagnosticsP95MsMax, direction: "<=" },
    { label: "Formatting p95", row: samplesRow(report.formatting.rawMs, "ms"), observed: p95(report.formatting.rawMs), reported: report.formatting.p95Ms, threshold: b.formatP95MsMax, direction: "<=" },
    { label: "Safe code-action p95", row: samplesRow(report.codeActions.rawMs, "ms"), observed: p95(report.codeActions.rawMs), reported: report.codeActions.p95Ms, threshold: b.codeActionP95MsMax, direction: "<=" },
    { label: "RSS after 1,000-edit soak", row: scalarRow(), observed: report.memory.rssAfterSoakMiB, threshold: b.residentMemoryMiBMax, direction: "<=" },
    { label: "RSS growth through soak", row: scalarRow(), observed: report.memory.growthMiB, threshold: b.editSoakGrowthMiBMax, direction: "<=" },
  ];
}

function expectComparative(report) {
  const tools = report.tools;
  const b = report.budgets;
  return [
    { label: "OXC for TSRX / official Oxlint (matched TSX lane)", row: ratioRow("ms", "OXC for TSRX, all-TSX lane", tools.oxcTsrx.rawMs, "official Oxlint", tools.oxlint.rawMs), observed: med(tools.oxcTsrx.rawMs) / med(tools.oxlint.rawMs), reported: report.ratios.oxcTsrxVsOxlint, threshold: b.oxcTsrxVsOxlintMax, direction: "<=" },
    { label: "ESLint / OXC for TSRX (matched TSX lane)", row: ratioRow("ms", "ESLint + typescript-eslint", tools.eslint.rawMs, "OXC for TSRX, all-TSX lane", tools.oxcTsrx.rawMs), observed: med(tools.eslint.rawMs) / med(tools.oxcTsrx.rawMs), reported: report.ratios.eslintVsOxcTsrx, threshold: b.eslintVsOxcTsrxMin, direction: ">=" },
    { label: "Paired mixed-file-types / all-TSX product workload", row: ratioRow("ms", "mixed file types workload", tools.oxcTsrxMixed.rawMs, "all-TSX product workload", tools.oxcTsrx.rawMs), observed: med(tools.oxcTsrxMixed.rawMs) / med(tools.oxcTsrx.rawMs), reported: report.ratios.mixedVsTsx, threshold: b.mixedVsTsxMax, direction: "<=" },
  ];
}

// Presentation renames applied by the page generator.
const RENAMES = new Map([
  ["P05 fresh-process upstream latency ratio", "Direct Rust / official Oxlint npm-launcher p95 ratio"],
  ["p05_upstream_ratio", "Direct Rust / official Oxfmt npm-launcher p95 ratio"],
  ["p04_historical_incumbent_derived_floor_mib_s", "Sequential throughput vs absolute 16.6 MiB/s floor"],
]);

function arrayFamilyExpectations(report, expectMap) {
  const rows = [];
  for (const assertion of report.assertions) {
    if (!expectMap.has(assertion.name)) {
      // Everything unmapped must be an exactly-1 boolean invariant.
      assert.equal(assertion.observed, 1, `${assertion.name} unmapped but not an invariant`);
      assert.equal(assertion.threshold, 1, `${assertion.name} unmapped but not an invariant`);
      continue;
    }
    const expected = expectMap.get(assertion.name);
    // native-format uses descriptive comparison strings; infer the direction
    // from the outcome exactly like the page generator does.
    let direction = assertion.comparison;
    if (direction !== "<=" && direction !== ">=") {
      direction = assertion.pass === assertion.observed <= assertion.threshold ? "<=" : ">=";
    }
    rows.push({
      label: RENAMES.get(assertion.name) ?? assertion.name.replace(/_/g, " "),
      row: expected.row,
      observed: expected.observed,
      reported: assertion.observed,
      threshold: assertion.threshold,
      direction,
    });
  }
  return rows;
}

// ---------- helpers to dig values out of a built chart option ----------
const series = (option, name) => option.series.find((entry) => entry.name === name);
const budgetLine = (option) => series(option, "gate").markLine.data[0].xAxis;
const gateValue = (option) => series(option, "gate").data[0][0];
const failRegion = (option) => series(option, "gate").markArea.data[0];

function checkRow(family, expected, built) {
  const where = `${family}: ${expected.label}`;
  assert.equal(built.label, expected.label, `${where}: label`);
  const reported = expected.reported ?? expected.observed;
  assert.equal(built.observed, reported, `${where}: module row observed`);
  assert.equal(built.threshold, expected.threshold, `${where}: module row threshold`);
  assert.equal(built.direction, expected.direction, `${where}: module row direction`);

  const option = benchmarkChartOption(built, "light");
  const { row } = expected;
  const isRss = row.kind === "samples" && row.plotThreshold !== undefined;
  const expectedGate = isRss ? reported / MIB : reported;
  const expectedBudget = row.plotThreshold ?? expected.threshold;

  // The gate strip plots exactly the asserted value against exactly the
  // frozen threshold, with the fail region on the correct side.
  assert.equal(gateValue(option), expectedGate, `${where}: gate value`);
  assert.equal(budgetLine(option), expectedBudget, `${where}: budget markLine`);
  const region = failRegion(option);
  if (expected.direction === "<=") {
    assert.equal(region[0].xAxis, expectedBudget, `${where}: fail region starts at budget`);
  } else {
    assert.equal(region[0].xAxis, 0, `${where}: fail region starts at zero`);
    assert.equal(region[1].xAxis, expectedBudget, `${where}: fail region ends at budget`);
  }
  const gateAxis = option.xAxis[0];
  assert.ok(
    gateAxis.min === 0 && gateAxis.max >= Math.max(expectedGate, expectedBudget),
    `${where}: gate axis contains the value and the budget`,
  );

  if (row.kind === "scalar") {
    assert.equal(series(option, "samples"), undefined, `${where}: scalar rows plot no samples`);
    assert.match(option.title.subtext, /single measurement per report/u, `${where}: scalar wording`);
    return;
  }

  const sampleAxis = option.xAxis[1];
  if (row.kind === "samples") {
    const plotted = series(option, "samples").data;
    assert.deepEqual(
      plotted.map(([value]) => value),
      row.values,
      `${where}: every retained sample is plotted, in order`,
    );
    const marks = series(option, "samples").markLine.data;
    assert.equal(marks[0].xAxis, row.median, `${where}: median markLine`);
    assert.equal(marks[1].xAxis, row.p95, `${where}: p95 markLine`);
    assert.ok(
      sampleAxis.min <= Math.min(...row.values) && sampleAxis.max >= Math.max(...row.values),
      `${where}: sample axis contains every sample`,
    );
    return;
  }

  // ratio rows: both raw runs plotted, never per-sample ratios.
  const num = series(option, "numerator samples").data;
  const den = series(option, "denominator samples").data;
  assert.deepEqual(num.map(([value]) => value), row.num.values, `${where}: numerator samples`);
  assert.deepEqual(den.map(([value]) => value), row.den.values, `${where}: denominator samples`);
  assert.ok(num.every(([, label]) => label === row.num.label), `${where}: numerator category`);
  assert.ok(den.every(([, label]) => label === row.den.label), `${where}: denominator category`);
  const medians = series(option, "medians").data;
  assert.deepEqual(
    medians,
    [
      [med(row.num.values), row.num.label],
      [med(row.den.values), row.den.label],
    ],
    `${where}: sub-strip median ticks`,
  );
  const all = [...row.num.values, ...row.den.values];
  assert.ok(
    sampleAxis.min === 0 && sampleAxis.max >= Math.max(...all),
    `${where}: shared raw axis starts at zero and contains every sample`,
  );
  assert.match(
    option.title.subtext,
    /sampled independently, so no per-sample ratios/u,
    `${where}: ratio honesty wording`,
  );
}

const setup = (async () => {
  const reports = await loadReports();
  const built = await benchmarkRowsByFamily();
  const expectations = new Map([
    ["native-lint", arrayFamilyExpectations(reports.get("native-lint").report, expectNativeLint(reports.get("native-lint").report))],
    ["native-format", arrayFamilyExpectations(reports.get("native-format").report, expectNativeFormat(reports.get("native-format").report))],
    ["type-aware", expectTypeAware(reports.get("type-aware").report)],
    ["vite", expectVite(reports.get("vite").report)],
    ["editor", expectEditor(reports.get("editor").report)],
    ["comparative", expectComparative(reports.get("comparative").report)],
  ]);
  return { reports, built, expectations };
})();

test("every gated observed value recomputes exactly from a retained sample array", async () => {
  const { expectations } = await setup;
  for (const [family, rows] of expectations) {
    for (const expected of rows) {
      if (expected.row.kind === "scalar") continue;
      // Bit-for-bit: the asserted number is a pure function of the retained
      // array plus in-report byte counts, nothing hand-entered.
      assert.equal(
        expected.observed,
        expected.reported ?? expected.observed,
        `${family}: ${expected.label}: observed value is not traceable to its retained array`,
      );
    }
  }
});

test("every chart option value matches recomputation from the selected reports", async () => {
  const { built, expectations } = await setup;
  for (const [family, rows] of expectations) {
    const builtRows = (built[family] ?? []).filter((row) => row.direction !== "==");
    assert.equal(builtRows.length, rows.length, `${family}: chart row count`);
    rows.forEach((expected, index) => checkRow(family, expected, builtRows[index]));
  }
});

test("the RSS budget used for plotting equals the asserted threshold", async () => {
  const { reports } = await setup;
  const report = reports.get("native-lint").report;
  const assertion = report.assertions.find((entry) => entry.name === "P07 TSRX peak RSS");
  assert.equal(report.summaries.p07.allowedRssBytes, assertion.threshold);
});

test("exactly-1 invariant rows stay out of the charts entirely", async () => {
  const { reports, built } = await setup;
  for (const family of ["native-lint", "native-format"]) {
    const report = reports.get(family).report;
    const invariants = report.assertions.filter(
      (entry) => entry.observed === 1 && entry.threshold === 1,
    );
    const charted = built[family]
      .filter((row) => row.direction !== "==")
      .map((row) => row.label);
    for (const invariant of invariants) {
      assert.ok(
        !charted.includes(invariant.name.replace(/_/g, " ")),
        `${family}: invariant ${invariant.name} must be table-only`,
      );
    }
  }
});
