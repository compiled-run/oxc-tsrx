import { mkdir, mkdtemp, readFile, readdir, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { basename, join, resolve } from "node:path";
import { NATIVE_TARGETS, nativePackageName } from "../packages/toolchain/dist/native-targets.js";
import { spawnCommand } from "../packages/toolchain/dist/spawn-command.js";
import { hostPlatformPackage, installAndExerciseRelease } from "./installed-release-check.mjs";
import { npmChildEnvironment, resolveNpmInvocation } from "./npm-invocation.mjs";
import { parseNpmPackResponse } from "./npm-pack-response.mjs";
import { startStagingRegistry } from "./publish-staging-registry.mjs";
import { readPackageTarball } from "./read-package-tarball.mjs";

/**
 * The pre-publish gate.
 *
 * 0.1.0 shipped every one of the eight platform packages without `parser.node`,
 * so `oxc-tsrx/parser` threw on every consumer machine on every platform, and
 * the release was called done because the publish workflow asked npm whether a
 * version string resolved. A version string resolving proves the package
 * exists. It proves nothing about what is inside it.
 *
 * This asks the four questions that would have caught it, in order:
 *
 *   1. Does every path each package promises actually exist inside it? That is
 *      oxc's `check-npm-packages.js` shape, extended: a platform package that
 *      honestly declares no addon is exactly what 0.1.0 published, so declaring
 *      no addon is itself a failure, and the packed bytes are cross-checked
 *      against the package's own `checksums.json`.
 *   2. Does the tarball install into an empty project outside this workspace?
 *   3. Does the installed package do real work: a diagnostic out of the
 *      installed linter and an AST out of the installed addon?
 *   4. Does `npm publish --dry-run` accept the artifact?
 *
 * Usage:
 *   node scripts/check-publish-artifacts.mjs --artifacts release --version 0.1.5
 *   node scripts/check-publish-artifacts.mjs --pack-host
 *
 *   --artifacts <dir>      gate every .tgz in <dir>
 *   --pack-host            pack this host's platform package and the public
 *                          package into a temporary directory, then gate those
 *   --version <version>    the version every artifact must declare
 *                          (default: the workspace version)
 *   --require-full-matrix  demand all eight platform packages and the public one
 *   --no-install           run the content assertions and the publish rehearsal
 *                          only, for artifacts with no runnable binary
 *   --json <path>          write the gate report
 */

const root = resolve(import.meta.dirname, "..");
const PUBLIC_PACKAGE = "oxc-tsrx";
const NATIVE_PREFIX = "@oxc-tsrx/native-";
const LIFECYCLE_SCRIPTS = ["preinstall", "install", "postinstall", "prepare", "prepublish"];

function parseArguments(argv) {
  const options = {
    artifacts: null,
    packHost: false,
    version: null,
    requireFullMatrix: false,
    install: true,
    json: null,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === "--pack-host") {
      options.packHost = true;
    } else if (argument === "--require-full-matrix") {
      options.requireFullMatrix = true;
    } else if (argument === "--no-install") {
      options.install = false;
    } else if (["--artifacts", "--version", "--json"].includes(argument)) {
      const value = argv[++index];
      if (!value) throw new Error(`${argument} requires a value`);
      options[argument === "--artifacts" ? "artifacts" : argument.slice(2)] = value;
    } else {
      throw new Error(`unsupported option: ${argument}`);
    }
  }
  if (options.packHost === (options.artifacts !== null)) {
    throw new Error("pass exactly one of --artifacts <dir> and --pack-host");
  }
  return options;
}

const inActions = process.env.GITHUB_ACTIONS === "true";
const failures = [];
const pending = [];

/**
 * Queued rather than printed where it is found, so the report for one package
 * stays readable and its problems arrive together underneath it.
 */
function fail(message) {
  failures.push(message);
  pending.push(message);
}

function flushFailures(indent = "    ") {
  for (const message of pending.splice(0)) {
    process.stdout.write(inActions ? `::error::${message}\n` : `${indent}error: ${message}\n`);
  }
}

function say(line = "") {
  process.stdout.write(`${line}\n`);
}

function size(bytes) {
  if (bytes >= 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MiB`;
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} KiB`;
  return `${bytes} B`;
}

function run(file, args, options = {}) {
  return new Promise((resolveRun, rejectRun) => {
    const child = spawnCommand(file, args, {
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

/**
 * Pack what this host can publish: its own platform package, built from the
 * binaries and the addon this lane already produced, plus the public package.
 * This is what makes the gate runnable on every Tier 1 platform without a
 * release artifact to download.
 */
async function packHostArtifacts(artifacts) {
  const host = hostPlatformPackage();
  const environment = { ...npmChildEnvironment(process.env), npm_config_cache: join(artifacts, ".npm-cache") };
  const packaged = await run(
    process.execPath,
    [
      "scripts/package-native.mjs",
      "--target",
      host.target.target,
      "--bin-dir",
      "target/release",
      "--parser-addon",
      "packages/toolchain/parser.node",
      "--out-dir",
      artifacts,
    ],
    { env: environment },
  );
  if (packaged.status !== 0) {
    throw new Error(`packing ${host.name} failed:\n${packaged.stderr || packaged.stdout}`);
  }
  const native = JSON.parse(packaged.stdout);
  const packInvocation = resolveNpmInvocation([
    "pack",
    "--json",
    "--pack-destination",
    artifacts,
    resolve(root, "packages/toolchain"),
  ]);
  const packed = await run(packInvocation.executable, packInvocation.args, { env: environment });
  if (packed.status !== 0) {
    throw new Error(`npm pack of the public package failed:\n${packed.stderr || packed.stdout}`);
  }
  const publicPackage = parseNpmPackResponse(packed.stdout);
  say(`  packed   ${basename(native.tarball)} and ${publicPackage.filename}`);
  return artifacts;
}

function entryFor(pkg, path) {
  const direct = pkg.entries.get(path);
  if (direct) return direct;
  const prefix = `${path}/`;
  const contained = [...pkg.entries.values()].filter((entry) => entry.path.startsWith(prefix));
  if (contained.length === 0) return null;
  return { path, type: "directory", contained };
}

function requireEntry(pkg, path, reason) {
  const entry = entryFor(pkg, path);
  if (!entry) {
    fail(`${pkg.manifest.name}: ${reason} is missing from the tarball: ${path}`);
    return null;
  }
  return entry;
}

function checksumRecord(pkg) {
  const entry = pkg.entries.get("checksums.json");
  if (!entry) {
    fail(`${pkg.manifest.name}: checksums.json is missing from the tarball`);
    return null;
  }
  try {
    return JSON.parse(entry.text());
  } catch (error) {
    fail(`${pkg.manifest.name}: checksums.json is not valid JSON: ${error.message}`);
    return null;
  }
}

function checkCommon(pkg, version) {
  const { manifest } = pkg;
  const name = manifest.name ?? pkg.tarball;
  if (typeof manifest.name !== "string" || manifest.name.length === 0) {
    fail(`${basename(pkg.tarball)}: the packed manifest declares no name`);
  }
  if (manifest.version !== version) {
    fail(`${name}: packed version is ${manifest.version}, expected ${version}`);
  }
  if (manifest.publishConfig?.access !== "public") {
    fail(`${name}: publishConfig.access is not "public"`);
  }
  if (manifest.publishConfig?.provenance !== true) {
    fail(`${name}: publishConfig.provenance is not true`);
  }
  for (const script of LIFECYCLE_SCRIPTS) {
    if (manifest.scripts?.[script]) {
      fail(`${name}: publishes a ${script} lifecycle script, which a consumer install would run`);
    }
  }

  // The oxc `check-npm-packages.js` assertion: everything the package promises
  // in its own `files` array has to be in the package.
  const files = Array.isArray(manifest.files) ? manifest.files : [];
  if (files.length === 0) {
    fail(`${name}: the packed manifest declares no files array`);
  }
  let present = 0;
  for (const file of files) {
    if (entryFor(pkg, file)) present += 1;
    else fail(`${name}: files entry "${file}" is missing from the tarball`);
  }

  for (const [binary, target] of Object.entries(
    typeof manifest.bin === "string" ? { [name]: manifest.bin } : (manifest.bin ?? {}),
  )) {
    requireEntry(pkg, target.replace(/^\.\//u, ""), `the declared bin "${binary}"`);
  }
  return { declaredFiles: files.length, presentFiles: present };
}

function collectExportTargets(node, found = []) {
  if (typeof node === "string") {
    if (node.startsWith("./")) found.push(node.slice(2));
    return found;
  }
  if (node && typeof node === "object") {
    for (const value of Object.values(node)) collectExportTargets(value, found);
  }
  return found;
}

function checkPublicPackage(pkg, version) {
  const { manifest } = pkg;
  const targets = [...new Set(collectExportTargets(manifest.exports ?? {}))];
  for (const target of targets) requireEntry(pkg, target, `the declared export "./${target}"`);
  for (const field of ["main", "types"]) {
    if (typeof manifest[field] === "string") {
      requireEntry(pkg, manifest[field].replace(/^\.\//u, ""), `the declared ${field}`);
    }
  }
  const expected = NATIVE_TARGETS.map(nativePackageName).sort();
  const optional = manifest.optionalDependencies ?? {};
  const declared = Object.keys(optional).sort();
  if (JSON.stringify(declared) !== JSON.stringify(expected)) {
    fail(
      `${manifest.name}: optionalDependencies must be exactly the eight platform packages, got ` +
        `${declared.join(", ") || "none"}`,
    );
  }
  for (const [dependency, pin] of Object.entries(optional)) {
    if (pin !== version) {
      fail(`${manifest.name}: optionalDependency ${dependency} is pinned to ${pin}, expected ${version}`);
    }
  }
  return { exports: targets.length, optional: declared.length };
}

function checkPlatformPackage(pkg, version) {
  const { manifest } = pkg;
  const name = manifest.name;
  const suffix = name.slice(NATIVE_PREFIX.length);
  const target = NATIVE_TARGETS.find((candidate) => candidate.packageSuffix === suffix);
  if (!target) {
    fail(`${name}: no published target matches this package name`);
    return null;
  }
  if (manifest.os?.[0] !== target.os || manifest.cpu?.[0] !== target.cpu) {
    fail(
      `${name}: declares os/cpu ${manifest.os?.join("+")}/${manifest.cpu?.join("+")}, ` +
        `expected ${target.os}/${target.cpu}`,
    );
  }
  const oxcTsrx = manifest.oxcTsrx ?? {};
  if (oxcTsrx.target !== target.target) {
    fail(`${name}: declares target ${oxcTsrx.target}, expected ${target.target}`);
  }

  // The 0.1.0 shape, stated as an assertion. Its packages were internally
  // consistent: no addon record, no `parser.node` in `files`, no `parser.node`
  // in the tarball, schemaVersion 1. Only a rule about what a platform package
  // must contain can reject a package that is wrong in a self-consistent way.
  if (oxcTsrx.schemaVersion !== 2) {
    fail(
      `${name}: oxcTsrx.schemaVersion is ${oxcTsrx.schemaVersion}, expected 2; ` +
        "schema 1 is a package with no parser addon, which is what 0.1.0 published",
    );
  }
  if (oxcTsrx.nativeProtocolVersion !== 2) {
    fail(`${name}: oxcTsrx.nativeProtocolVersion is ${oxcTsrx.nativeProtocolVersion}, expected 2`);
  }
  const files = Array.isArray(manifest.files) ? manifest.files : [];
  for (const required of ["bin", "parser.node"]) {
    if (!files.includes(required)) {
      fail(
        `${name}: the files array does not list "${required}", so npm would leave it out of the ` +
          "tarball; this is exactly how 0.1.0 published eight packages with no parser addon",
      );
    }
  }

  const checksums = checksumRecord(pkg);
  if (checksums && checksums.version !== version) {
    fail(`${name}: checksums.json records version ${checksums.version}, expected ${version}`);
  }
  if (checksums && checksums.target !== target.target) {
    fail(`${name}: checksums.json records target ${checksums.target}, expected ${target.target}`);
  }

  const binaries = Array.isArray(oxcTsrx.binaries) ? oxcTsrx.binaries : [];
  if (binaries.length === 0) fail(`${name}: oxcTsrx.binaries is empty`);
  const reportedBinaries = [];
  for (const binary of binaries) {
    const entry = requireEntry(pkg, `bin/${binary}`, `the declared binary "${binary}"`);
    if (!entry) continue;
    if (target.os !== "win32" && (entry.mode & 0o111) === 0) {
      fail(`${name}: bin/${binary} is packed without an executable bit (mode ${entry.mode.toString(8)})`);
    }
    const record = checksums?.binaries?.[binary];
    if (!record) {
      fail(`${name}: checksums.json has no record for bin/${binary}`);
      continue;
    }
    if (record.bytes !== entry.size || record.sha256 !== entry.sha256) {
      fail(
        `${name}: bin/${binary} does not match checksums.json: packed ${entry.size} bytes/` +
          `${entry.sha256}, recorded ${record.bytes} bytes/${record.sha256}`,
      );
      continue;
    }
    reportedBinaries.push({ binary, size: entry.size, sha256: entry.sha256 });
  }

  const addonRecord = oxcTsrx.addons?.["parser.node"];
  let reportedAddon = null;
  if (!addonRecord) {
    fail(
      `${name}: declares no parser addon; every oxc-tsrx/parser import on ${target.target} would ` +
        "fail on this package, which is the 0.1.0 defect",
    );
  } else {
    const entry = requireEntry(pkg, "parser.node", "the parser addon");
    const recorded = checksums?.addons?.["parser.node"];
    if (!recorded) fail(`${name}: checksums.json has no parser.node record`);
    if (entry && recorded) {
      if (addonRecord.sha256 !== recorded.sha256 || addonRecord.bytes !== recorded.bytes) {
        fail(`${name}: the manifest addon record and checksums.json disagree about parser.node`);
      } else if (entry.size !== recorded.bytes || entry.sha256 !== recorded.sha256) {
        fail(
          `${name}: parser.node does not match its own record: packed ${entry.size} bytes/` +
            `${entry.sha256}, recorded ${recorded.bytes} bytes/${recorded.sha256}`,
        );
      } else {
        reportedAddon = { size: entry.size, sha256: entry.sha256 };
      }
    }
    if (addonRecord.target !== target.target) {
      fail(`${name}: the parser addon records target ${addonRecord.target}, expected ${target.target}`);
    }
    if (addonRecord.packageVersion !== version) {
      fail(
        `${name}: the parser addon records package version ${addonRecord.packageVersion}, expected ${version}`,
      );
    }
    if (addonRecord.role !== "canonical-parser") {
      fail(`${name}: the parser addon records role ${addonRecord.role}, expected canonical-parser`);
    }
  }
  return { target, binaries: reportedBinaries, addon: reportedAddon };
}

function unpackedSize(pkg) {
  let total = 0;
  for (const entry of pkg.entries.values()) total += entry.type === "file" ? entry.size : 0;
  return total;
}

async function main() {
  const options = parseArguments(process.argv.slice(2));
  const workspaceVersion = JSON.parse(await readFile(join(root, "package.json"), "utf8")).version;
  const version = options.version ?? workspaceVersion;

  say("Pre-publish gate");
  say(`  version    ${version}${options.version ? "" : " (from the workspace manifest)"}`);
  let artifacts = options.artifacts ? resolve(root, options.artifacts) : null;
  if (options.packHost) {
    artifacts = await mkdtemp(join(tmpdir(), "oxc-tsrx-gate-artifacts-"));
    await mkdir(artifacts, { recursive: true });
    say(`  packing    this host's own publishable artifacts into ${artifacts}`);
    await packHostArtifacts(artifacts);
  }
  const tarballs = (await readdir(artifacts))
    .filter((file) => file.endsWith(".tgz"))
    .sort()
    .map((file) => join(artifacts, file));
  say(`  artifacts  ${artifacts} (${tarballs.length} tarball${tarballs.length === 1 ? "" : "s"})`);
  say();
  if (tarballs.length === 0) {
    fail(`no .tgz artifacts found in ${artifacts}`);
    return report(options, { version, artifacts, packages: [] });
  }

  say("[1/3] declared contents");
  say();
  const packages = [];
  for (const tarball of tarballs) {
    const pkg = await readPackageTarball(tarball);
    const name = pkg.manifest.name;
    say(`  ${basename(tarball)}`);
    say(
      `    package  ${name}@${pkg.manifest.version}  (${pkg.entries.size} entries, ` +
        `${size(unpackedSize(pkg))} unpacked)`,
    );
    const common = checkCommon(pkg, version);
    say(`    files    ${common.presentFiles}/${common.declaredFiles} declared entries present`);
    let platform = null;
    if (name === PUBLIC_PACKAGE) {
      const details = checkPublicPackage(pkg, version);
      say(`    exports  ${details.exports} entry points present`);
      say(`    natives  ${details.optional} optional platform packages, pinned to ${version}`);
    } else if (typeof name === "string" && name.startsWith(NATIVE_PREFIX)) {
      platform = checkPlatformPackage(pkg, version);
      for (const binary of platform?.binaries ?? []) {
        say(
          `    binary   bin/${binary.binary}  ${size(binary.size)}  sha256 ${binary.sha256.slice(0, 16)}` +
            "  matches checksums.json",
        );
      }
      if (platform?.addon) {
        say(
          `    addon    parser.node  ${size(platform.addon.size)}  sha256 ` +
            `${platform.addon.sha256.slice(0, 16)}  matches checksums.json and the manifest record`,
        );
      }
    } else {
      fail(`${name}: unexpected package in a release artifact set`);
    }
    packages.push({
      tarball,
      name,
      version: pkg.manifest.version,
      platform: platform?.target?.target ?? null,
      addon: platform?.addon ? { bytes: platform.addon.size, sha256: platform.addon.sha256 } : null,
    });
    flushFailures();
    say();
  }

  if (options.requireFullMatrix) {
    const expected = [...NATIVE_TARGETS.map(nativePackageName), PUBLIC_PACKAGE];
    const packed = packages.map((entry) => entry.name);
    for (const name of expected) {
      if (!packed.includes(name)) fail(`the artifact set is missing ${name}@${version}`);
    }
    for (const name of packed) {
      if (packed.filter((candidate) => candidate === name).length > 1) {
        fail(`the artifact set contains ${name} more than once`);
      }
    }
    if (failures.length === 0) {
      say(`  matrix   all ${expected.length} publishable packages are present exactly once`);
    }
    flushFailures("  ");
    say();
  }

  if (failures.length > 0) {
    say(`[2/3] tarball install    skipped, the contents assertion already failed`);
    say(`[3/3] publish rehearsal  skipped, the contents assertion already failed`);
    return report(options, { version, artifacts, packages });
  }

  let installed = null;
  if (options.install) {
    say("[2/3] install the tarballs into a project outside the workspace and do real work");
    say();
    const host = hostPlatformPackage();
    const wanted = [PUBLIC_PACKAGE, host.name].map((name) => {
      const entry = packages.find((candidate) => candidate.name === name);
      if (!entry) {
        fail(
          `no ${name}@${version} tarball to install, so this host cannot run the artifact it would publish`,
        );
      }
      return entry ?? null;
    });
    flushFailures("  ");
    if (wanted.every(Boolean)) {
      try {
        installed = await installAndExerciseRelease({
          specs: wanted.map((entry) => entry.tarball),
          expectedVersion: version,
          // The addon this host just packed, so npm cannot quietly satisfy the
          // platform package from a copy of the same version on the registry.
          expectedAddon: wanted[1].addon,
          log: (line) => say(line),
        });
      } catch (error) {
        fail(`the packed release does not work when installed: ${error.message}`);
      }
      flushFailures("  ");
    }
    say();
  } else {
    say("[2/3] tarball install    skipped by --no-install");
    say();
  }

  say("[3/3] rehearse the publish");
  say();
  const registry = await startStagingRegistry();
  try {
    for (const entry of packages) {
      const invocation = resolveNpmInvocation([
        "publish",
        entry.tarball,
        "--dry-run",
        "--access",
        "public",
        `--registry=${registry.url}`,
      ]);
      const rehearsal = await run(invocation.executable, invocation.args, {
        env: {
          ...npmChildEnvironment(process.env),
          NPM_CONFIG_PROVENANCE: "false",
          npm_config_registry: registry.url,
        },
      });
      if (rehearsal.status !== 0) {
        fail(
          `npm publish --dry-run rejected ${basename(entry.tarball)}:\n${rehearsal.stderr || rehearsal.stdout}`,
        );
        continue;
      }
      const files = /total files:\s*(\d+)/iu.exec(rehearsal.stdout + rehearsal.stderr)?.[1] ?? "?";
      say(`  ${entry.name}@${entry.version}  npm publish --dry-run accepted ${files} files`);
    }
    if (registry.wrote()) fail("the publish rehearsal attempted a write to the registry");
    flushFailures("  ");
  } finally {
    await registry.close();
  }
  say();
  return report(options, { version, artifacts, packages, installed });
}

async function report(options, details) {
  flushFailures("  ");
  const passed = failures.length === 0;
  if (options.json) {
    await writeFile(
      resolve(root, options.json),
      `${JSON.stringify(
        {
          schemaVersion: 1,
          completedAt: new Date().toISOString(),
          host: { platform: process.platform, arch: process.arch },
          result: passed ? "pass" : "fail",
          version: details.version,
          packages: details.packages,
          installed: details.installed ?? null,
          failures,
        },
        null,
        2,
      )}\n`,
    );
  }
  say(
    passed
      ? `gate: PASS  ${details.packages.length} package${details.packages.length === 1 ? "" : "s"} ` +
          `at ${details.version} on ${process.platform}-${process.arch}`
      : `gate: FAIL  ${failures.length} problem${failures.length === 1 ? "" : "s"} in ` +
          `${details.packages.length} package${details.packages.length === 1 ? "" : "s"}`,
  );
  if (!passed) {
    say();
    say("this release must not be published:");
    for (const failure of failures) say(`  - ${failure}`);
  }
  process.exitCode = passed ? 0 : 1;
}

await main();
