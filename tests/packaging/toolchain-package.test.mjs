import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { cp, mkdir, readFile, readdir, realpath, rm, writeFile } from "node:fs/promises";
import { join, relative, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import test from "node:test";
import { temporaryDirectory } from "./temporary-directory.mjs";

const root = resolve(import.meta.dirname, "../..");
const packageRoot = join(root, "packages", "toolchain");

async function writePackage(directory, manifest, files) {
  await mkdir(directory, { recursive: true });
  await writeFile(join(directory, "package.json"), `${JSON.stringify(manifest, null, 2)}\n`);
  await Promise.all(
    Object.entries(files).map(async ([relativePath, source]) => {
      const path = join(directory, relativePath);
      await mkdir(resolve(path, ".."), { recursive: true });
      await writeFile(path, source);
    }),
  );
}

function runNode(file, args = [], options = {}) {
  return spawnSync(process.execPath, [file, ...args], {
    cwd: options.cwd,
    encoding: "utf8",
    env: options.env ?? process.env,
  });
}

test("the public export map is backed by this package's own implementation", async () => {
  const [{ toolchain }, parser, lint, plugins, format, compat] = await Promise.all([
    import(pathToFileURL(join(packageRoot, "dist/index.js"))),
    import(pathToFileURL(join(packageRoot, "dist/parser.js"))),
    import(pathToFileURL(join(packageRoot, "dist/lint.js"))),
    import(pathToFileURL(join(packageRoot, "dist/lint-plugins-dev.js"))),
    import(pathToFileURL(join(packageRoot, "dist/format.js"))),
    import(pathToFileURL(join(packageRoot, "dist/compat.js"))),
  ]);

  assert.deepEqual(toolchain, {
    name: "oxc-tsrx",
    language: "tsrx",
    extensions: [".tsrx"],
    capabilities: ["parser", "lint", "format", "languageServer"],
  });
  assert.equal(Object.isFrozen(toolchain), true);
  assert.equal(typeof parser.parseSync, "function");
  assert.equal(typeof parser.parse, "function");
  assert.equal(typeof parser.ParserOperationalError, "function");
  assert.equal(typeof lint.defineConfig, "function");
  assert.equal(typeof plugins.RuleTester, "function");
  assert.equal(typeof format.format, "function");
  assert.equal(typeof format.jsTextToDoc, "function");
  assert.equal(typeof format.defineConfig, "function");
  assert.equal(typeof compat.setupCompatibility, "function");
  assert.equal(typeof compat.removeCompatibility, "function");

  // The export map used to hand every capability to a separate first-party
  // package. Nothing here may re-acquire one: a user should never have to know
  // any name but `oxc-tsrx`.
  const manifest = JSON.parse(await readFile(join(packageRoot, "package.json"), "utf8"));
  const wrappers = ["@oxc-tsrx/parser", "@oxc-tsrx/runtime", "oxlint-tsrx", "oxfmt-tsrx"];
  for (const field of ["dependencies", "optionalDependencies", "peerDependencies"]) {
    for (const wrapper of wrappers) {
      assert.equal(manifest[field]?.[wrapper], undefined, `${field}.${wrapper}`);
    }
  }
  const shipped = await readdir(join(packageRoot, "dist"));
  for (const file of shipped) {
    if (!file.endsWith(".js") && !file.endsWith(".d.ts")) continue;
    const source = await readFile(join(packageRoot, "dist", file), "utf8");
    for (const wrapper of wrappers) {
      assert.doesNotMatch(
        source,
        new RegExp(`from\\s+["']${wrapper.replace("/", "\\/")}(?:/|["'])`, "u"),
        `dist/${file} must not import ${wrapper}`,
      );
    }
  }
});

/**
 * Bin keys that are entry points a host resolves by canonical tool name, or the
 * package's own general CLI. A capability target that is one of these turns a
 * discovering host into a caller of another host: an adopting linter would
 * execute a linter, discover the same provider, and recurse without bound.
 */
const GENERAL_HOST_BINS = ["oxlint", "oxfmt", "oxc-tsrx"];

/**
 * Markers that only a general host can legitimately carry. The contrast
 * assertion below requires each of them to appear in the general host bins, so
 * the leaf assertion cannot pass by matching nothing anywhere.
 */
const HOST_MARKERS = [
  { label: "provider discovery", pattern: /provider-resolve|discoverProviders|providers-report/u },
  { label: "language-server multiplexing", pattern: /multiplexer|--lsp/u },
  { label: "delegation to another host wrapper", pattern: /importDeclaredPackageBinary/u },
];

/** Everything a leaf capability executor must not do. */
const LEAF_FORBIDDEN = [
  ...HOST_MARKERS,
  { label: "file-extension dispatch", pattern: /extname|\.tsrx|endsWith\(|extensionOf/u },
  { label: "argument partitioning", pattern: /\bfilter\(|\bpartition\b|startsWith\(/u },
];

test("no capability target is a general host entry point", async () => {
  const manifest = JSON.parse(await readFile(join(packageRoot, "package.json"), "utf8"));
  const [language] = manifest.oxc.provider.languages;
  const binTargets = Object.entries(language.capabilities).filter(
    ([, target]) => typeof target.bin === "string",
  );
  assert.deepEqual(
    binTargets.map(([capability]) => capability).sort(),
    ["format", "lint", "lsp"],
    "every executable capability must be covered by this assertion",
  );

  const generalHostPaths = new Set(GENERAL_HOST_BINS.map((name) => manifest.bin[name]));
  for (const [capability, target] of binTargets) {
    assert.equal(
      GENERAL_HOST_BINS.includes(target.bin),
      false,
      `the ${capability} capability must not point at the ${target.bin} host entry point`,
    );
    assert.equal(
      generalHostPaths.has(manifest.bin[target.bin]),
      false,
      `the ${capability} capability must not resolve to a general host file`,
    );
    assert.equal(typeof manifest.bin[target.bin], "string", target.bin);
  }
});

test("every capability executor is a leaf: no discovery, no extension dispatch", async () => {
  const manifest = JSON.parse(await readFile(join(packageRoot, "package.json"), "utf8"));
  const [language] = manifest.oxc.provider.languages;

  for (const [capability, target] of Object.entries(language.capabilities)) {
    if (typeof target.bin !== "string") continue;
    const source = await readFile(join(packageRoot, manifest.bin[target.bin]), "utf8");
    assert.ok(source.length > 0, target.bin);
    for (const { label, pattern } of LEAF_FORBIDDEN) {
      assert.doesNotMatch(
        source,
        pattern,
        `the ${capability} capability executor must not perform ${label}`,
      );
    }
    // One resolution, one process, one hand-off of the argv it was given.
    assert.equal(source.match(/resolveNativeCommand\(/gu)?.length, 1, target.bin);
    assert.equal(source.match(/runPassthrough\(/gu)?.length, 1, target.bin);
    assert.match(source, /process\.argv\.slice\(2\)/u, target.bin);
  }

  // The same markers really are what separates a leaf from a host, so the
  // assertion above cannot pass by matching nothing anywhere.
  const hosts = await Promise.all(
    GENERAL_HOST_BINS.map((name) => readFile(join(packageRoot, manifest.bin[name]), "utf8")),
  );
  const combined = hosts.join("\n");
  for (const { label, pattern } of HOST_MARKERS) {
    assert.match(combined, pattern, `a general host is expected to perform ${label}`);
  }
});

test("a capability executor reports a child killed by a signal as exit 2", {
  // Windows has no POSIX signals: `process.kill(process.pid, "SIGTERM")` there
  // is an unconditional `TerminateProcess`, and the parent is told only an exit
  // status. A child cannot report a termination signal for itself, so the input
  // this assertion needs does not exist on that host, and every other host in
  // CI covers it.
  skip: process.platform === "win32"
    ? "a child cannot be killed by a signal on Windows, which has no POSIX signals"
    : false,
}, async () => {
  // The convention promises hosts that 2 means "the executor or its tool
  // broke". A child that dies from a signal has no exit status of its own, so
  // the runtime the executors share has to supply one.
  const { runPassthrough } = await import(
    pathToFileURL(join(packageRoot, "dist/process.js"))
  );
  const result = await runPassthrough(process.execPath, [
    "-e",
    "process.kill(process.pid, 'SIGTERM')",
  ]);
  assert.equal(result.signal, "SIGTERM");
  assert.equal(result.status, 2);
});

test("the capability calling convention is documented where an adopting host will look", async () => {
  const readme = await readFile(join(packageRoot, "README.md"), "utf8");
  const start = readme.indexOf("### Capability calling convention");
  assert.notEqual(start, -1, "the README must document the calling convention");
  const end = readme.indexOf("\n### ", start + 1);
  const section = end === -1 ? readme.slice(start) : readme.slice(start, end);
  for (const required of [
    "#### argv",
    "#### Output",
    "#### Exit codes",
    "oxc-tsrx-lint",
    // The honest scope label. Nothing calls lint or format through discovery.
    "no host calls `lint` or `format` through discovery",
  ]) {
    assert.ok(section.includes(required), `the convention must document ${required}`);
  }
});

/**
 * The published package is self-contained: every public export and every bin
 * resolves inside `node_modules/oxc-tsrx` plus that package's own third-party
 * dependencies. There is no first-party package under it any more, so this lane
 * installs nothing first-party and stubs only the seams a published install
 * genuinely has: the pinned Oxlint and Oxfmt packages, and the platform-native
 * artifact.
 */
test("an isolated consumer resolves every public export and bin from the package alone", async () => {
  const temporary = await temporaryDirectory("oxc-tsrx-toolchain-");
  const consumer = join(temporary, "consumer");
  const installed = join(consumer, "node_modules", "oxc-tsrx");
  const nested = join(installed, "node_modules");

  try {
    await mkdir(join(consumer, "node_modules"), { recursive: true });
    // The installed copy must arrive without `packages/toolchain/node_modules`.
    // This lane stubs the two pinned third-party packages itself a few lines
    // below, and `cp` turns pnpm's relative store symlinks into absolute ones,
    // so copying that directory would make those stub writes land in the real
    // pnpm store instead of in this fixture.
    await cp(packageRoot, installed, {
      recursive: true,
      filter: (path) => !/[\\/]node_modules([\\/]|$)/u.test(path),
    });
    // `files` excludes the parser addon built into the source tree for local
    // development, so an installed copy must not carry it either.
    for (const artifact of ["parser.node", "parser.node.json"]) {
      await rm(join(installed, artifact), { force: true });
    }
    await writeFile(
      join(consumer, "package.json"),
      `${JSON.stringify({
        name: "clean-oxc-tsrx-consumer",
        private: true,
        type: "module",
        devDependencies: { "oxc-tsrx": "0.1.3" },
      }, null, 2)}\n`,
    );

    // The stub stands in for the native tool. Two environment switches let the
    // calling-convention assertions below drive the two branches the convention
    // describes: a tool that ran and returned a status, and a native package
    // that could not be resolved at all. Replacing the module rather than a
    // package is the seam the merged package actually has.
    await writeFile(
      join(installed, "dist", "runtime.js"),
      [
        "const SUBCOMMANDS = { lint: [], format: ['fmt'], server: ['lsp'] };",
        "export function resolveNativeCommand(kind, args = []) {",
        "  if (process.env.STUB_RESOLVE_FAILURE) {",
        "    throw new Error(`stub native package for ${kind} is unavailable`);",
        "  }",
        "  return { executable: `nested:${kind}`, args: [...SUBCOMMANDS[kind], ...args] };",
        "}",
        "export async function runPassthrough(executable, args) {",
        '  process.stdout.write(JSON.stringify({ tool: "passthrough", executable, args }));',
        "  return { status: Number(process.env.STUB_STATUS ?? 0) };",
        "}",
        "export async function runCaptured(executable, args) {",
        "  return { status: 0, stdout: '', stderr: '', signal: null };",
        "}",
        "",
      ].join("\n"),
    );

    // The two third-party packages this one pins by npm alias. Ordinary files
    // are still their work, and the canonical command names still enter their
    // own declared launchers in process.
    await writePackage(
      join(nested, "oxlint-current"),
      {
        name: "oxlint",
        version: "1.74.0",
        type: "module",
        bin: { oxlint: "./bin/oxlint" },
        exports: {
          ".": "./index.js",
          "./plugins-dev": "./plugins-dev.js",
          "./package.json": "./package.json",
        },
      },
      {
        "index.js": "export function defineConfig(config) { return config; }\n",
        "plugins-dev.js":
          'export const pluginMarker = "nested-plugin";\nexport class RuleTester {}\n',
        "bin/oxlint":
          'process.stdout.write(JSON.stringify({ tool: "oxlint", args: process.argv.slice(2) }));\n',
      },
    );
    await writePackage(
      join(nested, "oxfmt-current"),
      {
        name: "oxfmt",
        version: "0.59.0",
        type: "module",
        bin: { oxfmt: "./bin/oxfmt" },
        exports: { ".": "./index.js", "./package.json": "./package.json" },
      },
      {
        "index.js": [
          "export function defineConfig(config) { return config; }",
          "export async function format() { return { code: '', errors: [] }; }",
          "export async function jsTextToDoc() { return { code: '', errors: [] }; }",
          "",
        ].join("\n"),
        "bin/oxfmt":
          'process.stdout.write(JSON.stringify({ tool: "oxfmt", args: process.argv.slice(2) }));\n',
      },
    );

    const probe = join(consumer, "probe.mjs");
    await writeFile(
      probe,
      [
        'import { toolchain } from "oxc-tsrx";',
        'import { parseSync } from "oxc-tsrx/parser";',
        'import { defineConfig } from "oxc-tsrx/lint";',
        'import { pluginMarker } from "oxc-tsrx/lint/plugins-dev";',
        'import { format } from "oxc-tsrx/format";',
        'import { setupCompatibility } from "oxc-tsrx/compat";',
        'import { discoverProviders } from "oxc-tsrx/provider-resolve";',
        "process.stdout.write(JSON.stringify({",
        "  toolchain,",
        "  parserMarker: typeof parseSync,",
        "  lintMarker: typeof defineConfig,",
        "  pluginMarker,",
        "  formatMarker: typeof format,",
        "  compatMarker: typeof setupCompatibility,",
        "  providerMarker: typeof discoverProviders,",
        "}));",
        "",
      ].join("\n"),
    );

    const imported = runNode(probe, [], { cwd: consumer });
    assert.equal(imported.status, 0, imported.stderr);
    assert.deepEqual(JSON.parse(imported.stdout), {
      toolchain: {
        name: "oxc-tsrx",
        language: "tsrx",
        extensions: [".tsrx"],
        capabilities: ["parser", "lint", "format", "languageServer"],
      },
      parserMarker: "function",
      lintMarker: "function",
      pluginMarker: "nested-plugin",
      formatMarker: "function",
      compatMarker: "function",
      providerMarker: "function",
    });

    // The canonical command names still hand an ordinary-only invocation to the
    // pinned package's own declared launcher, in this process.
    await writeFile(join(consumer, "ordinary.tsx"), "export const value = 1;\n");
    for (const [binary, args, expected] of [
      ["oxlint", ["ordinary.tsx", "--deny-warnings"], {
        tool: "oxlint",
        args: ["ordinary.tsx", "--deny-warnings"],
      }],
      ["oxfmt", ["ordinary.tsx", "--check"], {
        tool: "oxfmt",
        args: ["ordinary.tsx", "--check"],
      }],
      ["oxc-tsrx-lsp", ["--stdio"], {
        tool: "passthrough",
        executable: "nested:server",
        args: ["lsp", "--stdio"],
      }],
      // The leaf capability executors hand their argv to the native binary
      // untouched: no partitioning, no discovery, no second host. The one
      // native binary carries all three tools, so a leading subcommand selects
      // which one runs. It is a tool selector, not an argv rewrite: everything
      // the host passed follows it in order. Linting needs no selector.
      ["oxc-tsrx-lint", ["src/View.tsrx", "--deny-warnings"], {
        tool: "passthrough",
        executable: "nested:lint",
        args: ["src/View.tsrx", "--deny-warnings"],
      }],
      ["oxc-tsrx-fmt", ["src/View.tsrx", "--check"], {
        tool: "passthrough",
        executable: "nested:format",
        args: ["fmt", "src/View.tsrx", "--check"],
      }],
    ]) {
      const result = runNode(join(installed, "bin", binary), args, { cwd: consumer });
      assert.equal(result.status, 0, result.stderr);
      assert.deepEqual(JSON.parse(result.stdout), expected);
    }

    // --- The capability calling convention -------------------------------
    // Documented in packages/toolchain/README.md, "Capability calling
    // convention". No host calls lint or format through discovery yet, so
    // these assertions are what an adopting host would be able to rely on.
    const executors = [
      ["oxc-tsrx-lint", "lint", []],
      ["oxc-tsrx-fmt", "format", ["fmt"]],
    ];

    // argv: whatever a host passes is what the native tool parses. Awkward
    // paths, values with spaces, and option order all survive untouched.
    const hostArgv = [
      "--config",
      "config dir/.oxlintrc.json",
      join(consumer, "src", "A B.tsrx"),
      "src/View.tsrx",
    ];
    for (const [binary, kind, subcommand] of executors) {
      const result = runNode(join(installed, "bin", binary), hostArgv, { cwd: consumer });
      assert.equal(result.status, 0, result.stderr);
      assert.deepEqual(JSON.parse(result.stdout), {
        tool: "passthrough",
        executable: `nested:${kind}`,
        args: [...subcommand, ...hostArgv],
      });
    }

    // exit codes: 0 is a clean run, 1 is findings, 2 is breakage, and any
    // other code the native tool produces reaches the host unchanged.
    for (const [binary] of executors) {
      for (const status of ["0", "1", "2", "87"]) {
        const result = runNode(join(installed, "bin", binary), ["src/View.tsrx"], {
          cwd: consumer,
          env: { ...process.env, STUB_STATUS: status },
        });
        assert.equal(result.status, Number(status), `${binary} must report ${status}`);
      }
    }

    // A broken executor is distinguishable from findings: exit 2, one stderr
    // line naming the executor, and no stdout for a host to misparse.
    for (const [binary] of executors) {
      const result = runNode(join(installed, "bin", binary), ["src/View.tsrx"], {
        cwd: consumer,
        env: { ...process.env, STUB_RESOLVE_FAILURE: "1" },
      });
      assert.equal(result.status, 2, `${binary} must report an executor failure as 2`);
      assert.equal(result.stdout, "", `${binary} must write nothing to stdout when it breaks`);
      assert.match(result.stderr, new RegExp(`^${binary}: `, "u"));
      assert.equal(result.stderr.trimEnd().split("\n").length, 1, result.stderr);
    }

    const negativeProbe = join(consumer, "negative-probe.mjs");
    await writeFile(negativeProbe, "await import(process.argv[2]);\n");
    // The folded wrapper names must stay gone, and folding them in must not
    // have turned this package's internals into a second public surface: the
    // export map is still the whole of what a consumer can reach.
    for (const implementation of [
      "@oxc-tsrx/parser",
      "@oxc-tsrx/runtime",
      "oxlint-tsrx",
      "oxfmt-tsrx",
      "oxc-tsrx/dist/runtime.js",
      "oxc-tsrx/dist/lint-cli.js",
      "oxc-tsrx/dist/format-cli.js",
      "oxc-tsrx/dist/index.js",
      "oxc-tsrx/runtime",
    ]) {
      const result = runNode(negativeProbe, [implementation], { cwd: consumer });
      assert.notEqual(
        result.status,
        0,
        `${implementation} must not be importable from the consumer root`,
      );
    }

    const manifest = JSON.parse(await readFile(join(installed, "package.json"), "utf8"));
    assert.equal(manifest.scripts, undefined);
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
});

/**
 * `oxlint` and `oxfmt` are canonical command names this package does not own.
 * It publishes them because a plain install has no other way to reach a released
 * host, and it hands them back the moment a project says what it means by them.
 * The end-to-end proof is `released-host-install.test.mjs`; these are the branch
 * assertions that do not need a registry.
 */
test("the canonical command names arbitrate from the project manifest alone", async () => {
  const { decideCanonicalCommand, deferralNotice, providedArguments } = await import(
    pathToFileURL(join(packageRoot, "dist/canonical-command.js"))
  );
  const temporary = await temporaryDirectory("oxc-tsrx-canonical-");

  const project = async (name, dependencies, modules = {}) => {
    const directory = join(temporary, name);
    await mkdir(directory, { recursive: true });
    await writeFile(
      join(directory, "package.json"),
      `${JSON.stringify({ name, private: true, dependencies }, null, 2)}\n`,
    );
    for (const [packageName, { manifest, files = {} }] of Object.entries(modules)) {
      await writePackage(join(directory, "node_modules", packageName), manifest, files);
    }
    return directory;
  };

  const officialOxlint = {
    manifest: { name: "oxlint", version: "1.72.0", bin: { oxlint: "./bin/oxlint" } },
    files: { "bin/oxlint": "#!/usr/bin/env node\n" },
  };

  try {
    // Nothing declared: the launcher keeps the name, which is the only reason a
    // plain install reaches a released host at all.
    const plain = await project("plain", { "oxc-tsrx": "0.1.3" });
    assert.deepEqual(await decideCanonicalCommand("oxlint", { cwd: plain }), {
      command: "oxlint",
      owner: "oxc-tsrx",
      reason: "not-directly-declared",
      projectRoot: plain,
    });

    // A transitive official package is not a statement about the command name,
    // so an installed-but-undeclared `oxlint` changes nothing. This is the case
    // every Vite+ project is in.
    const transitive = await project("transitive", { "oxc-tsrx": "0.1.3" }, { oxlint: officialOxlint });
    assert.equal((await decideCanonicalCommand("oxlint", { cwd: transitive })).owner, "oxc-tsrx");

    // A direct declaration is such a statement, and it wins outright.
    const pinned = await project(
      "pinned",
      { "oxc-tsrx": "0.1.3", oxlint: "1.72.0" },
      { oxlint: officialOxlint },
    );
    const deferred = await decideCanonicalCommand("oxlint", { cwd: pinned });
    assert.equal(deferred.owner, "project");
    assert.equal(deferred.reason, "declared-in-dependencies");
    assert.equal(deferred.officialVersion, "1.72.0");
    // `path.relative` is the comparison rather than `===` because Windows
    // resolves the same file to different spellings — `C:\Users\RUNNER~1` and
    // `C:\Users\runneradmin`, `C:` and `c:` — and compares them case
    // insensitively, while POSIX keeps the exact string it was given.
    assert.equal(
      relative(deferred.binPath, await realpath(join(pinned, "node_modules/oxlint/bin/oxlint"))),
      "",
      deferred.binPath,
    );

    // devDependencies say it just as clearly, and the decision is made from the
    // nearest project root, so a nested directory inherits it.
    const development = await project(
      "development",
      { "oxc-tsrx": "0.1.3" },
      { oxlint: officialOxlint },
    );
    const developmentManifest = JSON.parse(
      await readFile(join(development, "package.json"), "utf8"),
    );
    developmentManifest.devDependencies = { oxlint: "1.72.0" };
    await writeFile(
      join(development, "package.json"),
      `${JSON.stringify(developmentManifest, null, 2)}\n`,
    );
    await mkdir(join(development, "src/deep"), { recursive: true });
    const nested = await decideCanonicalCommand("oxlint", { cwd: join(development, "src/deep") });
    assert.equal(nested.owner, "project");
    assert.equal(nested.reason, "declared-in-devDependencies");

    // The compatibility bridge writes a package named `oxlint` into that slot.
    // Deferring to it would re-enter this launcher without bound, so it does not.
    const bridged = await project(
      "bridged",
      { "oxc-tsrx": "0.1.3", oxlint: "1.72.0" },
      {
        oxlint: {
          manifest: {
            name: "oxlint",
            version: "0.1.3",
            bin: { oxlint: "./bin/oxlint" },
            oxcTsrxCompatibility: {
              schemaVersion: 1,
              provider: "oxc-tsrx",
              providerVersion: "0.1.3",
              capability: "lint",
            },
          },
          files: { "bin/oxlint": "#!/usr/bin/env node\n" },
        },
      },
    );
    const facade = await decideCanonicalCommand("oxlint", { cwd: bridged });
    assert.equal(facade.owner, "oxc-tsrx");
    assert.equal(facade.reason, "compatibility-facade");

    // Genuinely ambiguous: the project named a package that is not there. There
    // is no safe guess, so it refuses instead of quietly linting with the wrong
    // tool.
    const missing = await project("missing", { "oxc-tsrx": "0.1.3", oxfmt: "0.44.0" });
    await assert.rejects(
      () => decideCanonicalCommand("oxfmt", { cwd: missing }),
      /declares the official oxfmt package in dependencies.*not installed/su,
    );

    // The one line a deferring run may print, and only when the caller actually
    // asked about a file this package's provider block claims.
    assert.deepEqual(providedArguments(["--fix", "a.ts", "b.tsrx", "--config=x.tsrx"]), ["b.tsrx"]);
    assert.equal(deferralNotice(deferred, ["src/app.ts"]), null);
    assert.equal(deferralNotice(facade, ["src/View.tsrx"]), null);
    const notice = deferralNotice(deferred, ["src/View.tsrx", "src/app.ts"]);
    assert.match(notice, /official oxlint 1\.72\.0/u);
    assert.match(notice, /src\/View\.tsrx/u);
    assert.match(notice, /npx oxc-tsrx-lint/u);
    assert.equal(notice.includes("\n"), false, "the notice must be one line");
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
});

/**
 * Handing the command name back is only kept if the official binary can actually
 * be executed, and "executed" is not the same operation on every host this
 * package publishes for.
 *
 * These assertions run identically everywhere on purpose. The Windows shapes —
 * a `.cmd` launcher, a drive-lettered path that is not a valid import
 * specifier, a byte-order mark in front of a shebang — are reachable from any
 * host because the platform is a parameter, and a lane that could only run on
 * Windows would be a lane that never runs. What they cannot do is stand in for
 * a Windows host: the `install-arbitration` CI job is what observes that.
 */
test("the official binary is executed the way each host requires", async (context) => {
  const { escapeCommandArgument, resolveCommandInvocation } = await import(
    pathToFileURL(join(packageRoot, "dist/spawn-command.js"))
  );
  const { runOfficialCommand, usesNodeInterpreter } = await import(
    pathToFileURL(join(packageRoot, "dist/canonical-command.js"))
  );
  const temporary = await temporaryDirectory("oxc-tsrx-official-run-");
  context.after(() => rm(temporary, { recursive: true, force: true }));

  // Windows cannot execute a `.cmd` launcher directly, and `npm` writes exactly
  // that into `node_modules/.bin`. It goes to the command interpreter, verbatim,
  // with every argument escaped for `cmd.exe` rather than concatenated the way
  // `shell: true` would.
  const batch = resolveCommandInvocation(
    "C:\\Program Files\\app\\node_modules\\.bin\\oxlint.cmd",
    ["--format=json", "a b.tsrx", "x&whoami"],
    "win32",
  );
  assert.match(batch.file, /cmd(?:\.exe)?$/iu);
  assert.deepEqual(batch.args.slice(0, 3), ["/d", "/s", "/c"]);
  assert.equal(batch.windowsVerbatimArguments, true);
  assert.equal(batch.args.length, 4);
  assert.match(batch.args[3], /^"/u);
  assert.match(batch.args[3], /oxlint\.cmd/u);
  assert.equal(
    batch.args[3].includes("&whoami") && !batch.args[3].includes("^&whoami"),
    false,
    "an unescaped & would let an argument start a second command",
  );
  assert.equal(escapeCommandArgument("a b.tsrx"), '^"a^ b.tsrx^"');
  assert.equal(escapeCommandArgument("x&whoami"), '^"x^&whoami^"');
  assert.equal(escapeCommandArgument('say "hi"'), '^"say^ \\^"hi\\^"^"');
  assert.equal(escapeCommandArgument("C:\\dir\\"), '^"C:\\dir\\\\^"');

  // Everything else is spawned as itself, on every host. A `.cmd` name on a
  // POSIX host is an ordinary file and must not be routed through an
  // interpreter that is not there.
  for (const [file, platform] of [
    ["C:\\app\\oxlint.exe", "win32"],
    ["C:\\app\\node_modules\\oxlint\\bin\\oxlint", "win32"],
    ["/app/node_modules/.bin/oxlint.cmd", "linux"],
    ["/app/node_modules/oxlint/bin/oxlint", "darwin"],
  ]) {
    const invocation = resolveCommandInvocation(file, ["--version"], platform);
    assert.deepEqual(invocation, {
      file,
      args: ["--version"],
      windowsVerbatimArguments: false,
    });
  }

  // A byte-order mark in front of a shebang is ordinary in a file authored on
  // Windows. Reading past it is what keeps a Node wrapper on the in-process
  // path; classifying it as a native executable would spawn an extensionless
  // file, which Windows cannot run at all.
  const wrapper = join(temporary, "bom-wrapper");
  await writeFile(wrapper, '\uFEFF#!/usr/bin/env node\nprocess.exitCode = 0;\n');
  assert.equal(await usesNodeInterpreter(wrapper), true, "a BOM must not hide the shebang");
  const native = join(temporary, "native-binary");
  await writeFile(native, Buffer.from([0x4d, 0x5a, 0x90, 0x00, 0x03]));
  assert.equal(await usesNodeInterpreter(native), false);

  // The in-process branch imports through a file URL. That is not cosmetic: a
  // path is not a module specifier, and the difference shows on any host as
  // soon as the path contains a character a URL reads as syntax. On Windows
  // every path does, because `C:` parses as a scheme.
  const awkward = join(temporary, "a b#c");
  await mkdir(join(awkward, "bin"), { recursive: true });
  await writeFile(join(awkward, "package.json"), '{ "name": "awkward", "type": "module" }\n');
  const binPath = join(awkward, "bin", "oxlint");
  await writeFile(
    binPath,
    [
      "#!/usr/bin/env node",
      'import { writeFileSync } from "node:fs";',
      'writeFileSync(new URL("./ran.marker", import.meta.url), "ran");',
      "",
    ].join("\n"),
  );
  await assert.rejects(
    () => import(binPath),
    "a bare path is not a module specifier; this is the failure pathToFileURL prevents",
  );
  await runOfficialCommand(
    { command: "oxlint", binPath, officialRoot: awkward },
    {
      spawn: () => {
        throw new Error("a Node wrapper must run in this process, not as a child");
      },
    },
  );
  assert.equal(await readFile(join(awkward, "bin", "ran.marker"), "utf8"), "ran");

  // A declared binary that cannot start is the launcher's error to report. Left
  // unhandled it would be an `error` event on the child, which surfaces as a
  // stack trace out of node:child_process instead of one actionable line.
  const unrunnable = join(temporary, "not-runnable");
  await writeFile(unrunnable, "this is not an executable and has no shebang\n");
  await assert.rejects(
    () => runOfficialCommand({ command: "oxlint", binPath: unrunnable, officialRoot: temporary }),
    /could not execute .*not-runnable.*oxlint binary/su,
  );
});
