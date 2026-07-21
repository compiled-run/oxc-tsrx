import { resolvePackageBinary } from "../packages/runtime/dist/package-binary.js";

/**
 * Resolve VSCE through its public npm manifest and execute that JavaScript entry with Node.
 * This deliberately avoids platform-specific npm `.bin` shell shims such as `vsce.cmd`.
 */
export function resolveVsceInvocation(
  args,
  { fromUrl = import.meta.url, nodeExecutable = process.execPath } = {},
) {
  if (!Array.isArray(args) || args.some((argument) => typeof argument !== "string")) {
    throw new TypeError("VSCE arguments must be an array of strings");
  }

  const entry = resolvePackageBinary("@vscode/vsce", "vsce", fromUrl);
  return {
    executable: nodeExecutable,
    args: [entry, ...args],
  };
}
