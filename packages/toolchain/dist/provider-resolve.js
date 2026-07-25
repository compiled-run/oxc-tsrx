import { readFile as readFileFromDisk } from "node:fs/promises";
import { createRequire } from "node:module";
import { dirname, join, parse, relative, resolve as resolvePath, sep } from "node:path";

/**
 * Reference implementation of the static `oxc.provider` discovery protocol.
 *
 * A provider declares one static top-level `oxc.provider` block in its own
 * package.json. A host reads the project root manifest, takes the union of the
 * direct dependency fields, resolves `<name>/package.json` for each, parses the
 * JSON, and builds an extension index. Nothing in this file imports, requires,
 * or spawns a dependency: module resolution and `JSON.parse` are the only
 * operations performed against a candidate package.
 *
 * The module deliberately contains no knowledge of any individual provider, so
 * it can be vendored by an unrelated host unchanged.
 */

export const PROTOCOL_VERSION = 1;
export const SUPPORTED_PROTOCOLS = Object.freeze([1]);
export const CAPABILITY_NAMES = Object.freeze(["parse", "lint", "format", "lsp"]);
export const DEPENDENCY_FIELDS = Object.freeze([
  "dependencies",
  "devDependencies",
  "optionalDependencies",
]);

/**
 * Extensions the core toolchain owns. A provider claiming one is a hard error,
 * which is what structurally keeps ordinary source files off provider paths.
 */
export const RESERVED_EXTENSIONS = Object.freeze([
  ".astro",
  ".cjs",
  ".cts",
  ".js",
  ".json",
  ".json5",
  ".jsonc",
  ".jsx",
  ".mjs",
  ".mts",
  ".svelte",
  ".ts",
  ".tsx",
  ".vue",
]);

const RESERVED = new Set(RESERVED_EXTENSIONS);
const IDENTIFIER = /^[a-z][a-z0-9-]*$/u;
const FATAL_CODES = new Set(["duplicate-id", "extension-conflict", "reserved-extension"]);

export class ProviderProtocolError extends Error {
  constructor(diagnostics) {
    const failures = diagnostics.filter((diagnostic) => diagnostic.severity === "error");
    super(
      `the declared language providers cannot be indexed:\n${failures
        .map((diagnostic) => `- ${diagnostic.message}`)
        .join("\n")}`,
    );
    this.name = "ProviderProtocolError";
    this.diagnostics = diagnostics;
  }
}

function defaultReadFile(path) {
  return readFileFromDisk(path, "utf8");
}

/**
 * Issuer-aware Node resolution. A host with a different module map (Yarn PnP)
 * injects `pnp.resolveRequest`, which has the same `(request, issuer)` shape.
 */
function createDefaultResolver() {
  const requires = new Map();
  return (request, issuer) => {
    let resolver = requires.get(issuer);
    if (resolver === undefined) {
      resolver = createRequire(issuer);
      requires.set(issuer, resolver);
    }
    return resolver.resolve(request);
  };
}

function isPlainObject(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function parseJson(text, path) {
  try {
    return JSON.parse(String(text));
  } catch (error) {
    throw new Error(
      `${path} is not valid JSON: ${error instanceof Error ? error.message : String(error)}`,
    );
  }
}

function diagnostic(severity, code, message, details = {}) {
  return { severity, code, message, ...details };
}

function isInside(directory, path) {
  const offset = relative(directory, path);
  return offset.length > 0 && !offset.startsWith("..") && !offset.startsWith(`${sep}`);
}

/** Union of the direct dependency fields, sorted for deterministic output. */
export function dependencyNames(manifest) {
  const names = new Set();
  for (const field of DEPENDENCY_FIELDS) {
    const declared = manifest?.[field];
    if (!isPlainObject(declared)) continue;
    for (const name of Object.keys(declared)) names.add(name);
  }
  return [...names].sort();
}

export function providerDeclaration(manifest) {
  const declaration = manifest?.oxc?.provider;
  return isPlainObject(declaration) ? declaration : null;
}

export function extensionOf(filePath) {
  if (typeof filePath !== "string") return null;
  const name = filePath.split(/[/\\]/u).at(-1) ?? "";
  const dot = name.lastIndexOf(".");
  if (dot <= 0) return null;
  return name.slice(dot).toLowerCase();
}

export function isReservedExtension(extension) {
  return typeof extension === "string" && RESERVED.has(extension.toLowerCase());
}

/** The nearest ancestor directory of `start` that contains a package.json. */
export async function findProjectRoot(start = process.cwd(), options = {}) {
  const readFile = options.readFile ?? defaultReadFile;
  const from = resolvePath(start);
  const filesystemRoot = parse(from).root;
  let directory = from;
  for (;;) {
    try {
      await readFile(join(directory, "package.json"), "utf8");
      return directory;
    } catch {
      if (directory === filesystemRoot) {
        throw new Error(`no package.json was found at or above ${from}`);
      }
      directory = dirname(directory);
    }
  }
}

function normalizeBinMap(manifest) {
  if (typeof manifest.bin === "string") {
    return typeof manifest.name === "string" ? { [manifest.name]: manifest.bin } : {};
  }
  return isPlainObject(manifest.bin) ? manifest.bin : {};
}

function moduleSpecifier(name, subpath) {
  if (subpath === ".") return name;
  return `${name}/${subpath.replace(/^\.\/?/u, "")}`;
}

function readCapabilities(declared, context) {
  const capabilities = {};
  if (!isPlainObject(declared)) return capabilities;
  for (const capability of CAPABILITY_NAMES) {
    const value = declared[capability];
    if (value === undefined) continue;
    const target = readCapabilityTarget(capability, value, context);
    if (target !== null) capabilities[capability] = target;
  }
  return capabilities;
}

function readCapabilityTarget(capability, value, context) {
  const { name, providerRoot, manifest, diagnostics, resolve, issuer } = context;
  if (!isPlainObject(value)) {
    diagnostics.push(
      diagnostic(
        "warning",
        "invalid-capability",
        `package ${name} declares a non-object ${capability} capability`,
        { packages: [name], capability },
      ),
    );
    return null;
  }
  if (typeof value.bin === "string" && value.bin.length > 0) {
    const declared = normalizeBinMap(manifest)[value.bin];
    if (typeof declared !== "string" || declared.length === 0) {
      diagnostics.push(
        diagnostic(
          "warning",
          "invalid-capability",
          `package ${name} declares the ${capability} capability as bin "${value.bin}", which is not a key of its own bin map`,
          { packages: [name], capability },
        ),
      );
      return null;
    }
    const path = resolvePath(providerRoot, declared);
    if (!isInside(providerRoot, path)) {
      diagnostics.push(
        diagnostic(
          "warning",
          "invalid-capability",
          `package ${name} declares the ${capability} capability outside its own package directory`,
          { packages: [name], capability },
        ),
      );
      return null;
    }
    return { kind: "bin", bin: value.bin, path };
  }
  if (typeof value.module === "string" && value.module.startsWith(".")) {
    const specifier = moduleSpecifier(name, value.module);
    let path = null;
    try {
      path = resolve(specifier, issuer);
    } catch {
      diagnostics.push(
        diagnostic(
          "warning",
          "unresolved-capability",
          `package ${name} declares the ${capability} capability at ${specifier}, which its exports map does not resolve for this host`,
          { packages: [name], capability },
        ),
      );
    }
    return { kind: "module", subpath: value.module, specifier, path };
  }
  diagnostics.push(
    diagnostic(
      "warning",
      "invalid-capability",
      `package ${name} declares a ${capability} capability that is neither an export subpath nor an own bin key`,
      { packages: [name], capability },
    ),
  );
  return null;
}

function readLanguages(declared, context) {
  const { name, diagnostics } = context;
  if (!Array.isArray(declared) || declared.length === 0) {
    diagnostics.push(
      diagnostic("warning", "invalid-provider", `package ${name} declares no provider languages`, {
        packages: [name],
      }),
    );
    return [];
  }
  const languages = [];
  for (const entry of declared) {
    if (!isPlainObject(entry) || typeof entry.id !== "string" || !IDENTIFIER.test(entry.id)) {
      diagnostics.push(
        diagnostic(
          "warning",
          "invalid-provider",
          `package ${name} declares a language without a valid id`,
          { packages: [name] },
        ),
      );
      continue;
    }
    const extensions = [];
    for (const extension of Array.isArray(entry.extensions) ? entry.extensions : []) {
      if (
        typeof extension !== "string" ||
        extension.length < 2 ||
        !extension.startsWith(".") ||
        extension !== extension.toLowerCase() ||
        /[\s/\\]/u.test(extension)
      ) {
        diagnostics.push(
          diagnostic(
            "warning",
            "invalid-provider",
            `package ${name} declares the malformed extension ${JSON.stringify(extension)} for language ${entry.id}`,
            { packages: [name] },
          ),
        );
        continue;
      }
      if (RESERVED.has(extension)) {
        diagnostics.push(
          diagnostic(
            "error",
            "reserved-extension",
            `package ${name} claims the reserved ${extension} extension, which protocol ${PROTOCOL_VERSION} keeps on the core toolchain`,
            { packages: [name], extension },
          ),
        );
        continue;
      }
      if (!extensions.includes(extension)) extensions.push(extension);
    }
    if (extensions.length === 0) continue;
    languages.push({
      id: entry.id,
      extensions,
      capabilities: readCapabilities(entry.capabilities, context),
    });
  }
  return languages;
}

function readProvider(name, record, declaration, context) {
  const { diagnostics, protocols } = context;
  const { protocol, id } = declaration;
  if (!Number.isInteger(protocol)) {
    diagnostics.push(
      diagnostic(
        "warning",
        "invalid-provider",
        `package ${name} declares a non-integer provider protocol`,
        { packages: [name] },
      ),
    );
    return null;
  }
  if (!protocols.has(protocol)) {
    diagnostics.push(
      diagnostic(
        "warning",
        "unsupported-protocol",
        `package ${name} declares provider protocol ${protocol}, which this host does not support; it is ignored`,
        { packages: [name], protocol },
      ),
    );
    return null;
  }
  if (typeof id !== "string" || !IDENTIFIER.test(id)) {
    diagnostics.push(
      diagnostic(
        "warning",
        "invalid-provider",
        `package ${name} declares an invalid provider id ${JSON.stringify(id ?? null)}`,
        { packages: [name] },
      ),
    );
    return null;
  }
  const languages = readLanguages(declaration.languages, {
    ...context,
    name,
    manifest: record.manifest,
    providerRoot: record.root,
  });
  if (languages.length === 0) return null;
  return {
    name,
    version: typeof record.manifest.version === "string" ? record.manifest.version : null,
    root: record.root,
    manifest: record.manifestPath,
    protocol,
    id,
    languages,
  };
}

/**
 * Read one candidate dependency's manifest.
 *
 * The two failure modes below look alike and are not alike. Resolution failing
 * means the package is simply not installed, which is ordinary and stays quiet;
 * an uninstalled optional dependency must never warn. A manifest that resolves
 * and then cannot be read or parsed is different: resolution already proved the
 * package is there, so failing to read it is a host or environment fault, and it
 * must never be mistaken for "this package is not a provider".
 */
async function readDependencyManifest(name, issuer, resolve, readFile, diagnostics) {
  let manifestPath;
  try {
    manifestPath = resolve(`${name}/package.json`, issuer);
  } catch {
    // Not installed. Ordinary, and deliberately quiet.
    return null;
  }
  let manifest;
  try {
    manifest = JSON.parse(String(await readFile(manifestPath, "utf8")));
  } catch (error) {
    diagnostics?.push(
      diagnostic(
        "warning",
        "unreadable-manifest",
        `package ${name} resolved to ${manifestPath}, which this host could not read or parse: ${
          error instanceof Error ? error.message : String(error)
        }; a host with its own module map must supply a readFile that reads through the same layer`,
        { packages: [name], manifest: manifestPath },
      ),
    );
    return null;
  }
  if (!isPlainObject(manifest)) {
    diagnostics?.push(
      diagnostic(
        "warning",
        "unreadable-manifest",
        `package ${name} resolved to ${manifestPath}, which does not contain a JSON object`,
        { packages: [name], manifest: manifestPath },
      ),
    );
    return null;
  }
  return { manifestPath, root: dirname(manifestPath), manifest };
}

async function inspectAncestors(root, directNames, context) {
  const { readFile, resolve, diagnostics } = context;
  const filesystemRoot = parse(root).root;
  let directory = root;
  while (directory !== filesystemRoot) {
    directory = dirname(directory);
    const manifestPath = join(directory, "package.json");
    let manifest;
    try {
      manifest = JSON.parse(String(await readFile(manifestPath, "utf8")));
    } catch {
      continue;
    }
    for (const name of dependencyNames(manifest)) {
      if (directNames.has(name)) continue;
      const record = await readDependencyManifest(
        name,
        manifestPath,
        resolve,
        readFile,
        diagnostics,
      );
      if (record === null || providerDeclaration(record.manifest) === null) continue;
      diagnostics.push(
        diagnostic(
          "warning",
          "ancestor-provider",
          `package ${name} declares a language provider and is a dependency of ${manifestPath}, but it is not a direct dependency of ${join(root, "package.json")}; it is not activated`,
          { packages: [name], manifest: manifestPath, root: join(root, "package.json") },
        ),
      );
    }
  }
}

/**
 * Build the provider index for one project root.
 *
 * `resolve(request, issuer)` and `readFile(path, encoding)` are injectable so a
 * host with a non-filesystem module map can supply its own. Fatal protocol
 * violations (a reserved extension, two providers claiming one extension, two
 * providers claiming one id) throw unless `throwOnError` is `false`; a report
 * command uses `false` to show every violation at once.
 */
export async function discoverProviders(options = {}) {
  const root = resolvePath(options.root ?? process.cwd());
  const readFile = options.readFile ?? defaultReadFile;
  const resolve = options.resolve ?? createDefaultResolver();
  const protocols = new Set(options.protocols ?? SUPPORTED_PROTOCOLS);
  const throwOnError = options.throwOnError !== false;
  const manifestPath = join(root, "package.json");
  const diagnostics = [];

  const manifest = parseJson(await readFile(manifestPath, "utf8"), manifestPath);
  const names = dependencyNames(manifest);
  const context = { diagnostics, protocols, resolve, readFile, issuer: manifestPath };

  const declared = [];
  for (const name of names) {
    const record = await readDependencyManifest(
      name,
      manifestPath,
      resolve,
      readFile,
      diagnostics,
    );
    if (record === null) continue;
    const declaration = providerDeclaration(record.manifest);
    if (declaration === null) continue;
    const provider = readProvider(name, record, declaration, context);
    if (provider !== null) declared.push(provider);
  }

  const byIdentifier = new Map();
  for (const provider of declared) {
    const group = byIdentifier.get(provider.id);
    if (group === undefined) byIdentifier.set(provider.id, [provider]);
    else group.push(provider);
  }
  const rejectedIdentifiers = new Set();
  for (const [id, group] of byIdentifier) {
    if (group.length < 2) continue;
    rejectedIdentifiers.add(id);
    const packages = group.map((provider) => provider.name);
    diagnostics.push(
      diagnostic(
        "error",
        "duplicate-id",
        `packages ${packages.join(" and ")} both declare the provider id "${id}"`,
        { packages, id },
      ),
    );
  }
  const providers = declared.filter((provider) => !rejectedIdentifiers.has(provider.id));

  const claims = new Map();
  for (const provider of providers) {
    for (const language of provider.languages) {
      for (const extension of language.extensions) {
        const claim = { provider, language };
        const group = claims.get(extension);
        if (group === undefined) claims.set(extension, [claim]);
        else group.push(claim);
      }
    }
  }
  const extensions = {};
  for (const extension of [...claims.keys()].sort()) {
    const group = claims.get(extension);
    if (group.length > 1) {
      const packages = group.map((claim) => claim.provider.name);
      diagnostics.push(
        diagnostic(
          "error",
          "extension-conflict",
          `packages ${packages.join(" and ")} both claim the ${extension} extension; protocol ${PROTOCOL_VERSION} never picks a winner`,
          { packages, extension },
        ),
      );
      continue;
    }
    const [{ provider, language }] = group;
    extensions[extension] = {
      extension,
      package: provider.name,
      providerId: provider.id,
      providerRoot: provider.root,
      language: language.id,
      capabilities: language.capabilities,
    };
  }

  if (options.inspectAncestors !== false) {
    await inspectAncestors(root, new Set(names), context);
  }

  const index = { root, providers, extensions, diagnostics };
  if (throwOnError && diagnostics.some((entry) => entry.severity === "error")) {
    throw new ProviderProtocolError(diagnostics);
  }
  return index;
}

export function providerExtensions(index) {
  return Object.keys(index?.extensions ?? {}).sort();
}

export function hasProviderErrors(index) {
  return (index?.diagnostics ?? []).some((entry) => entry.severity === "error");
}

export function isFatalDiagnostic(entry) {
  return FATAL_CODES.has(entry?.code);
}

/**
 * Look up one capability for one file. Returns `null` for every extension the
 * index does not own, which is the fast path for ordinary source files.
 */
export function resolveCapability(index, filePath, capability) {
  const extension = extensionOf(filePath);
  if (extension === null) return null;
  const entry = index?.extensions?.[extension];
  if (entry === undefined) return null;
  const target = entry.capabilities?.[capability];
  if (target === undefined) return null;
  return {
    package: entry.package,
    providerId: entry.providerId,
    providerRoot: entry.providerRoot,
    language: entry.language,
    extension,
    capability,
    ...target,
  };
}
