"use strict";

const assert = require("node:assert/strict");
const { dirname, join } = require("node:path");
const vscode = require("vscode");

async function waitFor(read, predicate, label, timeout = 7000) {
  const started = Date.now();
  for (;;) {
    const value = await read();
    if (predicate(value)) return value;
    if (Date.now() - started >= timeout) throw new Error(`Timed out waiting for ${label}`);
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
}

async function run() {
  assert.equal(
    vscode.extensions.getExtension("thejackshelton.oxc-tsrx-vscode"),
    undefined,
    "the companion extension must not be installed in this proof",
  );
  const extension = vscode.extensions.getExtension("oxc.oxc-vscode");
  assert.ok(extension, "the official OXC extension is not installed in the test host");
  const config = await vscode.workspace.openTextDocument(
    vscode.Uri.file(
      join(dirname(process.env.OXC_TSRX_EDITOR_FILE), "oxlint-custom-parser.json"),
    ),
  );
  await vscode.window.showTextDocument(config);
  await waitFor(
    () => extension.isActive,
    Boolean,
    "official OXC activation from its JSON config",
  );

  // Let the proxy's client/registerCapability round trip finish before opening
  // the file, so the official client owns the first didOpen notification.
  await new Promise((resolve) => setTimeout(resolve, 250));

  const uri = vscode.Uri.file(process.env.OXC_TSRX_EDITOR_FILE);
  const document = await vscode.workspace.openTextDocument(uri);
  await vscode.window.showTextDocument(document);
  const diagnostics = await waitFor(
    () => vscode.languages.getDiagnostics(uri),
    (items) =>
      items.some(
        (item) =>
          item.source === "oxc" &&
          item.code === "tsrx-demo(no-tsrx-if)" &&
          item.message.includes("prefer a declarative component"),
      ),
    "official OXC custom JavaScript-plugin diagnostic",
  );
  const diagnostic = diagnostics.find(
    (item) => item.source === "oxc" && item.code === "tsrx-demo(no-tsrx-if)",
  );
  assert.ok(diagnostic);
  assert.equal(document.getText(diagnostic.range).startsWith("@if"), true);
  assert.equal(document.getText(diagnostic.range).endsWith("}"), true);

  const originalLine = diagnostic.range.start.line;
  const edit = new vscode.WorkspaceEdit();
  edit.insert(uri, new vscode.Position(0, 0), "// unsaved editor change\n");
  assert.equal(await vscode.workspace.applyEdit(edit), true);
  await waitFor(
    () => vscode.languages.getDiagnostics(uri),
    (items) =>
      items.some(
        (item) =>
          item.source === "oxc" &&
          item.code === "tsrx-demo(no-tsrx-if)" &&
          item.range.start.line === originalLine + 1,
      ),
    "official OXC custom rule after an unsaved .tsrx edit",
  );
}

module.exports = { run };
