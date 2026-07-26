"use strict";

const { isAbsolute, join, resolve } = require("node:path");
const { existsSync, statSync } = require("node:fs");
const vscode = require("vscode");
const { LanguageClient } = require("vscode-languageclient/node");
const { discoverProviders } = require("oxc-tsrx/provider-resolve");
const {
  LANGUAGE_SERVER_ARGUMENTS,
  clientForDocument,
  discoverWorkspaceFolders,
  documentExtension,
  providerDocumentSelector,
} = require("./provider-client.cjs");

/**
 * This extension is a provider-driven host, not a client for one language.
 *
 * Every decision below comes from `./provider-client.cjs`, which knows nothing
 * about any particular provider: discovery runs once per workspace folder, one
 * language client exists per discovered provider that declares a language
 * server, and a client is created only when a document whose extension that
 * folder's index owns is actually opened. A workspace of ordinary JavaScript and
 * TypeScript files therefore starts no process at all.
 *
 * The `oxcTsrx.server.path` / `OXC_TSRX_LSP_BIN` / bundled-binary /
 * toolchain-runtime chain below is a **compatibility-only** fallback for a
 * folder in which discovery finds no provider language server — for example a
 * platform VSIX used against a project that has not declared the toolchain as a
 * dependency. It is lazy in exactly the same way: it starts nothing until a
 * document it claims is opened.
 */

/** Extensions the compatibility fallback serves when nothing was discovered. */
const COMPATIBILITY_EXTENSIONS = Object.freeze([".tsrx"]);
const COMPATIBILITY_CLIENT_ID = "compatibility";
const CLIENT_NAME = "OXC for TSRX";

/** One independent discovery state per workspace folder; never merged. */
let folderStates = [];
/** folder path -> (client id -> LanguageClient). */
const running = new Map();
let channel;
let extensionRoot;

function log(message) {
  channel?.appendLine(message);
}

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

function folderOptions(folderUri) {
  const config = vscode.workspace.getConfiguration("oxcTsrx", folderUri);
  return {
    workspaceUri: folderUri.toString(),
    options: {
      typeAware: config.get("typeAware", false) || config.get("typeCheck", false),
      typeCheck: config.get("typeCheck", false),
      lintConfigPath: config.get("lint.configPath", ""),
      formatConfigPath: config.get("format.configPath", ""),
    },
  };
}

/**
 * Initialization options stay scoped to the folder the client serves, so two
 * workspace folders never see each other's configuration.
 */
function workspaceOptions(folderPath) {
  return (vscode.workspace.workspaceFolders ?? [])
    .filter((folder) => folder.uri.fsPath === folderPath)
    .map((folder) => folderOptions(folder.uri));
}

/**
 * The subcommand that selects the language server inside the native
 * multi-call executable. One `oxc-tsrx` binary replaced the three separate
 * release binaries, and it dispatches on `argv[0]` as well, but a VSIX-embedded
 * or configured path is spawned under its real file name, so the compatibility
 * fallback always asks for the tool explicitly. Discovered provider servers are
 * unaffected: they keep the plain provider-protocol argument vector.
 */
const NATIVE_SERVER_SUBCOMMAND = Object.freeze(["lsp"]);

/**
 * Compatibility-only resolution chain. Unchanged from the pre-discovery host
 * except for the native artifact it names.
 */
async function resolveCompatibilityServer() {
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
  const executable = process.platform === "win32" ? "oxc-tsrx.exe" : "oxc-tsrx";
  const bundled = join(extensionRoot ?? "", "dist", "native", executable);
  if (existsSync(bundled)) return assertServerPath(bundled, "platform VSIX");
  // The last resort resolves the native artifact through the toolchain's own
  // platform-package logic. It is named by source path because it is an
  // internal module of `oxc-tsrx`, not part of that package's public export
  // map; the shipped extension is a single Rolldown bundle, so this import is
  // inlined at build time and never resolved on a user's machine.
  const { resolveNativeBinary } = await import("../../toolchain/dist/runtime.js");
  return resolveNativeBinary("server");
}

async function compatibilityDescriptor(documentPath) {
  const extension = documentExtension(documentPath);
  if (extension === null || !COMPATIBILITY_EXTENSIONS.includes(extension)) return null;
  return {
    id: COMPATIBILITY_CLIENT_ID,
    package: null,
    extensions: [...COMPATIBILITY_EXTENSIONS],
    command: await resolveCompatibilityServer(),
    args: [...NATIVE_SERVER_SUBCOMMAND, ...LANGUAGE_SERVER_ARGUMENTS],
    selector: providerDocumentSelector(COMPATIBILITY_EXTENSIONS),
  };
}

/**
 * The descriptor that owns a document, or `null`. A folder with at least one
 * discovered provider language server is served exclusively by discovery; the
 * compatibility chain is consulted only when discovery produced no client.
 */
async function descriptorForDocument(state, documentPath) {
  if (state.clients.length > 0) return clientForDocument(state, documentPath);
  return compatibilityDescriptor(documentPath);
}

/**
 * A provider bin with a Node shebang is started under this editor's own runtime,
 * which `provider-client.cjs` reports as `process.execPath`. In an Electron host
 * that path is the editor binary, and it only behaves as Node when
 * `ELECTRON_RUN_AS_NODE` is set. That is host knowledge, so the host supplies it
 * here instead of the vendor-neutral decision module guessing at it. Any other
 * command is spawned with this process's environment unchanged.
 */
function serverEnvironment(descriptor) {
  if (descriptor.command !== process.execPath) return undefined;
  return { ...process.env, ELECTRON_RUN_AS_NODE: "1", ELECTRON_NO_ASAR: "1" };
}

async function startClient(state, descriptor) {
  const clients = running.get(state.folder) ?? new Map();
  running.set(state.folder, clients);
  if (clients.has(descriptor.id)) return;

  // No `transport` is declared: the descriptor already carries the complete
  // argument vector the provider protocol specifies, and asking the client
  // library for stdio transport would append a second `--stdio` to it.
  const executable = {
    command: descriptor.command,
    args: descriptor.args,
    options: { cwd: state.folder, env: serverEnvironment(descriptor) },
  };
  const client = new LanguageClient(
    `oxc-provider-${descriptor.id}`,
    descriptor.package ? `${CLIENT_NAME} (${descriptor.package})` : CLIENT_NAME,
    { run: executable, debug: executable },
    {
      documentSelector: descriptor.selector,
      workspaceFolder: vscode.workspace
        .getWorkspaceFolder(vscode.Uri.file(state.folder)),
      initializationOptions: workspaceOptions(state.folder),
    },
  );
  clients.set(descriptor.id, client);
  try {
    await client.start();
    log(
      `started ${descriptor.id} for ${descriptor.extensions.join(", ")} in ${state.folder}`,
    );
  } catch (error) {
    clients.delete(descriptor.id);
    log(
      `could not start ${descriptor.id} in ${state.folder}: ${
        error instanceof Error ? error.message : String(error)
      }`,
    );
  }
}

/**
 * Lazy start. A document that no folder index claims never reaches `startClient`,
 * which is what keeps ordinary source files off every provider path.
 */
async function ensureClientForDocument(document) {
  if (document?.uri?.scheme !== "file") return;
  const folder = vscode.workspace.getWorkspaceFolder(document.uri);
  if (!folder) return;
  const folderPath = folder.uri.fsPath;
  const state = folderStates.find((entry) => entry.folder === folderPath);
  if (state === undefined) return;
  let descriptor;
  try {
    descriptor = await descriptorForDocument(state, document.uri.fsPath);
  } catch (error) {
    log(
      `no language server is available in ${folderPath}: ${
        error instanceof Error ? error.message : String(error)
      }`,
    );
    return;
  }
  if (descriptor === null) return;
  await startClient(state, descriptor);
}

async function stopFolder(folderPath) {
  const clients = running.get(folderPath);
  if (clients === undefined) return;
  running.delete(folderPath);
  for (const client of clients.values()) {
    await client.stop().catch(() => {});
  }
}

function reportDiagnostics(state) {
  for (const entry of state.diagnostics) {
    if (entry.severity === "error" || entry.severity === "warning") {
      log(`${entry.severity}: ${entry.message}`);
    }
  }
  if (state.failure) {
    log(`provider discovery failed in ${state.folder}: ${state.failure.message}`);
  }
}

async function refreshWorkspace() {
  const folders = (vscode.workspace.workspaceFolders ?? []).map(
    (folder) => folder.uri.fsPath,
  );
  folderStates = await discoverWorkspaceFolders(folders, { discover: discoverProviders });
  for (const state of folderStates) reportDiagnostics(state);
  const known = new Set(folders);
  for (const folderPath of [...running.keys()]) {
    if (!known.has(folderPath)) await stopFolder(folderPath);
  }
  for (const document of vscode.workspace.textDocuments) {
    await ensureClientForDocument(document);
  }
}

function synchronizeWorkspaceOptions() {
  for (const [folderPath, clients] of running) {
    for (const client of clients.values()) {
      void client
        .sendNotification("workspace/didChangeConfiguration", {
          settings: workspaceOptions(folderPath),
        })
        .catch((error) => {
          console.error("OXC for TSRX could not apply workspace settings", error);
        });
    }
  }
}

async function activate(context) {
  const config = vscode.workspace.getConfiguration("oxcTsrx");
  if (!config.get("enable", true)) return;
  extensionRoot = context.extensionPath;
  channel = vscode.window.createOutputChannel(CLIENT_NAME);
  context.subscriptions.push(channel);
  await refreshWorkspace();
  context.subscriptions.push(
    vscode.workspace.onDidOpenTextDocument((document) => {
      void ensureClientForDocument(document);
    }),
    vscode.workspace.onDidChangeWorkspaceFolders(() => {
      void refreshWorkspace();
    }),
    vscode.workspace.onDidChangeConfiguration((event) => {
      if (event.affectsConfiguration("oxcTsrx")) synchronizeWorkspaceOptions();
    }),
  );
}

async function deactivate() {
  for (const folderPath of [...running.keys()]) await stopFolder(folderPath);
  folderStates = [];
}

module.exports = { activate, deactivate };
