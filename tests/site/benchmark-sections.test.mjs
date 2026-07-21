import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import test from "node:test";

import {
  benchmarksSectionsHtml,
  comparativeChartHtml,
  escapeDatasetHtml,
} from "../../docs/benchmarks-data.mjs";

const root = resolve(import.meta.dirname, "../..");
const families = ["comparative", "native-lint", "native-format", "type-aware", "vite", "editor"];

async function latestReports() {
  const aggregate = JSON.parse(
    await readFile(join(root, "docs/acceptance/performance-report.json"), "utf8"),
  );
  const latest = new Map();
  for (const family of families) {
    const selected = aggregate.results?.[family]?.path;
    assert.match(selected ?? "", new RegExp(`^benchmarks/${family}/results-\\d+\\.json$`));
    const file = selected.split("/").at(-1);
    latest.set(family, {
      file,
      report: JSON.parse(await readFile(join(root, selected), "utf8")),
    });
  }
  return latest;
}

test("the static benchmark page renders every aggregate-selected release-gate family", async () => {
  const html = await benchmarksSectionsHtml();
  assert.equal((html.match(/<h2 id="/g) ?? []).length, 6);
  for (const id of families) {
    assert.match(html, new RegExp(`<h2 id="${id}">`));
  }
  assert.doesNotMatch(html, /BUDGET FAILURES PRESENT/);
});

test("launch-facing performance links point only at each aggregate-selected report", async () => {
  const latest = await latestReports();

  for (const relative of [
    "README.md",
    "docs/acceptance/matrix.md",
    "docs/architecture/rust-oxc-core.md",
    "docs/integrations/configuration.md",
    "docs/integrations/editor.md",
    "docs/integrations/vite-plus.md",
    "docs/releasing/v0.1.0.md",
  ]) {
    const source = await readFile(join(root, relative), "utf8");
    for (const match of source.matchAll(
      /benchmarks\/(comparative|native-lint|native-format|type-aware|vite|editor)\/(results-\d+\.json)/gu,
    )) {
      assert.equal(match[2], latest.get(match[1]).file, `${relative}: ${match[1]}`);
    }
  }

  for (const [family, prefix] of [
    ["comparative", "Aggregate-selected representative report:"],
    ["native-lint", "Aggregate-selected representative report:"],
    ["native-format", "Aggregate-selected representative report:"],
    ["vite", "Aggregate-selected representative report:"],
  ]) {
    const source = await readFile(join(root, "benchmarks", family, "README.md"), "utf8");
    assert.match(
      source,
      new RegExp(`${prefix.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&")}\\s*\`${latest.get(family).file}\``),
      `benchmarks/${family}/README.md`,
    );
  }
});

test("the matched comparison is honest about absolute timings and frozen ratio gates", async () => {
  const source = await readFile(join(root, "docs", "benchmarks-data.mjs"), "utf8");
  const build = await readFile(join(root, "docs", "build.mjs"), "utf8");
  const siteConfig = await readFile(join(root, "docs", "site.config.mjs"), "utf8");
  const html = await comparativeChartHtml();

  assert.doesNotMatch(source, /data-pass="true"/u);
  assert.match(html, /absolute median wall-clock/u);
  assert.match(html, /frozen ratio gate/u);
  assert.doesNotMatch(build, /Each number is also a release gate/u);
  assert.doesNotMatch(build, /Matched 1,000-file TSX CLI comparison[\s\S]{0,500}dashed line/u);
  assert.doesNotMatch(siteConfig, /Every headline number above[\s\S]*frozen release gate/u);
});

test("the matched comparison publishes the exact ordinary and mixed process routes", async () => {
  const latest = await latestReports();
  const report = latest.get("comparative").report;
  const route = report.validation.routeEvidence;
  const html = await comparativeChartHtml();

  assert.equal(route.ordinaryDispatchEvents, 0);
  assert.equal(route.publicCanonicalNodeChildren, 1);
  assert.equal(route.nativeTsrxChildren, 1);
  assert.equal(route.privateInProcessAdapterChildren, 0);
  assert.match(html, /same Node process with zero TSRX dispatch/u);
  assert.match(
    html,
    new RegExp(
      `${route.publicCanonicalNodeChildren} public canonical Node child, ${route.nativeTsrxChildren} native TSRX child, and ${route.privateInProcessAdapterChildren} private adapter children`,
      "u",
    ),
  );
  assert.doesNotMatch(html, /all-TSX[^.]*native TSRX lane/iu);
  assert.match(html, /Matched cross-tool bars use the same[^.]*TSX files/iu);
  assert.match(html, /separately patterned mixed bar is the paired internal workload/iu);
  assert.doesNotMatch(html, /Bar lengths show[^.]*same[^.]*TSX files/iu);
});

test("public methodology explains fail-closed near-threshold adjudication", async () => {
  // Collapse the markdown's hard line wrapping so phrase assertions cannot
  // fail on where a sentence happens to break.
  const source = (await readFile(join(root, "docs/reference/benchmarks.md"), "utf8")).replace(
    /\s+/gu,
    " ",
  );
  const aggregate = JSON.parse(
    await readFile(join(root, "docs/acceptance/performance-report.json"), "utf8"),
  );
  const html = await benchmarksSectionsHtml();
  assert.match(source, /3% near-threshold band/u);
  assert.match(source, /exactly two additional fresh\s+reports/u);
  assert.match(source, /two[-\s]of[-\s]three/u);
  assert.match(source, /median normalized budget pressure/u);
  assert.match(source, /report-path tie-break/u);
  assert.match(source, /selected representative is red/u);
  assert.equal((html.match(/Near-threshold adjudication\./gu) ?? []).length, 2);
  for (const family of ["native-format", "comparative"]) {
    for (const report of aggregate.results[family].adjudication.reports) {
      assert.match(html, new RegExp(report.path.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&"), "u"));
    }
  }
});

test("benchmark tooltip dataset values survive an innerHTML consumer as text", () => {
  const hostile = '<img src=x onerror="window.__benchXss=1">';
  const encoded = escapeDatasetHtml(hostile);
  assert.equal(encoded.includes("<"), false);
  assert.match(encoded, /&amp;lt;img/u);
  assert.match(encoded, /&amp;quot;/u);
});

test("benchmark charts plot retained distributions honestly", async () => {
  const source = await readFile(join(root, "docs", "benchmarks-data.mjs"), "utf8");
  const html = await benchmarksSectionsHtml();

  // Six charts, each drawing one dot per retained sample on the shared
  // percent-of-budget axis, with shape-distinct median and p95 markers.
  assert.equal((html.match(/<svg class="bench-chart"/gu) ?? []).length, 6);
  const dotCount = (html.match(/class="bench-dot /gu) ?? []).length;
  assert.ok(dotCount >= 500, `expected the retained sample arrays to appear as dots, saw ${dotCount}`);
  assert.equal(
    (html.match(/class="bench-median /gu) ?? []).length,
    (html.match(/class="bench-p95 /gu) ?? []).length,
  );

  // Gates asserted on a single recorded value must say so instead of faking
  // a distribution.
  assert.match(html, /single measurement per report/u);

  // Ratio gates plot the asserted ratio plus both raw arrays; per-sample
  // ratios are never derived because the runs are not index-paired.
  assert.match(html, /runs are sampled independently, so no per-sample ratios/u);
  const gateMarkers = (html.match(/class="bench-gate /gu) ?? []).length;
  assert.equal((html.match(/class="bench-subtrack"/gu) ?? []).length, gateMarkers * 2);

  // Every tooltip payload, including the new sample line, stays inside the
  // double-escaping contract consumed by app.js.
  assert.match(source, /data-samples="\$\{escapeDatasetHtml\(/u);
  assert.match(html, /data-samples="\d+ samples/u);
});

test("aggregate-selected performance copy cannot retain superseded measurements", async () => {
  const stale = [
    /257\.66 MiB\/s/u,
    /127\.40 MiB\/s/u,
    /827\.9[23] MiB\/s/u,
    /3\.33 ms/u,
    /3\.06 ms/u,
    /256\.61 MiB\/s/u,
    /131\.47 MiB\/s/u,
    /840\.09 MiB\/s/u,
    /3\.22 ms/u,
    /4\.20 ms/u,
    /results-178431(?:677|678|679|680|682|893|894|895|896|898)\d*\.json/u,
    /Latest passing report: `results-178421/u,
  ];
  for (const relative of [
    "docs/releasing/v0.1.0.md",
    "docs/site.config.mjs",
    "benchmarks/native-lint/README.md",
    "benchmarks/native-format/README.md",
    "benchmarks/vite/README.md",
  ]) {
    const source = await readFile(join(root, relative), "utf8");
    for (const pattern of stale) assert.doesNotMatch(source, pattern, relative);
  }
});

test("release headlines are derived from the aggregate-selected measurements", async () => {
  const latest = await latestReports();
  const lintAssertion = (name) =>
    latest.get("native-lint").report.assertions.find((entry) => entry.name === name).observed;
  const formatAssertion = (name) =>
    latest.get("native-format").report.assertions.find((entry) => entry.name === name).observed;
  const editor = latest.get("editor").report;
  const headlines = [
    `${lintAssertion("P02 median scan+copy+parse throughput").toFixed(2)} MiB/s`,
    `${formatAssertion("p04_sequential_median_mib_s").toFixed(2)} MiB/s`,
    `${formatAssertion("p04_default_thread_mib_s").toFixed(2)} MiB/s`,
    `${lintAssertion("P05 fresh-process TSRX p95 latency").toFixed(2)} ms cold lint p95`,
    `${editor.initialOpen.p95Ms.toFixed(2)} ms fresh editor`,
  ];
  const release = await readFile(join(root, "docs/releasing/v0.1.0.md"), "utf8");
  for (const headline of headlines) assert.match(release, new RegExp(headline.replace(".", "\\.")));
});

test("public benchmark copy names non-comparable boundaries without speedup marketing", async () => {
  const benchmarkSource = await readFile(join(root, "docs/benchmarks-data.mjs"), "utf8");
  const buildSource = await readFile(join(root, "docs/build.mjs"), "utf8");
  assert.doesNotMatch(benchmarkSource, /about .* times a historical/iu);
  assert.doesNotMatch(benchmarkSource, /keystroke in VS Code/iu);
  assert.doesNotMatch(benchmarkSource, /RSS after 100-edit soak/iu);
  assert.doesNotMatch(benchmarkSource, /Lint speed vs stock Oxlint/iu);
  assert.doesNotMatch(benchmarkSource, /typescript-eslint recommended/iu);
  assert.doesNotMatch(benchmarkSource, /median of 3 runs/iu);
  assert.doesNotMatch(benchmarkSource, /same generated .*20%.*tsrx/iu);
  assert.doesNotMatch(buildSource, /oxlint-tsrx keeps that speed/iu);
});
