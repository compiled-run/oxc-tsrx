// Run a project's own Oxlint JavaScript plugins over `.tsrx` files.
//
// The native TSRX lint target is a Rust process with no Node runtime, so it
// cannot host a JavaScript rule. The published Oxlint binary can, and already
// does on ordinary files. What it cannot do is parse `.tsrx`. The two halves fit
// together because the native side already builds one legal-TSX projection of
// every `.tsrx` file and already knows how to move a byte range in that
// projection back to the byte range the user actually wrote.
//
// So this lane:
//
//   1. asks the native binary for each file's projection
//      (`--emit-plugin-projection`);
//   2. mirrors those projections into a throwaway directory, keeping each file's
//      path relative to the working directory and appending `.tsx` so Oxlint
//      parses it;
//   3. mirrors the user's own `.oxlintrc.json` files alongside them so Oxlint
//      resolves severities, rule options, `extends`, and `overrides` itself,
//      exactly as it would have in the real tree;
//   4. runs the published `oxlint` binary over the mirror;
//   5. sends the diagnostics back through the native binary
//      (`--map-plugin-diagnostics`) so every label lands on authored bytes.
//
// Nothing here imports an Oxlint module: the binary is the one its package
// manifest declares, and the version is read from its public `package.json`
// export. The projection's span map never leaves Rust.
import { existsSync } from "node:fs";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { dirname, isAbsolute, join, parse, relative, resolve, sep } from "node:path";
import { pathToFileURL } from "node:url";
import { resolveNativeCommand, resolvePackageBinary, runCaptured } from "./runtime.js";

// The lane drives the published Oxlint binary through its command line, but the
// shape of that command line is still a contract: `jsPlugins`, `--format=json`,
// the diagnostic JSON's `labels[].span.offset`, and nested-config resolution all
// have to behave the way this file assumes. That was established against 1.74.0.
// A version outside this range is refused out loud rather than run hopefully,
// because silently not running a rule the user wrote and enabled is the exact
// failure this lane exists to remove.
export const OXLINT_JS_PLUGIN_LANE_MINIMUM = "1.74.0";
export const OXLINT_JS_PLUGIN_LANE_BELOW = "2.0.0";

const CONFIG_FILE_NAMES = [".oxlintrc.json", ".oxlintrc.jsonc"];

// Every category of built-in rules, from Oxlint 1.74.0's own
// configuration_schema.json. The projection run turns all of them off: the
// native lane already reports built-in rules against the authored `.tsrx`
// source, so leaving any of them on here would print each of those diagnostics
// twice.
const BUILTIN_CATEGORIES = [
  "correctness",
  "nursery",
  "pedantic",
  "perf",
  "restriction",
  "style",
  "suspicious",
];

function versionParts(version) {
  return String(version)
    .split(/[-+]/u, 1)[0]
    .split(".")
    .map((part) => Number.parseInt(part, 10));
}

function compareVersions(left, right) {
  const a = versionParts(left);
  const b = versionParts(right);
  for (let index = 0; index < 3; index += 1) {
    const first = Number.isInteger(a[index]) ? a[index] : 0;
    const second = Number.isInteger(b[index]) ? b[index] : 0;
    if (first !== second) return first < second ? -1 : 1;
  }
  return 0;
}

export function laneSupportsOxlintVersion(version) {
  if (typeof version !== "string" || !/^\d+\.\d+\.\d+/u.test(version)) return false;
  return (
    compareVersions(version, OXLINT_JS_PLUGIN_LANE_MINIMUM) >= 0 &&
    compareVersions(version, OXLINT_JS_PLUGIN_LANE_BELOW) < 0
  );
}

export function oxlintVersionRefusal(version) {
  return (
    `oxlint (oxc-tsrx): JS plugins on .tsrx require oxlint ` +
    `>=${OXLINT_JS_PLUGIN_LANE_MINIMUM} <${OXLINT_JS_PLUGIN_LANE_BELOW}; found ${version}. ` +
    `Refusing rather than silently skipping your rules.`
  );
}

/** The pinned Oxlint's own version, read through its public `./package.json` export. */
export function installedOxlintVersion(fromUrl = import.meta.url) {
  const localRequire = createRequire(fromUrl);
  const manifest = localRequire("oxlint-current/package.json");
  return typeof manifest.version === "string" ? manifest.version : "unknown";
}

/**
 * The one line this lane prints before the report.
 *
 * The extra parse is real and the user is told about it every time, on stderr,
 * with the exact key that turns it off. `--silent` suppresses it along with
 * everything else the command would have printed.
 */
export function jsPluginDisclosure(fileCount) {
  return (
    `oxlint (oxc-tsrx): running JS plugins on ${fileCount} .tsrx file(s) by linting the ` +
    `TSX projection; this parses each of those files once more. Disable with ` +
    `"settings": { "oxcTsrx": { "jsPluginsOnTsrx": false } }.`
  );
}

/**
 * Read a `.oxlintrc.json` or `.oxlintrc.jsonc`.
 *
 * Comments and trailing commas are stripped rather than parsed, because this
 * file only ever re-emits plain JSON. Anything it does not understand is copied
 * through untouched, so Oxlint keeps deciding what the configuration means.
 */
export function parseOxlintConfigText(text) {
  let stripped = "";
  let index = 0;
  while (index < text.length) {
    const character = text[index];
    if (character === '"') {
      const start = index;
      index += 1;
      while (index < text.length) {
        if (text[index] === "\\") {
          index += 2;
          continue;
        }
        if (text[index] === '"') {
          index += 1;
          break;
        }
        index += 1;
      }
      stripped += text.slice(start, index);
      continue;
    }
    if (character === "/" && text[index + 1] === "/") {
      while (index < text.length && text[index] !== "\n") index += 1;
      continue;
    }
    if (character === "/" && text[index + 1] === "*") {
      const end = text.indexOf("*/", index + 2);
      index = end === -1 ? text.length : end + 2;
      continue;
    }
    stripped += character;
    index += 1;
  }
  return JSON.parse(stripped.replace(/,(\s*[}\]])/gu, "$1"));
}

async function readOxlintConfig(path) {
  try {
    const parsed = parseOxlintConfigText(await readFile(path, "utf8"));
    return parsed !== null && typeof parsed === "object" && !Array.isArray(parsed) ? parsed : null;
  } catch {
    // A config this file cannot read is a config Oxlint will complain about on
    // its own, in its own words. Staying quiet here keeps one bad file from
    // turning into two different error messages.
    return null;
  }
}

/** The nearest Oxlint config at or above `directory`, the way Oxlint looks for one. */
export function findOxlintConfig(directory) {
  let current = resolve(directory);
  const root = parse(current).root;
  for (;;) {
    for (const name of CONFIG_FILE_NAMES) {
      const candidate = join(current, name);
      if (existsSync(candidate)) return candidate;
    }
    if (current === root) return null;
    current = dirname(current);
  }
}

/**
 * Every entry of one config's `jsPlugins`, normalized.
 *
 * Oxlint accepts both a bare specifier and `{ name, specifier }`, where `name`
 * is the alias the plugin's rules are configured under. Vite+ writes the second
 * form. The alias, when there is one, saves this lane from having to import the
 * module to learn the plugin's namespace.
 */
function declaredJsPlugins(config) {
  const declared = config?.jsPlugins;
  if (!Array.isArray(declared)) return [];
  const entries = [];
  for (const entry of declared) {
    if (typeof entry === "string" && entry.length > 0) {
      entries.push({ specifier: entry, name: null });
      continue;
    }
    if (entry !== null && typeof entry === "object" && typeof entry.specifier === "string") {
      entries.push({
        specifier: entry.specifier,
        name: typeof entry.name === "string" && entry.name.length > 0 ? entry.name : null,
      });
    }
  }
  return entries;
}

/**
 * Every JavaScript plugin one configuration brings in, and whether the project
 * turned this lane off.
 *
 * `extends` is followed because a project that keeps its shared rules in one
 * file and its per-package config in another still expects its plugins to run.
 * Missing this would not be a visible error; it would be the rule quietly not
 * running, which is exactly the failure this lane exists to remove. Each plugin
 * specifier travels with the directory of the config that declared it, because
 * that is what it resolves against.
 */
async function collectLaneFacts(path, directoryOverride = null, seen = new Set(), depth = 0) {
  const facts = { config: null, jsPlugins: [], optedOut: undefined };
  if (depth > 8 || seen.has(path)) return facts;
  seen.add(path);
  const config = await readOxlintConfig(path);
  if (config === null) return facts;
  facts.config = config;
  // A Vite+ `lint` block is resolved in Node and written to a throwaway file,
  // so its relative plugin and `extends` specifiers still belong to the
  // directory the `vite.config.ts` was authored in. That is the same directory
  // the native lane is handed as `--config-base`.
  const directory = directoryOverride ?? dirname(path);

  if (Array.isArray(config.extends)) {
    for (const specifier of config.extends) {
      if (typeof specifier !== "string") continue;
      const resolved = resolveSpecifier(specifier, directory);
      if (!isAbsolute(resolved) || !existsSync(resolved)) continue;
      const inherited = await collectLaneFacts(resolved, null, seen, depth + 1);
      facts.jsPlugins.push(...inherited.jsPlugins);
      if (inherited.optedOut !== undefined) facts.optedOut = inherited.optedOut;
    }
  }
  for (const entry of declaredJsPlugins(config)) {
    facts.jsPlugins.push({ ...entry, directory });
  }
  if (Array.isArray(config.overrides)) {
    for (const override of config.overrides) {
      for (const entry of declaredJsPlugins(override)) {
        facts.jsPlugins.push({ ...entry, directory });
      }
    }
  }
  const own = config.settings?.oxcTsrx?.jsPluginsOnTsrx;
  if (typeof own === "boolean") facts.optedOut = own === false;
  return facts;
}

/** Resolve one plugin or extends specifier against the directory its config lives in. */
function resolveSpecifier(specifier, configDirectory) {
  if (isAbsolute(specifier)) return specifier;
  if (specifier.startsWith(".")) return resolve(configDirectory, specifier);
  try {
    return createRequire(join(configDirectory, "package.json")).resolve(specifier);
  } catch {
    // Not resolvable from here: hand it back untouched so Oxlint reports its own
    // resolution failure rather than a rewritten one.
    return specifier;
  }
}

/**
 * The plugin namespaces this project's `jsPlugins` contribute, or `null` when
 * they cannot all be determined.
 *
 * A rule's diagnostic code is `<plugin meta.name>(<rule>)`, so this is what
 * separates a diagnostic the user's own JavaScript produced from a built-in one
 * that a `rules` entry re-enabled behind the categories this lane turns off.
 * `null` means "do not filter by namespace", which is strictly more permissive
 * and can only ever leave a duplicate in, never drop a user's rule.
 */
async function pluginNamespaces(declared) {
  const namespaces = new Set();
  for (const { specifier, name: alias, directory } of declared) {
    if (alias !== null) {
      // The config already named it, so there is nothing to load.
      namespaces.add(alias);
      continue;
    }
    const resolved = resolveSpecifier(specifier, directory);
    try {
      const module = await import(
        isAbsolute(resolved) ? pathToFileURL(resolved).href : resolved
      );
      const name = module.default?.meta?.name ?? module.meta?.name;
      if (typeof name !== "string" || name.length === 0) return null;
      namespaces.add(name);
    } catch {
      return null;
    }
  }
  return namespaces;
}

/**
 * A glob and the same glob with `.tsx` appended.
 *
 * The mirror names each projection `<authored name>.tsx`, so a project that
 * wrote `overrides: [{ files: ["**\/*.tsrx"] }]` would match nothing there. This
 * was measured rather than assumed: `**\/*.tsrx` does not match `demo.tsrx.tsx`,
 * and `**\/*.tsrx.tsx` does.
 */
function projectedGlobs(globs) {
  if (!Array.isArray(globs)) return globs;
  const expanded = [];
  for (const glob of globs) {
    expanded.push(glob);
    if (typeof glob === "string" && !expanded.includes(`${glob}.tsx`)) {
      expanded.push(`${glob}.tsx`);
    }
  }
  return expanded;
}

/** One `jsPlugins` entry with its specifier resolved, in either form Oxlint accepts. */
function absoluteJsPlugin(entry, configDirectory) {
  if (typeof entry === "string") return resolveSpecifier(entry, configDirectory);
  if (entry !== null && typeof entry === "object" && typeof entry.specifier === "string") {
    return { ...entry, specifier: resolveSpecifier(entry.specifier, configDirectory) };
  }
  return entry;
}

/**
 * The user's configuration as the projection run should see it.
 *
 * Everything Oxlint understands survives, because Oxlint is the thing resolving
 * it. Four edits, each for one reason:
 *
 *   * every built-in category off, so the native lane stays the only reporter of
 *     built-in rules and nothing is printed twice;
 *   * `jsPlugins` and `extends` made absolute, because the config is read from a
 *     different directory than the one it was written in;
 *   * `ignorePatterns` dropped, because the native lane has already applied them
 *     and they were written against `.tsrx` names the mirror does not use;
 *   * every `overrides` glob given a `.tsx` twin, so an override aimed at
 *     `.tsrx` still selects that file's projection.
 */
export function projectionConfig(config, configDirectory) {
  const projected = { ...config };
  delete projected.$schema;
  delete projected.ignorePatterns;

  projected.categories = { ...(config.categories ?? {}) };
  for (const category of BUILTIN_CATEGORIES) projected.categories[category] = "off";

  if (Array.isArray(config.jsPlugins)) {
    projected.jsPlugins = config.jsPlugins.map((entry) =>
      absoluteJsPlugin(entry, configDirectory),
    );
  }
  if (Array.isArray(config.extends)) {
    projected.extends = config.extends.map((specifier) =>
      typeof specifier === "string" ? resolveSpecifier(specifier, configDirectory) : specifier,
    );
  }
  if (Array.isArray(config.overrides)) {
    projected.overrides = config.overrides.map((override) => {
      if (override === null || typeof override !== "object") return override;
      const mapped = { ...override };
      mapped.files = projectedGlobs(override.files);
      if (override.excludeFiles !== undefined) {
        mapped.excludeFiles = projectedGlobs(override.excludeFiles);
      }
      if (Array.isArray(override.jsPlugins)) {
        mapped.jsPlugins = override.jsPlugins.map((entry) =>
          absoluteJsPlugin(entry, configDirectory),
        );
      }
      return mapped;
    });
  }
  return projected;
}

/** The user's configuration with `jsPlugins` removed, for the native lane. */
export function nativeLaneConfig(config) {
  const stripped = { ...config };
  delete stripped.jsPlugins;
  if (Array.isArray(config.overrides)) {
    stripped.overrides = config.overrides.map((override) => {
      if (override === null || typeof override !== "object") return override;
      const mapped = { ...override };
      delete mapped.jsPlugins;
      return mapped;
    });
  }
  return stripped;
}

/** Where one authored path lives inside the mirror, relative to the mirror root. */
export function mirrorRelativePath(cwd, path) {
  const relativePath = relative(cwd, path);
  if (relativePath !== "" && !relativePath.startsWith("..") && !isAbsolute(relativePath)) {
    return `${relativePath}.tsx`;
  }
  // A file outside the working directory still has to land inside the mirror, or
  // the lane would write into the user's tree. Its full path becomes its name so
  // two such files cannot collide.
  const flattened = path
    .replace(/^[A-Za-z]:/u, "")
    .split(/[\\/]/u)
    .filter((segment) => segment.length > 0 && segment !== "..")
    .join(sep);
  return `${join("__outside_cwd__", flattened)}.tsx`;
}

async function writeMirrorFile(root, relativePath, contents) {
  const absolute = join(root, relativePath);
  await mkdir(dirname(absolute), { recursive: true });
  await writeFile(absolute, contents);
  return absolute;
}

/**
 * Decide whether the JavaScript plugin lane runs for this invocation, and set it
 * up if it does.
 *
 * Returns `null` when there is nothing to do, or one of:
 *
 *   * `{ status: "opted-out" }` — the project turned the lane off, so the native
 *     lane keeps `jsPlugins` and answers with its own refusal;
 *   * `{ status: "version-refused", message }` — the installed Oxlint is outside
 *     the supported range, so the command must stop rather than skip rules;
 *   * `{ status: "active", ... }` — ready to run.
 */
export async function preparePluginLane({ cwd, files, viteConfig, explicitConfig }) {
  if (files.length === 0) return null;

  // The native lane resolves its configuration the way it always has: an
  // explicit `-c`, a resolved Vite+ config, or a walk up from the working
  // directory. That is the config whose `jsPlugins` would reach the Rust gate.
  const nativeSource = viteConfig
    ? { path: viteConfig.path, base: viteConfig.base, explicit: true, directory: viteConfig.base }
    : explicitConfig
      ? {
          path: resolve(cwd, explicitConfig),
          base: dirname(resolve(cwd, explicitConfig)),
          explicit: true,
        }
      : (() => {
          const discovered = findOxlintConfig(cwd);
          return discovered === null
            ? null
            : { path: discovered, base: dirname(discovered), explicit: false };
        })();

  // An explicit `-c` turns Oxlint's nested configuration off entirely, measured
  // against the pinned binary. Without one, each file is governed by the nearest
  // config at or above it, which is why this resolves per file rather than once.
  const configs = new Map();
  const laneFiles = [];
  let sawOptOut = false;
  for (const file of files) {
    const path = nativeSource?.explicit ? nativeSource.path : findOxlintConfig(dirname(file));
    if (path === null || path === undefined) continue;
    let entry = configs.get(path);
    if (entry === undefined) {
      const directoryOverride = path === nativeSource?.path ? (nativeSource.directory ?? null) : null;
      const facts = await collectLaneFacts(path, directoryOverride);
      entry = {
        path,
        config: facts.config,
        directory: directoryOverride ?? dirname(path),
        jsPlugins: facts.jsPlugins,
        // The gate in the native engine reads the top-level `jsPlugins` of the
        // config it loaded, so that is the field this lane has to remove.
        stripsNative: declaredJsPlugins(facts.config).length > 0,
        optedOut: facts.optedOut === true,
        files: [],
      };
      configs.set(path, entry);
    }
    if (entry.jsPlugins.length === 0) continue;
    if (entry.optedOut) {
      sawOptOut = true;
      continue;
    }
    entry.files.push(file);
    laneFiles.push(file);
  }

  const nativeConfigEntry = nativeSource === null ? null : configs.get(nativeSource.path);
  const nativeNeedsStrip = Boolean(
    nativeConfigEntry && nativeConfigEntry.stripsNative && !nativeConfigEntry.optedOut,
  );

  if (laneFiles.length === 0) {
    return sawOptOut ? { status: "opted-out" } : null;
  }

  const version = installedOxlintVersion();
  if (!laneSupportsOxlintVersion(version)) {
    return { status: "version-refused", message: oxlintVersionRefusal(version) };
  }

  const active = [...configs.values()].filter((entry) => entry.files.length > 0);
  const temporary = [];
  let nativeConfig = null;
  if (nativeNeedsStrip) {
    const directory = await mkdtemp(join(tmpdir(), "oxc-tsrx-native-config-"));
    temporary.push(directory);
    const path = join(directory, ".oxlintrc.json");
    await writeFile(path, `${JSON.stringify(nativeLaneConfig(nativeConfigEntry.config))}\n`);
    nativeConfig = {
      path,
      base: nativeSource.base,
      typeAware: viteConfig?.typeAware === true,
      typeCheck: viteConfig?.typeCheck === true,
    };
  }

  return {
    status: "active",
    files: laneFiles,
    fileCount: laneFiles.length,
    nativeConfig,
    notice: jsPluginDisclosure(laneFiles.length),
    async run() {
      return runPluginLane({ cwd, configs: active, nativeConfig, explicit: Boolean(nativeSource?.explicit), temporary });
    },
    async cleanup() {
      await Promise.all(
        temporary.map((directory) => rm(directory, { recursive: true, force: true })),
      );
    },
  };
}

async function emitProjections(cwd, files, nativeConfig) {
  const args = ["--emit-plugin-projection"];
  if (nativeConfig) args.push("--config", nativeConfig.path, "--config-base", nativeConfig.base);
  const command = resolveNativeCommand("lint", [...args, ...files]);
  const result = await runCaptured(command.executable, command.args, { cwd });
  if (result.status !== 0) {
    throw new Error(
      `the native TSRX projection needed for JS plugins failed:\n${result.stderr || result.stdout}`,
    );
  }
  let parsed;
  try {
    parsed = JSON.parse(result.stdout);
  } catch {
    throw new Error(
      `the native TSRX projection needed for JS plugins returned non-JSON output:\n${result.stdout}`,
    );
  }
  return Array.isArray(parsed.projections) ? parsed.projections : [];
}

async function mapDiagnostics(cwd, byFile) {
  const command = resolveNativeCommand("lint", ["--map-plugin-diagnostics"]);
  const request = JSON.stringify({
    files: [...byFile].map(([path, diagnostics]) => ({ path, diagnostics })),
  });
  const result = await runCaptured(command.executable, command.args, { cwd, input: request });
  if (result.status !== 0) {
    throw new Error(
      `mapping JS plugin diagnostics back to authored .tsrx positions failed:\n${result.stderr || result.stdout}`,
    );
  }
  let parsed;
  try {
    parsed = JSON.parse(result.stdout);
  } catch {
    throw new Error(
      `mapping JS plugin diagnostics back to authored .tsrx positions returned non-JSON output:\n${result.stdout}`,
    );
  }
  return Array.isArray(parsed.files) ? parsed.files : [];
}

function diagnosticNamespace(diagnostic) {
  const code = typeof diagnostic.code === "string" ? diagnostic.code : "";
  const open = code.indexOf("(");
  return open === -1 ? code : code.slice(0, open);
}

async function runPluginLane({ cwd, configs, nativeConfig, explicit, temporary }) {
  const laneFiles = configs.flatMap((entry) => entry.files);
  const projections = await emitProjections(cwd, laneFiles, nativeConfig);
  if (projections.length === 0) return { diagnostics: [], files: 0, extraParses: 0 };

  const mirror = await mkdtemp(join(tmpdir(), "oxc-tsrx-js-plugins-"));
  temporary.push(mirror);

  const authoredByMirrorPath = new Map();
  const mirrored = [];
  for (const projection of projections) {
    if (typeof projection?.path !== "string" || typeof projection.projected !== "string") continue;
    const relativePath = mirrorRelativePath(cwd, projection.path);
    await writeMirrorFile(mirror, relativePath, projection.projected);
    authoredByMirrorPath.set(relativePath, projection.path);
    mirrored.push(relativePath);
  }
  if (mirrored.length === 0) return { diagnostics: [], files: 0, extraParses: 0 };

  // Oxlint resolves the configuration itself, from copies sitting where it
  // expects to find them: an explicit `-c` becomes one config at the mirror
  // root, and a discovered one keeps its position relative to the working
  // directory so nested configs go on governing the same subtrees.
  const namespaces = new Set();
  let namespacesKnown = true;
  for (const entry of configs) {
    const projected = projectionConfig(entry.config, entry.directory);
    const relativeConfig = explicit
      ? ".oxlintrc.json"
      : (() => {
          const candidate = relative(cwd, entry.path);
          return candidate !== "" && !candidate.startsWith("..") && !isAbsolute(candidate)
            ? candidate
            : ".oxlintrc.json";
        })();
    await writeMirrorFile(mirror, relativeConfig, `${JSON.stringify(projected, null, 2)}\n`);
    entry.mirrorConfig = relativeConfig;
    const found = await pluginNamespaces(entry.jsPlugins);
    if (found === null) namespacesKnown = false;
    else for (const name of found) namespaces.add(name);
  }

  // No `--no-ignore` here, deliberately. The mirror is a fresh temporary
  // directory with nothing to ignore, and passing the flag changes what Oxlint
  // 1.74.0 puts in `context.filename`: with it, a rule sees a relative path;
  // without it, the absolute one it already sees on ordinary files.
  const oxlintBinary = resolvePackageBinary("oxlint-current", "oxlint", import.meta.url);
  const oxlintArgs = [oxlintBinary, "--format=json"];
  if (explicit) oxlintArgs.push("--config", configs[0].mirrorConfig);
  const result = await runCaptured(process.execPath, [...oxlintArgs, ...mirrored], {
    cwd: mirror,
    env: process.env,
  });
  if (result.status > 1) {
    throw new Error(
      `running your JS plugins over the .tsrx projection failed:\n${result.stderr || result.stdout}`,
    );
  }
  let report;
  try {
    report = JSON.parse(result.stdout);
  } catch {
    throw new Error(
      `running your JS plugins over the .tsrx projection returned non-JSON output:\n${result.stdout}${result.stderr}`,
    );
  }

  const byFile = new Map();
  for (const relativePath of mirrored) byFile.set(authoredByMirrorPath.get(relativePath), []);
  for (const diagnostic of report.diagnostics ?? []) {
    const authored = authoredByMirrorPath.get(diagnostic.filename);
    if (authored === undefined) continue;
    // A diagnostic with no rule code is a parse or semantic complaint about the
    // projection, and the native lane already owns those against the source the
    // user actually wrote. Anything left that is not one of this project's own
    // JavaScript plugins is a built-in rule the native lane also reports.
    const namespace = diagnosticNamespace(diagnostic);
    if (namespace === "") continue;
    if (namespacesKnown && !namespaces.has(namespace)) continue;
    byFile.get(authored).push(diagnostic);
  }

  const nonEmpty = new Map([...byFile].filter(([, diagnostics]) => diagnostics.length > 0));
  const diagnostics = [];
  if (nonEmpty.size > 0) {
    for (const file of await mapDiagnostics(cwd, nonEmpty)) {
      for (const diagnostic of file.diagnostics ?? []) {
        diagnostics.push({ ...diagnostic, filename: file.path });
      }
    }
  }
  return { diagnostics, files: mirrored.length, extraParses: mirrored.length };
}
