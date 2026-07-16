import {
  argumentValue,
  canonicalToolEnvironment,
  discoverTsrxFiles,
  isViteConfigPath,
  prepareVitePlusConfig,
  positionalIndices,
  removeExplicitTsrx,
  replaceConfigArgument,
  resolveNativeBinary,
  resolvePackageBinary,
  runCaptured,
} from "@oxc-tsrx/runtime";

const VALUE_OPTIONS = new Set([
  "-c",
  "--config",
  "--migrate",
  "--stdin-filepath",
  "--ignore-path",
  "--threads",
]);
const DELEGATE_ONLY = new Set(["--help", "-h", "--version", "-V", "--init", "--migrate", "--lsp"]);
const NATIVE_VALUE_OPTIONS = new Map([
  ["-c", "--config"],
  ["--config", "--config"],
  ["--threads", "--threads"],
]);
const WRAPPER_OPTIONS = new Set(["--no-error-on-unmatched-pattern"]);

function parseOption(argument) {
  const equals = argument.indexOf("=");
  return equals === -1
    ? { name: argument, value: null }
    : { name: argument.slice(0, equals), value: argument.slice(equals + 1) };
}

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
    const { name, value: inlineValue } = parseOption(argument);
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
  const result = await runCaptured(upstream, args, { cwd, input });
  process.stdout.write(result.stdout);
  process.stderr.write(result.stderr);
  return result.status;
}

function stdinPath(args) {
  for (let index = 0; index < args.length; index += 1) {
    const argument = args[index];
    if (argument === "--stdin-filepath") return args[index + 1] ?? null;
    if (argument.startsWith("--stdin-filepath=")) return argument.slice("--stdin-filepath=".length);
  }
  return null;
}

export async function runCli(args, options = {}) {
  const cwd = options.cwd ?? process.cwd();
  const input = options.input;
  if (args.some((argument) => DELEGATE_ONLY.has(argument.split("=")[0]))) {
    return delegate(args, cwd, input);
  }

  const requestedStdin = stdinPath(args);
  if (requestedStdin !== null) {
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
      const result = await runCaptured(resolveNativeBinary("format"), nativeArgs, { cwd, input });
      process.stdout.write(result.stdout);
      process.stderr.write(result.stderr);
      return result.status;
    } finally {
      await viteConfig?.cleanup();
    }
  }

  const positions = positionalIndices(args, VALUE_OPTIONS).map((index) => args[index]);
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
    const nativeBinary = nativeArgs ? resolveNativeBinary("format") : null;
    const [upstreamResult, nativeResult] = await Promise.all([
      shouldRunUpstream
        ? runCaptured(upstream, upstreamArgs, {
            cwd,
            env: canonicalToolEnvironment(useMaterializedUpstreamConfig),
          })
        : Promise.resolve({ status: 0, stdout: "", stderr: "", signal: null }),
      nativeArgs
        ? runCaptured(nativeBinary, nativeArgs, { cwd })
        : Promise.resolve({ status: 0, stdout: "", stderr: "", signal: null }),
    ]);
    process.stdout.write(upstreamResult.stdout + nativeResult.stdout);
    process.stderr.write(upstreamResult.stderr + nativeResult.stderr);
    return Math.max(upstreamResult.status, nativeResult.status);
  } finally {
    await viteConfig?.cleanup();
  }
}
