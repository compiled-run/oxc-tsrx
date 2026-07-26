import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdir, mkdtemp, readFile, realpath, rm, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { cpus, release as osRelease, tmpdir } from "node:os";
import { delimiter, dirname, join, resolve } from "node:path";
import { performance } from "node:perf_hooks";
import { pathToFileURL } from "node:url";
import { installPhysicalToolPackages } from "../../tests/vite/physical-consumer.mjs";

const require = createRequire(import.meta.url);
const root = resolve(import.meta.dirname, "../..");
const fixture = join(root, "tests/fixtures/vite/toolchain/diagnostics");
const budgetsPath = join(root, "benchmarks/vite/budgets.json");
const budgets = JSON.parse(await readFile(budgetsPath, "utf8"));
const lintBin = join(root, "target/release/oxc-tsrx");
const formatBin = join(root, "target/release/oxc-tsrx");
// pnpm keeps `oxlint-current` and `oxfmt-current` under the package that
// declares them, so they are resolved from that manifest rather than from a
// hoisted repository-root `node_modules`.
const fromToolchain = createRequire(pathToFileURL(join(root, "packages/toolchain/package.json")).href);
const toolchainPackage = (name) => dirname(fromToolchain.resolve(`${name}/package.json`));
const productLintBin = join(root, "packages/toolchain/bin/oxlint");
const productFormatBin = join(root, "packages/toolchain/bin/oxfmt");
const canonicalOxfmtBin = join(toolchainPackage("oxfmt-current"), "bin/oxfmt");
const canonicalOxlintBin = join(toolchainPackage("oxlint-current"), "bin/oxlint");
const warmups = 5;
// Twenty samples make nearest-rank p95 the second-highest observation instead
// of the single maximum, so one scheduler outlier cannot decide a release.
const samples = 20;

function run(command, args, options) {
  return new Promise((resolveRun, rejectRun) => {
    const started = performance.now();
    const child = spawn(command, args, { ...options, stdio: ["ignore", "pipe", "pipe"] });
    let stdout = "";
    let stderr = "";
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => (stdout += chunk));
    child.stderr.on("data", (chunk) => (stderr += chunk));
    child.on("error", rejectRun);
    child.on("close", (status) =>
      resolveRun({ status, stdout, stderr, milliseconds: performance.now() - started }),
    );
  });
}

async function measure(factory, expectedStatus) {
  for (let index = 0; index < warmups; index += 1) {
    const warmup = await factory();
    assert.equal(warmup.status, expectedStatus, warmup.stderr || warmup.stdout);
  }
  const values = [];
  let finalResult;
  for (let index = 0; index < samples; index += 1) {
    finalResult = await factory();
    assert.equal(finalResult.status, expectedStatus, finalResult.stderr || finalResult.stdout);
    values.push(finalResult.milliseconds);
  }
  return { values, finalResult };
}

async function corpusIdentity() {
  const files = [
    ".oxfmtrc.json",
    ".oxlintrc.json",
    "package.json",
    "vite.config.mjs",
    "src/ordinary.tsx",
    "src/view-equivalent.tsx",
    "src/view.tsrx",
  ];
  const hash = createHash("sha256");
  let bytes = 0;
  for (const relative of files) {
    const source = await readFile(join(fixture, relative));
    hash.update(relative).update("\0").update(source);
    bytes += source.length;
  }
  return {
    kind: "mixed authored TSX/TSRX command-boundary fixture with equivalent TSX control",
    files: files.map((relative) => `tests/fixtures/vite/toolchain/diagnostics/${relative}`),
    bytes,
    sha256: hash.digest("hex"),
  };
}

function percentile(values, quantile) {
  const sorted = [...values].sort((left, right) => left - right);
  return sorted[Math.min(sorted.length - 1, Math.ceil(sorted.length * quantile) - 1)];
}

function summarize(measurement) {
  return {
    rawMs: measurement.values,
    medianMs: percentile(measurement.values, 0.5),
    p95Ms: percentile(measurement.values, 0.95),
  };
}

async function makeVitePlusConsumer() {
  const project = await realpath(await mkdtemp(join(tmpdir(), "oxc-tsrx-vite-benchmark-")));
  await mkdir(join(project, "src"), { recursive: true });
  for (const file of ["ordinary.tsx", "view.tsrx"]) {
    await writeFile(join(project, "src", file), await readFile(join(fixture, "src", file), "utf8"));
  }
  for (const file of ["package.json", ".oxlintrc.json", ".oxfmtrc.json", "vite.config.mjs"]) {
    await writeFile(join(project, file), await readFile(join(fixture, file), "utf8"));
  }
  const modules = join(project, "node_modules");
  await mkdir(modules, { recursive: true });
  await installPhysicalToolPackages(modules, "vite-plus-current");
  return project;
}

const ordinary = join(fixture, "src/ordinary.tsx");
const equivalent = join(fixture, "src/view-equivalent.tsx");
const tsrx = join(fixture, "src/view.tsrx");
const lintConfig = join(fixture, ".oxlintrc.json");
const formatConfig = join(fixture, ".oxfmtrc.json");

const canonicalLint = await measure(
  () =>
    run(
      canonicalOxlintBin,
      ["--format=json", "--config", lintConfig, ordinary, equivalent],
      { cwd: root, env: process.env },
    ),
  1,
);
const directLint = await measure(
  () =>
    run(
      productLintBin,
      ["--format=json", "--config", lintConfig, ordinary, tsrx],
      { cwd: root, env: { ...process.env, OXC_TSRX_LINT_BIN: lintBin } },
    ),
  1,
);
const lintMetadata = JSON.parse(directLint.finalResult.stdout).oxcTsrx;

const canonicalFormat = await measure(
  () =>
    run(
      canonicalOxfmtBin,
      ["--check", "--config", formatConfig, ordinary, equivalent],
      { cwd: root, env: process.env },
    ),
  1,
);
const directOrdinaryFormat = await measure(
  () =>
    run(
      productFormatBin,
      ["--check", "--config", formatConfig, ordinary, equivalent],
      { cwd: root, env: process.env },
    ),
  1,
);
const directFormat = await measure(
  () =>
    run(
      productFormatBin,
      ["--check", "--config", formatConfig, ordinary, tsrx],
      { cwd: root, env: { ...process.env, OXC_TSRX_FORMAT_BIN: formatBin } },
    ),
  1,
);

const ordinaryFormatResult = ({ status, stdout, stderr }) => ({
  status,
  stdout: stdout.replace(/<?\d+(?:\.\d+)?ms\b/gu, "<runtime>"),
  stderr,
});
assert.deepEqual(
  ordinaryFormatResult(directOrdinaryFormat.finalResult),
  ordinaryFormatResult(canonicalFormat.finalResult),
  "ordinary npm formatter must preserve canonical process behavior",
);
const routeDirectory = await mkdtemp(join(tmpdir(), "oxc-tsrx-vite-route-"));
const ordinaryFormatTrace = join(routeDirectory, "ordinary-format.jsonl");
let ordinaryFormatDispatchEvents = 0;
try {
  const tracedOrdinaryFormat = await run(
    productFormatBin,
    ["--check", "--config", formatConfig, ordinary, equivalent],
    {
      cwd: root,
      env: { ...process.env, OXC_TSRX_TRACE_FILE: ordinaryFormatTrace },
    },
  );
  assert.deepEqual(
    ordinaryFormatResult(tracedOrdinaryFormat),
    ordinaryFormatResult(canonicalFormat.finalResult),
    "traced ordinary npm formatter parity",
  );
  try {
    ordinaryFormatDispatchEvents = (await readFile(ordinaryFormatTrace, "utf8"))
      .trim()
      .split("\n")
      .filter(Boolean).length;
  } catch (error) {
    if (error?.code !== "ENOENT") throw error;
  }
  assert.equal(
    ordinaryFormatDispatchEvents,
    0,
    "ordinary npm formatting must not enter the TSRX process-dispatch layer",
  );
} finally {
  await rm(routeDirectory, { recursive: true, force: true });
}

const consumer = await makeVitePlusConsumer();
let vitePlusLint;
try {
  const vitePlusRoot = join(consumer, "node_modules/vite-plus");
  vitePlusLint = await measure(
    () =>
      run(process.execPath, [join(vitePlusRoot, "dist/bin.js"), "lint", "src"], {
        cwd: consumer,
        env: {
          ...process.env,
          NO_COLOR: "1",
          NODE_PATH: [join(consumer, "node_modules"), join(root, "node_modules")].join(delimiter),
          OXC_TSRX_LINT_BIN: lintBin,
          OXC_TSRX_FORMAT_BIN: formatBin,
        },
      }),
    1,
  );
} finally {
  await rm(consumer, { recursive: true, force: true });
}

const summary = {
  schemaVersion: 2,
  timestamp: new Date().toISOString(),
  host: {
    platform: process.platform,
    arch: process.arch,
    node: process.version,
    cpu: cpus()[0]?.model ?? "unknown",
    osRelease: osRelease(),
  },
  build: {
    profile: "release",
    lintBinary: "target/release/oxc-tsrx",
    formatBinary: "target/release/oxc-tsrx",
    lintLauncher: "node_modules/oxc-tsrx/bin/oxlint",
    formatLauncher: "node_modules/oxc-tsrx/bin/oxfmt",
    oxcRevision: lintMetadata.oxcRevision,
  },
  corpus: await corpusIdentity(),
  versions: {
    vite: require("vite/package.json").version,
    vitePlusCurrent: require("vite-plus-current/package.json").version,
    oxlint: require("oxlint-current/package.json").version,
    oxfmt: require("oxfmt-current/package.json").version,
  },
  samplePolicy: {
    warmups,
    measured: samples,
    statistic: "median and nearest-rank p95 over fresh companion processes",
  },
  canonicalLint: summarize(canonicalLint),
  directMixedLint: summarize(directLint),
  canonicalFormat: summarize(canonicalFormat),
  directOrdinaryFormat: summarize(directOrdinaryFormat),
  directMixedFormat: summarize(directFormat),
  vitePlusCurrentMixedLint: summarize(vitePlusLint),
  ratios: {},
  invariants: {
    nativeTsrxParseCount: lintMetadata.parseCount,
    nativeTsrxFiles: lintMetadata.files.tsrx,
    ordinaryFilesInNativeLane: lintMetadata.files.standard,
    ordinaryFormatProcessParity: true,
    ordinaryFormatDispatchEvents,
  },
  budgets,
  assertions: {},
};
summary.ratios.directLintVsCanonicalP95 =
  summary.directMixedLint.p95Ms / summary.canonicalLint.p95Ms;
summary.ratios.directFormatVsCanonicalP95 =
  summary.directMixedFormat.p95Ms / summary.canonicalFormat.p95Ms;
summary.ratios.directOrdinaryFormatVsCanonicalP95 =
  summary.directOrdinaryFormat.p95Ms / summary.canonicalFormat.p95Ms;
summary.assertions = {
  directLintP95: summary.directMixedLint.p95Ms <= budgets.directLintP95MsMax,
  directLintRatio:
    summary.ratios.directLintVsCanonicalP95 <= budgets.directLintVsCanonicalP95RatioMax,
  directFormatP95: summary.directMixedFormat.p95Ms <= budgets.directFormatP95MsMax,
  directFormatRatio:
    summary.ratios.directFormatVsCanonicalP95 <= budgets.directFormatVsCanonicalP95RatioMax,
  directOrdinaryFormatP95:
    summary.directOrdinaryFormat.p95Ms <= budgets.directOrdinaryFormatP95MsMax,
  directOrdinaryFormatRatio:
    summary.ratios.directOrdinaryFormatVsCanonicalP95 <=
    budgets.directOrdinaryFormatVsCanonicalP95RatioMax,
  vitePlusLintP95: summary.vitePlusCurrentMixedLint.p95Ms <= budgets.vitePlusCurrentLintP95MsMax,
  oneNativeParse:
    lintMetadata.parseCount === budgets.nativeTsrxParseCountPerFile &&
    lintMetadata.files.tsrx === 1 &&
    lintMetadata.files.standard === 0,
};

const output = join(root, `benchmarks/vite/results-${Date.now()}.json`);
await writeFile(output, `${JSON.stringify(summary, null, 2)}\n`);
console.log(JSON.stringify({ output, ...summary.assertions, summary }, null, 2));
if (Object.values(summary.assertions).some((passed) => !passed)) process.exitCode = 1;
