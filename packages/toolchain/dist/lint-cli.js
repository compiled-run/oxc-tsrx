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
  resolveNativeCommand,
  resolvePackageBinary,
  runCaptured,
  runPassthrough,
} from "./runtime.js";
import {
  DELEGATE_ONLY,
  VALUE_OPTIONS,
  parseOxlintInvocation,
  parseOxlintOption,
  withOxlintOutputFormat,
} from "./lint-invocation.js";

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
// Canonical Oxlint's own wording for "every path you named matched nothing",
// reproduced verbatim so a `.tsrx` positional reports it the same way a `.ts`
// positional already does.
const UNMATCHED_PATTERN_MESSAGE =
  "No files found to lint. Please check your paths and ignore patterns.";

// Canonical Oxlint's own wording for an option it has never heard of,
// reproduced verbatim, on the same stream and with the same exit code. A flag
// canonical Oxlint does not know is not a TSRX gap: telling the user it "is not
// yet supported for .tsrx" sends them to the Oxlint docs looking for an option
// that does not exist. Only a flag canonical Oxlint really does accept gets
// that message.
function unknownOptionMessage(name) {
  return `Error: \`${name}\` is not expected in this context`;
}

// The first option in the command line that canonical Oxlint's own option set
// does not contain, or null when every option is one it knows. `VALUE_OPTIONS`
// consume their value so an option-looking argument such as
// `--ignore-pattern -foo` is never mistaken for an option of its own.
function unknownCanonicalOption(args) {
  let positionalOnly = false;
  for (let index = 0; index < args.length; index += 1) {
    const argument = args[index];
    if (positionalOnly) continue;
    if (argument === "--") {
      positionalOnly = true;
      continue;
    }
    if (!argument.startsWith("-") || argument === "-") continue;
    const { name, value } = parseOxlintOption(argument);
    if (VALUE_OPTIONS.has(name)) {
      if (value === null) index += 1;
      continue;
    }
    if (!parseOxlintInvocation([name]).known) return name;
  }
  return null;
}

// The native leaf prefixes its errors with its own name because that is correct
// when it is run directly as the capability target the provider manifest names.
// Relayed through this command the user typed `oxlint`, so the lines this
// wrapper passes on are re-labelled with the name they ran, using the same
// prefix `bin/oxlint` already puts on its own errors.
function attributeNativeErrors(stderr) {
  return stderr.replace(/^oxc-tsrx(?:-lint)?: /gmu, "oxlint (oxc-tsrx): ");
}

function hasTsrxPositional(positionals) {
  return positionals.some((argument) => argument.split("?")[0].endsWith(".tsrx"));
}

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

// A half that exits above 1 still hands back whatever it had composed when it
// failed, and this command forces `--format=json` on both halves internally
// (`withOxlintOutputFormat(..., "json")` and `nativeArguments`). Writing that
// stdout through untouched is what turns one `.tsrx` syntax error into a raw
// internal JSON dump in answer to a plain `oxlint src`. Split each half into the
// report it did produce and any output that is not a report, so the failure path
// can render through the same formatter the success path uses and only genuinely
// unstructured output is passed through.
function splitCapturedReport(result) {
  if (result.stdout.trim() === "") return { report: null, passthrough: "" };
  try {
    const parsed = JSON.parse(result.stdout);
    if (parsed !== null && typeof parsed === "object") return { report: parsed, passthrough: "" };
  } catch {
    // Not a report. Fall through and pass it on verbatim.
  }
  return { report: null, passthrough: result.stdout };
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

// Canonical Oxlint picks a reporter for itself when the command line does not
// name one, so the same `oxlint` command prints one shape on a laptop and
// another on a runner. A batch that has to be composed here is rendered by this
// file instead of by canonical Oxlint, so it has to make the same choice or the
// wrapper's output starts depending on which files were in the batch.
//
// These are the rules the shipped Oxlint 1.74.0 binary really follows, read off
// it directly rather than assumed:
//
//   * an explicit `--format`/`-f` always wins;
//   * otherwise any of the coding-agent variables below selects `agent`, the
//     compact `file:line:col: severity code: message help: help` form with no
//     summary. This is why a laptop inside a coding agent never reproduced the
//     runner's output even with GITHUB_ACTIONS set: the agent reporter outranks
//     the Actions one;
//   * otherwise GITHUB_ACTIONS set to exactly `true` selects `github`, the
//     workflow-command annotations. `1`, `false`, and an empty value do not;
//   * otherwise `default`, the graphical report with source excerpts.
const AGENT_ENVIRONMENT_VARIABLES = [
  "AI_AGENT",
  "CLAUDECODE",
  "CLAUDE_CODE",
  "CODEX_SANDBOX",
  "CODEX_THREAD_ID",
  "COPILOT_CLI",
  "CURSOR_AGENT",
  "GEMINI_CLI",
  "JUNIE_DATA",
  "JUNIE_SHIM_PATH",
  "OPENCODE",
  "REPL_ID",
];

function inAgentEnvironment(env) {
  if (AGENT_ENVIRONMENT_VARIABLES.some((name) => (env[name] ?? "") !== "")) return true;
  if ((env.EDITOR ?? "").includes("devin")) return true;
  return env.TERM_PROGRAM === "kiro";
}

// `requestedOutputFormat` answers `default` for both `--format=default` and no
// `--format` at all, and the difference is the whole question here.
export function explicitOutputFormat(args) {
  for (let index = 0; index < args.length; index += 1) {
    const argument = args[index];
    if (argument === "--") return null;
    const { name, value } = parseOxlintOption(argument);
    if (name === "--format" || name === "-f") return value ?? args[index + 1] ?? null;
  }
  return null;
}

export function effectiveOutputFormat(args, env = process.env) {
  const explicit = explicitOutputFormat(args);
  if (explicit !== null) return explicit;
  if (inAgentEnvironment(env)) return "agent";
  if (env.GITHUB_ACTIONS === "true") return "github";
  return "default";
}

// The formats a composed batch can be rendered in here. `default` is on the
// list because the graphical reporter cannot be rebuilt from the JSON the two
// halves hand back, so a composed batch falls back to the compact `agent` shape
// for it; every other name still reaches canonical Oxlint's own refusal.
const COMPOSABLE_FORMATS = new Set(["default", "agent", "github", "json"]);

function sortedDiagnostics(result) {
  return [...(result.diagnostics ?? [])].sort((left, right) => {
    const filename = left.filename.localeCompare(right.filename);
    if (filename !== 0) return filename;
    return (left.labels?.[0]?.span?.offset ?? 0) - (right.labels?.[0]?.span?.offset ?? 0);
  });
}

function renderCompact(result, cwd) {
  const diagnostics = sortedDiagnostics(result);
  if (diagnostics.length === 0) return "";
  const lines = diagnostics.map((diagnostic) => {
    const location = primaryLocation(diagnostic);
    const filename = relative(cwd, diagnostic.filename) || diagnostic.filename;
    // Canonical Oxlint's own default renderer, reproduced field for field:
    // `file:line:col: severity code: message help: help`, with the code omitted
    // (and its space with it) for a diagnostic that carries none, such as a
    // parse error. Composing a mixed batch must not cost the ordinary half the
    // `:` separator or the `help:` suggestion it prints without a .tsrx file in
    // the run; both are already in the JSON this function receives. TSRX
    // diagnostics carry `rule` and no `help`, so they render the same shape
    // minus the suggestion the native leaf does not emit yet.
    const code = diagnostic.code ?? diagnostic.rule ?? "";
    const help = diagnostic.help ? ` help: ${diagnostic.help}` : "";
    const prefix = `${filename}:${location.line}:${location.column}: ${diagnostic.severity}`;
    return `${prefix}${code ? ` ${code}` : ""}: ${diagnostic.message}${help}`.trimEnd();
  });
  const errors = diagnostics.filter((diagnostic) => diagnostic.severity === "error").length;
  const warnings = diagnostics.filter((diagnostic) => diagnostic.severity === "warning").length;
  lines.push(`Found ${errors} error(s) and ${warnings} warning(s).`);
  return `${lines.join("\n")}\n`;
}

function plural(count, noun) {
  return `${count} ${noun}${count === 1 ? "" : "s"}`;
}

// A GitHub workflow command, reproduced from canonical Oxlint's own annotation
// reporter field for field:
//
//   ::warning file=b.ts,line=2,endLine=2,col=9,endColumn=20,title=eslint(no-unused-vars)::message
//
// `title` is the rule code, or the literal `oxlint` for a diagnostic that has
// none, which is what canonical Oxlint prints for a parse error. The help text
// is not part of an annotation. The end position is the label span's end, which
// this file resolves from `offset + length` the same way it resolves the start.
function renderGitHub(result, cwd) {
  const diagnostics = sortedDiagnostics(result);
  const lines = diagnostics.map((diagnostic) => {
    const span = diagnostic.labels?.[0]?.span;
    const line = span?.line ?? 1;
    const column = span?.column ?? 1;
    const endLine = span?.endLine ?? line;
    const endColumn = span?.endColumn ?? column;
    const filename = relative(cwd, diagnostic.filename) || diagnostic.filename;
    const severity = diagnostic.severity === "error" ? "error" : "warning";
    // A diagnostic with no rule behind it, such as a parse error, is titled
    // `oxlint`, which is what canonical Oxlint titles its own.
    const title = diagnostic.code || diagnostic.rule || "oxlint";
    const location = `file=${filename},line=${line},endLine=${endLine},col=${column},endColumn=${endColumn}`;
    return `::${severity} ${location},title=${title}::${diagnostic.message}`;
  });
  const errors = diagnostics.filter((diagnostic) => diagnostic.severity === "error").length;
  const warnings = diagnostics.filter((diagnostic) => diagnostic.severity === "warning").length;
  // Canonical Oxlint separates the annotations from its summary with a blank
  // line and words it warnings first. Its second summary line, `Finished in
  // <elapsed> on <n> files with <n> rules using <n> threads.`, reports one
  // process's own run; a composed batch is two processes, so this renderer
  // stops after the counts rather than inventing a number for it.
  if (lines.length > 0) lines.push("");
  lines.push(`Found ${plural(warnings, "warning")} and ${plural(errors, "error")}.`);
  return `${lines.join("\n")}\n`;
}

// The end of every primary label span, in the same byte-counted line and column
// canonical Oxlint reports. Only the annotation reporter needs it, so the source
// files are read only when it is the one that was selected.
async function addEndPositions(diagnostics) {
  const offsetsByFile = new Map();
  for (const diagnostic of diagnostics) {
    const span = diagnostic.labels?.[0]?.span;
    if (span === undefined || span.offset === undefined || span.length === undefined) continue;
    if (span.endLine !== undefined && span.endColumn !== undefined) continue;
    const spans = offsetsByFile.get(diagnostic.filename) ?? [];
    spans.push(span);
    offsetsByFile.set(diagnostic.filename, spans);
  }

  for (const [filename, spans] of offsetsByFile) {
    const bytes = await readFile(filename);
    const positions = resolveOxlintBytePositions(
      bytes,
      spans.map((span) => span.offset + span.length),
      filename,
    );
    for (const span of spans) {
      const location = positions.get(span.offset + span.length);
      span.endLine = location.line;
      span.endColumn = location.column;
    }
  }
}

async function renderReport(report, cwd, format) {
  if (format === "json") return `${JSON.stringify(report)}\n`;
  if (format !== "github") return renderCompact(report, cwd);
  try {
    await addEndPositions(report.diagnostics ?? []);
  } catch {
    // A span's end only becomes a line and column by reading the source, which
    // the failure that produced this report may have made unreadable. An
    // annotation that ends where it starts still points at the right place.
  }
  return renderGitHub(report, cwd);
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
  // Only this route answers for a `.tsrx` path. An ordinary-only invocation
  // keeps reaching canonical Oxlint, which prints its own rejection itself, so
  // nothing here can drift from the tool it is reproducing on that path.
  if (files.length > 0 || hasTsrxPositional(positions)) {
    const unknown = unknownCanonicalOption(args);
    if (unknown !== null) {
      process.stderr.write(`${unknownOptionMessage(unknown)}\n`);
      return 1;
    }
  }
  // Composing a batch takes the report away from canonical Oxlint's reporters,
  // so this route has to reach the same reporter canonical Oxlint would have
  // reached on its own. Otherwise `oxlint src` prints annotations on a runner
  // when the directory holds no `.tsrx` file and compact text when it does.
  const format = effectiveOutputFormat(args);
  if (!COMPOSABLE_FORMATS.has(format)) ensureSupportedOutput(format, files);
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
    // Every positional was a `.tsrx` path and discovery matched none of them, so
    // neither half has anything to lint and neither would print. Canonical
    // Oxlint answers the same invocation over `.ts` with this line on stdout and
    // exit 1, and stays silent with exit 0 under
    // `--no-error-on-unmatched-pattern`; a mistyped `.tsrx` filename must not be
    // the one path that reports a green CI run for work nobody did.
    if (!shouldRunUpstream && files.length === 0) {
      if (args.includes("--no-error-on-unmatched-pattern")) return 0;
      process.stdout.write(`${UNMATCHED_PATTERN_MESSAGE}\n`);
      if (format === "json") {
        process.stdout.write(`${JSON.stringify({ diagnostics: [], number_of_files: 0 })}\n`);
      }
      return 1;
    }
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
    const nativeCommand = nativeArgs ? resolveNativeCommand("lint", nativeArgs) : null;
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
      nativeCommand
        ? runCaptured(nativeCommand.executable, nativeCommand.args, { cwd })
        : Promise.resolve({ status: 0, stdout: "", stderr: "", signal: null }),
    ]);

    if (upstreamResult.status > 1 || nativeResult.status > 1) {
      // One half failing hard abandons the merge, but the report the other half
      // produced is still a report and the user still asked for their chosen
      // format. Render it instead of dumping the internal JSON.
      const upstreamHalf = splitCapturedReport(upstreamResult);
      const nativeHalf = splitCapturedReport(nativeResult);
      if (nativeHalf.report) {
        try {
          await addLineColumns(nativeHalf.report.diagnostics ?? []);
        } catch {
          // Byte offsets only become line/column by reading the source, which
          // the same failure may have made unreadable. The diagnostics that did
          // arrive are still worth printing without exact positions.
        }
      }
      const report =
        upstreamHalf.report && nativeHalf.report
          ? combine(upstreamHalf.report, nativeHalf.report)
          : (upstreamHalf.report ?? nativeHalf.report);
      const rendered = report === null ? "" : await renderReport(report, cwd, format);
      process.stdout.write(upstreamHalf.passthrough + nativeHalf.passthrough + rendered);
      process.stderr.write(upstreamResult.stderr + attributeNativeErrors(nativeResult.stderr));
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
      process.stderr.write(upstreamResult.stderr + attributeNativeErrors(nativeResult.stderr));
      process.stdout.write(await renderReport(result, cwd, format));
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
