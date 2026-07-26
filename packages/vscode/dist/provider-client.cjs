"use strict";

/**
 * Provider-driven language client decisions for an editor host.
 *
 * This module is the transposable half of the editor integration: it turns one
 * workspace folder plus an injected provider-discovery function into pure data
 * — a per-folder index, a document selector, and one language client descriptor
 * per discovered provider that declares a language-server capability.
 *
 * It deliberately knows nothing about any individual provider, imports no
 * editor API, and spawns nothing. Everything it touches is injectable, so it is
 * unit-testable in-process and can be dropped into an unrelated editor host
 * unchanged.
 *
 * Two rules are structural rather than conventional:
 *
 * - Discovery runs once per workspace folder and indexes are never merged. A
 *   document is only ever matched against the index of the folder that contains
 *   it, so one folder's dependencies can never route another folder's files.
 * - A client descriptor exists only for an extension the folder's index owns.
 *   Ordinary source files whose extensions the discovery protocol reserves for
 *   the core toolchain never produce a descriptor, so they can never start a
 *   provider process.
 */

const { closeSync, existsSync, openSync, readSync } = require("node:fs");
const { createRequire } = require("node:module");
const { join, relative, sep } = require("node:path");

/** Yarn Plug'n'Play manifests, in the order a host should prefer them. */
const PLUG_AND_PLAY_FILES = Object.freeze([".pnp.cjs", ".pnp.js"]);

/** The capability a language client is built from. */
const LANGUAGE_SERVER_CAPABILITY = "lsp";

/** Arguments every discovered language server is started with. */
const LANGUAGE_SERVER_ARGUMENTS = Object.freeze(["--stdio"]);

const SHEBANG_BYTES = 128;

function isInside(directory, path) {
  if (typeof directory !== "string" || typeof path !== "string") return false;
  const offset = relative(directory, path);
  return offset.length > 0 && !offset.startsWith("..") && !offset.startsWith(sep);
}

/** The lowercased extension of a document path, or `null` when it has none. */
function documentExtension(filePath) {
  if (typeof filePath !== "string") return null;
  const name = filePath.split(/[/\\]/u).at(-1) ?? "";
  const dot = name.lastIndexOf(".");
  if (dot <= 0) return null;
  return name.slice(dot).toLowerCase();
}

/**
 * Read the first line of an executable to decide whether it is a Node script.
 * This is a static file read: the file is never executed to find out.
 */
function readShebangFromDisk(path) {
  let descriptor;
  try {
    descriptor = openSync(path, "r");
    const buffer = Buffer.alloc(SHEBANG_BYTES);
    const read = readSync(descriptor, buffer, 0, SHEBANG_BYTES, 0);
    return buffer.subarray(0, read).toString("utf8").split("\n", 1)[0];
  } catch {
    return "";
  } finally {
    if (descriptor !== undefined) closeSync(descriptor);
  }
}

function isInterpretedScript(shebang) {
  return (
    typeof shebang === "string" && shebang.startsWith("#!") && /\bnode\b/u.test(shebang)
  );
}

/** A document selector matching exactly the given extensions, and nothing else. */
function providerDocumentSelector(extensions) {
  return [...new Set(extensions ?? [])]
    .filter((extension) => typeof extension === "string" && extension.startsWith("."))
    .sort()
    .map((extension) => ({ scheme: "file", pattern: `**/*${extension}` }));
}

function indexExtensions(index) {
  return Object.keys(index?.extensions ?? {}).sort();
}

/**
 * Turn one folder's index into language client descriptors.
 *
 * Only providers that declare a language-server capability contribute, and a
 * descriptor claims only the extensions the index actually routed to that
 * package — a conflicting or reserved claim was already dropped upstream, so it
 * can never reach a client. The executable must live inside the declaring
 * package: a descriptor is never built from a bin shim directory or a lookup
 * path.
 */
function providerLanguageClients(index, options = {}) {
  const capability = options.capability ?? LANGUAGE_SERVER_CAPABILITY;
  const readShebang = options.readShebang ?? readShebangFromDisk;
  const interpreter = options.interpreter ?? process.execPath;
  const serverArguments = [...(options.serverArguments ?? LANGUAGE_SERVER_ARGUMENTS)];
  const clients = [];

  for (const provider of index?.providers ?? []) {
    let executable = null;
    const extensions = [];
    for (const language of provider?.languages ?? []) {
      const declared = language?.capabilities?.[capability];
      if (declared?.kind !== "bin" || typeof declared.path !== "string") continue;
      if (!isInside(provider.root, declared.path)) continue;
      executable = declared.path;
      for (const extension of language.extensions ?? []) {
        if (index?.extensions?.[extension]?.package === provider.name) {
          extensions.push(extension);
        }
      }
    }
    if (executable === null || extensions.length === 0) continue;
    const interpreted = isInterpretedScript(readShebang(executable));
    const claimed = [...new Set(extensions)].sort();
    clients.push({
      id: provider.id,
      package: provider.name,
      providerRoot: provider.root,
      capability,
      extensions: claimed,
      executable,
      command: interpreted ? interpreter : executable,
      args: interpreted ? [executable, ...serverArguments] : serverArguments,
      selector: providerDocumentSelector(claimed),
    });
  }

  return clients.sort((left, right) => left.id.localeCompare(right.id));
}

function defaultRequire(path) {
  return createRequire(path)(path);
}

/**
 * A folder that ships a Plug'n'Play manifest answers module resolution from
 * that manifest rather than from a directory walk. `resolveRequest(request,
 * issuer)` has exactly the shape the discovery protocol injects, so the manifest
 * is loaded once per folder and handed straight through.
 */
function loadFolderResolver(folder, options = {}) {
  const exists = options.existsSync ?? existsSync;
  const load = options.requireModule ?? defaultRequire;
  for (const name of PLUG_AND_PLAY_FILES) {
    const path = join(folder, name);
    if (!exists(path)) continue;
    let api;
    try {
      api = load(path);
    } catch {
      continue;
    }
    if (typeof api?.resolveRequest === "function") {
      return (request, issuer) => api.resolveRequest(request, issuer);
    }
  }
  return undefined;
}

function emptyIndex(root) {
  return { root, providers: [], extensions: {}, diagnostics: [] };
}

/**
 * Discover the providers of exactly one workspace folder.
 *
 * Discovery never throws at the host: protocol violations come back as
 * diagnostics so the editor can report them and still serve every other folder.
 * A folder whose discovery fails outright degrades to an empty index, which is
 * the host's pre-existing behavior by construction.
 */
async function discoverWorkspaceFolder(folder, options = {}) {
  const { discover } = options;
  if (typeof discover !== "function") {
    throw new TypeError("discoverWorkspaceFolder requires a discover(options) function");
  }
  const resolve = options.resolve ?? loadFolderResolver(folder, options);
  let index = emptyIndex(folder);
  let failure = null;
  try {
    index =
      (await discover({
        root: folder,
        resolve,
        readFile: options.readFile,
        throwOnError: false,
      })) ?? emptyIndex(folder);
  } catch (error) {
    failure = error instanceof Error ? error : new Error(String(error));
  }
  const extensions = indexExtensions(index);
  return {
    folder,
    index,
    extensions,
    selector: providerDocumentSelector(extensions),
    clients: providerLanguageClients(index, options),
    diagnostics: index?.diagnostics ?? [],
    failure,
  };
}

/** One independent state per folder. Indexes are never combined. */
async function discoverWorkspaceFolders(folders, options = {}) {
  const states = [];
  for (const folder of folders ?? []) {
    states.push(await discoverWorkspaceFolder(folder, options));
  }
  return states;
}

/** The client that owns a document, or `null` when the folder's index does not. */
function clientForDocument(state, documentPath) {
  const extension = documentExtension(documentPath);
  if (extension === null) return null;
  const owner = state?.index?.extensions?.[extension];
  if (owner === undefined) return null;
  return (
    (state.clients ?? []).find(
      (client) => client.package === owner.package && client.extensions.includes(extension),
    ) ?? null
  );
}

/**
 * The clients a host should have running for a set of open documents.
 *
 * Each document carries the folder it belongs to; a document is only ever
 * matched against that folder's state. A document that belongs to no folder, or
 * whose extension its folder's index does not own, contributes nothing — which
 * is why a session of ordinary source files starts no provider process at all.
 */
function plannedClientStarts(states, documents) {
  const byFolder = new Map((states ?? []).map((state) => [state.folder, state]));
  const started = new Set();
  const starts = [];
  for (const document of documents ?? []) {
    const state = byFolder.get(document?.folder);
    if (state === undefined) continue;
    const client = clientForDocument(state, document?.path);
    if (client === null) continue;
    const key = `${state.folder} ${client.id}`;
    if (started.has(key)) continue;
    started.add(key);
    starts.push({ folder: state.folder, client, document: document.path });
  }
  return starts;
}

module.exports = {
  LANGUAGE_SERVER_ARGUMENTS,
  LANGUAGE_SERVER_CAPABILITY,
  PLUG_AND_PLAY_FILES,
  clientForDocument,
  discoverWorkspaceFolder,
  discoverWorkspaceFolders,
  documentExtension,
  loadFolderResolver,
  plannedClientStarts,
  providerDocumentSelector,
  providerLanguageClients,
};
