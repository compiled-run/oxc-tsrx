import { statSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, resolve } from "node:path";
import { pathToFileURL } from "node:url";

/** Resolve the executable declared by an installed npm package's `bin` field. */
export function resolvePackageBinary(packageName, binaryName, fromUrl) {
  const localRequire = createRequire(fromUrl);
  const manifestPath = localRequire.resolve(`${packageName}/package.json`);
  const manifest = localRequire(manifestPath);
  const declared =
    typeof manifest.bin === "string" ? manifest.bin : manifest.bin?.[binaryName];
  if (typeof declared !== "string" || declared.length === 0) {
    throw new Error(`${packageName} does not declare its ${binaryName} npm binary`);
  }

  const entry = resolve(dirname(manifestPath), declared);
  let metadata;
  try {
    metadata = statSync(entry);
  } catch {
    throw new Error(`${packageName} declares a missing ${binaryName} npm binary at ${entry}`);
  }
  if (!metadata.isFile()) {
    throw new Error(`${packageName} declares a non-file ${binaryName} npm binary at ${entry}`);
  }
  return entry;
}

/** Execute a declared JavaScript npm binary in this process. */
export async function importDeclaredPackageBinary(packageName, binaryName, fromUrl) {
  const entry = resolvePackageBinary(packageName, binaryName, fromUrl);
  await import(pathToFileURL(entry).href);
}
