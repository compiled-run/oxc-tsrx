import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { chmod, mkdir, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import test from "node:test";
import { nativePackageName, nativeTargetForHost } from "../../packages/toolchain/dist/native-targets.js";
import { npmChildEnvironment, resolveNpmInvocation } from "../../scripts/npm-invocation.mjs";
import { temporaryDirectory } from "./temporary-directory.mjs";

/**
 * The pre-publish gate's own failure path.
 *
 * A gate nobody has watched reject anything is decoration, and the release it
 * exists for happens a few times a year, so the only way its rejection stays
 * true is to break a package deliberately on every run. Each case below packs a
 * real npm tarball from a real `npm pack`, so the artifact the gate reads is
 * shaped by npm's own file exclusion rather than by a fixture that asserts what
 * it wants to prove.
 *
 * The two cases that matter most are the two shapes of the 0.1.0 defect:
 *
 *   - a `files` entry whose file is absent, which npm silently drops;
 *   - a platform package that is internally consistent about having no parser
 *     addon, which is what 0.1.0 actually published. Eight packages declared no
 *     addon, listed none in `files`, contained none, and every
 *     `oxc-tsrx/parser` import failed on every platform. A gate that only reads
 *     the `files` array cannot see that one, so the gate also holds platform
 *     packages to what they must contain.
 */

const root = resolve(import.meta.dirname, "../..");
const gate = join(root, "scripts/check-publish-artifacts.mjs");
const FIXTURE_VERSION = "3.2.1";

function linuxLibc() {
  if (process.platform !== "linux") return undefined;
  return process.report?.getReport?.().header?.glibcVersionRuntime ? "glibc" : "musl";
}

const target = nativeTargetForHost(process.platform, process.arch, linuxLibc());
const packageName = nativePackageName(target);
const binaryName = target.os === "win32" ? "oxc-tsrx.exe" : "oxc-tsrx";

function run(args, options = {}) {
  return new Promise((resolveRun, rejectRun) => {
    const child = spawn(process.execPath, args, {
      cwd: options.cwd ?? root,
      env: options.env ?? process.env,
      stdio: ["ignore", "pipe", "pipe"],
    });
    let stdout = "";
    let stderr = "";
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => (stdout += chunk));
    child.stderr.on("data", (chunk) => (stderr += chunk));
    child.on("error", rejectRun);
    child.on("close", (status) => resolveRun({ status, stdout, stderr }));
  });
}

function sha256(contents) {
  return createHash("sha256").update(contents).digest("hex");
}

/**
 * A platform package with the shape `scripts/package-native.mjs` produces, small
 * enough to pack in milliseconds. `mutate` receives the staged package before it
 * is packed, which is where each deliberate break is introduced.
 */
async function packFixture(name, mutate = () => {}) {
  const directory = await temporaryDirectory(`oxc-tsrx-gate-fixture-${name}-`);
  const stage = join(directory, "package");
  const artifacts = join(directory, "artifacts");
  await mkdir(join(stage, "bin"), { recursive: true });
  await mkdir(artifacts, { recursive: true });

  const binary = Buffer.from(`fixture executable for ${target.target}\n`);
  const addon = Buffer.from(`fixture parser addon for ${target.target}\n`);
  const addonRecord = {
    packageVersion: FIXTURE_VERSION,
    target: target.target,
    bytes: addon.length,
    sha256: sha256(addon),
    object: { format: "fixture", os: target.os, bits: 64, architectures: [target.cpu] },
    nodeApi: 8,
    capabilities: { lazy: true, async: true },
    role: "canonical-parser",
    file: "parser.node",
    apiVersion: 1,
    transportAbi: 1,
  };
  const files = {
    [`bin/${binaryName}`]: binary,
    "parser.node": addon,
    LICENSE: Buffer.from("MIT\n"),
    "checksums.json": null,
  };
  const manifest = {
    name: packageName,
    version: FIXTURE_VERSION,
    license: "MIT",
    files: ["bin", "parser.node", "checksums.json", "LICENSE"],
    os: [target.os],
    cpu: [target.cpu],
    ...(target.libc ? { libc: [target.libc] } : {}),
    publishConfig: { access: "public", provenance: true },
    oxcTsrx: {
      schemaVersion: 2,
      nativeProtocolVersion: 2,
      target: target.target,
      vscodeTarget: target.vscodeTarget,
      binaries: [binaryName],
      addons: { "parser.node": addonRecord },
    },
  };
  const checksums = {
    schemaVersion: 2,
    packageName,
    version: FIXTURE_VERSION,
    target: target.target,
    binaries: { [binaryName]: { bytes: binary.length, sha256: sha256(binary) } },
    addons: { "parser.node": addonRecord },
  };

  await mutate({ manifest, checksums, files, addonRecord });

  files["checksums.json"] = Buffer.from(`${JSON.stringify(checksums, null, 2)}\n`);
  await writeFile(join(stage, "package.json"), `${JSON.stringify(manifest, null, 2)}\n`);
  for (const [path, contents] of Object.entries(files)) {
    if (contents === null) continue;
    await writeFile(join(stage, path), contents);
  }
  if (target.os !== "win32" && files[`bin/${binaryName}`] !== null) {
    await chmod(join(stage, "bin", binaryName), 0o755);
  }

  const invocation = resolveNpmInvocation([
    "pack",
    "--json",
    "--pack-destination",
    artifacts,
    stage,
  ]);
  const packed = await run(invocation.args, {
    cwd: stage,
    env: { ...npmChildEnvironment(process.env), npm_config_cache: join(directory, ".npm-cache") },
  });
  assert.equal(packed.status, 0, packed.stderr || packed.stdout);
  return artifacts;
}

async function runGate(artifacts) {
  return run([gate, "--artifacts", artifacts, "--version", FIXTURE_VERSION, "--no-install"]);
}

test("the gate accepts a well-formed platform package and says what it checked", async () => {
  const artifacts = await packFixture("healthy");
  const result = await runGate(artifacts);
  assert.equal(result.status, 0, result.stdout + result.stderr);
  assert.match(result.stdout, new RegExp(`package\\s+${packageName.replace("/", "\\/")}@${FIXTURE_VERSION}`, "u"));
  assert.match(result.stdout, /files\s+4\/4 declared entries present/u);
  assert.match(result.stdout, /binary\s+bin\/oxc-tsrx(\.exe)?\s.*matches checksums\.json/u);
  assert.match(result.stdout, /addon\s+parser\.node\s.*matches checksums\.json and the manifest record/u);
  // The rehearsal is part of the gate, not a separate ritual, so a healthy
  // artifact has to survive `npm publish --dry-run` too.
  assert.match(result.stdout, /npm publish --dry-run accepted 5 files/u);
  assert.match(result.stdout, /gate: PASS  1 package at 3\.2\.1/u);
  assert.doesNotMatch(result.stdout, /error/u);
});

test("the gate rejects a package whose files entry names a file that is not there", async () => {
  // The 0.1.0 mechanism exactly: `files` promises the addon, the addon is not
  // on disk, npm drops it without a word, and the package publishes broken.
  const artifacts = await packFixture("missing-files-entry", ({ files }) => {
    files["parser.node"] = null;
  });
  const result = await runGate(artifacts);
  assert.equal(result.status, 1, result.stdout);
  assert.match(
    result.stdout,
    new RegExp(`${packageName.replace("/", "\\/")}: files entry "parser\\.node" is missing from the tarball`, "u"),
  );
  assert.match(result.stdout, /gate: FAIL/u);
  assert.match(result.stdout, /this release must not be published/u);
  // A gate that stops at the first problem hides the rest; the report names the
  // addon three ways, because three separate promises were broken.
  assert.match(result.stdout, /the parser addon is missing from the tarball: parser\.node/u);
  // Nothing downstream may run once the contents are known to be wrong.
  assert.match(result.stdout, /publish rehearsal\s+skipped/u);
});

test("the gate rejects the package 0.1.0 actually published, which promised no addon at all", async () => {
  const artifacts = await packFixture("no-addon", ({ manifest, checksums, files }) => {
    // Byte for byte the 0.1.0 shape: the packager treated the addon as
    // optional, so the manifest, the files array, the checksums, and the
    // tarball all agreed that there was no addon. Nothing was inconsistent.
    // Everything was wrong.
    manifest.files = manifest.files.filter((file) => file !== "parser.node");
    manifest.oxcTsrx.schemaVersion = 1;
    delete manifest.oxcTsrx.addons;
    delete checksums.addons;
    checksums.schemaVersion = 1;
    files["parser.node"] = null;
  });
  const result = await runGate(artifacts);
  assert.equal(result.status, 1, result.stdout);
  assert.match(
    result.stdout,
    /the files array does not list "parser\.node", so npm would leave it out of the tarball/u,
  );
  assert.match(result.stdout, /declares no parser addon; every oxc-tsrx\/parser import on/u);
  assert.match(result.stdout, /oxcTsrx\.schemaVersion is 1, expected 2/u);
  assert.match(result.stdout, /gate: FAIL/u);
});

test("the gate rejects a package whose declared binary is not in the tarball", async () => {
  const artifacts = await packFixture("missing-binary", ({ files }) => {
    files[`bin/${binaryName}`] = null;
  });
  const result = await runGate(artifacts);
  assert.equal(result.status, 1, result.stdout);
  assert.match(
    result.stdout,
    new RegExp(`the declared binary "${binaryName.replace(".", "\\.")}" is missing from the tarball: bin/${binaryName.replace(".", "\\.")}`, "u"),
  );
});

test("the gate rejects a package whose packed bytes disagree with its own checksums", async () => {
  const artifacts = await packFixture("checksum-drift", ({ files }) => {
    // The addon shipped, and it is not the addon that was verified. A file list
    // cannot see this; only hashing what was packed can.
    files["parser.node"] = Buffer.from("a different addon than the one that was recorded\n");
  });
  const result = await runGate(artifacts);
  assert.equal(result.status, 1, result.stdout);
  assert.match(result.stdout, /parser\.node does not match its own record: packed \d+ bytes/u);
});

test("the gate rejects an artifact set that is not the version being released", async () => {
  const artifacts = await packFixture("version-drift", ({ manifest }) => {
    manifest.version = "3.2.0";
  });
  const result = await runGate(artifacts);
  assert.equal(result.status, 1, result.stdout);
  assert.match(result.stdout, /packed version is 3\.2\.0, expected 3\.2\.1/u);
});

test("the gate rejects an artifact set that is missing a platform package", async () => {
  const artifacts = await packFixture("incomplete-matrix");
  const result = await run([
    gate,
    "--artifacts",
    artifacts,
    "--version",
    FIXTURE_VERSION,
    "--no-install",
    "--require-full-matrix",
  ]);
  assert.equal(result.status, 1, result.stdout);
  assert.match(result.stdout, /the artifact set is missing oxc-tsrx@3\.2\.1/u);
  const others = [...new Set([...result.stdout.matchAll(/missing (@oxc-tsrx\/native-[\w-]+)@/gu)].map((match) => match[1]))];
  assert.equal(others.length, 7, `expected the seven other platform packages to be named, got ${others.join(", ")}`);
});
