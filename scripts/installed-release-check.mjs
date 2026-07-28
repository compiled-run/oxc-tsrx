import { createHash } from "node:crypto";
import { mkdtemp, readFile, realpath, rm, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { nativePackageName, nativeTargetForHost } from "../packages/toolchain/dist/native-targets.js";
import { spawnCommand } from "../packages/toolchain/dist/spawn-command.js";
import { npmChildEnvironment, resolveNpmInvocation } from "./npm-invocation.mjs";

/**
 * Install a release the way a consumer installs it, then make it do real work.
 *
 * Both halves of the release gate need exactly this: the pre-publish gate hands
 * it the tarballs that are about to be published, and the post-publish backstop
 * hands it `oxc-tsrx@<version>` resolved from the registry. Neither one asserts
 * anything about the source tree. The project is created outside the workspace,
 * the environment carries none of this repository's overrides, and the only
 * evidence accepted is a diagnostic produced by the installed binary and an AST
 * produced by the installed addon.
 *
 * 0.1.0 is the reason the addon is exercised as well as the linter. Every
 * platform package shipped without `parser.node`, and the CLI was unaffected:
 * a lint alone would have gone green on a release that was broken for every
 * `oxc-tsrx/parser` importer on every platform.
 */

const root = resolve(import.meta.dirname, "..");

export class ReleaseCheckError extends Error {
  constructor(message, { project } = {}) {
    super(message);
    this.name = "ReleaseCheckError";
    this.project = project ?? null;
  }
}

function linuxLibc() {
  if (process.platform !== "linux") return undefined;
  return process.report?.getReport?.().header?.glibcVersionRuntime ? "glibc" : "musl";
}

/** The platform package a consumer on this host is meant to resolve. */
export function hostPlatformPackage() {
  const target = nativeTargetForHost(process.platform, process.arch, linuxLibc());
  return { target, name: nativePackageName(target) };
}

function run(file, args, options = {}) {
  return new Promise((resolveRun, rejectRun) => {
    const child = spawnCommand(file, args, {
      cwd: options.cwd,
      env: options.env,
      stdio: ["ignore", "pipe", "pipe"],
    });
    let stdout = "";
    let stderr = "";
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => (stdout += chunk));
    child.stderr.on("data", (chunk) => (stderr += chunk));
    child.on("error", rejectRun);
    child.on("close", (status, signal) => resolveRun({ status, signal, stdout, stderr }));
  });
}

/**
 * An environment with no memory of this repository. Every `OXC_TSRX_*` override
 * is what the CI lanes use to point the suites at a locally built binary, so a
 * check that inherited them could pass while the installed package was empty.
 */
function consumerEnvironment(project, registry) {
  const environment = npmChildEnvironment(process.env);
  for (const key of Object.keys(environment)) {
    if (
      key === "NODE_PATH" ||
      key === "NODE_OPTIONS" ||
      key.startsWith("OXC_TSRX_") ||
      key.startsWith("OXLINT_TSGOLINT")
    ) {
      delete environment[key];
    }
  }
  environment.NO_COLOR = "1";
  environment.npm_config_cache = join(project, ".npm-cache");
  environment.npm_config_audit = "false";
  environment.npm_config_fund = "false";
  if (registry) environment.npm_config_registry = registry;
  return environment;
}

/** `node_modules/.bin/<name>` in the form this host can execute. */
function binCommand(project, binary) {
  const shim = join(project, "node_modules", ".bin", binary);
  return process.platform === "win32" ? `${shim}.cmd` : shim;
}

const LINT_FIXTURES = {
  ".oxlintrc.json": `${JSON.stringify({ rules: { "no-var": "error" } }, null, 2)}\n`,
  "View.tsrx": "export function View( ) @{var count=0;<button>{count}</button>}\n",
  "ordinary.tsx": "export var ordinary={value:1}\n",
  // Loaded through the installed package's own `oxc-tsrx/parser` export, so the
  // addon that answers is whichever one npm actually put in `node_modules`.
  "parser-probe.mjs": `const parser = await import("oxc-tsrx/parser");
const parsed = parser.parseSync(
  "Gate.tsrx",
  "const bytes = 9007199254740993n;\\nfunction View() @{ <main>{bytes}</main> }\\n",
);
const broken = parser.parseSync("Gate.tsrx", "function View() @{ <main> }\\n");
process.stdout.write(
  \`\${JSON.stringify({
    program: parsed.program.type,
    literal: String(parsed.program.body[0].declarations[0].init.value),
    errors: parsed.errors.length,
    diagnostic: broken.errors[0]?.message ?? null,
  })}\\n\`,
);
`,
};

async function sha256(path) {
  return createHash("sha256")
    .update(await readFile(path))
    .digest("hex");
}

/**
 * Install `specs` into a fresh project outside the workspace, then lint with the
 * installed linter and parse with the installed addon.
 */
export async function installAndExerciseRelease({
  specs,
  registry = null,
  expectedVersion,
  expectedAddon = null,
  log = (line) => process.stdout.write(`${line}\n`),
}) {
  if (!Array.isArray(specs) || specs.length === 0) {
    throw new TypeError("installAndExerciseRelease needs at least one install spec");
  }
  const project = await realpath(await mkdtemp(join(tmpdir(), "oxc-tsrx-release-check-")));
  const workspace = await realpath(root);
  if (project === workspace || project.startsWith(`${workspace}/`) || project.startsWith(`${workspace}\\`)) {
    throw new ReleaseCheckError(
      `the consumer project must live outside the workspace, got ${project}`,
      { project },
    );
  }
  let keep = true;
  try {
    await writeFile(
      join(project, "package.json"),
      `${JSON.stringify(
        { name: "oxc-tsrx-release-check", version: "0.0.0", private: true, type: "module" },
        null,
        2,
      )}\n`,
    );
    const environment = consumerEnvironment(project, registry);
    // Optional dependencies stay on. The public package reaches its platform
    // artifact through one, and so does the official `oxlint` this package
    // delegates plain TypeScript to: omitting them produces a tree that lints
    // TSRX and crashes on `.tsx`, which is a broken check rather than a strict
    // one. An optional dependency npm cannot resolve is skipped rather than
    // fatal, which is what makes this work for a version that is not published
    // yet.
    const install = resolveNpmInvocation([
      "install",
      "--ignore-scripts",
      "--no-audit",
      "--no-fund",
      ...specs,
    ]);
    log(`  install  ${specs.join(" ")}`);
    log(`  project  ${project}`);
    const installed = await run(install.executable, install.args, {
      cwd: project,
      env: environment,
    });
    if (installed.status !== 0) {
      throw new ReleaseCheckError(
        `npm install of the release failed (exit ${installed.status}):\n${installed.stderr || installed.stdout}`,
        { project },
      );
    }

    const toolchainRoot = join(project, "node_modules", "oxc-tsrx");
    const toolchainManifest = await readFile(join(toolchainRoot, "package.json"), "utf8").catch(
      () => null,
    );
    if (toolchainManifest === null) {
      throw new ReleaseCheckError("the install produced no node_modules/oxc-tsrx", { project });
    }
    const toolchain = JSON.parse(toolchainManifest);
    if (expectedVersion && toolchain.version !== expectedVersion) {
      throw new ReleaseCheckError(
        `installed oxc-tsrx is ${toolchain.version}, expected ${expectedVersion}`,
        { project },
      );
    }

    const host = hostPlatformPackage();
    const platformRoot = join(project, "node_modules", ...host.name.split("/"));
    const platformManifestSource = await readFile(join(platformRoot, "package.json"), "utf8").catch(
      () => null,
    );
    if (platformManifestSource === null) {
      throw new ReleaseCheckError(
        `the install produced no ${host.name}, so this host has no native artifact`,
        { project },
      );
    }
    const platform = JSON.parse(platformManifestSource);
    if (expectedVersion && platform.version !== expectedVersion) {
      throw new ReleaseCheckError(
        `installed ${host.name} is ${platform.version}, expected ${expectedVersion}`,
        { project },
      );
    }

    // The 0.1.0 defect, asserted on an installed tree rather than on a tarball:
    // npm's own file exclusion is the last thing that can drop the addon, and
    // this is the only place it can be seen.
    const addonPath = join(platformRoot, "parser.node");
    const addonStat = await stat(addonPath).catch(() => null);
    if (!addonStat?.isFile()) {
      throw new ReleaseCheckError(
        `${host.name} installed without parser.node; every oxc-tsrx/parser import would fail on ${host.target.target}`,
        { project },
      );
    }
    const checksums = JSON.parse(await readFile(join(platformRoot, "checksums.json"), "utf8"));
    const record = checksums.addons?.["parser.node"];
    if (!record) {
      throw new ReleaseCheckError(`${host.name} ships checksums.json with no parser.node record`, {
        project,
      });
    }
    const installedSha = await sha256(addonPath);
    if (record.sha256 !== installedSha || record.bytes !== addonStat.size) {
      throw new ReleaseCheckError(
        `${host.name} parser.node does not match its own checksums.json: ` +
          `${addonStat.size} bytes/${installedSha} on disk against ${record.bytes} bytes/${record.sha256} recorded`,
        { project },
      );
    }
    // The artifact under test has to be the artifact that got installed. Only
    // the caller knows which bytes it packed, and at a version the registry
    // already holds npm could otherwise satisfy the same name from there.
    if (expectedAddon && (expectedAddon.sha256 !== installedSha || expectedAddon.bytes !== addonStat.size)) {
      throw new ReleaseCheckError(
        `the installed ${host.name} is not the artifact under test: installed ${addonStat.size} bytes/` +
          `${installedSha}, packed ${expectedAddon.bytes} bytes/${expectedAddon.sha256}`,
        { project },
      );
    }
    log(
      `  installed oxc-tsrx@${toolchain.version} and ${host.name}@${platform.version} ` +
        `(parser.node ${addonStat.size} bytes, sha256 ${installedSha.slice(0, 16)})`,
    );

    for (const [name, contents] of Object.entries(LINT_FIXTURES)) {
      await writeFile(join(project, name), contents);
    }

    const lint = await run(
      binCommand(project, "oxlint"),
      ["--format=json", "View.tsrx", "ordinary.tsx"],
      { cwd: project, env: environment },
    );
    if (lint.status !== 1) {
      throw new ReleaseCheckError(
        `the installed linter exited ${lint.status} on a file that must produce a diagnostic:\n${lint.stderr || lint.stdout}`,
        { project },
      );
    }
    let lintReport;
    try {
      lintReport = JSON.parse(lint.stdout);
    } catch {
      throw new ReleaseCheckError(
        `the installed linter did not print JSON:\n${lint.stdout}\n${lint.stderr}`,
        { project },
      );
    }
    const diagnosed = new Set(
      (lintReport.diagnostics ?? []).map((diagnostic) => diagnostic.filename.replaceAll("\\", "/")),
    );
    for (const file of ["View.tsrx", "ordinary.tsx"]) {
      if (![...diagnosed].some((name) => name.endsWith(file))) {
        throw new ReleaseCheckError(
          `the installed linter reported no diagnostic for ${file}: ${[...diagnosed].join(", ") || "no diagnostics"}`,
          { project },
        );
      }
    }
    if (lintReport.oxcTsrx?.parseCount !== 1) {
      throw new ReleaseCheckError(
        `the installed linter parsed ${lintReport.oxcTsrx?.parseCount ?? "no"} TSRX files, expected 1; ` +
          "this is the shape of a lint answered by a linter that is not the TSRX one",
        { project },
      );
    }
    const rules = (lintReport.diagnostics ?? [])
      .map((diagnostic) => diagnostic.rule ?? diagnostic.code)
      .filter(Boolean);
    log(
      `  lint     ${lintReport.diagnostics.length} diagnostics across View.tsrx and ordinary.tsx ` +
        `(${[...new Set(rules)].join(", ")}), parseCount ${lintReport.oxcTsrx.parseCount}`,
    );

    const parse = await run(process.execPath, ["parser-probe.mjs"], {
      cwd: project,
      env: environment,
    });
    if (parse.status !== 0) {
      throw new ReleaseCheckError(
        `oxc-tsrx/parser failed through the installed package:\n${parse.stderr || parse.stdout}`,
        { project },
      );
    }
    const parsed = JSON.parse(parse.stdout);
    if (parsed.program !== "Program" || parsed.errors !== 0 || parsed.literal !== "9007199254740993") {
      throw new ReleaseCheckError(
        `oxc-tsrx/parser produced an unexpected AST through the installed package: ${parse.stdout}`,
        { project },
      );
    }
    if (!parsed.diagnostic) {
      throw new ReleaseCheckError(
        "oxc-tsrx/parser produced no diagnostic for a malformed view, so nothing proves the addon answered",
        { project },
      );
    }
    log(`  parser   ${parsed.program} with a BigInt literal, and a real diagnostic: ${parsed.diagnostic}`);

    keep = false;
    return {
      project,
      toolchain: { name: toolchain.name, version: toolchain.version },
      platform: {
        name: platform.name,
        version: platform.version,
        target: host.target.target,
        addon: { bytes: addonStat.size, sha256: installedSha },
      },
      lint: {
        diagnostics: lintReport.diagnostics.length,
        parseCount: lintReport.oxcTsrx.parseCount,
        rules: [...new Set(rules)],
      },
      parser: { diagnostic: parsed.diagnostic },
    };
  } finally {
    if (keep) {
      log(`  the consumer project was left in place for inspection: ${project}`);
    } else {
      // Windows keeps a native module mapped for the lifetime of the process
      // that loaded it, and the probe has exited, but an antivirus scan can
      // still hold the file briefly. A temporary directory that outlives the
      // run is not a reason to fail a release check.
      await rm(project, { recursive: true, force: true, maxRetries: 5 }).catch(() => {});
    }
  }
}
