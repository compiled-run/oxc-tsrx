import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import test from "node:test";

import { LspClient, pathToFileUri } from "./lsp-client.mjs";

const root = resolve(import.meta.dirname, "../..");
const workspace = join(root, "examples/vscode-lints");
const sourcePath = join(workspace, "LintDemo.tsrx");
const server = resolve(
  process.env.OXC_TSRX_LSP_BIN ?? join(root, "target/release/oxc-tsrx-lsp"),
);

test("the committed VS Code demo publishes all intentional authored diagnostics", async () => {
  const [source, config, settings, launch, tasks] = await Promise.all([
    readFile(sourcePath, "utf8"),
    readFile(join(workspace, ".oxlintrc.native.json"), "utf8").then(JSON.parse),
    readFile(join(workspace, ".vscode/settings.json"), "utf8").then(JSON.parse),
    readFile(join(root, ".vscode/launch.json"), "utf8").then(JSON.parse),
    readFile(join(root, ".vscode/tasks.json"), "utf8").then(JSON.parse),
  ]);

  assert.deepEqual(Object.keys(config.rules).sort(), [
    "eqeqeq",
    "no-console",
    "no-debugger",
    "no-unused-vars",
    "no-var",
  ]);
  assert.equal(settings["oxcTsrx.lint.configPath"], ".oxlintrc.native.json");
  assert.equal(
    settings["oxc.path.oxlint"],
    "oxlint-custom-parser-lsp.mjs",
  );
  assert.equal(settings["oxc.configPath"], "oxlint-custom-parser.json");
  assert.equal(settings["oxc.requireConfig"], false);
  assert.equal(settings["oxc.enable.oxlint"], true);
  assert.ok(launch.configurations.some((item) => item.name === "TSRX: lint demo"));
  assert.ok(tasks.tasks.some((item) => item.label === "build TSRX lint demo"));

  const uri = pathToFileUri(sourcePath);
  const rootUri = pathToFileUri(workspace);
  const client = new LspClient(server, { cwd: workspace });
  try {
    await client.initialize(rootUri, [
      {
        workspaceUri: rootUri,
        options: {
          lintConfigPath: ".oxlintrc.native.json",
          formatConfigPath: ".oxfmtrc.json",
        },
      },
    ]);
    client.notify("textDocument/didOpen", {
      textDocument: {
        uri,
        languageId: "markless-tsrx",
        version: 1,
        text: source,
      },
    });
    const published = await client.waitFor(
      (message) => message.method === "textDocument/publishDiagnostics",
      5000,
      "VS Code demo diagnostics",
    );
    assert.deepEqual(
      published.params.diagnostics.map((diagnostic) => diagnostic.code).sort(),
      ["eqeqeq", "no-console", "no-debugger", "no-unused-vars", "no-var"],
    );
    assert.ok(
      published.params.diagnostics.every((diagnostic) => diagnostic.source === "oxlint-tsrx"),
    );
    await client.close();
  } finally {
    client.terminate();
  }
});
