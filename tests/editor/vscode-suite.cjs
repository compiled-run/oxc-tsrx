"use strict";

const assert = require("node:assert/strict");
const { existsSync, readFileSync } = require("node:fs");
const nodePath = require("node:path");
const vscode = require("vscode");
const {
  assertNoLookupPaths,
  createStep,
  declaredLanguageServer,
  diagnosticCode,
  liveLanguageServer,
  pathVariants,
  processTable,
  waitFor: waitForShared,
} = require("./official-oxc-toolchain-suite.cjs");

/**
 * Two real VS Code sessions share this suite, selected by
 * `OXC_TSRX_SUITE_MODE`.
 *
 * - `markless` (default) is the long-standing walkthrough against a real
 *   Markless workspace. The server is handed to the extension through
 *   `OXC_TSRX_LSP_BIN`, so it exercises the compatibility fallback. Its
 *   assertions are unchanged.
 * - `discovery` is the pointer-free proof. Nothing but `npm install` ran, the
 *   whole of `node_modules/.bin` is deleted, every tool name is shadowed on
 *   `PATH` by a decoy proven to fire, and there is no `OXC_TSRX_LSP_BIN`, no
 *   `oxcTsrx.server.path`, and no workspace setting at all. A language server
 *   can then only exist because `discoverProviders` read the installed
 *   package's own `oxc.provider` block, and the real process table has to show
 *   the `lsp` bin that block declares.
 */

const extensionId = "thejackshelton.oxc-tsrx-vscode";

/** Environment variables the pointer-free session is allowed to carry. */
const DISCOVERY_ENVIRONMENT_KEYS = Object.freeze([
  "OXC_TSRX_DISCOVERY_ROOT",
  "OXC_TSRX_EDITOR_FILE",
  "OXC_TSRX_ORDINARY_EDITOR_FILE",
  "OXC_TSRX_PATH_DECOY_DIR",
  "OXC_TSRX_PATH_DECOY_MARKER",
  "OXC_TSRX_SUITE_MODE",
]);

async function waitFor(read, predicate, label, timeout = 10000) {
  const started = Date.now();
  for (;;) {
    const value = await read();
    if (predicate(value)) return value;
    if (Date.now() - started >= timeout) throw new Error(`Timed out waiting for ${label}`);
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
}

async function runMarkless() {
  const uri = vscode.Uri.file(process.env.OXC_TSRX_EDITOR_FILE);
  const document = await vscode.workspace.openTextDocument(uri);
  await vscode.window.showTextDocument(document);
  assert.equal(document.languageId, "markless-tsrx");

  const extension = vscode.extensions.getExtension(extensionId);
  assert.ok(extension, `${extensionId} is not installed in the Extension Host`);
  await waitFor(() => extension.isActive, Boolean, "automatic OXC for TSRX activation");

  const diagnostics = await waitFor(
    () => vscode.languages.getDiagnostics(uri),
    (items) => {
      const native = items.filter((item) => item.source === "oxlint-tsrx");
      return (
        native.some((item) => item.code === "no-debugger") &&
        native.some((item) => item.code === "no-var")
      );
    },
    "native authored-span diagnostics",
  );
  const nativeDiagnostics = diagnostics.filter((item) => item.source === "oxlint-tsrx");
  const debuggerOffset = document.getText().indexOf("debugger;");
  assert.notEqual(debuggerOffset, -1);
  const debuggerDiagnostic = nativeDiagnostics.find((item) => item.code === "no-debugger");
  assert.deepEqual(debuggerDiagnostic.range, new vscode.Range(
    document.positionAt(debuggerOffset),
    document.positionAt(debuggerOffset + "debugger;".length),
  ));

  const oxcConfig = vscode.workspace.getConfiguration("oxcTsrx", document.uri);
  await oxcConfig.update(
    "lint.configPath",
    "config/no-var-only.json",
    vscode.ConfigurationTarget.Workspace,
  );
  await waitFor(
    () => vscode.languages.getDiagnostics(uri),
    (items) => {
      const native = items.filter((item) => item.source === "oxlint-tsrx");
      return (
        native.some((item) => item.code === "no-var") &&
        !native.some((item) => item.code === "no-debugger")
      );
    },
    "workspace config-path change and diagnostic refresh",
  );

  const editorConfig = vscode.workspace.getConfiguration("editor", document);
  await editorConfig.update(
    "defaultFormatter",
    extensionId,
    vscode.ConfigurationTarget.Workspace,
    true,
  );
  await waitFor(
    () => ({
      formatter: vscode.workspace
        .getConfiguration("editor", document)
        .get("defaultFormatter"),
      onSave: vscode.workspace.getConfiguration("editor", document).get("formatOnSave"),
    }),
    (value) => value.formatter === extensionId && value.onSave === true,
    "language-specific formatter settings",
  );
  await editorConfig.update(
    "formatOnSave",
    true,
    vscode.ConfigurationTarget.Workspace,
    true,
  );
  const availableEdits = await vscode.commands.executeCommand(
    "vscode.executeFormatDocumentProvider",
    uri,
    { tabSize: 2, insertSpaces: true },
  );
  assert.ok(availableEdits.length > 0, "the native formatter provider returned no edits");

  const declaration = document.getText().indexOf("let saved=state('none');");
  assert.notEqual(declaration, -1);
  const changed = new vscode.WorkspaceEdit();
  changed.insert(uri, document.positionAt(declaration + 3), "  ");
  assert.equal(await vscode.workspace.applyEdit(changed), true);
  await waitFor(() => document.isDirty, Boolean, "dirty editor document");
  await new Promise((resolve) => setTimeout(resolve, 250));
  assert.equal(await document.save(), true);
  await waitFor(
    () => document.getText(),
    (text) => text.includes("let saved = state('none');"),
    "real format-on-save edit",
  );
  assert.match(document.getText(), /var editorProbe = 0;/);

  const varOffset = document.getText().indexOf("var editorProbe");
  assert.notEqual(varOffset, -1);
  const range = new vscode.Range(
    document.positionAt(varOffset),
    document.positionAt(varOffset + 3),
  );
  const actions = await vscode.commands.executeCommand(
    "vscode.executeCodeActionProvider",
    uri,
    range,
    "quickfix",
  );
  const action = actions.find((candidate) => /no-var/.test(candidate.title));
  assert.ok(action?.edit, "validated no-var quick fix was not returned");
  assert.equal(await vscode.workspace.applyEdit(action.edit), true);
  await waitFor(
    () => document.getText(),
    (text) => !text.includes("var editorProbe"),
    "identity-safe code action",
  );
  await waitFor(
    () => vscode.languages.getDiagnostics(uri),
    (items) =>
      !items.some((item) => item.source === "oxlint-tsrx" && item.code === "no-var"),
    "updated diagnostics after code action",
  );
  assert.equal(await document.save(), true);
}

/**
 * The pointer-free session.
 *
 * Every route to a language server that does not go through provider discovery
 * has been removed before this runs, and each removal is re-checked here from
 * inside the editor rather than trusted from the harness.
 */
async function runDiscovery() {
  const step = createStep("vscode-discovery");
  const root = process.env.OXC_TSRX_DISCOVERY_ROOT;
  const decoyDirectory = process.env.OXC_TSRX_PATH_DECOY_DIR;
  const decoyMarker = process.env.OXC_TSRX_PATH_DECOY_MARKER;
  assert.equal(typeof root, "string");
  assert.equal(typeof decoyDirectory, "string");
  assert.equal(typeof decoyMarker, "string");

  const extension = await step("load this repository's own client, alone", async () => {
    assert.equal(vscode.extensions.getExtension("oxc.oxc-vscode"), undefined);
    const own = vscode.extensions.getExtension(extensionId);
    assert.ok(own, `${extensionId} is not loaded in the Extension Host`);
    return own;
  });

  await step("start from an install-only workspace with no pointer", async () => {
    assert.equal(
      existsSync(nodePath.join(root, "node_modules", ".bin")),
      false,
      "node_modules/.bin must not exist in the pointer-free workspace",
    );
    for (const facade of ["oxlint", "oxfmt", "oxc-parser"]) {
      assert.equal(
        existsSync(nodePath.join(root, "node_modules", facade)),
        false,
        `${facade} is present, so oxc-tsrx setup ran in the pointer-free workspace`,
      );
    }
    const manifest = JSON.parse(
      readFileSync(nodePath.join(root, "package.json"), "utf8"),
    );
    assert.deepEqual(manifest.dependencies, { "oxc-tsrx": "0.1.0" });
    assert.equal(manifest.scripts, undefined);

    // No pointer of any kind: not in the environment, not in the settings.
    const carried = Object.keys(process.env)
      .filter((key) => key.startsWith("OXC_TSRX_") || key.startsWith("OXLINT_TSGOLINT"))
      .sort();
    assert.deepEqual(
      carried,
      [...DISCOVERY_ENVIRONMENT_KEYS].sort(),
      "the pointer-free session carries an extra tool environment variable",
    );
    assert.equal(process.env.OXC_TSRX_LSP_BIN, undefined);
    const settings = JSON.parse(
      readFileSync(nodePath.join(root, ".vscode", "settings.json"), "utf8"),
    );
    assert.deepEqual(Object.keys(settings), []);
    const configuration = vscode.workspace.getConfiguration("oxcTsrx");
    assert.equal(configuration.get("server.path", ""), "");
    assert.equal(vscode.workspace.getConfiguration("oxc").get("path.oxlint"), undefined);

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

  const server = await step(
    "resolve the language server from package presence alone",
    async () => declaredLanguageServer(root),
  );

  const isServer = (entry) =>
    server.executables.some((candidate) => entry.command.includes(candidate));

  const ordinaryUri = vscode.Uri.file(process.env.OXC_TSRX_ORDINARY_EDITOR_FILE);
  await step("open ordinary TypeScript and start nothing at all", async () => {
    const document = await vscode.workspace.openTextDocument(ordinaryUri);
    await vscode.window.showTextDocument(document);
    await waitForShared(
      () => extension.isActive,
      Boolean,
      "automatic activation of this repository's client",
      30000,
    );
    assert.equal(document.languageId, "typescript");
    // Give the host every chance to act on an ordinary file before claiming it
    // did not.
    await new Promise((settle) => setTimeout(settle, 3000));
    assert.deepEqual(
      vscode.languages
        .getDiagnostics(ordinaryUri)
        .filter((item) => item.source === "oxlint-tsrx"),
      [],
      "an ordinary TypeScript file was routed to the provider",
    );
    const started = processTable().filter(isServer);
    assert.deepEqual(
      started,
      [],
      `an ordinary TypeScript file started ${JSON.stringify(started)}`,
    );
  });

  const tsrxUri = vscode.Uri.file(process.env.OXC_TSRX_EDITOR_FILE);
  const tsrx = await step(
    "publish native TSRX diagnostics through the discovered provider",
    async () => {
      const document = await vscode.workspace.openTextDocument(tsrxUri);
      await vscode.window.showTextDocument(document);
      const diagnostics = await waitForShared(
        () => vscode.languages.getDiagnostics(tsrxUri),
        (items) =>
          items.some(
            (item) =>
              item.source === "oxlint-tsrx" && diagnosticCode(item).includes("no-var"),
          ) &&
          items.some(
            (item) =>
              item.source === "oxlint-tsrx" &&
              diagnosticCode(item).includes("no-debugger"),
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

    // The extension host itself spawned it. Nothing sits between this process
    // and the bin the provider block declares.
    assert.equal(
      languageServer.ppid,
      process.pid,
      `the language server was not spawned by this extension host: ${JSON.stringify(
        languageServer,
      )}`,
    );

    // And that wrapper launched the native server out of the same installation.
    const children = table.filter((entry) => entry.ppid === languageServer.pid);
    assert.equal(
      children.length,
      1,
      `expected one native server under the wrapper, saw ${JSON.stringify(children)}`,
    );
    const nativeRoots = pathVariants(
      nodePath.join(root, "node_modules", "@oxc-tsrx"),
    );
    assert.ok(
      nativeRoots.some((candidate) =>
        children[0].command.startsWith(`${candidate}${nodePath.sep}`),
      ),
      `the native server is not inside the installed packages: ${children[0].command}`,
    );

    // Print what the operating system actually reported, so the transcript
    // carries the evidence rather than only the verdict.
    process.stdout.write(
      `[vscode-discovery] extension host pid ${process.pid}\n` +
        `[vscode-discovery] language server ${languageServer.pid} ${languageServer.command}\n` +
        `[vscode-discovery] native server ${children[0].pid} ${children[0].command}\n`,
    );

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
    const actions = await waitForShared(
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
    await waitForShared(
      () => tsrx.getText(),
      (text) => !text.includes("var count"),
      "applied no-var quick fix",
    );
    await waitForShared(
      () => vscode.languages.getDiagnostics(tsrxUri),
      (items) =>
        !items.some(
          (item) =>
            item.source === "oxlint-tsrx" && diagnosticCode(item).includes("no-var"),
        ),
      "updated diagnostics after the quick fix",
    );
  });

  await step("format TSRX through the discovered provider", async () => {
    const edits = await waitForShared(
      () =>
        vscode.commands.executeCommand(
          "vscode.executeFormatDocumentProvider",
          tsrxUri,
          { tabSize: 2, insertSpaces: true },
        ),
      (items) => Array.isArray(items) && items.length > 0,
      "TSRX formatting edits from the discovered language server",
      30000,
    );
    const workspaceEdit = new vscode.WorkspaceEdit();
    workspaceEdit.set(tsrxUri, edits);
    assert.equal(await vscode.workspace.applyEdit(workspaceEdit), true);
    await waitForShared(
      () => tsrx.getText(),
      (text) => text.includes("export function View() @{"),
      "formatted authored TSRX",
    );
  });

  await step("leave ordinary TypeScript alone for the whole session", async () => {
    assert.deepEqual(
      vscode.languages
        .getDiagnostics(ordinaryUri)
        .filter((item) => item.source === "oxlint-tsrx"),
      [],
      "the ordinary TypeScript file picked up provider diagnostics",
    );
    const running = processTable().filter(isServer);
    assert.equal(
      running.length,
      1,
      `expected exactly one language server for the whole session, saw ${JSON.stringify(
        running,
      )}`,
    );
  });
}

async function run() {
  const mode = process.env.OXC_TSRX_SUITE_MODE ?? "markless";
  if (mode === "discovery") return runDiscovery();
  assert.equal(mode, "markless", `unknown suite mode ${mode}`);
  return runMarkless();
}

module.exports = { run };
