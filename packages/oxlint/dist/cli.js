import { readFile } from "node:fs/promises";
import { relative } from "node:path";
import {
  argumentValue,
  canonicalToolEnvironment,
  discoverTsrxFiles,
  ensureSupportedOutput,
  isViteConfigPath,
  prepareVitePlusConfig,
  removeExplicitTsrx,
  replaceConfigArgument,
  requestedOutputFormat,
  resolveNativeBinary,
  resolvePackageBinary,
  runCaptured,
  runPassthrough,
} from "@oxc-tsrx/runtime";
import {
  DELEGATE_ONLY,
  VALUE_OPTIONS,
  parseOxlintInvocation,
  parseOxlintOption,
  withOxlintOutputFormat,
} from "./invocation.js";

// Mixed invocations need captured JSON so the canonical and TSRX diagnostics
// can be combined. Run the public, manifest-declared JavaScript launcher via
// Node: this is stable across package layout changes and Windows does not have
// to execute a POSIX shebang. Ordinary-only executable invocations never reach
// this path; the lightweight front router imports the launcher in-process.
async function runUpstreamOxlint(binary, args, options) {
  return runCaptured(process.execPath, [binary, ...args], options);
}

const NATIVE_VALUE_OPTIONS = new Map([
  ["-c", "--config"],
  ["--config", "--config"],
  ["-A", "--allow"],
  ["--allow", "--allow"],
  ["-W", "--warn"],
  ["--warn", "--warn"],
  ["-D", "--deny"],
  ["--deny", "--deny"],
]);
const WRAPPER_OPTIONS = new Set([
  "--quiet",
  "--silent",
  "--deny-warnings",
  "--no-ignore",
  "--no-error-on-unmatched-pattern",
]);

function nativeArguments(args, files, resolvedConfig) {
  const output = [];
  let positionalOnly = false;
  for (let index = 0; index < args.length; index += 1) {
    const argument = args[index];
    if (positionalOnly) continue;
    if (argument === "--") {
      positionalOnly = true;
      continue;
    }
    if (!argument.startsWith("-") || argument === "-") continue;
    const { name, value: inlineValue } = parseOxlintOption(argument);
    if (NATIVE_VALUE_OPTIONS.has(name)) {
      const value = inlineValue ?? args[++index];
      if (!value) throw new Error(`${name} requires a value`);
      if (resolvedConfig && (name === "-c" || name === "--config")) continue;
      output.push(NATIVE_VALUE_OPTIONS.get(name), value);
      continue;
    }
    if (name === "--fix") {
      output.push("--fix");
      continue;
    }
    if (name === "--type-aware" || name === "--type-check") {
      output.push(name);
      continue;
    }
    if (name === "--format" || name === "-f") {
      if (inlineValue === null) index += 1;
      continue;
    }
    if (name === "--threads" || name === "--max-warnings") {
      if (inlineValue === null) index += 1;
      continue;
    }
    if (WRAPPER_OPTIONS.has(name)) continue;
    throw new Error(
      `${name} is not yet supported for .tsrx by the drop-in Oxlint command; canonical Oxlint still handles ordinary files`,
    );
  }
  if (resolvedConfig) {
    output.push("--config", resolvedConfig.path, "--config-base", resolvedConfig.base);
    if (resolvedConfig.typeCheck && !output.includes("--type-check")) {
      output.push("--type-check");
    } else if (resolvedConfig.typeAware && !output.includes("--type-aware")) {
      output.push("--type-aware");
    }
  }
  return [...output, "--format=json", ...files];
}

export function resolveOxlintBytePositions(bytes, byteOffsets, filename = "<source>") {
  const offsets = [...new Set(byteOffsets)];
  for (const byteOffset of offsets) {
    if (!Number.isSafeInteger(byteOffset) || byteOffset < 0 || byteOffset > bytes.length) {
      throw new Error(`invalid diagnostic byte offset ${byteOffset} for ${filename}`);
    }
    if (byteOffset < bytes.length && (bytes[byteOffset] & 0xc0) === 0x80) {
      throw new Error(`diagnostic byte offset ${byteOffset} splits UTF-8 in ${filename}`);
    }
  }

  const positions = new Map();
  const pending = new Set(offsets);
  let line = 1;
  let column = 1;
  let previousWasCarriageReturn = false;
  for (let cursor = 0; cursor <= bytes.length && pending.size > 0; cursor += 1) {
    if (pending.delete(cursor)) positions.set(cursor, { line, column });
    if (cursor === bytes.length) break;
    const byte = bytes[cursor];
    if (byte === 0x0d) {
      line += 1;
      column = 1;
      previousWasCarriageReturn = true;
    } else if (byte === 0x0a) {
      if (!previousWasCarriageReturn) line += 1;
      column = 1;
      previousWasCarriageReturn = false;
    } else {
      column += 1;
      previousWasCarriageReturn = false;
    }
  }
  return positions;
}

async function addLineColumns(diagnostics) {
  const labelsByFile = new Map();
  for (const diagnostic of diagnostics) {
    for (const label of diagnostic.labels ?? []) {
      if (
        (label.span?.line !== undefined && label.span?.column !== undefined) ||
        label.span?.offset === undefined
      ) {
        continue;
      }
      let labelsByOffset = labelsByFile.get(diagnostic.filename);
      if (labelsByOffset === undefined) {
        labelsByOffset = new Map();
        labelsByFile.set(diagnostic.filename, labelsByOffset);
      }
      const labels = labelsByOffset.get(label.span.offset) ?? [];
      labels.push(label);
      labelsByOffset.set(label.span.offset, labels);
    }
  }

  for (const [filename, labelsByOffset] of labelsByFile) {
    let bytes;
    try {
      bytes = await readFile(filename);
    } catch (error) {
      const detail = error instanceof Error ? error.message : String(error);
      throw new Error(`cannot read diagnostic source ${filename}: ${detail}`);
    }
    const positions = resolveOxlintBytePositions(bytes, labelsByOffset.keys(), filename);
    for (const [byteOffset, labels] of labelsByOffset) {
      const location = positions.get(byteOffset);
      for (const label of labels) {
        label.span.line = location.line;
        label.span.column = location.column;
      }
    }
  }
}

function parseJson(result, label) {
  try {
    return result.stdout.trim()
      ? JSON.parse(result.stdout)
      : { diagnostics: [], number_of_files: 0 };
  } catch {
    throw new Error(
      `${label} returned non-JSON output while composing diagnostics:\n${result.stdout}${result.stderr}`,
    );
  }
}

function combine(upstream, native) {
  return {
    ...upstream,
    diagnostics: [...(upstream.diagnostics ?? []), ...(native.diagnostics ?? [])],
    number_of_files: (upstream.number_of_files ?? 0) + (native.number_of_files ?? 0),
    number_of_rules: Math.max(upstream.number_of_rules ?? 0, native.number_of_rules ?? 0),
    oxcTsrx: native.oxcTsrx,
  };
}

function primaryLocation(diagnostic) {
  const span = diagnostic.labels?.[0]?.span;
  return { line: span?.line ?? 1, column: span?.column ?? 1 };
}

function renderDefault(result, cwd) {
  const diagnostics = [...result.diagnostics].sort((left, right) => {
    const filename = left.filename.localeCompare(right.filename);
    if (filename !== 0) return filename;
    return (left.labels?.[0]?.span?.offset ?? 0) - (right.labels?.[0]?.span?.offset ?? 0);
  });
  if (diagnostics.length === 0) return "";
  const lines = diagnostics.map((diagnostic) => {
    const location = primaryLocation(diagnostic);
    const filename = relative(cwd, diagnostic.filename) || diagnostic.filename;
    return `${filename}:${location.line}:${location.column}: ${diagnostic.severity} ${diagnostic.code ?? diagnostic.rule ?? ""} ${diagnostic.message}`.trimEnd();
  });
  const errors = diagnostics.filter((diagnostic) => diagnostic.severity === "error").length;
  const warnings = diagnostics.filter((diagnostic) => diagnostic.severity === "warning").length;
  lines.push(`Found ${errors} error(s) and ${warnings} warning(s).`);
  return `${lines.join("\n")}\n`;
}

async function delegate(args, cwd) {
  const upstream = resolvePackageBinary("oxlint-current", "oxlint", import.meta.url);
  const upstreamArgs = [upstream, ...args];
  // --lsp starts a long-lived stdio LSP server, so the session must stream
  // through the wrapper instead of being captured and replayed on exit.
  if (args.some((argument) => argument.split("=")[0] === "--lsp")) {
    const result = await runPassthrough(process.execPath, upstreamArgs, { cwd });
    return result.status;
  }
  const result = await runCaptured(process.execPath, upstreamArgs, { cwd });
  process.stdout.write(result.stdout);
  process.stderr.write(result.stderr);
  return result.status;
}

export async function runCli(args, options = {}) {
  const cwd = options.cwd ?? process.cwd();
  if (args.some((argument) => DELEGATE_ONLY.has(argument.split("=")[0]))) {
    return delegate(args, cwd);
  }

  const positions = parseOxlintInvocation(args).positionals;
  const files = await discoverTsrxFiles(positions, cwd);
  const format = requestedOutputFormat(args);
  ensureSupportedOutput(format, files);
  const explicitConfig = argumentValue(args, new Set(["-c", "--config"]));
  const bridgeViteConfig = explicitConfig === null || isViteConfigPath(explicitConfig);
  const viteConfig =
    files.length > 0 && bridgeViteConfig
      ? await prepareVitePlusConfig(
          "lint",
          cwd,
          isViteConfigPath(explicitConfig) ? explicitConfig : null,
        )
      : null;

  try {
    const stripped = removeExplicitTsrx(args, VALUE_OPTIONS);
    const shouldRunUpstream = !stripped.hadPositionals || stripped.remainingPositionals > 0;
    const upstreamBinary = resolvePackageBinary("oxlint-current", "oxlint", import.meta.url);
    const useMaterializedUpstreamConfig = Boolean(viteConfig && !viteConfig.requiresAuthoredBase);
    let upstreamArgs = withOxlintOutputFormat(stripped.args, "json");
    if (useMaterializedUpstreamConfig) {
      upstreamArgs = replaceConfigArgument(upstreamArgs, viteConfig.path);
    }
    const nativeArgs = files.length > 0 ? nativeArguments(args, files, viteConfig) : null;
    // Mutating invocations never prestart. Preflight their native lane before
    // canonical Oxlint can apply fixes to the ordinary half of a mixed batch.
    // Missing or mismatched artifacts therefore fail atomically instead of
    // leaving a partially fixed project.
    const nativeBinary = nativeArgs ? resolveNativeBinary("lint") : null;
    let upstreamPromise;
    if (!shouldRunUpstream) {
      upstreamPromise = Promise.resolve({ status: 0, stdout: "", stderr: "", signal: null });
    } else if (options.prestartedUpstream !== null && options.prestartedUpstream !== undefined) {
      if (JSON.stringify(options.prestartedUpstream.args) !== JSON.stringify(upstreamArgs)) {
        await options.prestartedUpstream.result;
        throw new Error("canonical Oxlint prestart arguments diverged from the composed batch");
      }
      upstreamPromise = options.prestartedUpstream.result;
    } else {
      upstreamPromise = runUpstreamOxlint(upstreamBinary, upstreamArgs, {
        cwd,
        env: canonicalToolEnvironment(useMaterializedUpstreamConfig),
      });
    }

    const [upstreamResult, nativeResult] = await Promise.all([
      upstreamPromise,
      nativeArgs
        ? runCaptured(nativeBinary, nativeArgs, { cwd })
        : Promise.resolve({ status: 0, stdout: "", stderr: "", signal: null }),
    ]);

    if (upstreamResult.status > 1 || nativeResult.status > 1) {
      process.stdout.write(upstreamResult.stdout + nativeResult.stdout);
      process.stderr.write(upstreamResult.stderr + nativeResult.stderr);
      return Math.max(upstreamResult.status, nativeResult.status);
    }

    const upstream = parseJson(upstreamResult, "canonical Oxlint");
    const native = parseJson(nativeResult, "OXC for TSRX");
    await addLineColumns(native.diagnostics ?? []);
    let result = combine(upstream, native);
    if (args.includes("--quiet")) {
      result = {
        ...result,
        diagnostics: result.diagnostics.filter((diagnostic) => diagnostic.severity !== "warning"),
      };
    }

    if (!args.includes("--silent")) {
      process.stderr.write(upstreamResult.stderr + nativeResult.stderr);
      process.stdout.write(
        format === "json" ? `${JSON.stringify(result)}\n` : renderDefault(result, cwd),
      );
    }

    const warnings = result.diagnostics.filter(
      (diagnostic) => diagnostic.severity === "warning",
    ).length;
    const denyWarnings = args.includes("--deny-warnings");
    const maximum = argumentValue(args, new Set(["--max-warnings"]));
    const exceedsMaximum = maximum !== null && warnings > Number.parseInt(maximum, 10);
    return Math.max(
      upstreamResult.status,
      nativeResult.status,
      denyWarnings && warnings > 0 ? 1 : 0,
      exceedsMaximum ? 1 : 0,
    );
  } finally {
    await viteConfig?.cleanup();
  }
}
