"use strict";

const assert = require("node:assert/strict");
const { execFileSync } = require("node:child_process");
const { existsSync, readFileSync, realpathSync } = require("node:fs");
const nodePath = require("node:path");
const vscode = require("vscode");

/**
 * Two real VS Code sessions share this suite, selected by
 * `OXC_TSRX_SUITE_MODE`.
 *
 * - `compatibility` (default) is the long-standing proof: `oxc-tsrx setup` has
 *   run, the official extension finds `node_modules/.bin/oxlint`, and TSRX is
 *   served because this package owns the canonical `oxlint` bin name.
 * - `discovery` is the install-only proof: nothing but `npm install` ran, the
 *   whole of `node_modules/.bin` has been deleted, the compatibility facades
 *   `setup` writes are absent, and every tool name is shadowed on `PATH` by a
 *   decoy that records its own invocation. The TSRX language server can then
 *   only exist because the provider block in the installed package's own
 *   `package.json` was discovered and its declared `lsp` bin was started.
 *
 * The compatibility path still ships and its assertions are unchanged.
 */

function diagnosticCode(diagnostic) {
  return String(
    typeof diagnostic.code === "object" ? diagnostic.code?.value : diagnostic.code,
  );
}

async function waitFor(read, predicate, label, timeout = 15000) {
  const started = Date.now();
  for (;;) {
    const value = await read();
    if (predicate(value)) return value;
    if (Date.now() - started >= timeout) {
      throw new Error(`Timed out waiting for ${label}: ${JSON.stringify(value)}`);
    }
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
}

/**
 * A labelled step reporter. The prefix names the lane so two different real
 * sessions can share this file without their transcripts becoming ambiguous.
 */
function createStep(prefix) {
  return async function step(label, operation) {
    process.stdout.write(`[${prefix}] START ${label}\n`);
    try {
      const result = await operation();
      process.stdout.write(`[${prefix}] PASS ${label}\n`);
      return result;
    } catch (error) {
      const detail = error instanceof Error ? error.stack ?? error.message : String(error);
      throw new Error(`[${prefix}] FAIL ${label}\n${detail}`, { cause: error });
    }
  };
}

const step = createStep("official-toolchain");

async function assertOnlyTheOfficialExtension() {
  await step("use only the released official OXC extension", async () => {
    assert.equal(vscode.extensions.getExtension("thejackshelton.oxc-tsrx-vscode"), undefined);
    const extension = vscode.extensions.getExtension("oxc.oxc-vscode");
    assert.ok(extension, "the released official OXC extension is not loaded");
    assert.equal(extension.extensionPath, process.env.OXC_TSRX_EXPECTED_EXTENSION_PATH);
  });
}

async function runCompatibility() {
  await assertOnlyTheOfficialExtension();

  const ordinaryUri = vscode.Uri.file(process.env.OXC_TSRX_ORDINARY_EDITOR_FILE);
  const ordinary = await step("keep ordinary TypeScript on canonical Oxlint", async () => {
    const extension = vscode.extensions.getExtension("oxc.oxc-vscode");
    const document = await vscode.workspace.openTextDocument(ordinaryUri);
    await vscode.window.showTextDocument(document);
    await waitFor(() => extension.isActive, Boolean, "official extension activation");
    const diagnostics = await waitFor(
      () => vscode.languages.getDiagnostics(ordinaryUri),
      (items) =>
        items.some(
          (item) => item.source === "oxc" && diagnosticCode(item).includes("no-debugger"),
        ),
      "canonical ordinary-file diagnostic",
    );
    assert.equal(diagnostics.some((item) => item.source === "oxlint-tsrx"), false);
    return document;
  });
  assert.equal(ordinary.languageId, "typescript");

  const tsrxUri = vscode.Uri.file(process.env.OXC_TSRX_EDITOR_FILE);
  const tsrx = await step("publish native diagnostics for TSRX", async () => {
    const document = await vscode.workspace.openTextDocument(tsrxUri);
    await vscode.window.showTextDocument(document);
    const diagnostics = await waitFor(
      () => vscode.languages.getDiagnostics(tsrxUri),
      (items) =>
        items.some(
          (item) =>
            item.source === "oxlint-tsrx" && diagnosticCode(item).includes("no-var"),
        ) &&
        items.some(
          (item) =>
            item.source === "oxlint-tsrx" && diagnosticCode(item).includes("no-debugger"),
        ),
      "native TSRX diagnostics",
    );
    assert.equal(diagnostics.some((item) => item.source === "oxc"), false);
    return document;
  });

  await step("refresh TSRX diagnostics for an unsaved edit", async () => {
    const diagnostic = vscode.languages
      .getDiagnostics(tsrxUri)
      .find(
        (item) =>
          item.source === "oxlint-tsrx" && diagnosticCode(item).includes("no-var"),
      );
    assert.ok(diagnostic);
    const originalLine = diagnostic.range.start.line;
    const edit = new vscode.WorkspaceEdit();
    edit.insert(tsrxUri, new vscode.Position(0, 0), "// unsaved editor change\n");
    assert.equal(await vscode.workspace.applyEdit(edit), true);
    await waitFor(
      () => vscode.languages.getDiagnostics(tsrxUri),
      (items) =>
        items.some(
          (item) =>
            item.source === "oxlint-tsrx" &&
            diagnosticCode(item).includes("no-var") &&
            item.range.start.line === originalLine + 1,
        ),
      "native diagnostics after an unsaved change",
    );
  });

  await step("format TSRX through the dynamically registered provider", async () => {
    const edits = await waitFor(
      () =>
        vscode.commands.executeCommand(
          "vscode.executeFormatDocumentProvider",
          tsrxUri,
          { tabSize: 2, insertSpaces: true },
        ),
      (items) => Array.isArray(items) && items.length > 0,
      "TSRX formatting edits",
    );
    const workspaceEdit = new vscode.WorkspaceEdit();
    workspaceEdit.set(tsrxUri, edits);
    assert.equal(await vscode.workspace.applyEdit(workspaceEdit), true);
    await waitFor(
      () => tsrx.getText(),
      (text) =>
        text.includes("export function View() @{") &&
        text.includes("var count = 0;") &&
        text.includes("<button>{count}</button>;"),
      "formatted authored TSRX",
    );
  });

  await step("apply a native TSRX quick fix", async () => {
    const diagnostics = await waitFor(
      () => vscode.languages.getDiagnostics(tsrxUri),
      (items) =>
        items.find(
          (item) =>
            item.source === "oxlint-tsrx" && diagnosticCode(item).includes("no-var"),
      ),
      "no-var diagnostic after formatting",
    );
    const diagnostic = diagnostics.find(
      (item) =>
        item.source === "oxlint-tsrx" && diagnosticCode(item).includes("no-var"),
    );
    assert.ok(diagnostic);
    const actions = await vscode.commands.executeCommand(
      "vscode.executeCodeActionProvider",
      tsrxUri,
      diagnostic.range,
      "quickfix",
    );
    const action = actions.find((candidate) => /no-var/u.test(candidate.title));
    assert.ok(action?.edit, "the native TSRX server returned no no-var quick fix");
    assert.equal(await vscode.workspace.applyEdit(action.edit), true);
    await waitFor(
      () => tsrx.getText(),
      (text) => !text.includes("var count"),
      "applied no-var quick fix",
    );
    await waitFor(
      () => vscode.languages.getDiagnostics(tsrxUri),
      (items) =>
        !items.some(
          (item) =>
            item.source === "oxlint-tsrx" && diagnosticCode(item).includes("no-var"),
        ),
      "updated diagnostics after quick fix",
    );
    assert.equal(await tsrx.save(), true);
  });
}

/**
 * Every live process on this machine, with its parent, exactly as the operating
 * system reports it. Nothing about the language server is inferred from our own
 * bookkeeping: the evidence is the real process table.
 */
function processTable() {
  assert.notEqual(
    process.platform,
    "win32",
    "the discovery session reads the real process table and has no Windows probe yet",
  );
  const ps = "/bin/ps";
  assert.ok(existsSync(ps), `${ps} is required to inspect the real process table`);
  const output = execFileSync(ps, ["-axww", "-o", "pid=,ppid=,command="], {
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
  });
  const processes = [];
  for (const line of output.split("\n")) {
    const match = /^\s*(\d+)\s+(\d+)\s+(.+)$/u.exec(line);
    if (match === null) continue;
    processes.push({
      pid: Number(match[1]),
      ppid: Number(match[2]),
      command: match[3].trim(),
    });
  }
  assert.ok(processes.length > 0, "the process table came back empty");
  return processes;
}

/**
 * A path plus the same path with every symlink resolved. A spawned process
 * reports whichever form the operating system handed it, and on macOS a
 * temporary directory is reached through `/var` but reported as `/private/var`.
 */
function pathVariants(target) {
  const variants = new Set([target]);
  try {
    variants.add(realpathSync.native(target));
  } catch {
    // A path that does not exist has no resolved form; the literal one stands.
  }
  // Longest first: `/private/var/...` and `/var/...` are both suffixes of the
  // same command line, and only the longer one identifies the whole path.
  return [...variants].sort((left, right) => right.length - left.length);
}

/**
 * The language server this workspace's provider declares, read from package
 * presence alone: the installed package's own `package.json`, its `oxc.provider`
 * block, and its `bin` map. No `.bin` directory, no `PATH`, no lookup by tool
 * name, no state written by any setup command.
 */
function declaredLanguageServer(root) {
  const packageRoot = nodePath.join(root, "node_modules", "oxc-tsrx");
  const manifest = JSON.parse(
    readFileSync(nodePath.join(packageRoot, "package.json"), "utf8"),
  );
  const provider = manifest.oxc?.provider;
  assert.ok(provider, "the installed package declares no oxc.provider block");
  assert.equal(provider.protocol, 1);
  const language = (provider.languages ?? []).find((candidate) =>
    (candidate.extensions ?? []).includes(".tsrx"),
  );
  assert.ok(language, "no declared language claims .tsrx");
  const binName = language.capabilities?.lsp?.bin;
  assert.equal(typeof binName, "string");
  const declared = manifest.bin?.[binName];
  assert.equal(typeof declared, "string");
  const executable = nodePath.resolve(packageRoot, declared);
  assert.ok(
    executable.startsWith(`${packageRoot}${nodePath.sep}`),
    `the declared language server ${executable} escapes ${packageRoot}`,
  );
  assert.ok(existsSync(executable), `${executable} is not installed`);
  return { packageRoot, binName, executable, executables: pathVariants(executable) };
}

/**
 * The single live process that is the provider's declared language server.
 *
 * Waits for the real process table to contain it, requires that exactly one
 * process matches, requires the command line to end in the declared bin plus
 * `--stdio`, and requires the interpreter in front of it to be an absolute path
 * that exists. Nothing here is read from our own bookkeeping.
 */
async function liveLanguageServer(server, timeout = 30000) {
  const running = (entry) =>
    server.executables.some((candidate) => entry.command.includes(candidate));
  const table = await waitFor(
    () => processTable(),
    (processes) => processes.some(running),
    `a live ${server.executable} process`,
    timeout,
  );
  const matches = table.filter(running);
  assert.equal(
    matches.length,
    1,
    `expected exactly one live language server, saw ${JSON.stringify(matches)}`,
  );
  const [languageServer] = matches;
  const observed = server.executables.find((candidate) =>
    languageServer.command.endsWith(`${candidate} --stdio`),
  );
  assert.ok(observed, `unexpected language server command line: ${languageServer.command}`);
  const interpreter = languageServer.command.slice(
    0,
    languageServer.command.length - ` ${observed} --stdio`.length,
  );
  assert.ok(
    nodePath.isAbsolute(interpreter) && existsSync(interpreter),
    `the language server interpreter ${interpreter} is not an absolute existing path`,
  );
  return { table, languageServer, interpreter, observed };
}

/**
 * No live process anywhere on the machine was launched through this workspace's
 * `node_modules/.bin` or through the shadowed `PATH` directory, and no decoy
 * recorded being executed. This is what makes "discovery, not lookup by name"
 * falsifiable rather than asserted.
 */
function assertNoLookupPaths({ table, root, decoyDirectory, decoyMarker }) {
  const forbidden = [
    ...pathVariants(root).map(
      (candidate) => `${nodePath.join(candidate, "node_modules", ".bin")}${nodePath.sep}`,
    ),
    ...pathVariants(decoyDirectory).map((candidate) => `${candidate}${nodePath.sep}`),
  ];
  for (const entry of table) {
    for (const directory of forbidden) {
      assert.equal(
        entry.command.includes(directory),
        false,
        `a live process was launched through ${directory}: ${entry.command}`,
      );
    }
  }
  assert.equal(
    existsSync(decoyMarker),
    false,
    "a tool name was resolved from PATH during the discovery session",
  );
}

/**
 * The install-only path: an ordinary install and nothing else.
 *
 * The workspace has no `node_modules/.bin` at all, none of the compatibility
 * facades `oxc-tsrx setup` writes, and a `PATH` whose first entry shadows every
 * tool name with a decoy that records being run. If any of those had been the
 * route to the TSRX language server there would be no server; the server that
 * does exist is the one the provider block declares.
 */
/**
 * The host that started the language server, asserted against the process
 * table.
 *
 * In `discovery` mode that host is this repository's own general Oxlint
 * launcher, named by an absolute `oxc.path.oxlint` setting. In `patched-host`
 * mode it is upstream's own `oxlint` wrapper, carrying a locally built provider
 * patch, found by the released extension's ordinary resolution with no setting
 * of any kind — and this repository's launcher must then not appear anywhere in
 * the process table.
 */
function expectedHost(root, mode) {
  if (mode === "patched-host") {
    const declared = process.env.OXC_TSRX_EXPECTED_HOST_BIN;
    assert.equal(typeof declared, "string");
    return {
      bin: declared,
      forbidden: nodePath.join(root, "node_modules", "oxc-tsrx", "bin", "oxlint"),
    };
  }
  return {
    bin: nodePath.join(root, "node_modules", "oxc-tsrx", "bin", "oxlint"),
    forbidden: null,
  };
}

/**
 * The `oxlint` package this workspace resolves is upstream's host, not a facade
 * `oxc-tsrx setup` wrote and not a provider. Checked from the package itself.
 */
function assertUpstreamHostPackage(root) {
  const hostRoot = nodePath.join(root, "node_modules", "oxlint");
  const manifest = JSON.parse(readFileSync(nodePath.join(hostRoot, "package.json"), "utf8"));
  assert.equal(manifest.name, "oxlint");
  assert.equal(
    manifest.oxc?.provider,
    undefined,
    "the resolved host must not itself be a provider",
  );
  assert.equal(typeof manifest.bin?.oxlint, "string");
  const bin = nodePath.resolve(hostRoot, manifest.bin.oxlint);
  assert.ok(existsSync(bin), `${bin} is not installed`);
  assert.equal(
    /tsrx/iu.test(readFileSync(bin, "utf8")),
    false,
    "the host's own entry point names this repository's toolchain",
  );
  assert.equal(
    /tsrx/iu.test(JSON.stringify(manifest)),
    false,
    "the host's manifest names this repository's toolchain",
  );
  return { hostRoot, bin };
}

async function runDiscovery(mode = "discovery") {
  const root = process.env.OXC_TSRX_DISCOVERY_ROOT;
  const decoyDirectory = process.env.OXC_TSRX_PATH_DECOY_DIR;
  const decoyMarker = process.env.OXC_TSRX_PATH_DECOY_MARKER;
  assert.equal(typeof root, "string");
  assert.equal(typeof decoyDirectory, "string");
  assert.equal(typeof decoyMarker, "string");
  const host = expectedHost(root, mode);

  await assertOnlyTheOfficialExtension();

  await step("start from an install-only workspace", async () => {
    assert.equal(
      existsSync(nodePath.join(root, "node_modules", ".bin")),
      false,
      "node_modules/.bin must not exist in the discovery workspace",
    );
    // In `patched-host` mode `node_modules/oxlint` is upstream's own package, so
    // its presence is the point rather than a setup leftover. The other two
    // facades `setup` writes must still be absent.
    const facades =
      mode === "patched-host" ? ["oxfmt", "oxc-parser"] : ["oxlint", "oxfmt", "oxc-parser"];
    for (const facade of facades) {
      assert.equal(
        existsSync(nodePath.join(root, "node_modules", facade)),
        false,
        `${facade} is present, so oxc-tsrx setup ran in the discovery workspace`,
      );
    }
    const manifest = JSON.parse(
      readFileSync(nodePath.join(root, "package.json"), "utf8"),
    );
    assert.deepEqual(manifest.dependencies, { "oxc-tsrx": "0.1.0" });
    assert.equal(manifest.scripts, undefined);
    const search = (process.env.PATH ?? "").split(nodePath.delimiter);
    assert.equal(
      search[0],
      decoyDirectory,
      "the decoy directory must shadow every tool name first on PATH",
    );
    assert.equal(search.includes(nodePath.join(root, "node_modules", ".bin")), false);
    for (const name of ["oxlint", "oxfmt", "oxc-tsrx", "oxc-tsrx-lsp"]) {
      assert.ok(
        existsSync(nodePath.join(decoyDirectory, name)),
        `${name} is not shadowed on PATH`,
      );
    }
    assert.equal(
      existsSync(decoyMarker),
      false,
      "a shadowed tool name was already executed from PATH",
    );
  });

  if (mode === "patched-host") {
    await step("locate the host with no oxc.path setting at all", async () => {
      const settings = JSON.parse(
        readFileSync(nodePath.join(root, ".vscode", "settings.json"), "utf8"),
      );
      const pointers = Object.keys(settings).filter((key) => key.startsWith("oxc.path"));
      assert.deepEqual(pointers, [], `the workspace still points the host at ${pointers}`);
      const { bin } = assertUpstreamHostPackage(root);
      // `/var` and `/private/var` name the same file on macOS, so every path
      // comparison here is made over the resolved variants rather than the
      // literal strings.
      const expected = new Set(pathVariants(host.bin));
      const sameFile = (candidate) =>
        pathVariants(candidate).some((variant) => expected.has(variant));
      assert.ok(sameFile(bin), `${bin} is not the declared host ${host.bin}`);
      // Exactly what the released extension's own resolution chain answers,
      // recomputed here from the workspace folder with the same call it makes.
      const resolved = nodePath.resolve(
        nodePath.dirname(require.resolve("oxlint/package.json", { paths: [root] })),
        "bin",
        "oxlint",
      );
      assert.ok(
        sameFile(resolved),
        `ordinary Node resolution answered ${resolved}, not the patched host`,
      );
    });
  }

  const server = await step(
    "resolve the language server from package presence alone",
    async () => declaredLanguageServer(root),
  );

  const ordinaryUri = vscode.Uri.file(process.env.OXC_TSRX_ORDINARY_EDITOR_FILE);
  await step("keep ordinary TypeScript on canonical Oxlint", async () => {
    const extension = vscode.extensions.getExtension("oxc.oxc-vscode");
    const document = await vscode.workspace.openTextDocument(ordinaryUri);
    await vscode.window.showTextDocument(document);
    await waitFor(() => extension.isActive, Boolean, "official extension activation");
    assert.equal(document.languageId, "typescript");
    const diagnostics = await waitFor(
      () => vscode.languages.getDiagnostics(ordinaryUri),
      (items) =>
        items.some(
          (item) => item.source === "oxc" && diagnosticCode(item).includes("no-debugger"),
        ),
      "canonical ordinary-file diagnostic",
      30000,
    );
    assert.equal(diagnostics.some((item) => item.source === "oxlint-tsrx"), false);
  });

  const tsrxUri = vscode.Uri.file(process.env.OXC_TSRX_EDITOR_FILE);
  const tsrx = await step(
    "publish native TSRX diagnostics through the discovered provider",
    async () => {
      const document = await vscode.workspace.openTextDocument(tsrxUri);
      await vscode.window.showTextDocument(document);
      const diagnostics = await waitFor(
        () => vscode.languages.getDiagnostics(tsrxUri),
        (items) =>
          items.some(
            (item) =>
              item.source === "oxlint-tsrx" && diagnosticCode(item).includes("no-var"),
          ) &&
          items.some(
            (item) =>
              item.source === "oxlint-tsrx" && diagnosticCode(item).includes("no-debugger"),
          ),
        "native TSRX diagnostics from the discovered provider",
        30000,
      );
      assert.equal(diagnostics.some((item) => item.source === "oxc"), false);
      return document;
    },
  );

  await step("run the provider's declared lsp bin as a real process", async () => {
    const { table, languageServer } = await liveLanguageServer(server);

    const parent = table.find((entry) => entry.pid === languageServer.ppid);
    assert.ok(parent, "the language server has no live parent process");
    const hosts = pathVariants(host.bin);
    assert.ok(
      hosts.some((candidate) => parent.command.includes(candidate)) &&
        parent.command.includes("--lsp"),
      `the language server was not started by the discovering host: ${parent.command}`,
    );

    // With a patched upstream host, nothing in this repository may take part in
    // locating or starting anything. The only TSRX process on the machine must
    // be the provider's own declared language server.
    if (host.forbidden !== null) {
      for (const candidate of pathVariants(host.forbidden)) {
        for (const entry of table) {
          assert.equal(
            entry.command.includes(candidate),
            false,
            `this repository's own Oxlint launcher took part: ${entry.command}`,
          );
        }
      }
    }

    assertNoLookupPaths({ table, root, decoyDirectory, decoyMarker });
  });

  await step("answer a real request from the discovered language server", async () => {
    const diagnostic = vscode.languages
      .getDiagnostics(tsrxUri)
      .find(
        (item) =>
          item.source === "oxlint-tsrx" && diagnosticCode(item).includes("no-var"),
      );
    assert.ok(diagnostic);
    const actions = await waitFor(
      () =>
        vscode.commands.executeCommand(
          "vscode.executeCodeActionProvider",
          tsrxUri,
          diagnostic.range,
          "quickfix",
        ),
      (items) => Array.isArray(items) && items.some((item) => /no-var/u.test(item.title)),
      "a quick fix from the discovered language server",
      30000,
    );
    const action = actions.find((candidate) => /no-var/u.test(candidate.title));
    assert.ok(action?.edit, "the discovered language server returned no no-var quick fix");
    assert.equal(await vscode.workspace.applyEdit(action.edit), true);
    await waitFor(
      () => tsrx.getText(),
      (text) => !text.includes("var count"),
      "applied no-var quick fix",
    );
    await waitFor(
      () => vscode.languages.getDiagnostics(tsrxUri),
      (items) =>
        !items.some(
          (item) =>
            item.source === "oxlint-tsrx" && diagnosticCode(item).includes("no-var"),
        ),
      "updated diagnostics after the quick fix",
    );
  });
}

async function run() {
  const mode = process.env.OXC_TSRX_SUITE_MODE ?? "compatibility";
  if (mode === "discovery" || mode === "patched-host") return runDiscovery(mode);
  assert.equal(mode, "compatibility", `unknown suite mode ${mode}`);
  return runCompatibility();
}

module.exports = {
  run,
  // Shared with `tests/editor/vscode-suite.cjs`, which runs the same
  // process-table probe against this repository's own VS Code client.
  assertNoLookupPaths,
  createStep,
  declaredLanguageServer,
  diagnosticCode,
  liveLanguageServer,
  pathVariants,
  processTable,
  waitFor,
};
