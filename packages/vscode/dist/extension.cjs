"use strict";

const { isAbsolute, join, resolve } = require("node:path");
const { existsSync, statSync } = require("node:fs");
const vscode = require("vscode");
const {
  LanguageClient,
  TransportKind,
} = require("vscode-languageclient/node");

let client;

function assertServerPath(path, source) {
  let metadata;
  try {
    metadata = statSync(path);
  } catch {
    throw new Error(`OXC for TSRX language server is missing at ${path} (${source})`);
  }
  if (!metadata.isFile()) {
    throw new Error(`OXC for TSRX language server is not a file at ${path} (${source})`);
  }
  return path;
}

function workspaceOptions() {
  return (vscode.workspace.workspaceFolders ?? []).map((folder) => {
    const config = vscode.workspace.getConfiguration("oxcTsrx", folder.uri);
    return {
      workspaceUri: folder.uri.toString(),
      options: {
        typeAware: config.get("typeAware", false) || config.get("typeCheck", false),
        typeCheck: config.get("typeCheck", false),
        lintConfigPath: config.get("lint.configPath", ""),
        formatConfigPath: config.get("format.configPath", ""),
      },
    };
  });
}

async function resolveServer(context) {
  const configured = vscode.workspace.getConfiguration("oxcTsrx").get("server.path", "");
  if (configured) {
    if (!isAbsolute(configured)) {
      throw new Error("oxcTsrx.server.path must be an absolute trusted machine path");
    }
    return assertServerPath(configured, "oxcTsrx.server.path");
  }
  const environment = process.env.OXC_TSRX_LSP_BIN;
  if (environment) {
    return assertServerPath(resolve(environment), "OXC_TSRX_LSP_BIN");
  }
  const executable = process.platform === "win32" ? "oxc-tsrx-lsp.exe" : "oxc-tsrx-lsp";
  const bundled = join(context.extensionPath, "dist", "native", executable);
  if (existsSync(bundled)) return assertServerPath(bundled, "platform VSIX");
  const { resolveNativeBinary } = await import("@oxc-tsrx/runtime");
  return resolveNativeBinary("server");
}

function synchronizeWorkspaceOptions() {
  if (!client) return;
  void client.sendNotification("workspace/didChangeConfiguration", {
    settings: workspaceOptions(),
  }).catch((error) => {
    console.error("OXC for TSRX could not apply workspace settings", error);
  });
}

async function activate(context) {
  const config = vscode.workspace.getConfiguration("oxcTsrx");
  if (!config.get("enable", true)) return;
  const command = await resolveServer(context);
  const cwd = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
  const serverOptions = {
    run: { command, transport: TransportKind.stdio, options: { cwd } },
    debug: { command, transport: TransportKind.stdio, options: { cwd } },
  };
  const clientOptions = {
    documentSelector: [{ scheme: "file", pattern: "**/*.tsrx" }],
    initializationOptions: workspaceOptions(),
  };
  client = new LanguageClient(
    "oxc-tsrx",
    "OXC for TSRX",
    serverOptions,
    clientOptions,
  );
  context.subscriptions.push(client);
  await client.start();
  context.subscriptions.push(
    vscode.workspace.onDidChangeConfiguration((event) => {
      if (event.affectsConfiguration("oxcTsrx")) synchronizeWorkspaceOptions();
    }),
    vscode.workspace.onDidChangeWorkspaceFolders(synchronizeWorkspaceOptions),
  );
}

async function deactivate() {
  if (!client) return;
  await client.stop();
  client = undefined;
}

module.exports = { activate, deactivate };
