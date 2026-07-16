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

test("the static benchmark page renders every latest release-gate family", async () => {
  const html = await benchmarksSectionsHtml();
  assert.equal((html.match(/<h2 id="/g) ?? []).length, 6);
  for (const id of families) {
    assert.match(html, new RegExp(`<h2 id="${id}">`));
  }
  assert.doesNotMatch(html, /BUDGET FAILURES PRESENT/);
});

test("launch-facing performance links point only at each latest retained report", async () => {
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
    ["comparative", "Latest passing Apple M5 Pro report:"],
    ["native-lint", "Latest passing report:"],
    ["native-format", "Latest passing report:"],
    ["vite", "Latest passing Apple M5 Pro report:"],
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

test("benchmark tooltip dataset values survive an innerHTML consumer as text", () => {
  const hostile = '<img src=x onerror="window.__benchXss=1">';
  const encoded = escapeDatasetHtml(hostile);
  assert.equal(encoded.includes("<"), false);
  assert.match(encoded, /&amp;lt;img/u);
  assert.match(encoded, /&amp;quot;/u);
});

test("latest/current performance copy cannot retain superseded measurements", async () => {
  const stale = [
    /257\.66 MiB\/s/u,
    /127\.40 MiB\/s/u,
    /827\.9[23] MiB\/s/u,
    /3\.33 ms/u,
    /3\.06 ms/u,
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

test("release headlines are derived from the latest retained measurements", async () => {
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
