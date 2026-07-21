import { readFileSync, realpathSync, statSync } from "node:fs";
import {
  basename,
  delimiter as hostPathDelimiter,
  dirname,
  extname,
  isAbsolute,
  join,
  relative,
  resolve,
} from "node:path";

const JAVASCRIPT_EXTENSIONS = new Set([".cjs", ".js", ".mjs"]);

function fileRealpath(path) {
  try {
    const resolved = realpathSync(path);
    return statSync(resolved).isFile() ? resolved : null;
  } catch {
    return null;
  }
}

function isWithin(directory, path) {
  const child = relative(directory, path);
  return child === "" || (!child.startsWith("..") && !isAbsolute(child));
}

function isJavaScriptEntry(path) {
  const name = basename(path);
  if (name.startsWith(".")) return false;
  const extension = extname(name).toLowerCase();
  if (JAVASCRIPT_EXTENSIONS.has(extension)) return true;
  if (extension) return false;
  try {
    const [shebang] = readFileSync(path, "utf8").slice(0, 256).split(/\r?\n/u, 1);
    return /^#![\t ]*(?:(?:\S*[\\/])?node(?:\.exe)?|(?:\S*[\\/])?env(?:[\t ]+-S)?[\t ]+node(?:\.exe)?)(?:[\t ]|$)/iu.test(
      shebang,
    );
  } catch {
    return false;
  }
}

function declaredNpmEntry(manifestPath) {
  const resolvedManifest = fileRealpath(manifestPath);
  if (!resolvedManifest) return null;

  let manifest;
  try {
    manifest = JSON.parse(readFileSync(resolvedManifest, "utf8"));
  } catch {
    return null;
  }
  const declared = typeof manifest.bin === "string" ? manifest.bin : manifest.bin?.npm;
  if (
    manifest.name !== "npm" ||
    typeof manifest.version !== "string" ||
    manifest.version.length === 0 ||
    typeof declared !== "string" ||
    declared.length === 0
  ) {
    return null;
  }

  const packageRoot = dirname(resolvedManifest);
  const entry = fileRealpath(resolve(packageRoot, declared));
  if (!entry || !isWithin(packageRoot, entry) || !isJavaScriptEntry(entry)) return null;
  return entry;
}

function manifestForEntry(entryPath) {
  const entry = fileRealpath(entryPath);
  if (!entry || !isJavaScriptEntry(entry)) return null;

  let directory = dirname(entry);
  while (true) {
    const declared = declaredNpmEntry(join(directory, "package.json"));
    if (declared === entry) return entry;
    const parent = dirname(directory);
    if (parent === directory) return null;
    directory = parent;
  }
}

function pathValue(env) {
  const key = Object.keys(env).find((candidate) => candidate.toLowerCase() === "path");
  return key ? env[key] : undefined;
}

function nodeDirectories(nodeExecutable) {
  const directories = new Set([dirname(resolve(nodeExecutable))]);
  try {
    directories.add(dirname(realpathSync(nodeExecutable)));
  } catch {
    // The caller still gets a useful resolution error after the layout candidates are exhausted.
  }
  return directories;
}

/**
 * Resolve npm from its public package manifest and run its declared JavaScript CLI with Node.
 * Platform shell shims (`npm.cmd`, `npm`, and friends) are never parsed or executed.
 */
export function resolveNpmInvocation(
  args,
  {
    nodeExecutable = process.execPath,
    env = process.env,
    cwd = process.cwd(),
    pathDelimiter = hostPathDelimiter,
  } = {},
) {
  if (!Array.isArray(args) || args.some((argument) => typeof argument !== "string")) {
    throw new TypeError("npm arguments must be an array of strings");
  }
  if (typeof nodeExecutable !== "string" || !isAbsolute(nodeExecutable)) {
    throw new TypeError("nodeExecutable must be an absolute path");
  }
  if (!env || typeof env !== "object") throw new TypeError("env must be an object");

  const explicitEntry =
    typeof env.npm_execpath === "string" && env.npm_execpath.length > 0
      ? manifestForEntry(resolve(cwd, env.npm_execpath))
      : null;
  if (explicitEntry) {
    return { executable: nodeExecutable, args: [explicitEntry, ...args] };
  }

  for (const nodeDirectory of nodeDirectories(nodeExecutable)) {
    for (const manifestPath of [
      join(nodeDirectory, "node_modules/npm/package.json"),
      join(nodeDirectory, "../node_modules/npm/package.json"),
      join(nodeDirectory, "../lib/node_modules/npm/package.json"),
    ]) {
      const entry = declaredNpmEntry(manifestPath);
      if (entry) return { executable: nodeExecutable, args: [entry, ...args] };
    }
  }

  const searchPath = pathValue(env);
  if (typeof searchPath === "string") {
    for (const directory of searchPath.split(pathDelimiter)) {
      if (!directory) continue;
      const entry = manifestForEntry(join(directory, "npm"));
      if (entry) return { executable: nodeExecutable, args: [entry, ...args] };
    }
  }

  throw new Error(
    "could not resolve npm's manifest-declared JavaScript entry from npm_execpath, the Node installation, or a PATH launcher symlink",
  );
}
