import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";
import { basename, join, relative, resolve, sep } from "node:path";
import test from "node:test";

const root = resolve(import.meta.dirname, "../..");
const adapterManifestPath = join(root, "crates/oxc_adapter/Cargo.toml");
const adapterSourcePath = join(root, "crates/oxc_adapter/src/lib.rs");
const editorIntegrationPath = join(root, "docs/integrations/editor.md");
const canonicalRepository = "https://github.com/oxc-project/oxc";
const expectedAdapterDependencies = [
  "oxc_allocator",
  "oxc_ast",
  "oxc_ast_visit",
  "oxc_config",
  "oxc_formatter",
  "oxc_formatter_core",
  "oxc_language_server",
  "oxc_linter",
  "oxc_parser",
  "oxc_semantic",
  "oxc_span",
  "oxc_syntax",
];

function dependencyTables(manifest) {
  const dependencies = new Map();
  for (const match of manifest.matchAll(/^(oxc_[\w-]+)\s*=\s*\{([^\n}]*)\}\s*$/gm)) {
    dependencies.set(match[1], match[2]);
  }
  return dependencies;
}

function dependencyNames(manifest) {
  return [
    ...manifest.matchAll(/^(oxc_[\w-]+)\s*=/gm),
    ...manifest.matchAll(/^\[(?:[^\]\n]+\.)?dependencies\.(oxc_[\w-]+)\]\s*$/gm),
    ...manifest.matchAll(/\bpackage\s*=\s*"(oxc_[\w-]+)"/gm),
  ].map((match) => match[1]);
}

function quotedField(table, field) {
  return table.match(new RegExp(`(?:^|,)\\s*${field}\\s*=\\s*"([^"]+)"`))?.[1];
}

function lockPackages(lock) {
  return lock
    .split(/^\[\[package\]\]\s*$/m)
    .slice(1)
    .map((block) => ({
      block,
      name: block.match(/^name\s*=\s*"([^"]+)"/m)?.[1],
      source: block.match(/^source\s*=\s*"([^"]+)"/m)?.[1],
    }));
}

async function cargoManifests(directory) {
  const manifests = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    const manifestPath = join(directory, entry.name, "Cargo.toml");
    try {
      manifests.push([manifestPath, await readFile(manifestPath, "utf8")]);
    } catch (error) {
      if (error?.code !== "ENOENT") throw error;
    }
  }
  return manifests;
}

async function walkDirectories(directory, visit) {
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (!entry.isDirectory() || [".git", "node_modules", "target"].includes(entry.name)) continue;
    const path = join(directory, entry.name);
    visit(path);
    await walkDirectories(path, visit);
  }
}

test("canonical OXC crates resolve through one exact adapter revision", async () => {
  const [adapterManifest, adapterSource, lock, runtime, nativePackager, vscodePackager] =
    await Promise.all([
      readFile(adapterManifestPath, "utf8"),
      readFile(adapterSourcePath, "utf8"),
      readFile(join(root, "Cargo.lock"), "utf8"),
      readFile(join(root, "packages/runtime/dist/index.js"), "utf8"),
      readFile(join(root, "scripts/package-native.mjs"), "utf8"),
      readFile(join(root, "scripts/package-vscode.mjs"), "utf8"),
    ]);

  const dependencies = dependencyTables(adapterManifest);
  assert.deepEqual([...dependencies.keys()].sort(), expectedAdapterDependencies);

  const revisions = new Set();
  for (const [name, table] of dependencies) {
    assert.equal(
      quotedField(table, "git"),
      canonicalRepository,
      `${name} must use the canonical OXC repository`,
    );
    const revision = quotedField(table, "rev");
    assert.match(revision ?? "", /^[0-9a-f]{40}$/, `${name} must pin one full commit SHA`);
    assert.equal(quotedField(table, "path"), undefined, `${name} must not use a local checkout`);
    assert.equal(quotedField(table, "branch"), undefined, `${name} must not follow a branch`);
    assert.equal(quotedField(table, "tag"), undefined, `${name} must not follow a tag`);
    revisions.add(revision);
  }
  assert.equal(revisions.size, 1, "all adapter dependencies must move in one OXC upgrade");
  const [revision] = revisions;

  const constant = adapterSource.match(
    /pub const OXC_REVISION:\s*&str\s*=\s*"([0-9a-f]{40})"/,
  )?.[1];
  assert.equal(constant, revision, "the adapter's public revision must match its dependencies");

  const packages = lockPackages(lock);
  for (const name of expectedAdapterDependencies) {
    const resolved = packages.filter(
      (entry) => entry.name === name && entry.source?.startsWith(`git+${canonicalRepository}`),
    );
    assert.equal(resolved.length, 1, `${name} must resolve exactly once from canonical OXC`);
    assert.match(
      resolved[0].source,
      new RegExp(`\\?rev=${revision}#${revision}$`),
      `${name} lock entry must resolve the adapter's exact revision`,
    );
  }

  const canonicalLockSources = new Set(
    packages
      .map((entry) => entry.source)
      .filter((source) => source?.startsWith(`git+${canonicalRepository}`)),
  );
  assert.deepEqual(
    [...canonicalLockSources],
    [`git+${canonicalRepository}?rev=${revision}#${revision}`],
    "Cargo.lock must not contain a second canonical OXC source or revision",
  );
  const [canonicalLockSource] = canonicalLockSources;
  const canonicalPackageNames = new Set(
    packages
      .filter((entry) => entry.source === canonicalLockSource)
      .map((entry) => entry.name),
  );
  for (const name of canonicalPackageNames) {
    const conflictingIdentities = packages.filter(
      (entry) => entry.name === name && entry.source !== canonicalLockSource,
    );
    assert.deepEqual(
      conflictingIdentities,
      [],
      `${name} must not resolve from both canonical Git and another Cargo source`,
    );
  }

  const distributionPins = [
    ["runtime", runtime.match(/const OXC_REVISION\s*=\s*"([0-9a-f]{40})"/)?.[1]],
    ["native packager", nativePackager.match(/const revision\s*=\s*"([0-9a-f]{40})"/)?.[1]],
    ["VS Code packager", vscodePackager.match(/const revision\s*=\s*"([0-9a-f]{40})"/)?.[1]],
  ];
  for (const [label, pin] of distributionPins) {
    assert.equal(pin, revision, `${label} metadata must identify the exact adapter revision`);
  }
});

test("no crate bypasses the OXC adapter", async () => {
  const manifests = [
    [join(root, "Cargo.toml"), await readFile(join(root, "Cargo.toml"), "utf8")],
    ...(await cargoManifests(join(root, "crates"))),
  ];
  for (const [path, manifest] of manifests) {
    if (path === adapterManifestPath) continue;
    const directOxcDependencies = dependencyNames(manifest).filter(
      (name) => name !== "oxc_adapter",
    );
    assert.deepEqual(
      directOxcDependencies,
      [],
      `${relative(root, path)} imports canonical OXC outside oxc_adapter`,
    );
    assert.doesNotMatch(
      manifest,
      /github\.com\/oxc-project\/oxc|(?:^|[,\s])(?:git|rev|branch|tag)\s*=.*oxc/,
      `${relative(root, path)} must not declare an OXC source`,
    );
  }
});

test("the workspace has no Cargo patch, vendor tree, checkout, or copied OXC crate", async () => {
  const manifests = [
    [join(root, "Cargo.toml"), await readFile(join(root, "Cargo.toml"), "utf8")],
    ...(await cargoManifests(join(root, "crates"))),
  ];
  for (const [path, manifest] of manifests) {
    assert.doesNotMatch(
      manifest,
      /^\s*\[patch(?:\.|\])/m,
      `${relative(root, path)} must not patch Cargo sources`,
    );
    assert.doesNotMatch(
      manifest,
      /^\s*replace-with\s*=/m,
      `${relative(root, path)} must not replace Cargo sources`,
    );
  }

  for (const name of ["config", "config.toml"]) {
    const path = join(root, ".cargo", name);
    let config;
    try {
      config = await readFile(path, "utf8");
    } catch (error) {
      if (error?.code === "ENOENT") continue;
      throw error;
    }
    assert.doesNotMatch(
      config,
      /^\s*\[source\.|^\s*(?:replace-with|directory)\s*=/m,
      `${relative(root, path)} must not replace canonical Cargo sources`,
    );
  }

  const forbiddenDirectories = [];
  await walkDirectories(root, (path) => {
    const component = basename(path).toLowerCase();
    if (
      ["vendor", "vendored", "checkouts", "upstream"].includes(component) ||
      /^(?:oxc[-_])?(?:source|sources|checkout|upstream|vendor)$/.test(component) ||
      /^(?:source|checkout|upstream|vendor)[-_]oxc$/.test(component)
    ) {
      forbiddenDirectories.push(relative(root, path).split(sep).join("/"));
    }
  });
  assert.deepEqual(
    forbiddenDirectories,
    [],
    `forbidden source trees found: ${forbiddenDirectories.join(", ")}`,
  );

  for (const [path, manifest] of manifests) {
    if (path === adapterManifestPath) continue;
    const packageName = manifest.match(/^name\s*=\s*"([^"]+)"/m)?.[1];
    assert.equal(
      expectedAdapterDependencies.includes(packageName),
      false,
      `${relative(root, path)} copies canonical OXC crate ${packageName}`,
    );
  }
});

test("editor docs distinguish OXC's compiled tool seam from unavailable runtime hooks", async () => {
  const guide = await readFile(editorIntegrationPath, "utf8");
  assert.match(guide, /ToolBuilder/);
  assert.match(guide, /compile-time Rust embedding seam/i);
  assert.match(guide, /not a\s+runtime-configurable parser or tool loader/i);
  assert.match(guide, /github\.com\/oxc-project\/oxc\/discussions\/21936/);
  assert.match(guide, /github\.com\/oxc-project\/oxc\/pull\/24262/);
  assert.match(guide, /github\.com\/oxc-project\/oxc\/pull\/20250/);
});
