import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readFile, realpath } from "node:fs/promises";
import { createRequire } from "node:module";
import path from "node:path";
import { performance } from "node:perf_hooks";
import { pathToFileURL } from "node:url";

import { canonicalJson, semanticProjection } from "./lib.mjs";

const root = path.resolve(import.meta.dirname, "../..");
const contract = JSON.parse(await readFile(new URL("./frozen-contract.json", import.meta.url)));

function option(name, fallback = null) {
  const index = process.argv.indexOf(`--${name}`);
  return index === -1 ? fallback : process.argv[index + 1];
}

const referenceRoot = path.resolve(option("reference"));
const candidateRoot = path.resolve(option("candidate"));
const projectionManifest = path.resolve(option("projection-manifest", "/private/tmp/oxc-tsrx-projections/manifest.json"));
const childIndex = Number(option("child", "0"));
if (!Number.isInteger(childIndex) || childIndex < 0) throw new TypeError("--child must be non-negative");

function sha256(value) {
  return createHash("sha256").update(value).digest("hex");
}

function percentile(values, fraction) {
  const sorted = [...values].sort((left, right) => left - right);
  return sorted[Math.max(0, Math.ceil(sorted.length * fraction) - 1)];
}

function stats(values, files) {
  const meanMs = values.reduce((sum, value) => sum + value, 0) / values.length;
  const variance = values.reduce((sum, value) => sum + (value - meanMs) ** 2, 0) / values.length;
  return {
    rawMs: values,
    meanMs,
    medianMs: percentile(values, 0.5),
    p95Ms: percentile(values, 0.95),
    cv: Math.sqrt(variance) / meanMs,
    filesPerSecond: files / (meanMs / 1_000),
  };
}

function packageRequire(marklessRoot) {
  return createRequire(path.join(marklessRoot, "packages/compiler/package.json"));
}

async function loadArm(marklessRoot) {
  const require = packageRequire(marklessRoot);
  const entry = require.resolve("@tsrx/core");
  const api = await import(pathToFileURL(entry).href);
  const compiler = await import(pathToFileURL(path.join(marklessRoot, "packages/compiler/src/index.ts")).href);
  return { entry: await realpath(entry), api, compiler };
}

function parse(arm, entry) {
  const program = arm.api.parseModule(entry.source, entry.file);
  if (program?.type !== "Program" || !Array.isArray(program.body)) {
    throw new TypeError(`${entry.file} did not return a concrete Program`);
  }
  return program;
}

function parseGroup(arm, entries) {
  let witness = 0;
  for (const entry of entries) {
    const program = parse(arm, entry);
    witness = (witness + program.body.length + (program.end ?? 0)) >>> 0;
  }
  return witness;
}

function timeGroup(reference, candidate, entries) {
  let witness = null;
  for (let round = 0; round < contract.protocol.warmupsPerArm; round += 1) {
    const order = (round + childIndex) % 2 === 0
      ? [reference, candidate]
      : [candidate, reference];
    for (const arm of order) {
      const value = parseGroup(arm, entries);
      if (witness === null) witness = value;
      else if (witness !== value) throw new Error("Program witness changed during warmup");
    }
  }
  const raw = { stock: [], candidate: [] };
  for (let round = 0; round < contract.protocol.retainedRoundsPerArm; round += 1) {
    const order = (round + childIndex) % 2 === 0
      ? [["stock", reference], ["candidate", candidate]]
      : [["candidate", candidate], ["stock", reference]];
    for (const [name, arm] of order) {
      const start = performance.now();
      const value = parseGroup(arm, entries);
      raw[name].push(performance.now() - start);
      if (value !== witness) throw new Error(`${name} Program witness changed during timing`);
    }
  }
  return {
    files: entries.length,
    bytes: entries.reduce((sum, entry) => sum + entry.bytes, 0),
    stock: stats(raw.stock, entries.length),
    candidate: stats(raw.candidate, entries.length),
  };
}

function timeOfficial(parser, entries) {
  const run = () => {
    let witness = 0;
    for (const entry of entries) {
      const result = parser.parseSync(entry.filename, entry.source);
      if (result.errors.length !== 0 || result.program?.type !== "Program") {
        throw new Error(`official OXC projection failed for ${entry.file}`);
      }
      witness = (witness + result.program.body.length + (result.program.end ?? 0)) >>> 0;
    }
    return witness;
  };
  let witness = null;
  for (let round = 0; round < contract.protocol.warmupsPerArm; round += 1) {
    const value = run();
    if (witness === null) witness = value;
    else if (witness !== value) throw new Error("official OXC witness changed");
  }
  const raw = [];
  for (let round = 0; round < contract.protocol.retainedRoundsPerArm; round += 1) {
    const start = performance.now();
    const value = run();
    raw.push(performance.now() - start);
    if (value !== witness) throw new Error("official OXC witness changed during timing");
  }
  return stats(raw, entries.length);
}

async function semanticChecksum(reference, candidate, entries) {
  const aggregate = createHash("sha256");
  for (const entry of entries) {
    const input = {
      filename: entry.file,
      source: entry.source,
      buildId: "true-oxc-installed-performance-v1",
      resolverId: "true-oxc-installed-performance-v1",
      symbols: [],
    };
    const [stock, oxc] = await Promise.all([
      reference.compiler.compileTsrxModule(input),
      candidate.compiler.compileTsrxModule(input),
    ]);
    const stockJson = canonicalJson(semanticProjection(
      stock,
      reference.compiler.collectTsrxModuleDiagnostics(stock),
    ));
    const candidateJson = canonicalJson(semanticProjection(
      oxc,
      candidate.compiler.collectTsrxModuleDiagnostics(oxc),
    ));
    if (stockJson !== candidateJson) throw new Error(`semantic mismatch: ${entry.file}`);
    aggregate.update(entry.file).update("\0").update(sha256(stockJson)).update("\n");
  }
  return aggregate.digest("hex");
}

const referenceCommit = execFileSync("git", ["-C", referenceRoot, "rev-parse", "HEAD"], { encoding: "utf8" }).trim();
const candidateCommit = execFileSync("git", ["-C", candidateRoot, "rev-parse", "HEAD"], { encoding: "utf8" }).trim();
if (referenceCommit !== contract.marklessCommit || candidateCommit !== referenceCommit) {
  throw new Error(`unexpected Markless commits: ${referenceCommit}, ${candidateCommit}`);
}
const files = execFileSync("git", ["-C", referenceRoot, "ls-files", "-z", "--", "*.tsrx"])
  .toString("utf8").split("\0").filter(Boolean).sort();
if (files.length !== contract.discoveredFiles) throw new Error(`expected ${contract.discoveredFiles} files`);
const sourceManifest = createHash("sha256");
const discovered = [];
for (const file of files) {
  const bytes = await readFile(path.join(referenceRoot, file));
  const candidateBytes = await readFile(path.join(candidateRoot, file));
  if (!bytes.equals(candidateBytes)) throw new Error(`source mismatch: ${file}`);
  sourceManifest.update(file).update("\0").update(sha256(bytes)).update("\n");
  discovered.push({ file, source: bytes.toString("utf8"), bytes: bytes.length, raw: bytes });
}
if (sourceManifest.digest("hex") !== contract.sourceManifestSha256) throw new Error("source manifest changed");

const [reference, candidate] = await Promise.all([loadArm(referenceRoot), loadArm(candidateRoot)]);
const accepted = [];
const excluded = [];
for (const entry of discovered) {
  let stockOk = true;
  let candidateOk = true;
  try { parse(reference, entry); } catch { stockOk = false; }
  try { parse(candidate, entry); } catch { candidateOk = false; }
  if (stockOk !== candidateOk) throw new Error(`classification mismatch: ${entry.file}`);
  (stockOk ? accepted : excluded).push(entry);
}
if (accepted.length !== contract.acceptedFiles || excluded.length !== contract.discoveredFiles - contract.acceptedFiles) {
  throw new Error(`classification count changed: ${accepted.length}/${excluded.length}`);
}
if (accepted.reduce((sum, entry) => sum + entry.bytes, 0) !== contract.acceptedBytes) throw new Error("accepted byte count changed");
const semanticSha256 = await semanticChecksum(reference, candidate, accepted);
if (semanticSha256 !== contract.semanticSha256) throw new Error(`semantic hash changed: ${semanticSha256}`);

accepted.sort((left, right) => left.bytes - right.bytes || left.file.localeCompare(right.file));
const training = [];
const heldout = [];
for (const entry of accepted) {
  const digest = createHash("sha256").update(entry.file).update("\0").update(entry.raw).digest();
  (digest[0] % 4 === 0 ? training : heldout).push(entry);
}
for (const [name, entries] of [["training", training], ["heldout", heldout]]) {
  const frozen = contract.split[name];
  if (entries.length !== frozen.files || entries.reduce((sum, entry) => sum + entry.bytes, 0) !== frozen.bytes) {
    throw new Error(`${name} split changed`);
  }
}
const groups = {
  small: accepted.slice(0, 67),
  medium: accepted.slice(67, 134),
  large: accepted.slice(134),
  full: accepted,
  training,
  heldout,
};
const timing = {};
for (const name of ["small", "medium", "large", "full", "training", "heldout"]) {
  timing[name] = timeGroup(reference, candidate, groups[name]);
}

const projectionRows = JSON.parse(await readFile(projectionManifest));
const heldoutSet = new Set(heldout.map((entry) => entry.file));
const officialEntries = [];
for (const row of projectionRows) {
  if (!heldoutSet.has(row.file)) continue;
  officialEntries.push({
    file: row.file,
    filename: row.file.replace(/\.tsrx$/u, ".tsx"),
    source: await readFile(row.sourcePath, "utf8"),
  });
}
if (officialEntries.length !== heldout.length) throw new Error("official held-out projection set changed");
const officialRequire = createRequire(path.join(root, "package.json"));
const officialEntry = officialRequire.resolve("oxc-parser-reference");
const official = await import(pathToFileURL(officialEntry).href);

process.stdout.write(`${JSON.stringify({
  schema: "true-oxc-performance-child-v1",
  childIndex,
  environment: { platform: process.platform, arch: process.arch, node: process.version },
  packages: { stock: reference.entry, candidate: candidate.entry, official: officialEntry },
  corpus: { accepted: accepted.length, excluded: excluded.length, semanticSha256 },
  timing,
  officialHeldout: timeOfficial(official, officialEntries),
})}\n`);
