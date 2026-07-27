import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
import { join, resolve } from "node:path";
import test from "node:test";

const root = resolve(import.meta.dirname, "../..");

/**
 * The published surface is `oxc-tsrx` plus the eight platform packages a user
 * never names. `@oxc-tsrx/parser`, `@oxc-tsrx/runtime`, `oxlint-tsrx`, and
 * `oxfmt-tsrx` were first-party wrappers around this same code, so they are
 * gone rather than published: everything they exported is reachable at
 * `oxc-tsrx/parser`, `oxc-tsrx/lint`, and `oxc-tsrx/format`, and the `oxlint`
 * and `oxfmt` command names are declared by this package's own `bin`.
 */
test("no first-party wrapper package stands between the user and the toolchain", async () => {
  const workspace = await readdir(join(root, "packages"), { withFileTypes: true });
  const directories = workspace
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name)
    .sort();
  assert.deepEqual(directories, ["native", "toolchain", "tsrx-core-compat", "vscode"]);

  for (const removed of ["parser", "runtime", "oxlint", "oxfmt"]) {
    assert.equal(
      directories.includes(removed),
      false,
      `packages/${removed} must not come back as a separate published package`,
    );
  }

  const launch = JSON.parse(
    await readFile(join(root, "docs/releasing/v0.1.0-launch.json"), "utf8"),
  );
  assert.equal(launch.npm.publishOrder.length, 9);
  for (const wrapper of ["@oxc-tsrx/parser", "@oxc-tsrx/runtime", "oxlint-tsrx", "oxfmt-tsrx"]) {
    assert.equal(launch.npm.publishOrder.includes(wrapper), false, wrapper);
  }
});

test("oxc-tsrx is the complete public toolchain boundary", async () => {
  const packageRoot = join(root, "packages", "toolchain");
  const manifest = JSON.parse(await readFile(join(packageRoot, "package.json"), "utf8"));

  assert.equal(manifest.name, "oxc-tsrx");
  assert.equal(manifest.version, "0.1.4");
  assert.deepEqual(manifest.bin, {
    "oxc-tsrx": "./bin/oxc-tsrx",
    oxlint: "./bin/oxlint",
    oxfmt: "./bin/oxfmt",
    "oxc-tsrx-lint": "./bin/oxc-tsrx-lint",
    "oxc-tsrx-fmt": "./bin/oxc-tsrx-fmt",
    "oxc-tsrx-lsp": "./bin/oxc-tsrx-lsp",
  });
  // Every dependency is third-party. Nothing first-party sits between this
  // package and its own implementation.
  assert.deepEqual(manifest.dependencies, {
    "@oxc-project/types": "0.140.0",
    "oxfmt-current": "npm:oxfmt@0.59.0",
    "oxlint-current": "npm:oxlint@1.74.0",
    "oxlint-tsgolint": "0.24.0",
    "tinyglobby": "0.2.17",
  });
  // The eight-platform split is what makes a user download one binary instead
  // of eight, and this package is now the only place that declares it.
  assert.deepEqual(Object.entries(manifest.optionalDependencies), [
    ["@oxc-tsrx/native-darwin-arm64", "0.1.4"],
    ["@oxc-tsrx/native-darwin-x64", "0.1.4"],
    ["@oxc-tsrx/native-linux-arm64-gnu", "0.1.4"],
    ["@oxc-tsrx/native-linux-arm64-musl", "0.1.4"],
    ["@oxc-tsrx/native-linux-x64-gnu", "0.1.4"],
    ["@oxc-tsrx/native-linux-x64-musl", "0.1.4"],
    ["@oxc-tsrx/native-win32-arm64-msvc", "0.1.4"],
    ["@oxc-tsrx/native-win32-x64-msvc", "0.1.4"],
  ]);
  assert.deepEqual(Object.keys(manifest.exports), [
    ".",
    "./parser",
    "./lint",
    "./lint/plugins-dev",
    "./format",
    "./compat",
    "./provider-resolve",
    "./package.json",
  ]);
  assert.equal(manifest.repository.directory, "packages/toolchain");
  assert.match(manifest.homepage, /^https:\/\//);
  assert.match(manifest.bugs.url, /^https:\/\//);
  assert.ok(manifest.keywords.includes("tsrx"));
  assert.deepEqual(manifest.publishConfig, { access: "public", provenance: true });
  assert.equal(manifest.scripts, undefined);
  for (const file of ["README.md", "LICENSE", "THIRD_PARTY_NOTICES.md"]) {
    assert.ok(manifest.files.includes(file));
    assert.ok((await readFile(join(packageRoot, file), "utf8")).length > 100);
  }
});

test("oxc-tsrx declares itself as a static OXC language provider", async () => {
  const packageRoot = join(root, "packages", "toolchain");
  const manifest = JSON.parse(await readFile(join(packageRoot, "package.json"), "utf8"));

  assert.deepEqual(manifest.oxc, {
    provider: {
      protocol: 1,
      id: "tsrx",
      languages: [
        {
          id: "tsrx",
          extensions: [".tsrx"],
          capabilities: {
            parse: { module: "./parser" },
            lint: { bin: "oxc-tsrx-lint" },
            format: { bin: "oxc-tsrx-fmt" },
            lsp: { bin: "oxc-tsrx-lsp" },
          },
        },
      ],
    },
  });

  // The declaration may only point at this package's own published surface.
  const [language] = manifest.oxc.provider.languages;
  for (const target of Object.values(language.capabilities)) {
    if (target.bin !== undefined) {
      assert.equal(typeof manifest.bin[target.bin], "string", target.bin);
      assert.ok((await readFile(join(packageRoot, manifest.bin[target.bin]), "utf8")).length > 0);
    } else {
      assert.ok(manifest.exports[target.module], target.module);
    }
  }
  assert.ok(manifest.files.includes("dist"));
  assert.ok(manifest.exports["./package.json"], "a provider must publish its own manifest");
  for (const file of ["dist/provider-resolve.js", "dist/provider-resolve.d.ts"]) {
    assert.ok((await readFile(join(packageRoot, file), "utf8")).length > 100);
  }
});
