import { createRequire } from "node:module";
import {
  access,
  chmod,
  lstat,
  mkdir,
  readFile,
  readdir,
  realpath,
  rename,
  rm,
  rmdir,
  writeFile,
} from "node:fs/promises";
import { basename, dirname, isAbsolute, join, relative, resolve, sep } from "node:path";

const COMPATIBILITY_SCHEMA = 1;
const PROVIDER = "oxc-tsrx";
const DIRECT_DEPENDENCY_FIELDS = [
  "dependencies",
  "devDependencies",
  "optionalDependencies",
];
const SLOTS = Object.freeze([
  Object.freeze({
    name: "oxc-parser",
    capability: "parser",
    exportPath: "oxc-tsrx/parser",
    binary: null,
  }),
  Object.freeze({
    name: "oxlint",
    capability: "lint",
    exportPath: "oxc-tsrx/lint",
    binary: "oxlint",
  }),
  Object.freeze({
    name: "oxfmt",
    capability: "format",
    exportPath: "oxc-tsrx/format",
    binary: "oxfmt",
  }),
]);

/**
 * The fourth slot. It is not a package: it is one key in the user's own
 * `.vscode/settings.json`, and it exists because `setup` fixing *package*
 * resolution does not fix the editor. The official OXC extension finds its
 * linter through `node_modules/.bin/oxlint`, and in a Vite+ project that shim
 * belongs to Vite+, which knows nothing about `.tsrx`. The result is an editor
 * with no diagnostics and nothing anywhere saying why.
 *
 * This is the one place `setup` writes outside `node_modules`, so every report
 * names the file it touched.
 */
const EDITOR_SLOT = Object.freeze({
  name: "oxc.path.oxlint",
  capability: "editor",
  key: "oxc.path.oxlint",
  directory: ".vscode",
  file: "settings.json",
});

/** Where `setup` records what it did to the user's settings file. */
const EDITOR_RECEIPT = [".oxc-tsrx-compat", "editor-slot.json"];

/**
 * TSRX editor support that this package deliberately does not own. `.tsrx` as a
 * *language* belongs to the TSRX toolchain, so `setup` detects and reports these
 * and changes none of them.
 */
const TSRX_TYPESCRIPT_PLUGIN = "@tsrx/typescript-plugin";
const TSRX_FRAMEWORK_BINDINGS = Object.freeze([
  "@tsrx/react",
  "@tsrx/vue",
  "@tsrx/solid",
  "@tsrx/preact",
  "@tsrx/ripple",
  "octane",
]);
/**
 * `@tsrx/typescript-plugin` declares `peerDependencies.typescript: ^5.9.3`, and
 * `vp create` scaffolds TypeScript 6, so a stock Vite+ project sits outside the
 * plugin's supported range. That is a fact from the plugin's own manifest.
 *
 * What that mismatch actually causes is NOT asserted here. A stock scaffold with
 * TypeScript 6.0.3 was measured answering `hover: const legacy: number` three
 * times out of three, so this is reported as an unsupported combination rather
 * than as a known failure. Nothing here changes the version.
 */
const TYPESCRIPT_REQUIREMENT = ">=5.9 <6";

async function exists(path) {
  try {
    await access(path);
    return true;
  } catch {
    return false;
  }
}

async function readJson(path) {
  return JSON.parse(await readFile(path, "utf8"));
}

function toPosix(path) {
  return sep === "/" ? path : path.replaceAll(sep, "/");
}

function within(root, candidate) {
  const offset = relative(root, candidate);
  return offset !== ".." && !offset.startsWith(`..${sep}`) && !isAbsolute(offset);
}

async function realPathOrNull(path) {
  try {
    return await realpath(path);
  } catch {
    return null;
  }
}

// --- A tolerant reader for the user's own JSON --------------------------------
//
// `.vscode/settings.json` and `tsconfig.json` are JSON with comments, and VS
// Code also accepts trailing commas. This repository has no JSON5/JSONC
// dependency and this file must not acquire one, so the scanner below is the
// smallest thing that reads those two shapes: it knows strings, `//` and `/* */`
// comments, and structural punctuation, and nothing else. It is used two ways:
// to locate a top-level key by byte offset, so the settings file can be edited
// surgically and keep every comment and every byte this package does not own,
// and to strip comments and trailing commas before `JSON.parse` when only a
// value is wanted. Anything it cannot classify makes it return `null`, and every
// caller treats `null` as "refuse to touch this file".

const JSONC_PUNCTUATION = new Set(["{", "}", "[", "]", ",", ":"]);

function tokenizeJsonc(text) {
  const tokens = [];
  let index = 0;
  while (index < text.length) {
    const character = text[index];
    if (character === '"') {
      let cursor = index + 1;
      let closed = false;
      while (cursor < text.length) {
        if (text[cursor] === "\\") {
          cursor += 2;
          continue;
        }
        if (text[cursor] === '"') {
          closed = true;
          break;
        }
        if (text[cursor] === "\n") break;
        cursor += 1;
      }
      if (!closed) return null;
      tokens.push({
        kind: "string",
        start: index,
        end: cursor + 1,
        text: text.slice(index, cursor + 1),
      });
      index = cursor + 1;
      continue;
    }
    if (character === "/" && text[index + 1] === "/") {
      const newline = text.indexOf("\n", index);
      index = newline === -1 ? text.length : newline;
      continue;
    }
    if (character === "/" && text[index + 1] === "*") {
      const close = text.indexOf("*/", index + 2);
      if (close === -1) return null;
      index = close + 2;
      continue;
    }
    if (JSONC_PUNCTUATION.has(character)) {
      tokens.push({ kind: character, start: index, end: index + 1, text: character });
      index += 1;
      continue;
    }
    if (/\s/u.test(character)) {
      index += 1;
      continue;
    }
    let cursor = index;
    while (
      cursor < text.length &&
      !/[\s{}[\],:"]/u.test(text[cursor]) &&
      !(text[cursor] === "/" && (text[cursor + 1] === "/" || text[cursor + 1] === "*"))
    ) {
      cursor += 1;
    }
    if (cursor === index) return null;
    tokens.push({
      kind: "literal",
      start: index,
      end: cursor,
      text: text.slice(index, cursor),
    });
    index = cursor;
  }
  return tokens;
}

/** Comments and trailing commas removed, so `JSON.parse` can read the rest. */
function stripJsonc(text) {
  const tokens = tokenizeJsonc(text);
  if (!tokens) return null;
  let output = "";
  for (let index = 0; index < tokens.length; index += 1) {
    const token = tokens[index];
    if (token.kind === ",") {
      const next = tokens[index + 1];
      if (next && (next.kind === "}" || next.kind === "]")) continue;
    }
    output += token.text;
  }
  return output;
}

function parseJsoncValue(text) {
  const stripped = stripJsonc(text);
  if (stripped === null) return null;
  try {
    return JSON.parse(stripped);
  } catch {
    return null;
  }
}

/**
 * Every top-level entry of the document's object, with byte offsets. Returns
 * `null` for anything that is not a single top-level object, which is the shape
 * both settings files always have and the only shape this package will edit.
 */
// Reads the object whose opening `{` is `tokens[start]`. The top-level document
// is just the case where that is token 0, so `readTopLevelObject` is a wrapper
// that additionally insists the object is the whole file. Splitting it this way
// is what lets `compilerOptions` be edited as surgically as the top level:
// `plugins` has to be written one level down, and rewriting the document
// through `JSON.parse` would throw away every comment a scaffold ships.
function readObjectAt(tokens, start) {
  if (!tokens || tokens[start]?.kind !== "{") return null;
  const entries = [];
  let position = start + 1;
  while (position < tokens.length && tokens[position].kind !== "}") {
    const key = tokens[position];
    if (key.kind !== "string" || tokens[position + 1]?.kind !== ":") return null;
    const valueStart = position + 2;
    if (valueStart >= tokens.length) return null;
    let depth = 0;
    let valueEnd = -1;
    for (let scan = valueStart; scan < tokens.length; scan += 1) {
      const token = tokens[scan];
      if (token.kind === "{" || token.kind === "[") depth += 1;
      else if (token.kind === "}" || token.kind === "]") {
        depth -= 1;
        if (depth < 0) return null;
      }
      if (depth === 0) {
        valueEnd = scan;
        break;
      }
    }
    if (valueEnd === -1) return null;
    const comma = tokens[valueEnd + 1]?.kind === "," ? tokens[valueEnd + 1] : null;
    if (!comma && tokens[valueEnd + 1]?.kind !== "}") return null;
    let name;
    try {
      name = JSON.parse(key.text);
    } catch {
      return null;
    }
    entries.push({
      key: name,
      keyStart: key.start,
      valueStart: tokens[valueStart].start,
      valueEnd: tokens[valueEnd].end,
      valueTokens: tokens.slice(valueStart, valueEnd + 1),
      valueStartToken: valueStart,
      valueEndToken: valueEnd,
      commaEnd: comma ? comma.end : null,
    });
    position = comma ? valueEnd + 2 : valueEnd + 1;
  }
  if (tokens[position]?.kind !== "}") return null;
  return {
    entries,
    openEnd: tokens[start].end,
    closeStart: tokens[position].start,
    endToken: position,
  };
}

function readTopLevelObject(text) {
  const tokens = tokenizeJsonc(text);
  if (!tokens || tokens.length === 0) return null;
  const object = readObjectAt(tokens, 0);
  return object && object.endToken === tokens.length - 1 ? object : null;
}

// The `compilerOptions` object inside a tsconfig, located the same way, so
// `plugins` can be inserted into it without disturbing anything else in the
// file. Returns null unless the document is one object and `compilerOptions`
// is an object literal inside it, which is the only shape worth editing.
function readCompilerOptions(text) {
  const tokens = tokenizeJsonc(text);
  if (!tokens || tokens.length === 0) return null;
  const root = readObjectAt(tokens, 0);
  if (!root || root.endToken !== tokens.length - 1) return null;
  const entry = root.entries.find((candidate) => candidate.key === "compilerOptions");
  if (!entry) return null;
  const object = readObjectAt(tokens, entry.valueStartToken);
  return object && object.endToken === entry.valueEndToken ? object : null;
}

function stringEntryValue(entry) {
  if (entry.valueTokens.length !== 1 || entry.valueTokens[0].kind !== "string") return null;
  try {
    return JSON.parse(entry.valueTokens[0].text);
  } catch {
    return null;
  }
}

function detectIndent(text, structure) {
  const anchor = structure.entries[0]?.keyStart;
  if (anchor === undefined) return "  ";
  const lineStart = text.lastIndexOf("\n", anchor - 1) + 1;
  const prefix = text.slice(lineStart, anchor);
  return prefix.length > 0 && /^[\t ]*$/u.test(prefix) ? prefix : "  ";
}

function insertTopLevelEntry(text, structure, key, value) {
  return insertObjectEntry(text, structure, key, JSON.stringify(value));
}

// `rawValue` is already-rendered JSON rather than a value to stringify, because
// the tsconfig entry is written the way the documentation prints it rather than
// the way `JSON.stringify` would compact it.
function insertObjectEntry(text, structure, key, rawValue) {
  const indent = detectIndent(text, structure);
  const literal = `${JSON.stringify(key)}: ${rawValue}`;
  if (structure.entries.length === 0) {
    const inner = text.slice(structure.openEnd, structure.closeStart);
    if (inner.trim().length === 0) {
      return `${text.slice(0, structure.openEnd)}\n${indent}${literal}\n${text.slice(structure.closeStart)}`;
    }
  }
  const separator = structure.entries.length > 0 ? "," : "";
  return `${text.slice(0, structure.openEnd)}\n${indent}${literal}${separator}${text.slice(structure.openEnd)}`;
}

function removeTopLevelEntry(text, structure, key) {
  const index = structure.entries.findIndex((entry) => entry.key === key);
  if (index === -1) return text;
  const entry = structure.entries[index];
  let start = entry.keyStart;
  let end = entry.commaEnd ?? entry.valueEnd;
  const lineStart = text.lastIndexOf("\n", start - 1) + 1;
  if (/^[\t ]*$/u.test(text.slice(lineStart, start))) start = lineStart;
  while (end < text.length && (text[end] === " " || text[end] === "\t")) end += 1;
  if (text[end] === "\r") end += 1;
  if (text[end] === "\n") end += 1;
  const output = text.slice(0, start) + text.slice(end);
  // A last entry carries no comma of its own, so the previous entry's comma has
  // to go with it or the document gains a trailing comma it did not have. That
  // one character is deleted on its own rather than as part of the span, so any
  // comment written between the two entries survives.
  if (entry.commaEnd === null && index > 0) {
    const comma = structure.entries[index - 1].commaEnd;
    if (comma !== null && comma <= start) {
      return output.slice(0, comma - 1) + output.slice(comma);
    }
  }
  return output;
}

export async function findProjectRoot(start = process.cwd()) {
  let directory = resolve(start);
  try {
    if (!(await lstat(directory)).isDirectory()) directory = dirname(directory);
  } catch {
    throw new Error(`project path does not exist: ${directory}`);
  }
  for (;;) {
    if (await exists(join(directory, "package.json"))) return directory;
    const parent = dirname(directory);
    if (parent === directory) {
      // The same condition provider-resolve.js reports, in its wording, so
      // `oxc-tsrx status` and `oxc-tsrx providers` no longer describe one
      // failure two ways, plus the next step neither of them offered. `--project`
      // is already a documented flag on every subcommand that reaches here.
      throw new Error(
        `no package.json was found at or above ${resolve(start)}; run oxc-tsrx from your project root, or pass --project <directory>`,
      );
    }
    directory = parent;
  }
}

export async function detectPackageManager(projectRoot, userAgent = process.env.npm_config_user_agent) {
  for (const [lockfile, manager] of [
    ["pnpm-lock.yaml", "pnpm"],
    ["bun.lock", "bun"],
    ["bun.lockb", "bun"],
    ["yarn.lock", "yarn"],
    ["package-lock.json", "npm"],
  ]) {
    if (await exists(join(projectRoot, lockfile))) return manager;
  }
  const agent = userAgent?.split("/")[0];
  if (["npm", "pnpm", "yarn", "bun"].includes(agent)) return agent;
  return "unknown";
}

function providerSelection(manifest) {
  return DIRECT_DEPENDENCY_FIELDS.find(
    (field) => typeof manifest[field]?.[PROVIDER] === "string",
  );
}

function directlySelected(manifest, packageName) {
  return DIRECT_DEPENDENCY_FIELDS.some(
    (field) => typeof manifest[field]?.[packageName] === "string",
  );
}

function compatibilityMetadata(manifest) {
  const metadata = manifest?.oxcTsrxCompatibility;
  if (
    metadata?.schemaVersion === COMPATIBILITY_SCHEMA &&
    metadata?.provider === PROVIDER &&
    typeof metadata.providerVersion === "string"
  ) {
    return metadata;
  }
  return null;
}

async function installedProvider(projectRoot) {
  const projectManifestPath = join(projectRoot, "package.json");
  const projectManifest = await readJson(projectManifestPath);
  const selectedFrom = providerSelection(projectManifest);
  if (!selectedFrom) {
    throw new Error(
      `${PROVIDER} must be a direct dependency or devDependency in ${projectManifestPath}`,
    );
  }
  const require = createRequire(projectManifestPath);
  let providerManifestPath;
  try {
    providerManifestPath = require.resolve(`${PROVIDER}/package.json`);
  } catch {
    throw new Error(
      `${PROVIDER} is declared but not installed under ${projectRoot}; install dependencies first`,
    );
  }
  const manifest = await readJson(providerManifestPath);
  if (manifest.name !== PROVIDER || typeof manifest.version !== "string") {
    throw new Error(`resolved ${providerManifestPath} is not a valid ${PROVIDER} package`);
  }
  return {
    manifest,
    manifestPath: providerManifestPath,
    projectManifest,
    root: dirname(providerManifestPath),
    selectedFrom,
  };
}

function facadeManifest(slot, providerVersion, replacedPackage) {
  const manifest = {
    name: slot.name,
    version: providerVersion,
    private: true,
    description: `${slot.name} compatibility facade generated by ${PROVIDER}`,
    type: "module",
    main: "./dist/index.js",
    types: "./dist/index.d.ts",
    exports: {
      ".": {
        types: "./dist/index.d.ts",
        import: "./dist/index.js",
        default: "./dist/index.js",
      },
      "./package.json": "./package.json",
    },
    oxcTsrxCompatibility: {
      schemaVersion: COMPATIBILITY_SCHEMA,
      provider: PROVIDER,
      providerVersion,
      capability: slot.capability,
      ...(replacedPackage ? { replacedPackage } : {}),
    },
  };
  if (slot.binary) {
    manifest.bin = { [slot.binary]: `./bin/${slot.binary}` };
  }
  return manifest;
}

function binarySource(binary) {
  return `#!/usr/bin/env node

import { createRequire } from "node:module";
import { dirname, resolve } from "node:path";
import { pathToFileURL } from "node:url";

try {
  const require = createRequire(import.meta.url);
  const manifestPath = require.resolve("oxc-tsrx/package.json");
  const manifest = require(manifestPath);
  const declared = typeof manifest.bin === "string" ? manifest.bin : manifest.bin?.[${JSON.stringify(binary)}];
  if (typeof declared !== "string" || declared.length === 0) {
    throw new Error("oxc-tsrx does not declare the ${binary} binary");
  }
  await import(pathToFileURL(resolve(dirname(manifestPath), declared)).href);
} catch (error) {
  console.error("${binary} (oxc-tsrx compatibility): " + (error instanceof Error ? error.message : String(error)));
  process.exitCode = 2;
}
`;
}

function backupPath(modules, slot) {
  return join(
    modules,
    ".oxc-tsrx-compat",
    "originals",
    slot.name.replaceAll("/", "__"),
  );
}

async function inspectSlot(modules, slot, providerVersion, projectManifest) {
  const destination = join(modules, ...slot.name.split("/"));
  if (!(await exists(destination))) {
    return { slot, destination, state: "missing", metadata: null };
  }
  const manifest = await readJson(join(destination, "package.json")).catch(() => null);
  const metadata = compatibilityMetadata(manifest);
  if (!metadata || metadata.capability !== slot.capability) {
    if (
      manifest?.name === slot.name &&
      typeof manifest.version === "string" &&
      !directlySelected(projectManifest, slot.name)
    ) {
      if (await exists(backupPath(modules, slot))) {
        return { slot, destination, state: "collision", metadata: null };
      }
      return {
        slot,
        destination,
        state: "replaceable",
        metadata: null,
        replacedPackage: { name: manifest.name, version: manifest.version },
      };
    }
    return { slot, destination, state: "collision", metadata: null };
  }
  if (metadata.replacedPackage && !(await exists(backupPath(modules, slot)))) {
    return { slot, destination, state: "collision", metadata: null };
  }
  return {
    slot,
    destination,
    state: metadata.providerVersion === providerVersion ? "active" : "stale",
    metadata,
    replacedPackage: metadata.replacedPackage ?? null,
  };
}

async function writeFacade(directory, slot, providerVersion, replacedPackage) {
  await mkdir(join(directory, "dist"), { recursive: true });
  await Promise.all([
    writeFile(
      join(directory, "package.json"),
      `${JSON.stringify(facadeManifest(slot, providerVersion, replacedPackage), null, 2)}\n`,
    ),
    writeFile(
      join(directory, "dist/index.js"),
      `export * from ${JSON.stringify(slot.exportPath)};\n`,
    ),
    writeFile(
      join(directory, "dist/index.d.ts"),
      `export * from ${JSON.stringify(slot.exportPath)};\n`,
    ),
  ]);
  if (slot.binary) {
    const binDirectory = join(directory, "bin");
    const bin = join(binDirectory, slot.binary);
    await mkdir(binDirectory, { recursive: true });
    await writeFile(bin, binarySource(slot.binary), { mode: 0o755 });
    await chmod(bin, 0o755);
  }
}

async function replaceOwnedFacade(status, providerVersion, modules) {
  const { destination, slot } = status;
  const parent = dirname(destination);
  const temporary = join(parent, `.oxc-tsrx-${slot.name}-new-${process.pid}`);
  const previous = join(parent, `.oxc-tsrx-${slot.name}-old-${process.pid}`);
  await rm(temporary, { recursive: true, force: true });
  await rm(previous, { recursive: true, force: true });
  await writeFacade(temporary, slot, providerVersion, status.replacedPackage);
  if (status.state === "replaceable") {
    const backup = backupPath(modules, slot);
    if (await exists(backup)) {
      throw new Error(
        `refusing to replace ${slot.name}: preserved package already exists at ${backup}`,
      );
    }
    await mkdir(dirname(backup), { recursive: true });
    await rename(destination, backup);
    try {
      await rename(temporary, destination);
    } catch (error) {
      await rename(backup, destination);
      throw error;
    }
    return;
  }
  if (status.state === "stale") {
    await rename(destination, previous);
    try {
      await rename(temporary, destination);
    } catch (error) {
      await rename(previous, destination);
      throw error;
    }
    await rm(previous, { recursive: true, force: true });
  } else {
    await rename(temporary, destination);
  }
}

// --- Does this package already win the editor's lookup? ----------------------

/**
 * The official OXC extension resolves its linter through
 * `node_modules/.bin/oxlint`. If that shim already lands inside this package
 * there is nothing to write and the slot is reported `unnecessary`; if another
 * tool owns it — Vite+ is the case this exists for — the setting is the only
 * thing that reaches the editor.
 *
 * Resolution differs per package manager, so this reads the shim three ways.
 * npm, pnpm, Yarn (node-modules linker) and Bun all publish a POSIX symlink,
 * which `realpath` answers directly. npm and pnpm on Windows publish `.cmd` and
 * `.ps1` text shims that name their target inline, which the text check reads.
 * Anything that classifies as neither is reported `unknown` and treated as *not*
 * ours, because writing the setting when it was not needed still points the
 * extension at the right binary, while skipping it when it was needed is the
 * silent dead editor this slot exists to prevent.
 */
async function inspectLinterShim(modules, providerRoot) {
  const binDirectory = join(modules, ".bin");
  const names = process.platform === "win32"
    ? ["oxlint.cmd", "oxlint.ps1", "oxlint"]
    : ["oxlint"];
  const providerReal = (await realPathOrNull(providerRoot)) ?? providerRoot;
  const facadeReal = await realPathOrNull(join(modules, "oxlint"));
  const facadeIsOurs = facadeReal
    ? Boolean(
        compatibilityMetadata(
          await readJson(join(modules, "oxlint", "package.json")).catch(() => null),
        ),
      )
    : false;
  for (const name of names) {
    const shim = join(binDirectory, name);
    const info = await lstat(shim).catch(() => null);
    if (!info) continue;
    const target = await realPathOrNull(shim);
    if (target && within(providerReal, target)) {
      return { path: shim, target, owner: PROVIDER, resolvedBy: "symlink" };
    }
    if (target && facadeIsOurs && facadeReal && within(facadeReal, target)) {
      return { path: shim, target, owner: PROVIDER, resolvedBy: "compatibility-facade" };
    }
    if (info.isFile() && !info.isSymbolicLink()) {
      const source = await readFile(shim, "utf8").catch(() => "");
      if (/oxc-tsrx[\\/]bin[\\/]oxlint/u.test(source)) {
        return { path: shim, target: target ?? null, owner: PROVIDER, resolvedBy: "shim-text" };
      }
      return { path: shim, target: target ?? null, owner: "other", resolvedBy: "shim-text" };
    }
    return {
      path: shim,
      target: target ?? null,
      owner: target ? "other" : "unknown",
      resolvedBy: target ? "symlink" : "unresolved",
    };
  }
  return { path: join(binDirectory, "oxlint"), target: null, owner: "none", resolvedBy: "absent" };
}

async function editorSettingValue(projectRoot, providerRoot) {
  const linked = join(projectRoot, "node_modules", PROVIDER, "bin", "oxlint");
  if (await exists(linked)) return `node_modules/${PROVIDER}/bin/oxlint`;
  const offset = relative(projectRoot, join(providerRoot, "bin", "oxlint"));
  return offset.startsWith("..") || isAbsolute(offset)
    ? join(providerRoot, "bin", "oxlint")
    : toPosix(offset);
}

async function readEditorReceipt(modules) {
  const receipt = await readJson(join(modules, ...EDITOR_RECEIPT)).catch(() => null);
  if (
    receipt?.schemaVersion === COMPATIBILITY_SCHEMA &&
    receipt?.provider === PROVIDER &&
    receipt?.key === EDITOR_SLOT.key
  ) {
    return receipt;
  }
  return null;
}

async function inspectEditorSlot(projectRoot, providerRoot, modules) {
  const directory = join(projectRoot, EDITOR_SLOT.directory);
  const path = join(directory, EDITOR_SLOT.file);
  const shim = await inspectLinterShim(modules, providerRoot);
  const value = await editorSettingValue(projectRoot, providerRoot);
  const base = {
    name: EDITOR_SLOT.name,
    capability: EDITOR_SLOT.capability,
    key: EDITOR_SLOT.key,
    path,
    value,
    linterShim: shim,
  };
  if (shim.owner === PROVIDER) {
    // The ordinary lookup already finds this package, so the setting would be
    // noise. Nothing is written and nothing is claimed.
    return { ...base, state: "unnecessary" };
  }
  if (!(await exists(path))) return { ...base, state: "missing", currentValue: null };
  const text = await readFile(path, "utf8").catch(() => null);
  if (text === null) return { ...base, state: "unreadable", currentValue: null };
  const structure = readTopLevelObject(text);
  if (!structure) return { ...base, state: "unreadable", currentValue: null };
  const entry = structure.entries.find((candidate) => candidate.key === EDITOR_SLOT.key);
  if (!entry) return { ...base, state: "missing", currentValue: null };
  const current = stringEntryValue(entry);
  if (typeof current === "string") {
    const resolved = await realPathOrNull(
      isAbsolute(current) ? current : join(projectRoot, current),
    );
    const providerReal = (await realPathOrNull(providerRoot)) ?? providerRoot;
    if (resolved && within(providerReal, resolved)) {
      return { ...base, state: "active", currentValue: current };
    }
    const receipt = await readEditorReceipt(modules);
    if (receipt && receipt.value === current) {
      // This package wrote it and it no longer resolves here, which is what a
      // clean reinstall or a hoisting change looks like. Ours to refresh.
      return { ...base, state: "stale", currentValue: current };
    }
  }
  return { ...base, state: "collision", currentValue: current };
}

async function writeEditorSlot(projectRoot, modules, slot) {
  const directory = join(projectRoot, EDITOR_SLOT.directory);
  const createdDirectory = !(await exists(directory));
  if (createdDirectory) await mkdir(directory, { recursive: true });
  const createdFile = !(await exists(slot.path));
  const previous = createdFile ? "{}\n" : await readFile(slot.path, "utf8");
  const structure = readTopLevelObject(previous);
  if (!structure) {
    throw new Error(
      `refusing to edit ${slot.path}: its top-level JSON object could not be located`,
    );
  }
  const cleaned = slot.state === "stale"
    ? removeTopLevelEntry(previous, structure, EDITOR_SLOT.key)
    : previous;
  const target = slot.state === "stale" ? readTopLevelObject(cleaned) : structure;
  if (!target) {
    throw new Error(`refusing to edit ${slot.path}: rewriting it would not round-trip`);
  }
  await writeFile(
    slot.path,
    insertTopLevelEntry(cleaned, target, EDITOR_SLOT.key, slot.value),
  );
  const existing = await readEditorReceipt(modules);
  await mkdir(join(modules, EDITOR_RECEIPT[0]), { recursive: true });
  await writeFile(
    join(modules, ...EDITOR_RECEIPT),
    `${JSON.stringify(
      {
        schemaVersion: COMPATIBILITY_SCHEMA,
        provider: PROVIDER,
        key: EDITOR_SLOT.key,
        value: slot.value,
        settingsPath: toPosix(relative(projectRoot, slot.path)),
        createdFile: existing?.createdFile === true ? true : createdFile,
        createdDirectory:
          existing?.createdDirectory === true ? true : createdDirectory,
      },
      null,
      2,
    )}\n`,
  );
}

async function revertEditorSlot(projectRoot, modules, slot) {
  const receipt = await readEditorReceipt(modules);
  const text = await readFile(slot.path, "utf8").catch(() => null);
  if (text !== null) {
    const structure = readTopLevelObject(text);
    if (!structure) {
      throw new Error(
        `refusing to edit ${slot.path}: its top-level JSON object could not be located`,
      );
    }
    const next = removeTopLevelEntry(text, structure, EDITOR_SLOT.key);
    const remaining = readTopLevelObject(next);
    const emptied =
      remaining !== null &&
      remaining.entries.length === 0 &&
      next.slice(remaining.openEnd, remaining.closeStart).trim().length === 0;
    if (emptied && receipt?.createdFile === true) {
      await rm(slot.path, { force: true });
      if (receipt.createdDirectory === true) {
        const directory = join(projectRoot, EDITOR_SLOT.directory);
        const left = await readdir(directory).catch(() => ["keep"]);
        if (left.length === 0) await rmdir(directory).catch(() => {});
      }
    } else {
      await writeFile(slot.path, next);
    }
  }
  await rm(join(modules, ...EDITOR_RECEIPT), { force: true });
}

// --- What this package deliberately does not own -----------------------------

async function resolveDependencyManifest(fromRequire, modules, name) {
  try {
    return await readJson(fromRequire.resolve(`${name}/package.json`));
  } catch {
    // Not every package exports `./package.json`, and a package can be present
    // without being importable from the project root.
  }
  const direct = join(modules, ...name.split("/"), "package.json");
  return (await exists(direct)) ? readJson(direct).catch(() => null) : null;
}

async function nearestTsconfig(projectRoot) {
  let directory = projectRoot;
  for (;;) {
    const candidate = join(directory, "tsconfig.json");
    if (await exists(candidate)) return candidate;
    const parent = dirname(directory);
    if (parent === directory) return null;
    directory = parent;
  }
}

function declaresTsrxPlugin(tsconfig) {
  const plugins = tsconfig?.compilerOptions?.plugins;
  return (
    Array.isArray(plugins) &&
    plugins.some((plugin) => plugin?.name === TSRX_TYPESCRIPT_PLUGIN)
  );
}

/**
 * A solution-style tsconfig owns no files: it is `{ "files": [], "references": [...] }`,
 * the shape `vp create` scaffolds. Naming it in the advice below is worse than useless,
 * because a plugin declared there is inert. Measured on a stock Vite+ React app with
 * TypeScript 5.9.3: the plugin in the solution root answers `hover: any`, the same
 * plugin in the referenced project that owns `src` answers `hover: const legacy: number`.
 * So point at the project that actually contains the source.
 */
function isSolutionStyle(tsconfig) {
  const files = tsconfig?.files;
  const references = tsconfig?.references;
  return (
    Array.isArray(references) &&
    references.length > 0 &&
    Array.isArray(files) &&
    files.length === 0 &&
    tsconfig?.include === undefined
  );
}

/** The referenced project a solution-style root delegates source files to. */
async function referencedSourceProject(tsconfigPath, tsconfig) {
  const references = Array.isArray(tsconfig?.references) ? tsconfig.references : [];
  const directory = dirname(tsconfigPath);
  for (const reference of references) {
    const target = typeof reference?.path === "string" ? reference.path : null;
    if (target === null) continue;
    const candidate = target.endsWith(".json") ? join(directory, target) : join(directory, target, "tsconfig.json");
    const text = await readFile(candidate, "utf8").catch(() => null);
    if (text === null) continue;
    const parsed = parseJsoncValue(text);
    // The one that includes source, not the one describing build tooling.
    const include = parsed?.include;
    if (Array.isArray(include) && include.some((entry) => typeof entry === "string" && entry.includes("src"))) {
      return { path: candidate, declaresPlugin: declaresTsrxPlugin(parsed) };
    }
  }
  return null;
}

// `setup --write-tsconfig` is the one thing that edits a tsconfig, and it is
// opt-in for that reason: without the flag this package still never touches the
// file, it only reports the gap. The entry is written to match what the
// documentation prints, so a reader who ran the flag and a reader who typed it
// by hand end up with the same line.
const TSCONFIG_PLUGIN_LITERAL = `[{ "name": ${JSON.stringify(TSRX_TYPESCRIPT_PLUGIN)} }]`;

async function writeTsconfigPlugin(tsconfigPath) {
  const text = await readFile(tsconfigPath, "utf8").catch(() => null);
  if (text === null) {
    throw new Error(`refusing to edit ${tsconfigPath}: it could not be read`);
  }
  const options = readCompilerOptions(text);
  if (!options) {
    throw new Error(
      `refusing to edit ${tsconfigPath}: its "compilerOptions" object could not be located, so add "plugins": ${TSCONFIG_PLUGIN_LITERAL} yourself`,
    );
  }
  const existing = options.entries.find((entry) => entry.key === "plugins");
  if (existing) {
    // An existing list is somebody else's, and TypeScript takes several
    // plugins, so the right edit is an append. Appending inside an array by
    // text surgery is a good way to quietly corrupt a config, so this refuses
    // and says what to add instead, the same way a taken package slot does.
    const already = text
      .slice(existing.valueStart, existing.valueEnd)
      .includes(TSRX_TYPESCRIPT_PLUGIN);
    if (already) return "present";
    throw new Error(
      `refusing to edit ${tsconfigPath}: "compilerOptions.plugins" already exists, so add { "name": ${JSON.stringify(TSRX_TYPESCRIPT_PLUGIN)} } to it yourself`,
    );
  }
  await writeFile(
    tsconfigPath,
    insertObjectEntry(text, options, "plugins", TSCONFIG_PLUGIN_LITERAL),
  );
  return "written";
}

function typescriptSupported(version) {
  const [major, minor] = String(version ?? "")
    .split(".")
    .map((part) => Number.parseInt(part, 10));
  if (!Number.isInteger(major) || !Number.isInteger(minor)) return false;
  return major === 5 && minor >= 9;
}

/**
 * Read-only. `.tsrx` as a language belongs to the TSRX toolchain, and `setup`
 * must not silently configure another project's tooling. It still has to say
 * what is missing, because a green bridge plus a dead editor otherwise gives a
 * user no way to tell which half is broken.
 */
async function inspectLanguageSupport(projectRoot, modules) {
  const fromProject = createRequire(join(projectRoot, "package.json"));
  const pluginManifest = await resolveDependencyManifest(
    fromProject,
    modules,
    TSRX_TYPESCRIPT_PLUGIN,
  );
  const bindings = await Promise.all(
    TSRX_FRAMEWORK_BINDINGS.map(async (name) => ({
      name,
      manifest: await resolveDependencyManifest(fromProject, modules, name),
    })),
  );
  const binding = bindings.find((candidate) => candidate.manifest !== null) ?? null;
  const tsconfigPath = await nearestTsconfig(projectRoot);
  const tsconfigText = tsconfigPath ? await readFile(tsconfigPath, "utf8").catch(() => null) : null;
  const tsconfig = tsconfigText === null ? null : parseJsoncValue(tsconfigText);
  const typescriptManifest = await resolveDependencyManifest(fromProject, modules, "typescript");
  const typescriptVersion = typescriptManifest?.version ?? null;
  const supported = typescriptSupported(typescriptVersion);

  const report = {
    typescriptPlugin: {
      package: TSRX_TYPESCRIPT_PLUGIN,
      present: pluginManifest !== null,
      version: pluginManifest?.version ?? null,
    },
    frameworkBinding: {
      candidates: [...TSRX_FRAMEWORK_BINDINGS],
      present: binding !== null,
      name: binding?.name ?? null,
      version: binding?.manifest?.version ?? null,
    },
    tsconfig: {
      path: tsconfigPath,
      readable: tsconfig !== null,
      declaresPlugin: tsconfig !== null && declaresTsrxPlugin(tsconfig),
      solutionStyle: tsconfig !== null && isSolutionStyle(tsconfig),
      delegate: null,
    },
    typescript: {
      requirement: TYPESCRIPT_REQUIREMENT,
      present: typescriptVersion !== null,
      version: typescriptVersion,
      supported,
    },
    notes: [],
  };

  if (!report.typescriptPlugin.present) {
    report.notes.push(
      `install ${TSRX_TYPESCRIPT_PLUGIN} yourself: it is what gives an editor TSRX language support, and oxc-tsrx never installs it`,
    );
  }
  if (!report.frameworkBinding.present) {
    report.notes.push(
      `install a TSRX framework binding yourself (one of ${TSRX_FRAMEWORK_BINDINGS.join(", ")}); oxc-tsrx does not choose one for you`,
    );
  }
  if (!report.tsconfig.path) {
    report.notes.push(
      `no tsconfig.json was found at or above ${projectRoot}; add one declaring "plugins": [{ "name": "${TSRX_TYPESCRIPT_PLUGIN}" }]`,
    );
  } else if (!report.tsconfig.readable) {
    report.notes.push(
      `${report.tsconfig.path} could not be read as JSON, so its "plugins" list was not checked; oxc-tsrx never edits it`,
    );
  } else if (report.tsconfig.solutionStyle) {
    const delegate = await referencedSourceProject(report.tsconfig.path, tsconfig);
    report.tsconfig.delegate = delegate?.path ?? null;
    if (delegate === null) {
      report.notes.push(
        `${report.tsconfig.path} is solution-style ("files": [], "references": [...]), so a plugin declared there is inert. Add "plugins": [{ "name": "${TSRX_TYPESCRIPT_PLUGIN}" }] to whichever referenced project includes your source; setup --write-tsconfig cannot pick one for you here`,
      );
    } else if (!delegate.declaresPlugin) {
      report.notes.push(
        `add "plugins": [{ "name": "${TSRX_TYPESCRIPT_PLUGIN}" }] under compilerOptions in ${delegate.path}, or rerun setup with --write-tsconfig to have it added for you. Not ${report.tsconfig.path}: that one is solution-style ("files": [], "references": [...]) and a plugin declared there is inert`,
      );
    }
  } else if (!report.tsconfig.declaresPlugin) {
    report.notes.push(
      `add "plugins": [{ "name": "${TSRX_TYPESCRIPT_PLUGIN}" }] under compilerOptions in ${report.tsconfig.path}, or rerun setup with --write-tsconfig to have it added for you`,
    );
  }
  if (!report.typescript.present) {
    report.notes.push(
      `typescript is not resolvable from ${projectRoot}; ${TSRX_TYPESCRIPT_PLUGIN} needs typescript ${TYPESCRIPT_REQUIREMENT}`,
    );
  } else if (!supported) {
    report.notes.push(
      `typescript ${typescriptVersion} is outside ${TSRX_TYPESCRIPT_PLUGIN}'s declared peer range (${TYPESCRIPT_REQUIREMENT}). It may still work; if the editor misbehaves, pinning typescript into that range is the first thing to try. oxc-tsrx never changes your typescript version`,
    );
  }
  report.ok = report.notes.length === 0;
  return report;
}

export async function compatibilityStatus(options = {}) {
  const projectRoot = await findProjectRoot(options.projectRoot);
  const provider = await installedProvider(projectRoot);
  const modules = join(projectRoot, "node_modules");
  const slots = await Promise.all(
    SLOTS.map((slot) =>
      inspectSlot(modules, slot, provider.manifest.version, provider.projectManifest),
    ),
  );
  return {
    projectRoot,
    packageManager: await detectPackageManager(projectRoot, options.userAgent),
    providerVersion: provider.manifest.version,
    selectedFrom: provider.selectedFrom,
    slots: slots.map(({ slot, destination, state, replacedPackage }) => ({
      name: slot.name,
      capability: slot.capability,
      path: destination,
      state,
      ...(replacedPackage ? { replacedPackage } : {}),
    })),
    editorSlot: await inspectEditorSlot(projectRoot, provider.root, modules),
    languageSupport: await inspectLanguageSupport(projectRoot, modules),
  };
}

export async function setupCompatibility(options = {}) {
  const status = await compatibilityStatus(options);
  const collisions = status.slots.filter((slot) => slot.state === "collision");
  if (collisions.length > 0) {
    throw new Error(
      `refusing to replace unowned package slot(s): ${collisions.map((slot) => slot.name).join(", ")}. Installing on top of the existing node_modules does not free the slot, so run rm -rf node_modules, install again, and run ${PROVIDER} setup again`,
    );
  }
  const modules = join(status.projectRoot, "node_modules");
  if (!(await exists(modules))) {
    throw new Error(`node_modules is missing under ${status.projectRoot}; install dependencies first`);
  }
  // Before the slots, so a refusal here aborts without having half-bridged
  // `node_modules`. A solution-style root owns no files, so the plugin has to
  // land in the referenced project that includes your source instead.
  let tsconfigWrite = null;
  if (options.writeTsconfig) {
    const { path: rootPath, solutionStyle, delegate } = status.languageSupport.tsconfig;
    if (!rootPath) {
      throw new Error(
        `no tsconfig.json was found at or above ${status.projectRoot}, so there is nothing to write`,
      );
    }
    if (solutionStyle && !delegate) {
      throw new Error(
        `refusing to edit ${rootPath}: it is solution-style ("files": [], "references": [...]), so a plugin declared there is inert, and no referenced project including your source was found`,
      );
    }
    const target = delegate ?? rootPath;
    tsconfigWrite = {
      path: target,
      state: options.dryRun ? "preview" : await writeTsconfigPlugin(target),
    };
  }
  // The status was read before the write, so its prerequisite notes still say
  // the entry is missing. Re-reading is what stops the report telling you to
  // add by hand the line it just added for you.
  const languageSupport = tsconfigWrite && tsconfigWrite.state === "written"
    ? await inspectLanguageSupport(status.projectRoot, modules)
    : status.languageSupport;
  const changed = status.slots
    .filter((slot) => ["missing", "replaceable", "stale"].includes(slot.state))
    .map((slot) => slot.name);
  if (!options.dryRun) {
    for (const slotStatus of status.slots) {
      if (!changed.includes(slotStatus.name)) continue;
      const slot = SLOTS.find((candidate) => candidate.name === slotStatus.name);
      await replaceOwnedFacade(
        {
          slot,
          destination: slotStatus.path,
          state: slotStatus.state,
          replacedPackage: slotStatus.replacedPackage,
        },
        status.providerVersion,
        modules,
      );
    }
  }
  // The editor slot is the one thing `setup` writes outside `node_modules`, so
  // it is decided and reported separately from the package slots rather than
  // folded into them. A collision is left exactly as the user wrote it: this
  // refuses to overwrite the key, the same way the package slots refuse a
  // direct or unrecognized package, and reports it instead of failing the
  // bridge that did work.
  const editorWritten = ["missing", "stale"].includes(status.editorSlot.state);
  if (editorWritten) {
    if (!options.dryRun) {
      await writeEditorSlot(status.projectRoot, modules, status.editorSlot);
    }
    changed.push(status.editorSlot.name);
  }
  const editorSlot = editorWritten && !options.dryRun
    ? { ...status.editorSlot, state: "active", currentValue: status.editorSlot.value }
    : status.editorSlot;
  return {
    ...status,
    action: options.dryRun ? "preview" : "setup",
    slots: status.slots.map((slot) =>
      !options.dryRun && changed.includes(slot.name) ? { ...slot, state: "active" } : slot,
    ),
    editorSlot,
    languageSupport,
    ...(tsconfigWrite ? { tsconfigWrite } : {}),
    changed,
    unchanged: [
      ...status.slots.filter((slot) => slot.state === "active").map((slot) => slot.name),
      ...(editorWritten ? [] : [status.editorSlot.name]),
    ],
  };
}

export async function removeCompatibility(options = {}) {
  const status = await compatibilityStatus(options);
  const removed = [];
  for (const slot of status.slots) {
    if (!["active", "stale"].includes(slot.state)) continue;
    removed.push(slot.name);
    if (!options.dryRun) {
      if (slot.replacedPackage) {
        const candidate = SLOTS.find((entry) => entry.name === slot.name);
        const backup = backupPath(join(status.projectRoot, "node_modules"), candidate);
        if (!(await exists(backup))) {
          throw new Error(
            `cannot remove ${slot.name}: preserved ${slot.replacedPackage.name}@${slot.replacedPackage.version} is missing at ${backup}`,
          );
        }
        const temporary = `${slot.path}.oxc-tsrx-remove-${process.pid}`;
        await rm(temporary, { recursive: true, force: true });
        await rename(slot.path, temporary);
        try {
          await rename(backup, slot.path);
        } catch (error) {
          await rename(temporary, slot.path);
          throw error;
        }
        await rm(temporary, { recursive: true, force: true });
      } else {
        await rm(slot.path, { recursive: true, force: true });
      }
    }
  }
  const editorRemoved = ["active", "stale"].includes(status.editorSlot.state);
  if (editorRemoved) {
    if (!options.dryRun) {
      await revertEditorSlot(
        status.projectRoot,
        join(status.projectRoot, "node_modules"),
        status.editorSlot,
      );
    }
    removed.push(status.editorSlot.name);
  }
  return {
    ...status,
    action: options.dryRun ? "preview-remove" : "remove",
    slots: status.slots.map((slot) =>
      !options.dryRun && removed.includes(slot.name)
        ? { ...slot, state: slot.replacedPackage ? "replaceable" : "missing" }
        : slot,
    ),
    editorSlot: editorRemoved && !options.dryRun
      ? { ...status.editorSlot, state: "missing", currentValue: null }
      : status.editorSlot,
    removed,
  };
}

const EDITOR_SLOT_EXPLANATION = Object.freeze({
  active: (slot, projectRoot) =>
    `${toPosix(relative(projectRoot, slot.path))} carries "${slot.key}": "${slot.value}". This is the one file setup writes outside node_modules; it merges that single key and never edits package.json or tsconfig.json.`,
  stale: (slot, projectRoot) =>
    `${toPosix(relative(projectRoot, slot.path))} carries a "${slot.key}" this package wrote that no longer resolves here; setup refreshes it to "${slot.value}".`,
  missing: (slot, projectRoot) =>
    `${slot.linterShim.path} does not resolve into this package, so the official OXC extension would find no .tsrx support and say nothing about it. setup writes "${slot.key}": "${slot.value}" into ${toPosix(relative(projectRoot, slot.path))}, which is your tree, not node_modules.`,
  unnecessary: (slot) =>
    `${slot.linterShim.path} already resolves into this package, so the editor needs no setting and none was written.`,
  collision: (slot, projectRoot) =>
    `${toPosix(relative(projectRoot, slot.path))} already sets "${slot.key}" to "${slot.currentValue}". That is yours, so it was left alone; the editor will not use this package until it reads "${slot.value}".`,
  unreadable: (slot, projectRoot) =>
    `${toPosix(relative(projectRoot, slot.path))} could not be read as a single top-level JSON object, so nothing was written. Set "${slot.key}": "${slot.value}" there yourself.`,
});

/**
 * The width the report wraps to. A terminal reports its own; anything else,
 * including the pipe a transcript is captured through, gets a fixed 80 so the
 * recorded output is identical on every machine.
 */
function reportWidth() {
  const columns = process.stdout?.columns;
  if (!Number.isInteger(columns) || columns <= 0) return 80;
  return Math.min(Math.max(columns, 60), 100);
}

/**
 * Colour is for a human at a terminal and nobody else. A pipe, a CI log, a
 * captured transcript, or `NO_COLOR` all get plain text, so the only consumer
 * that ever sees an escape sequence is the one that can render it.
 * `FORCE_COLOR` is honoured because that is how you ask for it through a pipe.
 */
function reportColorEnabled() {
  if (process.env.NO_COLOR !== undefined && process.env.NO_COLOR !== "") return false;
  if (process.env.FORCE_COLOR !== undefined && process.env.FORCE_COLOR !== "0") return true;
  return process.stdout?.isTTY === true;
}

const REPORT_STYLES = {
  bold: "1",
  dim: "2",
  green: "32",
  yellow: "33",
  cyan: "36",
};

function paint(text, style, enabled) {
  if (!enabled || !REPORT_STYLES[style]) return text;
  return `[${REPORT_STYLES[style]}m${text}[0m`;
}

/**
 * `missing` is the healthy answer outside Vite+, so no state here is coloured
 * as an error. Green marks a slot this package has taken over, dim marks one
 * that needs nothing, and yellow marks the states that are asking the reader
 * to look at something.
 */
const SLOT_STATE_STYLE = {
  active: "green",
  unnecessary: "dim",
  missing: "yellow",
  collision: "yellow",
  unreadable: "yellow",
  removed: "dim",
};

/**
 * Wraps at spaces only. A path, a version range, or a `"plugins": [{ ... }]`
 * fragment must survive intact, because the reader's next move is to copy it
 * out of the terminal.
 */
function wrapReportText(text, firstPrefix, restPrefix, width) {
  const limit = Math.max(width - restPrefix.length, 24);
  const lines = [];
  let current = "";
  for (const word of text.split(" ")) {
    if (current === "") current = word;
    else if (`${current} ${word}`.length <= limit) current = `${current} ${word}`;
    else {
      lines.push(current);
      current = word;
    }
  }
  if (current !== "") lines.push(current);
  return lines.map((line, index) => `${index === 0 ? firstPrefix : restPrefix}${line}`);
}

/**
 * One text report for `status`, `setup`, and `remove`, so all three describe the
 * same four slots and the same unowned editor prerequisites in the same words.
 *
 * The states are padded into a column and every prose line is wrapped: this
 * report is read in a terminal after an install has already scrolled past, and
 * an unwrapped wall of it hid a single `missing` among three `active`.
 */
export function formatCompatibilityReport(result) {
  const width = reportWidth();
  const color = reportColorEnabled();
  const lines = [];
  const changes = result.changed ?? result.removed ?? null;
  if (changes) {
    const verb = result.action === "remove" ? "removed" : result.action;
    const noun = changes.length === 1 ? "slot" : "slots";
    lines.push(
      paint(
        `${verb} ${changes.length} compatibility ${noun} for ${PROVIDER} ${result.providerVersion} (${result.packageManager})`,
        "bold",
        color,
      ),
    );
  } else {
    lines.push(
      paint(
        `${PROVIDER} ${result.providerVersion} compatibility (${result.packageManager})`,
        "bold",
        color,
      ),
    );
  }

  const editor = result.editorSlot;
  const rows = result.slots.map((slot) => [slot.name, slot.state, slot.state]);
  if (editor) rows.push([editor.name, `${editor.state} (editor)`, editor.state]);
  if (result.tsconfigWrite) {
    const { path, state } = result.tsconfigWrite;
    rows.push([basename(path), `${state} (tsconfig)`, state === "preview" ? "stale" : "active"]);
  }
  const nameWidth = Math.max(...rows.map(([name]) => name.length));
  lines.push("");
  for (const [name, label, state] of rows) {
    const gutter = `  ${`${name}:`.padEnd(nameWidth + 1)}  `;
    lines.push(`${gutter}${paint(label, SLOT_STATE_STYLE[state] ?? "cyan", color)}`);
  }
  if (editor) {
    const explain = EDITOR_SLOT_EXPLANATION[editor.state];
    if (explain) {
      lines.push("");
      for (const line of wrapReportText(
        explain(editor, result.projectRoot),
        "      ",
        "      ",
        width,
      )) {
        lines.push(paint(line, "dim", color));
      }
    }
  }

  const support = result.languageSupport;
  if (support && !support.ok) {
    lines.push("");
    lines.push(
      ...wrapReportText(
        "TSRX language support in the editor belongs to the TSRX toolchain, not to this package. Nothing below was installed, changed, or configured:",
        "",
        "",
        width,
      ).map((line) => paint(line, "dim", color)),
    );
    // A blank line between the notes, not just around the block. Four of these
    // run together as one paragraph otherwise, and each one is a separate thing
    // the reader has to go and do.
    for (const note of support.notes) {
      lines.push("");
      const [first, ...rest] = wrapReportText(note, "", "    ", width);
      lines.push(`  ${paint("!", "yellow", color)} ${first}`);
      lines.push(...rest);
    }
  }
  return `${lines.join("\n")}\n`;
}
