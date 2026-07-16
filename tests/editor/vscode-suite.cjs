"use strict";

const assert = require("node:assert/strict");
const vscode = require("vscode");

const extensionId = "thejackshelton.oxc-tsrx-vscode";

async function waitFor(read, predicate, label, timeout = 10000) {
  const started = Date.now();
  for (;;) {
    const value = await read();
    if (predicate(value)) return value;
    if (Date.now() - started >= timeout) throw new Error(`Timed out waiting for ${label}`);
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
}

async function run() {
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

module.exports = { run };
