import { spawn } from "node:child_process";
import { extname } from "node:path";

/**
 * Executing a declared `bin` target on every host this package publishes for.
 *
 * On POSIX the target is simply the file: it is executable, and `spawn` runs
 * it. Windows has two shapes this package must expect and one it must not use.
 *
 * The shape it must expect is a batch launcher. `npm` writes `<name>.cmd` (and
 * `<name>.ps1`, and a POSIX `sh` script under the bare name) into
 * `node_modules/.bin` instead of a symlink, and a package is free to declare a
 * `.cmd` or `.bat` file as its own `bin` entry. Windows cannot execute either
 * one directly: only `cmd.exe` can, and it re-parses the whole command line, so
 * every argument has to be escaped for `cmd.exe` rather than for `CreateProcess`.
 *
 * The shape it must not use is `shell: true`. Node concatenates `args` into the
 * command line unescaped in that mode — which is exactly the injection this
 * escaping exists to prevent — and since Node 22 it emits DEP0190 for it.
 *
 * Recent libuv escapes batch arguments itself, so on a new enough Node a plain
 * `spawn("x.cmd", args)` is already safe. This module does not depend on that:
 * `oxc-tsrx` supports Node 20.19 and up across eight published platforms, the
 * behaviour differs by libuv version rather than by Node major, and the whole
 * point of a launcher is that it behaves the same everywhere. Handing `cmd.exe`
 * a verbatim, pre-escaped command line is correct under either libuv, because
 * `cmd.exe` is an ordinary executable and never takes the batch path at all.
 *
 * This is the ~20 lines of `cross-spawn` that this package actually needs. It
 * does not need that package's `PATH`/`PATHEXT` search (every path here is
 * already absolute and resolved from a manifest) or its shebang re-targeting
 * (a Node wrapper is detected and imported in process before spawning is even
 * considered), and adding it would pull `path-key`, `shebang-command`,
 * `shebang-regex`, `which`, and `isexe` into a published dependency graph.
 */

/** Extensions Windows will only run through a command interpreter. */
const BATCH_EXTENSIONS = new Set([".cmd", ".bat"]);

/**
 * Characters `cmd.exe` acts on rather than passes through. A caret escapes each
 * one; the set is `cross-spawn`'s, which is the de facto reference for this.
 */
const COMMAND_METACHARACTERS = /([()\][%!^"`<>&|;, *?])/gu;

export function isBatchFile(file, platform = process.platform) {
  return platform === "win32" && BATCH_EXTENSIONS.has(extname(String(file)).toLowerCase());
}

/**
 * Quote one argument so `cmd.exe` reproduces it exactly. Backslashes that
 * precede a quote (or the closing quote this adds) are doubled first, because
 * the Windows command-line parser treats `\"` as a literal quote.
 */
export function escapeCommandArgument(argument) {
  const quoted = `"${String(argument)
    .replaceAll(/(\\*)"/gu, '$1$1\\"')
    .replace(/(\\*)$/u, "$1$1")}"`;
  return quoted.replaceAll(COMMAND_METACHARACTERS, "^$1");
}

/**
 * The file, argv, and options to hand `spawn`/`spawnSync` for `file`.
 *
 * `platform` is a parameter so the Windows branch is assertable from any host;
 * a lane that could only run this on Windows would never be run at all.
 */
export function resolveCommandInvocation(file, args = [], platform = process.platform) {
  if (!isBatchFile(file, platform)) {
    return { file, args: [...args], windowsVerbatimArguments: false };
  }
  const command = [
    String(file).replaceAll(COMMAND_METACHARACTERS, "^$1"),
    ...args.map((argument) => escapeCommandArgument(argument)),
  ].join(" ");
  return {
    file: process.env.ComSpec || process.env.comspec || "cmd.exe",
    args: ["/d", "/s", "/c", `"${command}"`],
    windowsVerbatimArguments: true,
  };
}

/** `child_process.spawn`, with the Windows batch-launcher case handled. */
export function spawnCommand(file, args = [], options: any = {}, spawnProcess = spawn) {
  const invocation = resolveCommandInvocation(file, args);
  return spawnProcess(invocation.file, invocation.args, {
    ...options,
    ...(invocation.windowsVerbatimArguments ? { windowsVerbatimArguments: true } : {}),
  });
}
