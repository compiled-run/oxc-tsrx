import { readFileSync } from "node:fs";
import {
  argumentValue,
  canonicalToolEnvironment,
  discoverTsrxFiles,
  isViteConfigPath,
  prepareVitePlusConfig,
  removeExplicitTsrx,
  replaceConfigArgument,
  resolveNativeCommand,
  resolvePackageBinary,
  runCaptured,
  runPassthrough,
} from "./runtime.js";
import { VALUE_OPTIONS, parseOxfmtInvocation, parseOxfmtOption } from "./format-invocation.js";
const NATIVE_VALUE_OPTIONS = new Map([
  ["-c", "--config"],
  ["--config", "--config"],
  ["--threads", "--threads"],
]);
const WRAPPER_OPTIONS = new Set(["--no-error-on-unmatched-pattern"]);

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
    const { name, value: inlineValue } = parseOxfmtOption(argument);
    if (NATIVE_VALUE_OPTIONS.has(name)) {
      const value = inlineValue ?? args[++index];
      if (!value) throw new Error(`${name} requires a value`);
      if (resolvedConfig && (name === "-c" || name === "--config")) continue;
      output.push(NATIVE_VALUE_OPTIONS.get(name), value);
      continue;
    }
    if (name === "--write" || name === "--check" || name === "--list-different") {
      output.push(name);
      continue;
    }
    if (WRAPPER_OPTIONS.has(name)) continue;
    throw new Error(
      `${name} is not yet supported for .tsrx by the drop-in Oxfmt command; canonical Oxfmt still handles ordinary files`,
    );
  }
  if (resolvedConfig) {
    output.push("--config", resolvedConfig.path, "--config-base", resolvedConfig.base);
  }
  return [...output, ...files];
}

async function delegate(args, cwd, input) {
  const upstream = resolvePackageBinary("oxfmt-current", "oxfmt", import.meta.url);
  const upstreamArgs = [upstream, ...args];
  // --lsp starts a long-lived stdio LSP server, so the session must stream
  // through the wrapper instead of being captured and replayed on exit.
  if (args.some((argument) => argument.split("=")[0] === "--lsp")) {
    const result = await runPassthrough(process.execPath, upstreamArgs, { cwd });
    return result.status;
  }
  const result = await runCaptured(process.execPath, upstreamArgs, { cwd, input });
  process.stdout.write(result.stdout);
  process.stderr.write(result.stderr);
  return result.status;
}

export async function runCli(args, options = {}) {
  const cwd = options.cwd ?? process.cwd();
  const invocation = parseOxfmtInvocation(args);
  if (invocation.delegateOnly) {
    return delegate(args, cwd, options.input);
  }

  const requestedStdin = invocation.stdinFilepath;
  if (requestedStdin !== null) {
    // The executable bridge owns stdin for TSRX. Keep bytes as a Buffer so the
    // native formatter receives the exact input without a UTF-8 decode/re-encode
    // round trip. Programmatic callers can still provide an explicit input.
    const input = options.input ?? readFileSync(0);
    if (!requestedStdin.split("?")[0].endsWith(".tsrx")) return delegate(args, cwd, input);
    const explicitConfig = argumentValue(args, new Set(["-c", "--config"]));
    const bridgeViteConfig = explicitConfig === null || isViteConfigPath(explicitConfig);
    const viteConfig = bridgeViteConfig
      ? await prepareVitePlusConfig(
          "fmt",
          cwd,
          isViteConfigPath(explicitConfig) ? explicitConfig : null,
        )
      : null;
    try {
      let nativeArgs = args.filter((argument) => argument !== "--no-error-on-unmatched-pattern");
      if (viteConfig) {
        nativeArgs = replaceConfigArgument(nativeArgs, viteConfig.path);
        nativeArgs.push("--config-base", viteConfig.base);
      }
      const nativeCommand = resolveNativeCommand("format", nativeArgs);
      const result = await runCaptured(nativeCommand.executable, nativeCommand.args, {
        cwd,
        input,
      });
      process.stdout.write(result.stdout);
      process.stderr.write(result.stderr);
      return result.status;
    } finally {
      await viteConfig?.cleanup();
    }
  }

  const positions = invocation.positionals;
  const files = await discoverTsrxFiles(positions, cwd);
  const explicitConfig = argumentValue(args, new Set(["-c", "--config"]));
  const bridgeViteConfig = explicitConfig === null || isViteConfigPath(explicitConfig);
  const viteConfig =
    files.length > 0 && bridgeViteConfig
      ? await prepareVitePlusConfig(
          "fmt",
          cwd,
          isViteConfigPath(explicitConfig) ? explicitConfig : null,
        )
      : null;
  try {
    const stripped = removeExplicitTsrx(args, VALUE_OPTIONS);
    const shouldRunUpstream = !stripped.hadPositionals || stripped.remainingPositionals > 0;
    const upstream = resolvePackageBinary("oxfmt-current", "oxfmt", import.meta.url);
    const useMaterializedUpstreamConfig = Boolean(viteConfig && !viteConfig.requiresAuthoredBase);
    const upstreamArgs = useMaterializedUpstreamConfig
      ? replaceConfigArgument(stripped.args, viteConfig.path)
      : stripped.args;
    const nativeArgs = files.length > 0 ? nativeArguments(args, files, viteConfig) : null;
    // Resolve and validate every artifact before either tool can mutate a mixed batch.
    // In particular, a missing platform package must not let canonical Oxfmt
    // rewrite ordinary files before the TSRX lane fails.
    const nativeCommand = nativeArgs ? resolveNativeCommand("format", nativeArgs) : null;
    const [upstreamResult, nativeResult] = await Promise.all([
      shouldRunUpstream
        ? runCaptured(process.execPath, [upstream, ...upstreamArgs], {
            cwd,
            env: canonicalToolEnvironment(useMaterializedUpstreamConfig),
          })
        : Promise.resolve({ status: 0, stdout: "", stderr: "", signal: null }),
      nativeCommand
        ? runCaptured(nativeCommand.executable, nativeCommand.args, { cwd })
        : Promise.resolve({ status: 0, stdout: "", stderr: "", signal: null }),
    ]);
    process.stdout.write(upstreamResult.stdout + nativeResult.stdout);
    process.stderr.write(upstreamResult.stderr + nativeResult.stderr);
    return Math.max(upstreamResult.status, nativeResult.status);
  } finally {
    await viteConfig?.cleanup();
  }
}
