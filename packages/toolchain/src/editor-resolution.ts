import { posix, win32 } from "node:path";

/**
 * What would the editor actually run?
 *
 * `setup` writes one key, `oxc.path.oxlint`, and then hopes. This module removes
 * the hoping. It replays the binary lookup that the official OXC extension
 * (`oxc.oxc-vscode` 1.59.0, `out/main.js`) performs, over a *fixture* tree that
 * the caller injects, so a test can ask "for this workspace topology, which
 * linter would the extension spawn, and does that binary know about `.tsrx`?"
 * without an install, an editor, or a spawned process.
 *
 * Everything that touches the outside world is a parameter. There is no
 * `vscode` import, no `node:fs`, no `child_process`, no network. The only
 * capability this module has is the `stat` function it is handed.
 *
 * The chain being replayed, verbatim from the extension:
 *
 *     async searchBinaryPath(e,t){return e?se(t,e):await M(t)??await ie(t)??await ae(t)??await oe(t)}
 *
 * The single most consequential fact in that line is the `?:`. A configured
 * value does not *win* the lookup, it *replaces* it. When `oxc.path.oxlint` is
 * set and does not resolve, the extension stops: no `node_modules/.bin`, no
 * `PATH`, no linter at all. A stale key is strictly worse than no key, which is
 * why every failure here still reports `source: "configured"` rather than
 * falling through.
 *
 * Deliberate divergence from `compat.js`'s `inspectLinterShim`: that function
 * reads `oxlint.cmd` and `oxlint.ps1` on Windows because that is what npm and
 * pnpm write. This one does not, because the extension does not. `j()` builds
 * exactly `<name>` and `<name>.exe`, so on Windows a tree whose only shim is
 * `oxlint.cmd` reads to the extension as a tree with no shim at all.
 */

/**
 * `T(e)` in the extension. Rejects a configured value outright, before any
 * filesystem access. `..` is checked as a substring, not as a path segment, so
 * a directory named `my..dir` is rejected too. `.\` is the two-character
 * sequence dot-backslash.
 */
const CONFIGURED_METACHARACTERS = Object.freeze([
  "$",
  "&",
  ";",
  "|",
  "`",
  ">",
  "<",
  "!",
  "%",
  "^",
]);

/** Yarn PnP loaders, in the order `re()` probes them. */
const PNP_LOADERS = Object.freeze([".pnp.cjs", ".pnp.js"]);

const TSRX_EXTENSION = ".tsrx";

function pathFor(platform) {
  return platform === "win32" ? win32 : posix;
}

/**
 * Normalise whatever the injected `stat` returned into one shape, or `null` when
 * the path does not exist. A fixture stat returns `{content, realPath}`; a
 * real-filesystem stat may return a `Stats` object or simply `true`, in which
 * case there is no content to read and `realPath` is the path itself. Throwing
 * counts as absent, which is how the extension's `try { await fs.stat } catch`
 * behaves.
 */
async function statEntry(stat, path) {
  let result;
  try {
    result = await stat(path);
  } catch {
    return null;
  }
  if (!result) return null;
  return {
    path,
    realPath: typeof result.realPath === "string" ? result.realPath : path,
    content: typeof result.content === "string" ? result.content : null,
  };
}

async function readPackageSource(context, directory) {
  const entry = await statEntry(context.stat, context.path.join(directory, "package.json"));
  return entry?.content ?? null;
}

/**
 * `A(e,t)` in the extension: given a file inside a package, walk up to that
 * package's `package.json` and resolve its `bin` entry for `name`.
 *
 * Two failure modes are load-bearing and both throw, exactly as the extension
 * does, because every caller wraps this in a `try` and treats a throw as "this
 * step of the chain found nothing": a `package.json` with no matching `bin`
 * entry, and a tree with no `package.json` at all. `JSON.parse` is outside the
 * read guard for the same reason it is in the extension: a malformed manifest
 * aborts the walk rather than being skipped over.
 */
async function resolvePackageBin(context, file, name) {
  let directory = context.path.dirname(file);
  while (directory !== context.path.dirname(directory)) {
    const source = await readPackageSource(context, directory);
    if (source === null) {
      directory = context.path.dirname(directory);
      continue;
    }
    const manifest = JSON.parse(source);
    const bin = typeof manifest.bin === "string" ? manifest.bin : manifest.bin?.[name];
    if (!bin) throw new Error(`No bin entry for "${name}" found in package.json`);
    return context.path.resolve(directory, bin);
  }
  throw new Error(`Could not find package.json for "${name}"`);
}

/**
 * `j(e,t)` in the extension. `t` is a list of `node_modules` directories; each
 * contributes `<dir>/.bin/<name>` and, on Windows only, `<dir>/.bin/<name>.exe`.
 * The extension stats every candidate in parallel and takes the first index that
 * succeeded, which is the same answer this sequential walk gives.
 *
 * Note `loader: "native"`. A `.bin` hit is never treated as a Node script, even
 * when it is one, so on Windows an extensionless shim here is unspawnable for
 * the same reason a configured extensionless path is.
 */
async function searchBinDirectories(context, moduleDirectories, source) {
  for (const directory of moduleDirectories) {
    const base = context.path.join(directory, ".bin", context.name);
    const candidates = context.platform === "win32" ? [base, `${base}.exe`] : [base];
    for (const candidate of candidates) {
      const entry = await statEntry(context.stat, candidate);
      if (entry) return { source, path: candidate, loader: "native", entry };
    }
  }
  return null;
}

/** `M(e)`: workspace `.bin`, then globbed-package `.bin`, then `require.resolve`. */
async function searchWorkspace(context) {
  const workspaceModules = context.workspaceFolders.map((folder) =>
    context.path.join(folder, "node_modules"),
  );
  const direct = await searchBinDirectories(context, workspaceModules, "workspace-node-modules");
  if (direct) return direct;

  // `te()`: findFiles("**/package.json", "**/node_modules/**") mapped to
  // dirname + "/node_modules". The caller injects the directories the glob
  // found, so nested projects reach the chain even when the workspace root
  // itself has no `node_modules`.
  const globbed = context.packageJsonDirectories.map((directory) =>
    context.path.join(directory, "node_modules"),
  );
  const nested = await searchBinDirectories(context, globbed, "package-json-node-modules");
  if (nested) return nested;

  if (typeof context.requireResolve !== "function") return null;
  try {
    const resolved = context.requireResolve(context.name, { paths: context.workspaceFolders });
    const binary = await resolvePackageBin(context, resolved, context.name);
    // The extension does not stat this result; it trusts the manifest. The stat
    // here only recovers the real path for the `tsrxAware` question and never
    // changes which binary is chosen.
    return {
      source: "require-resolve",
      path: binary,
      loader: "node",
      entry: await statEntry(context.stat, binary),
    };
  } catch {
    return null;
  }
}

/** `re(e)`: walk up looking for a PnP API, `.pnp.cjs` before `.pnp.js`. */
function findPnpApi(context, folder) {
  if (!context.pnp) return null;
  let directory = folder;
  for (;;) {
    for (const loader of PNP_LOADERS) {
      const loaderPath = context.path.join(directory, loader);
      const api =
        typeof context.pnp === "function" ? context.pnp(loaderPath) : context.pnp[loaderPath];
      if (api && typeof api.resolveRequest === "function") return { api, loaderPath };
    }
    const parent = context.path.dirname(directory);
    if (parent === directory) return null;
    directory = parent;
  }
}

/**
 * `ie(e)`: Yarn PnP. Guarded by workspace trust, so an untrusted window loses
 * PnP resolution as well as the configured value.
 */
async function searchYarnPnp(context) {
  if (!context.trusted) return null;
  for (const folder of context.workspaceFolders) {
    const discovered = findPnpApi(context, folder);
    if (!discovered) continue;
    try {
      const request = discovered.api.resolveRequest(context.name, folder + context.path.sep);
      if (!request) continue;
      const binary = await resolvePackageBin(context, request, context.name);
      const entry = await statEntry(context.stat, binary);
      if (!entry) continue;
      return {
        source: "yarn-pnp",
        path: binary,
        loader: "node",
        entry,
        yarnPnpLoaderPath: discovered.loaderPath,
      };
    } catch {
      continue;
    }
  }
  return null;
}

/**
 * `ae(e)`: the global roots. These are already `node_modules` directories
 * (`npm root -g`, `pnpm root -g`, `~/.bun/install/global/node_modules`), so
 * `.bin` is appended directly rather than `node_modules/.bin`.
 */
async function searchGlobalRoots(context) {
  const found = await searchBinDirectories(context, context.globalRoots, "global-node-modules");
  if (found) return found;
  if (typeof context.requireResolve !== "function") return null;
  try {
    const resolved = context.requireResolve(context.name, { paths: context.globalRoots });
    const binary = await resolvePackageBin(context, resolved, context.name);
    return {
      source: "global-require-resolve",
      path: binary,
      loader: "node",
      entry: await statEntry(context.stat, binary),
    };
  } catch {
    return null;
  }
}

/** `oe(e)`: the last resort, `PATH`, with the same `<name>` / `<name>.exe` pair. */
async function searchPathEntries(context) {
  for (const directory of context.pathEntries) {
    if (!directory) continue;
    const base = context.path.join(directory, context.name);
    const candidates = context.platform === "win32" ? [base, `${base}.exe`] : [base];
    for (const candidate of candidates) {
      const entry = await statEntry(context.stat, candidate);
      if (entry) return { source: "path", path: candidate, loader: "native", entry };
    }
  }
  return null;
}

/**
 * The loader test from `se()`, and the only place the extension infers a loader
 * from the shape of a path. It matters that `<name>/bin/<name>` is checked with
 * the *package* name: the value `setup` writes ends in `oxc-tsrx/bin/oxlint`,
 * not `oxlint/bin/oxlint`, so it classifies as `native` and takes the shell
 * spawn route on Windows.
 */
export function configuredLoader(name, value, platform) {
  const path = pathFor(platform);
  const nodeScript =
    value.endsWith(".js") ||
    value.endsWith(".cjs") ||
    value.endsWith(".mjs") ||
    value.endsWith(`${name}${path.sep}bin${path.sep}${name}`);
  return nodeScript ? "node" : "native";
}

/**
 * `T(e)`. Returns the reason a configured value is refused, or `null` when it is
 * acceptable. Split into two reasons because the remedies differ: a traversal
 * means "this value can never work, there is no relative escape from the
 * project", while a metacharacter usually means a path that simply needs
 * quoting out of existence.
 */
export function rejectConfiguredValue(value) {
  if (value.includes("..") || value.includes(".\\")) return "configured-rejected-traversal";
  for (const character of CONFIGURED_METACHARACTERS) {
    if (value.includes(character)) return "configured-rejected-metacharacter";
  }
  return null;
}

/**
 * Would the resolved command actually start?
 *
 * The extension spawns a `node` loader through `process.execPath`, but a
 * `native` loader through:
 *
 *     {command: isWin32 ? `"${e.path}"` : e.path, args:['--lsp'], options:{shell: isWin32}}
 *
 * `shell: true` on Windows is `cmd.exe`, which can only execute `.exe`, `.com`,
 * `.bat` and `.cmd`. An extensionless file is not one of those, so the spawn
 * fails and the editor goes quiet. The one escape hatch is `oxc.useExecPath`,
 * which forces the `node` route regardless of loader; that setting is not
 * modelled here because it is not something `setup` can write, and every value
 * this oracle judges must stand on its own.
 */
export function isSpawnable(path, loader, platform) {
  if (!path) return false;
  if (platform !== "win32") return true;
  if (loader !== "native") return true;
  return win32.extname(path) !== "";
}

function claimsTsrx(manifest) {
  const languages = manifest?.oxc?.provider?.languages;
  if (!Array.isArray(languages)) return false;
  return languages.some(
    (language) =>
      Array.isArray(language?.extensions) &&
      language.extensions.some(
        (extension) => String(extension).toLowerCase() === TSRX_EXTENSION,
      ),
  );
}

/**
 * Does the binary that will actually run belong to a package that understands
 * `.tsrx`?
 *
 * The question is asked of the *real* path, because the interesting failure is a
 * `node_modules/.bin/oxlint` that exists, stats fine, and is a text shim for
 * Vite+. Resolution succeeds and diagnostics never appear. Only the nearest
 * enclosing `package.json` is consulted: walking further up would eventually
 * reach the consumer's own manifest, which is not the package being run.
 */
async function isTsrxAware(context, file) {
  if (!file) return false;
  let directory = context.path.dirname(file);
  for (;;) {
    const source = await readPackageSource(context, directory);
    if (source !== null) {
      try {
        return claimsTsrx(JSON.parse(source));
      } catch {
        return false;
      }
    }
    const parent = context.path.dirname(directory);
    if (parent === directory) return false;
    directory = parent;
  }
}

function unresolved(source, reason, attempted = null): any {
  return {
    source,
    path: null,
    loader: null,
    spawnable: false,
    tsrxAware: false,
    reason,
    realPath: null,
    attempted,
  };
}

async function describe(context, found) {
  const realPath = found.entry?.realPath ?? found.path;
  const result: any = {
    source: found.source,
    path: found.path,
    loader: found.loader,
    spawnable: isSpawnable(found.path, found.loader, context.platform),
    tsrxAware: await isTsrxAware(context, realPath),
    reason: "resolved",
    realPath,
    attempted: found.path,
  };
  if (found.yarnPnpLoaderPath) result.yarnPnpLoaderPath = found.yarnPnpLoaderPath;
  return result;
}

/**
 * `se(e,t)`: the configured value.
 *
 * Order is the whole point and it is not the order anyone assumes. Trust first,
 * then character validation, then, only for a relative value, the join onto
 * `workspaceFolders[0]`. That first folder is whichever folder happens to be
 * first in the window, not the folder holding the `settings.json` the value was
 * written into and not the folder owning the open document, so the same relative
 * value that works in a single-folder window silently addresses the wrong tree
 * in a multi-root one.
 */
async function resolveConfigured(context, configured) {
  if (!context.trusted) return unresolved("configured", "untrusted-workspace", configured);
  const rejection = rejectConfiguredValue(configured);
  if (rejection) return unresolved("configured", rejection, configured);

  let value = configured;
  if (!context.path.isAbsolute(value)) {
    const first = context.workspaceFolders[0];
    if (!first) return unresolved("configured", "no-workspace-folder", configured);
    value = context.path.normalize(context.path.join(first, value));
  }
  if (context.platform !== "win32" && value.endsWith(".exe")) value = value.slice(0, -4);

  const loader = configuredLoader(context.name, value, context.platform);
  const entry = await statEntry(context.stat, value);
  if (entry) return describe(context, { source: "configured", path: value, loader, entry });

  if (context.platform === "win32") {
    const withExe = value.endsWith(".exe") ? value : `${value}.exe`;
    const exeEntry = await statEntry(context.stat, withExe);
    if (exeEntry) {
      return describe(context, {
        source: "configured",
        path: withExe,
        loader: "native",
        entry: exeEntry,
      });
    }
  }
  return unresolved("configured", "configured-missing", value);
}

/**
 * Resolve the linter binary the official extension would run.
 *
 * @param {object} options
 * @param {string} options.name Binary name the extension is looking for, `oxlint`.
 * @param {string|null} [options.configured] The `oxc.path.oxlint` value, if any.
 *   An empty string is not a value: the extension's `e ? ... : ...` treats it as
 *   unset and auto-detects.
 * @param {string[]} [options.workspaceFolders] Absolute workspace folder paths,
 *   in window order. `workspaceFolders[0]` anchors relative configured values.
 * @param {string[]} [options.packageJsonDirectories] Directories containing a
 *   `package.json`, as the extension's workspace-wide `package.json` glob would
 *   find them (that glob excludes `node_modules`).
 * @param {(request: string, options: {paths: string[]}) => string} [options.requireResolve]
 *   Stand-in for `require.resolve`. Throw to mean "not resolvable".
 * @param {object|((loaderPath: string) => object|null)} [options.pnp] Yarn PnP
 *   APIs keyed by loader path, or a lookup function. Each API needs
 *   `resolveRequest(request, issuer)`.
 * @param {string[]} [options.globalRoots] Global `node_modules` roots.
 * @param {string[]|string} [options.pathEntries] `PATH`, split or raw.
 * @param {NodeJS.Platform} [options.platform]
 * @param {boolean} [options.trusted] Workspace trust. Untrusted disables both the
 *   configured value and Yarn PnP.
 * @param {(path: string) => unknown} options.stat The only capability this
 *   module has. Return a falsy value or throw for "absent". Return an object to
 *   mean "present"; `realPath` names the file that will really execute (a
 *   symlink target, or the binary a text shim delegates to) and `content` lets
 *   `package.json` be read through the same seam.
 * @returns {Promise<{source: string|null, path: string|null, loader: string|null,
 *   spawnable: boolean, tsrxAware: boolean, reason: string, realPath: string|null,
 *   attempted: string|null, yarnPnpLoaderPath?: string}>}
 */
export async function resolveEditorLinter({
  name,
  configured = null,
  workspaceFolders = [],
  packageJsonDirectories = [],
  requireResolve = null,
  pnp = null,
  globalRoots = [],
  pathEntries = [],
  platform = process.platform,
  trusted = true,
  stat,
}: any = {}) {
  if (typeof name !== "string" || name.length === 0) {
    throw new TypeError("resolveEditorLinter requires a binary name");
  }
  if (typeof stat !== "function") {
    throw new TypeError("resolveEditorLinter requires an injected stat function");
  }

  const path = pathFor(platform);
  const context = {
    name,
    path,
    platform,
    trusted,
    stat,
    workspaceFolders,
    packageJsonDirectories,
    requireResolve,
    pnp,
    globalRoots,
    pathEntries:
      typeof pathEntries === "string" ? pathEntries.split(path.delimiter) : pathEntries,
  };

  // `e ? se(t,e) : ...` — a configured value replaces the chain, it does not
  // join it. Every return below this line is final.
  if (configured) return resolveConfigured(context, configured);

  const found =
    (await searchWorkspace(context)) ??
    (await searchYarnPnp(context)) ??
    (await searchGlobalRoots(context)) ??
    (await searchPathEntries(context));
  if (!found) return unresolved(null, "not-found");
  return describe(context, found);
}

function fixturePathFor(value) {
  return /^[A-Za-z]:[\\/]/u.test(value) || value.includes("\\") ? win32 : posix;
}

function fixtureKey(value) {
  return fixturePathFor(value).normalize(value);
}

/**
 * Build a `stat` over a plain object, so a test can state a workspace topology
 * as data instead of as a script.
 *
 * Each key is an absolute path. Values may be:
 *   - a string, the file's contents;
 *   - `{content}`, the same thing spelled out;
 *   - `{link}`, a symlink, resolved against the entry's own directory and
 *     followed, which is how npm, Yarn and Bun write `node_modules/.bin`;
 *   - `{runs}`, an explicit real path for a file that *is* a real file but
 *     executes something else, which is how pnpm writes `node_modules/.bin` and
 *     the exact shape that makes a `.bin/oxlint` collision invisible to a plain
 *     existence check;
 *   - `{directory: true}`, an empty directory.
 *
 * Ancestor directories are implied, and stat succeeds on them, because
 * `workspace.fs.stat` does not care what kind of node it found either.
 */
export function createFixtureStat(entries) {
  const files = new Map();
  const directories = new Set();

  for (const [rawPath, value] of Object.entries(entries)) {
    const key = fixtureKey(rawPath);
    const path = fixturePathFor(key);
    const descriptor: any = typeof value === "string" ? { content: value } : { ...(value as any) };
    if (descriptor.directory) directories.add(key);
    else files.set(key, descriptor);
    let directory = path.dirname(key);
    while (!directories.has(directory)) {
      directories.add(directory);
      const parent = path.dirname(directory);
      if (parent === directory) break;
      directory = parent;
    }
  }

  function follow(key, seen) {
    const descriptor = files.get(key);
    if (!descriptor) return null;
    const path = fixturePathFor(key);
    const next = descriptor.link
      ? fixtureKey(path.resolve(path.dirname(key), descriptor.link))
      : descriptor.runs
        ? fixtureKey(descriptor.runs)
        : null;
    if (next === null || seen.has(next)) {
      return { realPath: key, content: descriptor.content ?? null };
    }
    seen.add(next);
    // A text shim is a real file with its own contents; a symlink is not, so it
    // reports the target's contents, and a dangling one does not exist at all.
    const target = follow(next, seen);
    if (!target) {
      return descriptor.link ? null : { realPath: next, content: descriptor.content ?? null };
    }
    return {
      realPath: target.realPath,
      content: descriptor.link ? target.content : (descriptor.content ?? target.content),
    };
  }

  return function stat(candidate) {
    const key = fixtureKey(candidate);
    if (files.has(key)) return follow(key, new Set([key]));
    if (directories.has(key)) return { realPath: key, content: null };
    return null;
  };
}
