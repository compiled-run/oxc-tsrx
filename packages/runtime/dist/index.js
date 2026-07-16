import { spawn } from "node:child_process";
import { createRequire } from "node:module";
import { appendFileSync, existsSync, statSync } from "node:fs";
import { mkdtemp, readFile, rm, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, isAbsolute, join, parse, resolve, sep } from "node:path";
import { pathToFileURL } from "node:url";
import { glob } from "tinyglobby";
import { nativePackageName, nativeTargetForHost } from "./targets.js";

const require = createRequire(import.meta.url);
const runtimeManifest = require("../package.json");
const NATIVE_PROTOCOL_VERSION = 1;
const OXC_REVISION = "8e0ed2ebb96137fb1611cdbd5742d5cb46037d40";

const ENVIRONMENTS = {
  lint: "OXC_TSRX_LINT_BIN",
  format: "OXC_TSRX_FORMAT_BIN",
  server: "OXC_TSRX_LSP_BIN",
};

const EXECUTABLES = {
  lint: process.platform === "win32" ? "oxc-tsrx.exe" : "oxc-tsrx",
  format: process.platform === "win32" ? "oxc-tsrx-fmt.exe" : "oxc-tsrx-fmt",
  server: process.platform === "win32" ? "oxc-tsrx-lsp.exe" : "oxc-tsrx-lsp",
};

const VITE_CONFIG_FILES = [
  "vite.config.ts",
  "vite.config.mts",
  "vite.config.cts",
  "vite.config.js",
  "vite.config.mjs",
  "vite.config.cjs",
];

function linuxLibc() {
  if (process.platform !== "linux") return null;
  const report = process.report?.getReport?.();
  return report?.header?.glibcVersionRuntime ? "glibc" : "musl";
}

export function platformPackage() {
  return nativePackageName(nativeTargetForHost(process.platform, process.arch, linuxLibc()));
}

function assertExecutable(path, source) {
  let metadata;
  try {
    metadata = statSync(path);
  } catch {
    throw new Error(`OXC for TSRX native artifact is missing at ${path} (${source})`);
  }
  if (!metadata.isFile()) {
    throw new Error(`OXC for TSRX native artifact is not a file at ${path} (${source})`);
  }
  if (process.platform !== "win32" && (metadata.mode & 0o111) === 0) {
    throw new Error(`OXC for TSRX native artifact is not executable at ${path} (${source})`);
  }
  return path;
}

function validateNativeManifest(manifest, packageName, executable) {
  const metadata = manifest.oxcTsrx;
  if (manifest.version !== runtimeManifest.version) {
    throw new Error(
      `OXC for TSRX native package ${packageName} has version ${manifest.version}; ` +
        `runtime ${runtimeManifest.version} requires an exact match`,
    );
  }
  if (metadata?.nativeProtocolVersion !== NATIVE_PROTOCOL_VERSION) {
    throw new Error(
      `OXC for TSRX native package ${packageName} has unsupported protocol ` +
        `${metadata?.nativeProtocolVersion ?? "unknown"}; expected ${NATIVE_PROTOCOL_VERSION}`,
    );
  }
  const expectedTarget = nativeTargetForHost(
    process.platform,
    process.arch,
    linuxLibc(),
  ).target;
  if (metadata.target !== expectedTarget) {
    throw new Error(
      `OXC for TSRX native package ${packageName} targets ${metadata.target}; ` +
        `this process requires ${expectedTarget}`,
    );
  }
  if (metadata.oxcRevision !== OXC_REVISION) {
    throw new Error(
      `OXC for TSRX native package ${packageName} pins OXC ${metadata.oxcRevision}; ` +
        `runtime ${runtimeManifest.version} requires ${OXC_REVISION}`,
    );
  }
  if (!Array.isArray(metadata.binaries) || !metadata.binaries.includes(executable)) {
    throw new Error(
      `OXC for TSRX native package ${packageName} does not declare ${executable}`,
    );
  }
}

export function resolveNativeBinary(kind) {
  const environment = ENVIRONMENTS[kind];
  const executable = EXECUTABLES[kind];
  if (!environment || !executable) throw new Error(`unknown native binary kind: ${kind}`);

  const explicit = process.env[environment];
  if (explicit) return assertExecutable(resolve(explicit), environment);

  const packageName = platformPackage();
  let packageRoot;
  try {
    const manifestPath = require.resolve(`${packageName}/package.json`);
    const manifest = require(manifestPath);
    validateNativeManifest(manifest, packageName, executable);
    packageRoot = dirname(manifestPath);
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    throw new Error(
      `OXC for TSRX native package ${packageName} is unavailable; install it or set ${environment}. ${detail}`,
    );
  }
  return assertExecutable(join(packageRoot, "bin", executable), packageName);
}

export function resolvePackageBinary(packageName, binaryName, fromUrl) {
  const localRequire = createRequire(fromUrl);
  const main = localRequire.resolve(packageName);
  return join(dirname(dirname(main)), "bin", binaryName);
}

function findViteConfig(cwd) {
  let directory = resolve(cwd);
  const root = parse(directory).root;
  for (;;) {
    for (const file of VITE_CONFIG_FILES) {
      const candidate = join(directory, file);
      if (existsSync(candidate)) return candidate;
    }
    if (directory === root) return null;
    directory = dirname(directory);
  }
}

function moduleEntry(manifest) {
  const rootExport = manifest.exports?.["."];
  if (typeof rootExport === "string") return rootExport;
  if (rootExport && typeof rootExport === "object") {
    return rootExport.import ?? rootExport.default ?? rootExport.require;
  }
  return manifest.module ?? manifest.main;
}

function strictConfigJson(config, field) {
  const ancestors = [];
  return JSON.stringify(config, function serialize(key, value) {
    if (typeof value === "function" || typeof value === "symbol" || typeof value === "bigint") {
      throw new TypeError(
        `Vite+ ${field} config contains non-JSON value ${key || "<root>"}; the native TSRX lane requires serializable Oxlint/Oxfmt options`,
      );
    }
    if (value && typeof value === "object") {
      while (ancestors.length > 0 && ancestors.at(-1) !== this) ancestors.pop();
      if (ancestors.includes(value)) {
        throw new TypeError(`Vite+ ${field} config contains a circular object graph`);
      }
      ancestors.push(value);
    }
    return value;
  });
}

function requiresAuthoredConfigBase(field, config) {
  const fields =
    field === "lint"
      ? ["extends", "overrides", "ignorePatterns", "jsPlugins"]
      : ["overrides", "ignorePatterns"];
  return fields.some((name) => {
    const value = config[name];
    return Array.isArray(value) ? value.length > 0 : value !== undefined && value !== null;
  });
}

/**
 * Resolve Vite+'s public universal config once in the thin Node host and write only
 * the selected Oxlint/Oxfmt field to a disposable JSON file for the native process.
 */
export async function prepareVitePlusConfig(field, cwd = process.cwd(), explicitConfig = null) {
  const isVitePlus = Boolean(
    process.env.VP_VERSION ||
    process.env.VP_COMMAND ||
    process.env.NODE_PACKAGE_MANAGER === "vite-plus",
  );
  if (!isVitePlus) return null;
  const configFile = explicitConfig
    ? isAbsolute(explicitConfig)
      ? explicitConfig
      : resolve(cwd, explicitConfig)
    : findViteConfig(cwd);
  if (configFile === null) return null;

  const projectRequire = createRequire(join(resolve(cwd), "package.json"));
  const manifestPath = projectRequire.resolve("vite-plus/package.json");
  const packageRoot = dirname(manifestPath);
  const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
  const entry = moduleEntry(manifest);
  if (!entry) throw new Error("installed Vite+ package has no public module entry");
  const vitePlus = await import(pathToFileURL(resolve(packageRoot, entry)).href);
  if (typeof vitePlus.resolveConfig !== "function") {
    throw new Error("installed Vite+ package does not export public resolveConfig");
  }
  const resolved = await vitePlus.resolveConfig({ configFile }, "build");
  const selected = resolved[field] ?? {};
  if (!selected || typeof selected !== "object" || Array.isArray(selected)) {
    throw new TypeError(`Vite+ ${field} config must resolve to an object`);
  }

  const directory = await mkdtemp(join(tmpdir(), `oxc-tsrx-vite-plus-${field}-`));
  const filename = field === "lint" ? ".oxlintrc.json" : ".oxfmtrc.json";
  const path = join(directory, filename);
  try {
    await writeFile(path, `${strictConfigJson(selected, field)}\n`);
  } catch (error) {
    await rm(directory, { recursive: true, force: true });
    throw error;
  }
  return {
    path,
    source: configFile,
    base: dirname(configFile),
    requiresAuthoredBase: requiresAuthoredConfigBase(field, selected),
    typeAware: field === "lint" && selected.options?.typeAware === true,
    typeCheck: field === "lint" && selected.options?.typeCheck === true,
    async cleanup() {
      await rm(directory, { recursive: true, force: true });
    },
  };
}

export function isViteConfigPath(path) {
  if (!path) return false;
  return VITE_CONFIG_FILES.some((name) => path.endsWith(name));
}

export function replaceConfigArgument(args, configPath) {
  const output = [];
  for (let index = 0; index < args.length; index += 1) {
    const argument = args[index];
    if (argument === "-c" || argument === "--config") {
      index += 1;
      continue;
    }
    if (argument.startsWith("-c=") || argument.startsWith("--config=")) continue;
    output.push(argument);
  }
  const terminator = output.indexOf("--");
  const values = ["--config", configPath];
  if (terminator === -1) output.push(...values);
  else output.splice(terminator, 0, ...values);
  return output;
}

export function canonicalToolEnvironment(useResolvedViteConfig) {
  if (!useResolvedViteConfig) return process.env;
  const environment = { ...process.env };
  delete environment.VP_VERSION;
  return environment;
}

export function runCaptured(executable, args, options = {}) {
  return new Promise((resolveRun, rejectRun) => {
    const trace = process.env.OXC_TSRX_TRACE_FILE;
    const started = Date.now();
    if (trace) {
      appendFileSync(
        trace,
        `${JSON.stringify({
          event: "start",
          pid: process.pid,
          ppid: process.ppid,
          started,
          executable,
          args,
          host: {
            vpVersion: process.env.VP_VERSION ?? null,
            vpCommand: process.env.VP_COMMAND ?? null,
            packageManager: process.env.NODE_PACKAGE_MANAGER ?? null,
            tsgolint: process.env.OXLINT_TSGOLINT_PATH ?? null,
          },
        })}\n`,
      );
    }
    const child = spawn(executable, args, {
      cwd: options.cwd,
      env: options.env ?? process.env,
      stdio: ["pipe", "pipe", "pipe"],
    });
    let stdout = "";
    let stderr = "";
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => (stdout += chunk));
    child.stderr.on("data", (chunk) => (stderr += chunk));
    child.on("error", rejectRun);
    child.on("close", (status, signal) => {
      if (trace) {
        appendFileSync(
          trace,
          `${JSON.stringify({ event: "end", pid: process.pid, ppid: process.ppid, started, ended: Date.now(), executable, args, status, signal })}\n`,
        );
      }
      resolveRun({ status: status ?? 2, signal, stdout, stderr });
    });
    if (options.input === undefined) child.stdin.end();
    else child.stdin.end(options.input);
  });
}

export function positionalIndices(args, valueOptions) {
  const indices = [];
  let positionalOnly = false;
  for (let index = 0; index < args.length; index += 1) {
    const argument = args[index];
    if (positionalOnly) {
      indices.push(index);
      continue;
    }
    if (argument === "--") {
      positionalOnly = true;
      continue;
    }
    if (!argument.startsWith("-") || argument === "-") {
      indices.push(index);
      continue;
    }
    if (!argument.includes("=") && valueOptions.has(argument)) index += 1;
  }
  return indices;
}

export function removeExplicitTsrx(args, valueOptions) {
  const positions = positionalIndices(args, valueOptions);
  const removed = new Set(positions.filter((index) => args[index].split("?")[0].endsWith(".tsrx")));
  return {
    args: args.filter((_, index) => !removed.has(index)),
    hadPositionals: positions.length > 0,
    remainingPositionals: positions.length - removed.size,
  };
}

function slash(path) {
  return sep === "/" ? path : path.split(sep).join("/");
}

function hasMagic(path) {
  return /[*?[\]{}()!]/u.test(path);
}

async function classifyPattern(raw, cwd, positives, patterns) {
  const negative = raw.startsWith("!");
  const value = negative ? raw.slice(1) : raw;
  const absolute = isAbsolute(value) ? value : resolve(cwd, value);
  if (!hasMagic(value)) {
    try {
      const metadata = await stat(absolute);
      if (metadata.isFile()) {
        if (!negative && absolute.endsWith(".tsrx")) positives.add(absolute);
        else if (negative) patterns.push(`!${slash(absolute)}`);
        return;
      }
      if (metadata.isDirectory()) {
        patterns.push(`${negative ? "!" : ""}${slash(join(absolute, "**/*.tsrx"))}`);
        return;
      }
    } catch {
      // Let tinyglobby decide whether a non-literal pattern is unmatched.
    }
  }
  patterns.push(`${negative ? "!" : ""}${slash(value)}`);
}

export async function discoverTsrxFiles(positionals, cwd = process.cwd()) {
  const positives = new Set();
  const patterns = [];
  const inputs = positionals.length === 0 ? ["."] : positionals;
  for (const input of inputs) await classifyPattern(input, cwd, positives, patterns);
  if (patterns.length > 0) {
    const matches = await glob(patterns, {
      cwd,
      absolute: true,
      onlyFiles: true,
      dot: true,
      followSymbolicLinks: false,
      ignore: ["**/node_modules/**", "**/.git/**"],
    });
    for (const match of matches) if (match.endsWith(".tsrx")) positives.add(resolve(match));
  }
  return [...positives].sort();
}

export function replaceOutputFormat(args, valueOptions, format) {
  const output = [];
  for (let index = 0; index < args.length; index += 1) {
    const argument = args[index];
    if (argument === "--format" || argument === "-f") {
      index += 1;
      continue;
    }
    if (argument.startsWith("--format=") || argument.startsWith("-f=")) continue;
    output.push(argument);
  }
  const terminator = output.indexOf("--");
  if (terminator === -1) output.push(`--format=${format}`);
  else output.splice(terminator, 0, `--format=${format}`);
  return output;
}

export function requestedOutputFormat(args) {
  for (let index = 0; index < args.length; index += 1) {
    const argument = args[index];
    if (argument === "--format" || argument === "-f") return args[index + 1] ?? null;
    if (argument.startsWith("--format=")) return argument.slice("--format=".length);
    if (argument.startsWith("-f=")) return argument.slice(3);
  }
  return "default";
}

export function argumentValue(args, names) {
  for (let index = 0; index < args.length; index += 1) {
    const argument = args[index];
    if (names.has(argument)) return args[index + 1] ?? null;
    for (const name of names) {
      if (argument.startsWith(`${name}=`)) return argument.slice(name.length + 1);
    }
  }
  return null;
}

export function ensureSupportedOutput(format, files) {
  if (files.length > 0 && format !== "default" && format !== "json") {
    throw new Error(
      `OXC for TSRX currently combines default and json lint output; ${format} is unavailable for mixed .tsrx runs`,
    );
  }
}

export function pathExists(path) {
  return existsSync(path);
}
