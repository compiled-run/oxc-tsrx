import { cp, mkdir, readFile, stat, symlink } from "node:fs/promises";
import { createRequire } from "node:module";
import { dirname, join, resolve } from "node:path";

const require = createRequire(import.meta.url);
const root = resolve(import.meta.dirname, "../..");

async function linkPackage(modules, name, packageRoot) {
  const destination = join(modules, ...name.split("/"));
  await mkdir(dirname(destination), { recursive: true });
  await symlink(packageRoot, destination, "dir");
}

async function copyPackageEntries(source, destination, entries) {
  await mkdir(destination, { recursive: true });
  for (const entry of entries) {
    const from = join(source, entry);
    try {
      await stat(from);
    } catch {
      continue;
    }
    await cp(from, join(destination, entry), { recursive: true });
  }
}

async function resolvePackageRoot(packageRequire, name) {
  try {
    return dirname(packageRequire.resolve(`${name}/package.json`));
  } catch (error) {
    if (error?.code !== "ERR_PACKAGE_PATH_NOT_EXPORTED") throw error;
  }
  let directory = dirname(packageRequire.resolve(name));
  for (;;) {
    const manifestPath = join(directory, "package.json");
    try {
      const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
      if (manifest.name === name) return directory;
    } catch {
      // Continue walking through package-internal entry directories.
    }
    const parent = dirname(directory);
    if (parent === directory) throw new Error(`unable to locate package root for ${name}`);
    directory = parent;
  }
}

function platformSuffix() {
  if (process.platform === "darwin") return `darwin-${process.arch}`;
  if (process.platform === "win32") return `win32-${process.arch}-msvc`;
  if (process.platform === "linux") {
    const report = process.report?.getReport?.();
    const libc = report?.header?.glibcVersionRuntime ? "gnu" : "musl";
    return `linux-${process.arch}-${libc}`;
  }
  throw new Error(`unsupported package-test platform: ${process.platform}-${process.arch}`);
}

/**
 * Build the physical package layout a consumer install exposes to Vite+, the thin companions,
 * their canonical OXC delegates, and native bindings. Package files are copied; only unrelated
 * dependency packages and platform bindings are symlinked to the read-only workspace install.
 */
export async function installPhysicalToolPackages(modules, vitePlusPackage) {
  const vitePlusSource = dirname(require.resolve(`${vitePlusPackage}/package.json`));
  const vitePlusDestination = join(modules, "vite-plus");
  await copyPackageEntries(vitePlusSource, vitePlusDestination, [
    "package.json",
    "bin",
    "binding",
    "dist",
  ]);

  const vitePlusManifest = JSON.parse(await readFile(join(vitePlusSource, "package.json"), "utf8"));
  const vitePlusRequire = createRequire(join(vitePlusSource, "package.json"));
  for (const dependency of Object.keys(vitePlusManifest.dependencies ?? {})) {
    if (dependency === "oxlint" || dependency === "oxfmt" || dependency === "oxlint-tsgolint") {
      continue;
    }
    await linkPackage(modules, dependency, await resolvePackageRoot(vitePlusRequire, dependency));
  }
  const suffix = platformSuffix();
  const vitePlusBinding = `@voidzero-dev/vite-plus-${suffix}`;
  await linkPackage(
    modules,
    vitePlusBinding,
    await resolvePackageRoot(vitePlusRequire, vitePlusBinding),
  );

  const runtimeDestination = join(modules, "@oxc-tsrx/runtime");
  const lintDestination = join(modules, "oxlint-tsrx");
  const formatDestination = join(modules, "oxfmt-tsrx");
  await Promise.all([
    copyPackageEntries(join(root, "packages/runtime"), runtimeDestination, [
      "package.json",
      "dist",
      "LICENSE",
    ]),
    copyPackageEntries(join(root, "packages/oxlint"), lintDestination, [
      "package.json",
      "bin",
      "dist",
      "LICENSE",
    ]),
    copyPackageEntries(join(root, "packages/oxfmt"), formatDestination, [
      "package.json",
      "bin",
      "dist",
      "LICENSE",
    ]),
  ]);
  await linkPackage(modules, "oxlint", lintDestination);
  await linkPackage(modules, "oxfmt", formatDestination);

  const canonicalLint = dirname(require.resolve("oxlint-current/package.json"));
  const canonicalFormat = dirname(require.resolve("oxfmt-current/package.json"));
  await Promise.all([
    copyPackageEntries(canonicalLint, join(lintDestination, "node_modules/oxlint-current"), [
      "package.json",
      "bin",
      "dist",
      "configuration_schema.json",
      "LICENSE",
    ]),
    copyPackageEntries(canonicalFormat, join(formatDestination, "node_modules/oxfmt-current"), [
      "package.json",
      "bin",
      "dist",
      "LICENSE",
    ]),
  ]);
  for (const [source, packageRequire] of [
    [canonicalLint, createRequire(join(canonicalLint, "package.json"))],
    [canonicalFormat, createRequire(join(canonicalFormat, "package.json"))],
  ]) {
    const manifest = JSON.parse(await readFile(join(source, "package.json"), "utf8"));
    for (const dependency of Object.keys(manifest.dependencies ?? {})) {
      await linkPackage(modules, dependency, await resolvePackageRoot(packageRequire, dependency));
    }
  }
  await Promise.all([
    linkPackage(modules, "tinyglobby", await resolvePackageRoot(require, "tinyglobby")),
    linkPackage(
      modules,
      `@oxlint/binding-${suffix}`,
      await resolvePackageRoot(require, `@oxlint/binding-${suffix}`),
    ),
    linkPackage(
      modules,
      `@oxfmt/binding-${suffix}`,
      await resolvePackageRoot(require, `@oxfmt/binding-${suffix}`),
    ),
    linkPackage(modules, "oxlint-tsgolint", await resolvePackageRoot(require, "oxlint-tsgolint")),
  ]);
}
