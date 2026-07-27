import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import {
  access,
  cp,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  realpath,
  rm,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { delimiter, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { runTests } from "@vscode/test-electron";
import { parseNpmPackResponse } from "../../scripts/npm-pack-response.mjs";
import { startLocalRegistry } from "../packaging/local-registry.mjs";

/**
 * Two real VS Code sessions run against the released `oxc.oxc-vscode` build.
 *
 * 1. The compatibility session: `oxc-tsrx setup` runs, the official extension
 *    finds `node_modules/.bin/oxlint`, and TSRX is served because this package
 *    owns the canonical `oxlint` bin name. That is how adoption works today and
 *    its assertions are unchanged.
 * 2. The install-only discovery session: nothing but `npm install` runs, the
 *    whole of `node_modules/.bin` is deleted, the facades `setup` writes are
 *    absent, and every tool name is shadowed first on `PATH` by a decoy that
 *    records being executed. The official extension is pointed at the general
 *    Oxlint host with an absolute path inside the installed package, because no
 *    released OXC build discovers providers yet — that pointer names a host, not
 *    a language, an extension, or a server. Everything the session then proves
 *    happens strictly below it: the provider block in the installed package's
 *    own `package.json` is discovered, and the `lsp` bin it declares is started
 *    as a real process that answers real editor requests.
 * 3. The patched-host session, which only runs when
 *    `OXC_TSRX_PATCHED_OXLINT_PACKAGE` points at a locally built upstream Oxlint
 *    npm wrapper carrying the provider-dispatch patch. It is the same
 *    install-only workspace with **no `oxc.path.oxlint` setting at all**: the
 *    released extension resolves the literal `oxlint` package by ordinary Node
 *    resolution, and that package — upstream's, not this repository's — is what
 *    reads the provider block and starts the declared server. The patch is
 *    built and verified locally. It has never been submitted, merged, or
 *    released, and this lane must never be described as evidence that it was.
 */

const root = resolve(import.meta.dirname, "../..");
const npm = process.platform === "win32" ? "npm.cmd" : "npm";
const executable =
  process.env.VSCODE_EXECUTABLE_PATH ??
  "/Applications/Visual Studio Code.app/Contents/MacOS/Electron";

function hostTarget() {
  if (process.platform === "darwin") {
    return `${process.arch === "arm64" ? "aarch64" : "x86_64"}-apple-darwin`;
  }
  if (process.platform === "win32") {
    return `${process.arch === "arm64" ? "aarch64" : "x86_64"}-pc-windows-msvc`;
  }
  if (process.platform === "linux" && ["arm64", "x64"].includes(process.arch)) {
    const architecture = process.arch === "arm64" ? "aarch64" : "x86_64";
    const libc = process.report?.getReport?.().header?.glibcVersionRuntime ? "gnu" : "musl";
    return `${architecture}-unknown-linux-${libc}`;
  }
  throw new Error(`unsupported official-extension host ${process.platform}-${process.arch}`);
}

function run(executablePath, args, options = {}) {
  return new Promise((resolveRun, rejectRun) => {
    const child = spawn(executablePath, args, {
      cwd: options.cwd,
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
    child.on("close", (status, signal) => {
      resolveRun({ status, signal, stdout, stderr });
    });
  });
}

async function mustRun(executablePath, args, options = {}) {
  const result = await run(executablePath, args, options);
  assert.equal(result.status, 0, result.stderr || result.stdout);
  return result;
}

async function pack(packageRoot, artifacts, cache) {
  const result = await mustRun(
    npm,
    ["pack", "--json", "--pack-destination", artifacts, resolve(root, packageRoot)],
    { cwd: root, env: { ...process.env, npm_config_cache: cache } },
  );
  const packed = parseNpmPackResponse(result.stdout);
  return {
    ...packed,
    manifest: JSON.parse(await readFile(join(root, packageRoot, "package.json"), "utf8")),
    tarball: join(artifacts, packed.filename),
  };
}

async function resolveOfficialExtension() {
  if (process.env.OXC_OFFICIAL_VSCODE_EXTENSION) {
    const configured = resolve(process.env.OXC_OFFICIAL_VSCODE_EXTENSION);
    await access(join(configured, "package.json"));
    return configured;
  }
  const directory = join(process.env.HOME, ".vscode", "extensions");
  const candidates = (await readdir(directory))
    .filter((name) => name.startsWith("oxc.oxc-vscode-"))
    .sort((left, right) => right.localeCompare(left, undefined, { numeric: true }));
  assert.ok(
    candidates.length > 0,
    "Install the released OXC extension (oxc.oxc-vscode) before running this proof",
  );
  return join(directory, candidates[0]);
}

function cleanEnvironment(consumer, registry, extra = {}) {
  const environment = {
    ...process.env,
    NO_COLOR: "1",
    npm_config_cache: join(consumer, ".npm-cache"),
    npm_config_registry: registry,
  };
  for (const key of Object.keys(environment)) {
    if (
      key === "NODE_PATH" ||
      key.startsWith("OXC_TSRX_") ||
      key.startsWith("OXLINT_TSGOLINT")
    ) {
      delete environment[key];
    }
  }
  return { ...environment, ...extra };
}

async function assertMissing(path, message) {
  let present = true;
  try {
    await access(path);
  } catch {
    present = false;
  }
  assert.equal(present, false, message);
}

/** The workspace both sessions author, byte for byte. */
async function writeWorkspaceFixtures(directory, settings) {
  const ordinaryPath = join(directory, "ordinary.ts");
  const tsrxPath = join(directory, "View.tsrx");
  await mkdir(join(directory, ".vscode"), { recursive: true });
  await Promise.all([
    writeFile(
      join(directory, ".oxlintrc.json"),
      `${JSON.stringify(
        { rules: { "no-debugger": "error", "no-var": "error" } },
        null,
        2,
      )}\n`,
    ),
    writeFile(
      join(directory, ".oxfmtrc.json"),
      `${JSON.stringify({ semi: true, singleQuote: true }, null, 2)}\n`,
    ),
    writeFile(
      join(directory, ".vscode/settings.json"),
      `${JSON.stringify(settings, null, 2)}\n`,
    ),
    writeFile(
      ordinaryPath,
      "export function ordinary() {\n  debugger;\n  return 1;\n}\n",
    ),
    writeFile(
      tsrxPath,
      [
        "export function View( ) @{",
        "var count=0;",
        "debugger;",
        "<button>{count}</button>",
        "}",
        "",
      ].join("\n"),
    ),
  ]);
  return { ordinaryPath, tsrxPath };
}

/**
 * Shadow every tool name this toolchain publishes with a script that records
 * being run and fails. Placed first on `PATH`, it turns "no PATH lookup" into a
 * falsifiable claim: a lookup by tool name leaves a file behind.
 */
const DECOY_TOOL_NAMES = Object.freeze([
  "oxc-tsrx",
  "oxc-tsrx-fmt",
  "oxc-tsrx-lint",
  "oxc-tsrx-lsp",
  "oxfmt",
  "oxfmt-tsrx",
  "oxlint",
  "oxlint-tsrx",
]);
const DECOY_STATUS = 87;

async function writePathDecoys(directory, marker) {
  await mkdir(directory, { recursive: true });
  await Promise.all(
    DECOY_TOOL_NAMES.map((name) =>
      writeFile(
        join(directory, name),
        [
          "#!/bin/sh",
          `printf '%s %s\\n' "$0" "$*" >> '${marker}'`,
          `exit ${DECOY_STATUS}`,
          "",
        ].join("\n"),
        { mode: 0o755 },
      ),
    ),
  );
}

/**
 * The install-only session again, with the pointer removed and a **patched
 * upstream Oxlint** in the place an ordinary `npm install oxlint` would put it.
 *
 * Session 2 has to name the host with `oxc.path.oxlint` because no released OXC
 * build can locate a provider host on its own. This session removes that
 * setting entirely. The released extension's own resolution chain
 * (`node_modules/.bin` — deleted, workspace `package.json` scan, then
 * `require.resolve("oxlint")`) is what finds the host, and the host is
 * upstream's wrapper carrying the locally built provider-dispatch patch.
 *
 * The patch is a local source build. It is not published, not submitted, and
 * not merged, so this lane is opt-in: without
 * `OXC_TSRX_PATCHED_OXLINT_PACKAGE` it reports that it was skipped and the two
 * released-software sessions above stand on their own.
 */
async function runPatchedHostSession({
  root: temporary,
  registry,
  executable,
  officialExtension,
  decoys,
  decoyMarker,
  search,
}) {
  const patchedPackage = process.env.OXC_TSRX_PATCHED_OXLINT_PACKAGE;
  if (!patchedPackage) {
    process.stdout.write(
      "[patched-host] SKIP set OXC_TSRX_PATCHED_OXLINT_PACKAGE to a locally built patched upstream oxlint package\n",
    );
    return;
  }
  const patchedRoot = resolve(patchedPackage);
  const patchedManifest = JSON.parse(
    await readFile(join(patchedRoot, "package.json"), "utf8"),
  );
  assert.equal(
    patchedManifest.name,
    "oxlint",
    "OXC_TSRX_PATCHED_OXLINT_PACKAGE must point at a package named oxlint",
  );
  assert.equal(
    patchedManifest.oxc?.provider,
    undefined,
    "the host must be a host, not a provider",
  );

  const patched = join(temporary, "patched-host");
  await mkdir(patched, { recursive: true });
  const environment = cleanEnvironment(patched, registry.url);

  await writeFile(
    join(patched, "package.json"),
    `${JSON.stringify(
      {
        name: "oxc-tsrx-patched-host-discovery-proof",
        private: true,
        type: "module",
        dependencies: { "oxc-tsrx": "0.1.1" },
      },
      null,
      2,
    )}\n`,
  );
  await mustRun(
    npm,
    ["install", "--ignore-scripts", "--no-audit", "--no-fund"],
    { cwd: patched, env: environment },
  );

  // Same install-only conditions as session 2: no `.bin`, no `setup`.
  await rm(join(patched, "node_modules/.bin"), { recursive: true, force: true });
  await assertMissing(
    join(patched, "node_modules/.bin"),
    "node_modules/.bin survived in the patched-host workspace",
  );
  for (const facade of ["oxfmt", "oxc-parser"]) {
    await assertMissing(
      join(patched, "node_modules", facade),
      `${facade} exists without oxc-tsrx setup`,
    );
  }

  // The patched wrapper is placed exactly where `npm install oxlint` would put
  // it. It is a local source build of upstream, so it cannot come from the
  // local registry; the copy is the only thing this session does by hand, and
  // it is a *host*, carrying no TSRX knowledge of any kind.
  const hostRoot = join(patched, "node_modules/oxlint");
  await cp(patchedRoot, hostRoot, { recursive: true, dereference: true });
  const hostBin = join(hostRoot, "bin/oxlint");
  await access(hostBin);

  const fixtures = await writeWorkspaceFixtures(patched, {
    "oxc.enable.oxlint": true,
    "oxc.enable.oxfmt": false,
    "oxc.requireConfig": false,
    // No `oxc.path.oxlint`, and no `oxc.path.*` of any kind. `useExecPath` is
    // kept for the same reason session 2 keeps it: it stops the extension
    // rebuilding the child `PATH`, which is what makes the decoy contrast
    // falsifiable rather than decorative.
    "oxc.useExecPath": true,
  });

  const manifestBefore = await readFile(join(patched, "package.json"), "utf8");
  const lockfileBefore = await readFile(join(patched, "package-lock.json"), "utf8");

  await runTests({
    vscodeExecutablePath: executable,
    reuseMachineInstall: false,
    extensionDevelopmentPath: officialExtension,
    extensionTestsPath: join(root, "tests/editor/official-oxc-toolchain-suite.cjs"),
    extensionTestsEnv: cleanEnvironment(patched, registry.url, {
      PATH: search,
      SHELL: join(temporary, "absent-login-shell"),
      OXC_TSRX_SUITE_MODE: "patched-host",
      OXC_TSRX_DISCOVERY_ROOT: patched,
      OXC_TSRX_PATH_DECOY_DIR: decoys,
      OXC_TSRX_PATH_DECOY_MARKER: decoyMarker,
      OXC_TSRX_EDITOR_FILE: fixtures.tsrxPath,
      OXC_TSRX_ORDINARY_EDITOR_FILE: fixtures.ordinaryPath,
      OXC_TSRX_EXPECTED_EXTENSION_PATH: officialExtension,
      OXC_TSRX_EXPECTED_HOST_BIN: hostBin,
    }),
    launchArgs: [
      patched,
      `--extensions-dir=${join(temporary, "patched-host-extensions")}`,
      `--user-data-dir=${join(temporary, "patched-host-user")}`,
      "--disable-extensions",
      "--disable-workspace-trust",
      "--skip-welcome",
      "--skip-release-notes",
    ],
  });

  await assertMissing(
    decoyMarker,
    "a tool name was resolved from PATH during the patched-host session",
  );
  await assertMissing(
    join(patched, "node_modules/.bin"),
    "node_modules/.bin was recreated during the patched-host session",
  );
  assert.equal(await readFile(join(patched, "package.json"), "utf8"), manifestBefore);
  assert.equal(await readFile(join(patched, "package-lock.json"), "utf8"), lockfileBefore);
}

/**
 * Drive the real VS Code sessions.
 *
 * Everything above is exported harness: `tests/editor/vscode-run.mjs` reuses it
 * to build the same install-only workspace for this repository's own client, so
 * the packing, local registry, decoy, and fixture machinery has one definition.
 */
async function main() {
  const officialExtension = await resolveOfficialExtension();
  const temporary = await mkdtemp(join(tmpdir(), "otx-"));
  const artifacts = join(temporary, "artifacts");
  const consumer = join(temporary, "consumer");
  const discovery = join(temporary, "discovery");
  const decoys = join(temporary, "path-decoys");
  const decoyMarker = join(temporary, "path-decoy-invocations.log");
  const cache = join(temporary, ".pack-cache");
  const extensionDirectory = join(temporary, "extensions");
  const userDirectory = join(temporary, "user");
  await Promise.all([
    mkdir(artifacts, { recursive: true }),
    mkdir(consumer, { recursive: true }),
    mkdir(discovery, { recursive: true }),
    mkdir(extensionDirectory, { recursive: true }),
    mkdir(join(consumer, ".vscode"), { recursive: true }),
  ]);

  let registry;
  try {
    const nativeResult = await mustRun(
      process.execPath,
      [
        "scripts/package-native.mjs",
        "--target",
        hostTarget(),
        "--bin-dir",
        "target/release",
        "--out-dir",
        artifacts,
      ],
      { cwd: root, env: { ...process.env, npm_config_cache: cache } },
    );
    const native = JSON.parse(nativeResult.stdout);
    const packages = await Promise.all([pack("packages/toolchain", artifacts, cache)]);
    registry = await startLocalRegistry([
      ...packages,
      {
        manifest: { name: native.packageName, version: native.version },
        tarball: native.tarball,
        integrity: native.integrity,
        shasum: native.shasum,
      },
    ]);
    const environment = cleanEnvironment(consumer, registry.url);

    await writeFile(
      join(consumer, "package.json"),
      `${JSON.stringify(
        {
          name: "oxc-tsrx-official-extension-proof",
          private: true,
          type: "module",
          dependencies: { "oxc-tsrx": "0.1.1" },
        },
        null,
        2,
      )}\n`,
    );
    await mustRun(
      npm,
      ["install", "--ignore-scripts", "--no-audit", "--no-fund"],
      { cwd: consumer, env: environment },
    );
    await mustRun(
      process.execPath,
      [join(consumer, "node_modules/oxc-tsrx/bin/oxc-tsrx"), "setup"],
      { cwd: consumer, env: environment },
    );

    const installedOxlint = await realpath(join(consumer, "node_modules/.bin/oxlint"));
    assert.equal(
      installedOxlint,
      await realpath(join(consumer, "node_modules/oxc-tsrx/bin/oxlint")),
      "the official extension must discover the public package's oxlint launcher",
    );
    const directDependencies = JSON.parse(
      await readFile(join(consumer, "package.json"), "utf8"),
    ).dependencies;
    assert.deepEqual(directDependencies, { "oxc-tsrx": "0.1.1" });

    const { ordinaryPath, tsrxPath } = await writeWorkspaceFixtures(consumer, {
      "oxc.enable.oxlint": true,
      "oxc.enable.oxfmt": false,
      "oxc.requireConfig": false,
    });

    await runTests({
      vscodeExecutablePath: executable,
      reuseMachineInstall: false,
      extensionDevelopmentPath: officialExtension,
      extensionTestsPath: join(root, "tests/editor/official-oxc-toolchain-suite.cjs"),
      extensionTestsEnv: cleanEnvironment(consumer, registry.url, {
        OXC_TSRX_EDITOR_FILE: tsrxPath,
        OXC_TSRX_ORDINARY_EDITOR_FILE: ordinaryPath,
        OXC_TSRX_EXPECTED_EXTENSION_PATH: officialExtension,
      }),
      launchArgs: [
        consumer,
        `--extensions-dir=${extensionDirectory}`,
        `--user-data-dir=${userDirectory}`,
        "--disable-extensions",
        "--disable-workspace-trust",
        "--skip-welcome",
        "--skip-release-notes",
      ],
    });

    // ---------------------------------------------------------------------------
    // The install-only discovery session.
    // ---------------------------------------------------------------------------

    const discoveryEnvironment = cleanEnvironment(discovery, registry.url);
    await writeFile(
      join(discovery, "package.json"),
      `${JSON.stringify(
        {
          name: "oxc-tsrx-install-only-discovery-proof",
          private: true,
          type: "module",
          dependencies: { "oxc-tsrx": "0.1.1" },
        },
        null,
        2,
      )}\n`,
    );
    await mustRun(
      npm,
      ["install", "--ignore-scripts", "--no-audit", "--no-fund"],
      { cwd: discovery, env: discoveryEnvironment },
    );

    // Nothing else runs in this workspace. `.bin` is removed outright so the
    // compatibility route the official extension normally takes cannot exist, and
    // `oxc-tsrx setup` is never invoked, so none of its facades are installed.
    await rm(join(discovery, "node_modules/.bin"), { recursive: true, force: true });
    await assertMissing(
      join(discovery, "node_modules/.bin"),
      "node_modules/.bin survived in the install-only workspace",
    );
    for (const facade of ["oxlint", "oxfmt", "oxc-parser"]) {
      await assertMissing(
        join(discovery, "node_modules", facade),
        `${facade} exists without oxc-tsrx setup`,
      );
    }

    const discoveredHost = join(discovery, "node_modules/oxc-tsrx/bin/oxlint");
    const discoveredServer = join(discovery, "node_modules/oxc-tsrx/bin/oxc-tsrx-lsp");
    await access(discoveredHost);
    await access(discoveredServer);

    await writePathDecoys(decoys, decoyMarker);
    const search = [
      decoys,
      dirname(process.execPath),
      "/usr/bin",
      "/bin",
      "/usr/sbin",
      "/sbin",
    ].join(delimiter);

    // Contrast: prove the decoys really do answer a lookup by tool name, so the
    // "no decoy ran" assertion below cannot pass vacuously.
    const control = await run("/bin/sh", ["-c", "oxlint --lsp"], {
      cwd: discovery,
      env: { PATH: search },
    });
    assert.equal(control.status, DECOY_STATUS, control.stderr || control.stdout);
    assert.ok(
      (await readFile(decoyMarker, "utf8")).includes(`${join(decoys, "oxlint")} --lsp`),
      "the PATH decoy did not record its own invocation",
    );
    await rm(decoyMarker, { force: true });

    const discoveryFixtures = await writeWorkspaceFixtures(discovery, {
      "oxc.enable.oxlint": true,
      "oxc.enable.oxfmt": false,
      "oxc.requireConfig": false,
      // No released OXC build discovers providers, so the host still has to be
      // named. This is an absolute path to the general Oxlint host inside the
      // installed package: it is not `.bin`, not `PATH`, not an alias, and it
      // carries no language, extension, or server information.
      "oxc.path.oxlint": discoveredHost,
      // Run that host under the editor's own Node so the extension does not
      // prepend the package's `bin` directory to `PATH`, which would put the real
      // tool names ahead of the decoys and make the PATH claim unfalsifiable.
      "oxc.useExecPath": true,
    });

    const manifestBefore = await readFile(join(discovery, "package.json"), "utf8");
    const lockfileBefore = await readFile(join(discovery, "package-lock.json"), "utf8");

    await runTests({
      vscodeExecutablePath: executable,
      reuseMachineInstall: false,
      extensionDevelopmentPath: officialExtension,
      extensionTestsPath: join(root, "tests/editor/official-oxc-toolchain-suite.cjs"),
      extensionTestsEnv: cleanEnvironment(discovery, registry.url, {
        PATH: search,
        // The official extension otherwise replaces the child environment with a
        // login shell's, which would discard the shadowed `PATH` above. Pointing
        // `SHELL` at a path that does not exist makes it keep this one.
        SHELL: join(temporary, "absent-login-shell"),
        OXC_TSRX_SUITE_MODE: "discovery",
        OXC_TSRX_DISCOVERY_ROOT: discovery,
        OXC_TSRX_PATH_DECOY_DIR: decoys,
        OXC_TSRX_PATH_DECOY_MARKER: decoyMarker,
        OXC_TSRX_EDITOR_FILE: discoveryFixtures.tsrxPath,
        OXC_TSRX_ORDINARY_EDITOR_FILE: discoveryFixtures.ordinaryPath,
        OXC_TSRX_EXPECTED_EXTENSION_PATH: officialExtension,
      }),
      launchArgs: [
        discovery,
        `--extensions-dir=${join(temporary, "discovery-extensions")}`,
        `--user-data-dir=${join(temporary, "discovery-user")}`,
        "--disable-extensions",
        "--disable-workspace-trust",
        "--skip-welcome",
        "--skip-release-notes",
      ],
    });

    await assertMissing(
      decoyMarker,
      "a tool name was resolved from PATH during the install-only session",
    );
    await assertMissing(
      join(discovery, "node_modules/.bin"),
      "node_modules/.bin was recreated during the install-only session",
    );
    for (const facade of ["oxlint", "oxfmt", "oxc-parser"]) {
      await assertMissing(
        join(discovery, "node_modules", facade),
        `${facade} was installed during the install-only session`,
      );
    }
    assert.equal(await readFile(join(discovery, "package.json"), "utf8"), manifestBefore);
    assert.equal(
      await readFile(join(discovery, "package-lock.json"), "utf8"),
      lockfileBefore,
    );

    // ---------------------------------------------------------------------------
    // The patched-host session: the same workspace with no pointer at all.
    // ---------------------------------------------------------------------------

    await runPatchedHostSession({
      root: temporary,
      registry,
      executable,
      officialExtension,
      decoys,
      decoyMarker,
      search,
    });
  } finally {
    await registry?.close();
    await rm(temporary, { recursive: true, force: true });
  }
}

const invokedDirectly =
  typeof process.argv[1] === "string" &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (invokedDirectly) await main();

export {
  DECOY_STATUS,
  DECOY_TOOL_NAMES,
  assertMissing,
  cleanEnvironment,
  hostTarget,
  main,
  mustRun,
  pack,
  run,
  runPatchedHostSession,
  writePathDecoys,
  writeWorkspaceFixtures,
};
