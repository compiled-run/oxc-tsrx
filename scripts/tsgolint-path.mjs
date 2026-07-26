import { existsSync } from "node:fs";
import { join } from "node:path";

/**
 * Where the type-aware linter's native executable lives in this workspace.
 *
 * `node_modules/.bin/tsgolint` is not usable here. pnpm writes that entry as a
 * shell shim rather than a symlink into the package, and the adapter verifies
 * the tsgolint version by walking up from the executable to the owning
 * `package.json`, which a shim has no path to. The platform package itself is
 * the file the adapter wants, and pnpm-workspace.yaml hoists
 * `@oxlint-tsgolint/*` to the workspace root so this path exists.
 */
const PLATFORM_PACKAGES = new Map([
  ["darwin:arm64", "darwin-arm64"],
  ["darwin:x64", "darwin-x64"],
  ["linux:arm64", "linux-arm64"],
  ["linux:x64", "linux-x64"],
  ["win32:arm64", "win32-arm64"],
  ["win32:x64", "win32-x64"],
]);

/** The tsgolint executable for this host, or null when this host has no build. */
export function resolveTsgolintExecutable(root) {
  const platformPackage = PLATFORM_PACKAGES.get(`${process.platform}:${process.arch}`);
  if (!platformPackage) return null;
  const executable = join(
    root,
    "node_modules",
    "@oxlint-tsgolint",
    platformPackage,
    process.platform === "win32" ? "tsgolint.exe" : "tsgolint",
  );
  return existsSync(executable) ? executable : null;
}
