import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  lstat,
  readFile,
  readdir,
  realpath,
  writeFile,
} from "node:fs/promises";
import { createRequire } from "node:module";
import path from "node:path";
import { performance } from "node:perf_hooks";
import { pathToFileURL } from "node:url";

import { canonicalJson, semanticProjection, summarizeSamples } from "./lib.mjs";

const DEFAULT_REFERENCE = "/private/tmp/markless-reference-baseline";
const DEFAULT_CANDIDATE = "/private/tmp/markless-oxc-tsrx-dropin";

function usage() {
  return `Usage: node --experimental-strip-types benchmarks/markless-dropin/run.mjs [options]

Options:
  --reference <path>   Markless worktree with stock @tsrx/core
  --candidate <path>   Markless worktree with packed OXC-for-TSRX compatibility package
  --warmup <count>     Whole-corpus warmup rounds per arm (default: 2)
  --iterations <count> Timed whole-corpus rounds per arm (default: 10)
  --output <path>      Also write the complete JSON receipt
  --json               Print the complete JSON receipt instead of the concise summary
  --help                Show this help
`;
}

function positiveInteger(value, option) {
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed < 1) {
    throw new TypeError(`${option} must be a positive integer`);
  }
  return parsed;
}

function parseArguments(argv) {
  const options = {
    reference: DEFAULT_REFERENCE,
    candidate: DEFAULT_CANDIDATE,
    warmup: 2,
    iterations: 10,
    output: null,
    json: false,
    help: false,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === "--json" || argument === "--help") {
      options[argument.slice(2)] = true;
      continue;
    }
    if (!["--reference", "--candidate", "--warmup", "--iterations", "--output"].includes(argument)) {
      throw new TypeError(`unsupported option: ${argument}`);
    }
    const value = argv[++index];
    if (!value) throw new TypeError(`${argument} requires a value`);
    const name = argument.slice(2);
    options[name] = name === "warmup" || name === "iterations"
      ? positiveInteger(value, argument)
      : value;
  }
  options.reference = path.resolve(options.reference);
  options.candidate = path.resolve(options.candidate);
  if (options.output !== null) options.output = path.resolve(options.output);
  return options;
}

function sha256(value) {
  return createHash("sha256").update(value).digest("hex");
}

function git(root, args, encoding = "utf8") {
  return execFileSync("git", ["-C", root, ...args], {
    encoding,
    maxBuffer: 16 * 1024 * 1024,
  });
}

async function loadCorpus(referenceRoot, candidateRoot) {
  const referenceCommit = git(referenceRoot, ["rev-parse", "HEAD"]).trim();
  const candidateCommit = git(candidateRoot, ["rev-parse", "HEAD"]).trim();
  if (referenceCommit !== candidateCommit) {
    throw new Error(`Markless commit mismatch: ${referenceCommit} != ${candidateCommit}`);
  }

  const tracked = git(referenceRoot, ["ls-files", "-z", "--", "*.tsrx"], "buffer")
    .toString("utf8")
    .split("\0")
    .filter(Boolean)
    .sort();
  if (tracked.length === 0) throw new Error("Markless corpus contains no tracked .tsrx files");

  const entries = [];
  const manifest = createHash("sha256");
  for (const file of tracked) {
    const [referenceBytes, candidateBytes] = await Promise.all([
      readFile(path.join(referenceRoot, file)),
      readFile(path.join(candidateRoot, file)),
    ]);
    if (!referenceBytes.equals(candidateBytes)) {
      throw new Error(`tracked corpus source differs between worktrees: ${file}`);
    }
    const digest = sha256(referenceBytes);
    manifest.update(file).update("\0").update(digest).update("\n");
    entries.push({
      file,
      source: referenceBytes.toString("utf8"),
      bytes: referenceBytes.length,
      sha256: digest,
    });
  }
  return {
    commit: referenceCommit,
    checksum: manifest.digest("hex"),
    entries,
  };
}

async function packageRootFromEntry(entry) {
  let directory = path.dirname(await realpath(entry));
  for (;;) {
    const manifestPath = path.join(directory, "package.json");
    try {
      const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
      return { root: await realpath(directory), manifest, manifestPath };
    } catch (error) {
      if (error?.code !== "ENOENT") throw error;
    }
    const parent = path.dirname(directory);
    if (parent === directory) throw new Error(`cannot find package root for ${entry}`);
    directory = parent;
  }
}

async function loadArm(root) {
  const require = createRequire(path.join(root, "packages/compiler/package.json"));
  const entry = require.resolve("@tsrx/core");
  const packageIdentity = await packageRootFromEntry(entry);
  const api = await import(pathToFileURL(entry).href);
  const compilerEntry = path.join(root, "packages/compiler/src/index.ts");
  const compiler = await import(pathToFileURL(compilerEntry).href);
  if (typeof api.parseModule !== "function") {
    throw new TypeError(`${entry} does not export parseModule`);
  }
  if (
    typeof compiler.compileTsrxModule !== "function" ||
    typeof compiler.collectTsrxModuleDiagnostics !== "function"
  ) {
    throw new TypeError(`${compilerEntry} does not expose the Markless compiler contract`);
  }
  return {
    root,
    api,
    compiler,
    entry: await realpath(entry),
    packageRoot: packageIdentity.root,
    manifest: packageIdentity.manifest,
  };
}

function errorSummary(error) {
  return {
    name: error instanceof Error ? error.name : typeof error,
    message: error instanceof Error ? error.message : String(error),
  };
}

function parseOnce(arm, entry) {
  const program = arm.api.parseModule(entry.source, entry.file);
  if (program?.type !== "Program" || !Array.isArray(program.body)) {
    throw new TypeError(`${entry.file} did not return an ESTree Program`);
  }
  return program;
}

function validateParseClassification(reference, candidate, entries) {
  const valid = [];
  const excluded = [];
  for (const entry of entries) {
    let referenceError = null;
    let candidateError = null;
    try {
      parseOnce(reference, entry);
    } catch (error) {
      referenceError = errorSummary(error);
    }
    try {
      parseOnce(candidate, entry);
    } catch (error) {
      candidateError = errorSummary(error);
    }
    if (Boolean(referenceError) !== Boolean(candidateError)) {
      throw new Error(
        `parser success/failure classification differs for ${entry.file}: ` +
          canonicalJson({ referenceError, candidateError }),
      );
    }
    if (referenceError === null) valid.push(entry);
    else excluded.push({ file: entry.file, reference: referenceError, candidate: candidateError });
  }
  if (valid.length === 0) throw new Error("no parser-valid files remain for timing");
  return { valid, excluded };
}

async function validateSemantics(reference, candidate, entries) {
  const aggregate = createHash("sha256");
  const files = [];
  const mismatches = [];
  for (const entry of entries) {
    const input = {
      filename: entry.file,
      source: entry.source,
      buildId: "oxc-tsrx-markless-dropin-benchmark",
      resolverId: "oxc-tsrx-markless-dropin-resolver",
      symbols: [],
    };
    const [referenceResult, candidateResult] = await Promise.all([
      reference.compiler.compileTsrxModule(input),
      candidate.compiler.compileTsrxModule(input),
    ]);
    const referenceJson = canonicalJson(
      semanticProjection(
        referenceResult,
        reference.compiler.collectTsrxModuleDiagnostics(referenceResult),
      ),
    );
    const candidateJson = canonicalJson(
      semanticProjection(
        candidateResult,
        candidate.compiler.collectTsrxModuleDiagnostics(candidateResult),
      ),
    );
    const referenceHash = sha256(referenceJson);
    const candidateHash = sha256(candidateJson);
    files.push({ file: entry.file, sha256: referenceHash });
    aggregate.update(entry.file).update("\0").update(referenceHash).update("\n");
    if (referenceHash !== candidateHash || referenceJson !== candidateJson) {
      mismatches.push({ file: entry.file, referenceSha256: referenceHash, candidateSha256: candidateHash });
    }
  }
  if (mismatches.length > 0) {
    throw new Error(`Markless-consumed semantic output differs: ${canonicalJson(mismatches)}`);
  }
  return { checksum: aggregate.digest("hex"), files };
}

function parseCorpus(arm, entries) {
  let witness = 0;
  for (const entry of entries) {
    const program = parseOnce(arm, entry);
    witness = (witness + program.body.length + (program.end ?? 0)) >>> 0;
  }
  return witness;
}

function measureArm(arm, entries) {
  const start = performance.now();
  const witness = parseCorpus(arm, entries);
  return { elapsedMs: performance.now() - start, witness };
}

function runTiming(reference, candidate, entries, warmup, iterations) {
  let witness = null;
  for (let index = 0; index < warmup; index += 1) {
    const order = index % 2 === 0
      ? [reference, candidate]
      : [candidate, reference];
    for (const arm of order) {
      const observed = parseCorpus(arm, entries);
      if (witness === null) witness = observed;
      else if (observed !== witness) throw new Error("parse witness changed during warmup");
    }
  }

  const samples = { reference: [], candidate: [] };
  for (let index = 0; index < iterations; index += 1) {
    const order = index % 2 === 0
      ? [["reference", reference], ["candidate", candidate]]
      : [["candidate", candidate], ["reference", reference]];
    for (const [name, arm] of order) {
      const measured = measureArm(arm, entries);
      if (measured.witness !== witness) throw new Error(`parse witness changed for ${name}`);
      samples[name].push(measured.elapsedMs);
    }
  }
  return {
    witness,
    reference: summarizeSamples(samples.reference, entries.length),
    candidate: summarizeSamples(samples.candidate, entries.length),
    candidateSpeedup: Number(
      (
        summarizeSamples(samples.reference, entries.length).meanWholeCorpusMs /
        summarizeSamples(samples.candidate, entries.length).meanWholeCorpusMs
      ).toFixed(3),
    ),
    rawWholeCorpusMs: {
      reference: samples.reference.map((value) => Number(value.toFixed(3))),
      candidate: samples.candidate.map((value) => Number(value.toFixed(3))),
    },
  };
}

async function resolveDependencyRoot(packageRoot, dependency) {
  let directory = packageRoot;
  for (;;) {
    const direct = path.join(directory, "node_modules", ...dependency.split("/"));
    try {
      const root = await realpath(direct);
      await readFile(path.join(root, "package.json"));
      return root;
    } catch (error) {
      if (error?.code !== "ENOENT") throw error;
    }
    const parent = path.dirname(directory);
    if (parent === directory) break;
    directory = parent;
  }
  const require = createRequire(path.join(packageRoot, "package.json"));
  try {
    const manifest = require.resolve(`${dependency}/package.json`);
    return (await packageRootFromEntry(manifest)).root;
  } catch (manifestError) {
    try {
      return (await packageRootFromEntry(require.resolve(dependency))).root;
    } catch (entryError) {
      if (entryError?.code === "MODULE_NOT_FOUND" || manifestError?.code === "MODULE_NOT_FOUND") {
        return null;
      }
      throw entryError;
    }
  }
}

async function directoryBytes(root) {
  let total = 0;
  for (const entry of await readdir(root, { withFileTypes: true })) {
    if (entry.name === "node_modules") continue;
    const filename = path.join(root, entry.name);
    if (entry.isDirectory()) total += await directoryBytes(filename);
    else if (entry.isFile()) total += (await lstat(filename)).size;
  }
  return total;
}

async function installedGraph(arm) {
  const pending = [arm.packageRoot];
  const visited = new Set();
  const packages = [];
  while (pending.length > 0) {
    const root = await realpath(pending.pop());
    if (visited.has(root)) continue;
    visited.add(root);
    const manifest = JSON.parse(await readFile(path.join(root, "package.json"), "utf8"));
    const dependencyNames = [
      ...Object.keys(manifest.dependencies ?? {}),
      ...Object.keys(manifest.optionalDependencies ?? {}),
    ];
    const installedDependencies = [];
    for (const dependency of dependencyNames) {
      const dependencyRoot = await resolveDependencyRoot(root, dependency);
      if (dependencyRoot === null) continue;
      installedDependencies.push(dependency);
      pending.push(dependencyRoot);
    }
    packages.push({
      name: manifest.name ?? path.basename(root),
      version: manifest.version ?? null,
      root,
      installedBytes: await directoryBytes(root),
      declaredDependencyCount: dependencyNames.length,
      installedDependencies: installedDependencies.sort(),
    });
  }
  packages.sort((left, right) =>
    `${left.name}@${left.version}`.localeCompare(`${right.name}@${right.version}`),
  );
  return {
    rootPackage: `${arm.manifest.name}@${arm.manifest.version}`,
    dependencyCount: Math.max(0, packages.length - 1),
    packageCountIncludingRoot: packages.length,
    installedBytesIncludingRoot: packages.reduce(
      (total, entry) => total + entry.installedBytes,
      0,
    ),
    packages,
  };
}

function packageIdentity(arm) {
  return {
    requested: "@tsrx/core",
    resolvedName: arm.manifest.name,
    version: arm.manifest.version,
    entry: arm.entry,
    packageRoot: arm.packageRoot,
  };
}

function formatBytes(bytes) {
  const units = ["B", "KiB", "MiB", "GiB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1_024 && unit < units.length - 1) {
    value /= 1_024;
    unit += 1;
  }
  return `${value.toFixed(unit === 0 ? 0 : 2)} ${units[unit]}`;
}

function printSummary(report, output) {
  const lines = [
    `Correctness: ${report.corpus.discoveredFiles} tracked, ${report.corpus.timedFiles} parser-valid, ${report.corpus.excludedFiles.length} symmetrically rejected; semantic SHA-256 ${report.corpus.semanticSha256}`,
    `Stock @tsrx/core: ${report.timing.reference.meanWholeCorpusMs} ms/corpus, ${report.timing.reference.filesPerSecond} files/s`,
    `Packed OXC for TSRX: ${report.timing.candidate.meanWholeCorpusMs} ms/corpus, ${report.timing.candidate.filesPerSecond} files/s`,
    `Candidate speedup: ${report.timing.candidateSpeedup}x`,
    `Dependencies: ${report.dependencies.reference.dependencyCount} -> ${report.dependencies.candidate.dependencyCount}`,
    `Installed bytes (root + dependency closure): ${formatBytes(report.dependencies.reference.installedBytesIncludingRoot)} -> ${formatBytes(report.dependencies.candidate.installedBytesIncludingRoot)}`,
  ];
  if (output !== null) lines.push(`JSON receipt: ${output}`);
  process.stdout.write(`${lines.join("\n")}\n`);
}

async function main() {
  const options = parseArguments(process.argv.slice(2));
  if (options.help) {
    process.stdout.write(usage());
    return;
  }

  const corpus = await loadCorpus(options.reference, options.candidate);
  const [reference, candidate] = await Promise.all([
    loadArm(options.reference),
    loadArm(options.candidate),
  ]);
  const classification = validateParseClassification(reference, candidate, corpus.entries);
  const semantic = await validateSemantics(reference, candidate, classification.valid);
  const timing = runTiming(
    reference,
    candidate,
    classification.valid,
    options.warmup,
    options.iterations,
  );
  const [referenceDependencies, candidateDependencies] = await Promise.all([
    installedGraph(reference),
    installedGraph(candidate),
  ]);

  const report = {
    schemaVersion: 1,
    generatedAt: new Date().toISOString(),
    environment: {
      platform: process.platform,
      arch: process.arch,
      node: process.version,
    },
    markless: {
      commit: corpus.commit,
      referenceRoot: options.reference,
      candidateRoot: options.candidate,
    },
    packages: {
      reference: packageIdentity(reference),
      candidate: packageIdentity(candidate),
    },
    protocol: {
      warmupWholeCorpusRoundsPerArm: options.warmup,
      timedWholeCorpusRoundsPerArm: options.iterations,
      alternatingArmOrder: true,
      parserEntryPoint: "installed @tsrx/core parseModule",
      semanticProjection:
        "Markless-consumed diagnostics/componentEdges/styleScopes/protocolView/payloadScripts/publicRenderModule/symbolModules/runtimeDemandMap",
    },
    corpus: {
      discoveredFiles: corpus.entries.length,
      timedFiles: classification.valid.length,
      bytes: classification.valid.reduce((total, entry) => total + entry.bytes, 0),
      sourceManifestSha256: corpus.checksum,
      semanticSha256: semantic.checksum,
      excludedFiles: classification.excluded,
    },
    timing,
    dependencies: {
      reference: referenceDependencies,
      candidate: candidateDependencies,
      dependencyDelta: candidateDependencies.dependencyCount - referenceDependencies.dependencyCount,
      installedByteDelta:
        candidateDependencies.installedBytesIncludingRoot -
        referenceDependencies.installedBytesIncludingRoot,
    },
    verdict: {
      sameMarklessCommit: true,
      byteIdenticalTrackedCorpus: true,
      identicalParserSuccessFailureClassification: true,
      identicalMarklessConsumedSemanticChecksum: true,
    },
  };

  if (options.output !== null) {
    await writeFile(options.output, `${JSON.stringify(report, null, 2)}\n`, "utf8");
  }
  if (options.json) process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
  else printSummary(report, options.output);
}

function frozenOption(name, fallback = null) {
  const index = process.argv.indexOf(`--${name}`);
  return index === -1 ? fallback : process.argv[index + 1];
}

function medianNumber(values) {
  const sorted = [...values].sort((left, right) => left - right);
  return sorted[Math.floor(sorted.length / 2)];
}

function coefficientOfVariation(values) {
  const mean = values.reduce((sum, value) => sum + value, 0) / values.length;
  const variance = values.reduce((sum, value) => sum + (value - mean) ** 2, 0) / values.length;
  return Math.sqrt(variance) / mean;
}

async function frozenMain() {
  const contract = JSON.parse(await readFile(new URL("./frozen-contract.json", import.meta.url)));
  const reference = path.resolve(frozenOption("reference", DEFAULT_REFERENCE));
  const candidate = path.resolve(frozenOption("candidate", DEFAULT_CANDIDATE));
  const projectionManifest = path.resolve(
    frozenOption("projection-manifest", "/private/tmp/oxc-tsrx-projections/manifest.json"),
  );
  const output = frozenOption("output");
  const childScript = path.join(import.meta.dirname, "performance-child.mjs");
  const childEnvironment = { ...process.env };
  delete childEnvironment.OXC_TSRX_PARSER_ADDON;
  const children = [];
  for (let child = 0; child < contract.protocol.children; child += 1) {
    const stdout = execFileSync(
      process.execPath,
      [
        "--experimental-strip-types",
        childScript,
        "--reference", reference,
        "--candidate", candidate,
        "--projection-manifest", projectionManifest,
        "--child", String(child),
      ],
      { encoding: "utf8", env: childEnvironment, maxBuffer: 64 * 1024 * 1024 },
    );
    children.push(JSON.parse(stdout));
  }

  const groups = {};
  for (const name of ["small", "medium", "large", "full", "training", "heldout"]) {
    const first = children[0].timing[name];
    const stockMedians = children.map((child) => child.timing[name].stock.medianMs);
    const candidateMedians = children.map((child) => child.timing[name].candidate.medianMs);
    const stockMeans = children.map((child) => child.timing[name].stock.meanMs);
    const candidateMeans = children.map((child) => child.timing[name].candidate.meanMs);
    const stockMedianMs = medianNumber(stockMedians);
    const candidateMedianMs = medianNumber(candidateMedians);
    const stockMeanMs = medianNumber(stockMeans);
    const candidateMeanMs = medianNumber(candidateMeans);
    groups[name] = {
      files: first.files,
      bytes: first.bytes,
      stockMedianMs,
      candidateMedianMs,
      candidateToStock: candidateMedianMs / stockMedianMs,
      medianThroughputSpeedup: stockMedianMs / candidateMedianMs,
      stockMeanMs,
      candidateMeanMs,
      meanThroughputSpeedup: stockMeanMs / candidateMeanMs,
      acrossChildCv: {
        stockMedian: coefficientOfVariation(stockMedians),
        candidateMedian: coefficientOfVariation(candidateMedians),
      },
      childMedians: { stock: stockMedians, candidate: candidateMedians },
    };
  }
  const officialMedians = children.map((child) => child.officialHeldout.medianMs);
  const officialHeldoutMedianMs = medianNumber(officialMedians);
  const variancePass = Object.values(groups).every(
    (group) =>
      group.acrossChildCv.stockMedian <= contract.protocol.acrossChildCvMax &&
      group.acrossChildCv.candidateMedian <= contract.protocol.acrossChildCvMax,
  ) && coefficientOfVariation(officialMedians) <= contract.protocol.acrossChildCvMax;
  const checks = {
    variance: variancePass,
    smallAbsolute: groups.small.candidateMedianMs <= contract.gates.small.candidateMedianMsMax,
    smallStockRatio: groups.small.candidateToStock <= contract.gates.small.candidateToStockMax,
    mediumAbsolute: groups.medium.candidateMedianMs <= contract.gates.medium.candidateMedianMsMax,
    mediumStockRatio: groups.medium.candidateToStock <= contract.gates.medium.candidateToStockMax,
    largeAbsolute: groups.large.candidateMedianMs <= contract.gates.large.candidateMedianMsMax,
    largeStockRatio: groups.large.candidateToStock <= contract.gates.large.candidateToStockMax,
    fullAbsolute: groups.full.candidateMedianMs <= contract.gates.full.candidateMedianMsMax,
    fullStockRatio: groups.full.candidateToStock <= contract.gates.full.candidateToStockMax,
    fullMeanThroughput:
      groups.full.meanThroughputSpeedup >= contract.gates.full.throughputSpeedupMin,
    heldoutMedianThroughput:
      groups.heldout.medianThroughputSpeedup >=
      contract.gates.heldout.medianThroughputSpeedupMin,
    heldoutMeanThroughput:
      groups.heldout.meanThroughputSpeedup >= contract.gates.heldout.meanThroughputSpeedupMin,
    heldoutOfficialEnvelope:
      groups.heldout.candidateMedianMs / officialHeldoutMedianMs <=
      contract.gates.heldout.candidateToOfficialMedianMax,
  };
  const passed = Object.values(checks).every(Boolean);
  const report = {
    schema: "true-oxc-frozen-campaign-v1",
    generatedAt: new Date().toISOString(),
    contract,
    roots: { reference, candidate, projectionManifest },
    groups,
    officialHeldout: {
      medianMs: officialHeldoutMedianMs,
      acrossChildCv: coefficientOfVariation(officialMedians),
      childMedians: officialMedians,
    },
    checks,
    passed,
    children,
  };
  if (output) await writeFile(path.resolve(output), `${JSON.stringify(report, null, 2)}\n`);
  if (process.argv.includes("--json")) {
    process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
  } else {
    process.stdout.write(
      `Frozen campaign: ${passed ? "PASS" : "FAIL"}\n` +
      `Full: candidate ${groups.full.candidateMedianMs.toFixed(3)} ms, stock ` +
      `${groups.full.stockMedianMs.toFixed(3)} ms, ${groups.full.medianThroughputSpeedup.toFixed(3)}x\n` +
      `Held out: ${groups.heldout.medianThroughputSpeedup.toFixed(3)}x stock, ` +
      `${(groups.heldout.candidateMedianMs / officialHeldoutMedianMs).toFixed(3)}x official\n` +
      `Variance: ${variancePass ? "pass" : "fail"}\n`,
    );
  }
  if (!passed) process.exitCode = 1;
}

if (process.argv.includes("--frozen")) await frozenMain();
else await main();
