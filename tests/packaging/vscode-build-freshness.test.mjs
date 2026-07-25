import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { cp, mkdir, mkdtemp, readFile, realpath, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import test from "node:test";

const root = resolve(import.meta.dirname, "../..");

async function linkPackage(fixtureRoot, packageName, source) {
  const destination = join(fixtureRoot, "node_modules", ...packageName.split("/"));
  await mkdir(dirname(destination), { recursive: true });
  await symlink(source, destination, process.platform === "win32" ? "junction" : "dir");
}

// The bundle is the VSIX's only entry point and ships no node_modules, so
// `require("oxc-tsrx/provider-resolve")` has to be resolved and inlined at build
// time. A fixture that cannot resolve it would build a differently shaped bundle
// (Rolldown warns UNRESOLVED_IMPORT and externalises the specifier) and the
// freshness check would then only compare that degraded build against itself.
const RESOLVER_REGION = "//#region packages/toolchain/dist/provider-resolve.js";
const CLIENT_REGION = "//#region packages/vscode/dist/provider-client.cjs";
const EXTERNALISED_RESOLVER = /require\(["']oxc-tsrx\/provider-resolve["']\)/u;

const COMMITTED_BUNDLE = join(root, "packages/vscode/dist/extension.bundle.cjs");
const RESOLVER_SOURCE = "packages/toolchain/dist/provider-resolve.js";

/**
 * The bundled copy of one module, from its region marker to the next
 * `//#endregion`. Shape checks alone cannot tell a fresh region from a stale
 * one: a bundle carrying an old but perfectly well-shaped resolver satisfies
 * every marker assertion in this file. Comparing the bytes of the region is
 * what makes staleness fail.
 */
function moduleRegion(bundle, marker) {
  const start = bundle.indexOf(marker);
  assert.notEqual(start, -1, `${marker} is missing`);
  const end = bundle.indexOf("\n//#endregion", start);
  assert.notEqual(end, -1, `${marker} is not terminated`);
  return bundle.slice(start, end);
}

async function createFixture({ linkProvider = true } = {}) {
  // Rolldown reports module regions as paths relative to the build's cwd, after
  // resolving symlinks. Anchor the fixture on its real path so those regions read
  // the same way they do in the repository build.
  const fixtureRoot = await realpath(await mkdtemp(join(tmpdir(), "oxc-tsrx-vscode-build-")));
  await mkdir(join(fixtureRoot, "packages"), { recursive: true });
  await Promise.all([
    cp(join(root, "packages/vscode"), join(fixtureRoot, "packages/vscode"), {
      recursive: true,
    }),
    cp(join(root, "packages/toolchain"), join(fixtureRoot, "packages/toolchain"), {
      recursive: true,
    }),
  ]);
  await Promise.all([
    linkPackage(fixtureRoot, "rolldown", join(root, "node_modules/rolldown")),
    linkPackage(fixtureRoot, "tinyglobby", join(root, "node_modules/tinyglobby")),
    linkPackage(
      fixtureRoot,
      "vscode-languageclient",
      join(root, "node_modules/vscode-languageclient"),
    ),
    // The fixture must resolve the provider resolver exactly the way the repo
    // does, from a package inside the fixture, or the build it verifies is not
    // the build we ship.
    linkProvider
      ? linkPackage(fixtureRoot, "oxc-tsrx", join(fixtureRoot, "packages/toolchain"))
      : Promise.resolve(),
  ]);
  return fixtureRoot;
}

function runBuild(fixtureRoot, ...args) {
  return spawnSync(process.execPath, ["packages/vscode/build.mjs", ...args], {
    cwd: fixtureRoot,
    encoding: "utf8",
  });
}

test("editor bundle freshness check is read-only and fails closed", async (context) => {
  const fixtureRoot = await createFixture();
  context.after(() => rm(fixtureRoot, { recursive: true, force: true }));
  const bundlePath = join(fixtureRoot, "packages/vscode/dist/extension.bundle.cjs");
  const staleBundle = `${await readFile(bundlePath, "utf8")}\n// deliberately stale\n`;
  await writeFile(bundlePath, staleBundle);

  const staleCheck = runBuild(fixtureRoot, "--check");
  assert.notEqual(staleCheck.status, 0, "a stale committed bundle must fail --check");
  assert.match(`${staleCheck.stderr}\n${staleCheck.stdout}`, /bundle.*stale/iu);
  assert.equal(
    await readFile(bundlePath, "utf8"),
    staleBundle,
    "--check must not rewrite the committed bundle",
  );

  const build = runBuild(fixtureRoot);
  assert.equal(build.status, 0, build.stderr || build.stdout);
  const freshBundle = await readFile(bundlePath, "utf8");
  assert.notEqual(freshBundle, staleBundle);

  // The fixture build must have the shape of the shipped build, not merely be
  // self-consistent: every module the VSIX carries is inlined, nothing this
  // extension owns is left as a runtime require, and Rolldown resolved it all.
  assert.doesNotMatch(
    `${build.stderr}\n${build.stdout}`,
    /UNRESOLVED_IMPORT|Could not resolve/iu,
    "the fixture must resolve every import the real build resolves",
  );
  assert.ok(freshBundle.includes(RESOLVER_REGION), RESOLVER_REGION);
  assert.ok(freshBundle.includes(CLIENT_REGION), CLIENT_REGION);
  assert.doesNotMatch(
    freshBundle,
    EXTERNALISED_RESOLVER,
    "the provider resolver must be inlined, never required from the user's workspace",
  );

  const freshCheck = runBuild(fixtureRoot, "--check");
  assert.equal(freshCheck.status, 0, freshCheck.stderr || freshCheck.stdout);
  assert.equal(await readFile(bundlePath, "utf8"), freshBundle);
});

test("the committed editor bundle carries the provider resolver it discovers with", async () => {
  const bundle = await readFile(COMMITTED_BUNDLE, "utf8");
  assert.ok(bundle.includes(RESOLVER_REGION), RESOLVER_REGION);
  assert.ok(bundle.includes(CLIENT_REGION), CLIENT_REGION);
  assert.doesNotMatch(bundle, EXTERNALISED_RESOLVER);
});

// The shape assertions above are satisfied by a bundle built from *any* version
// of the resolver, so on their own they let a VSIX ship a resolver that no
// longer matches its source. `npm run build:editor:check` catches that, but it
// is not part of `npm run test:packaging`, so nothing in the packaging suite
// noticed. This test makes it fail here too.
test("the committed editor bundle inlines the current resolver, byte for byte", async (context) => {
  const fixtureRoot = await createFixture();
  context.after(() => rm(fixtureRoot, { recursive: true, force: true }));
  const fixtureBundle = join(fixtureRoot, "packages/vscode/dist/extension.bundle.cjs");

  // The fixture holds a copy of the repository's current resolver source, so
  // building it produces the region the committed bundle should already have.
  const build = runBuild(fixtureRoot);
  assert.equal(build.status, 0, build.stderr || build.stdout);

  const committed = moduleRegion(await readFile(COMMITTED_BUNDLE, "utf8"), RESOLVER_REGION);
  const fresh = moduleRegion(await readFile(fixtureBundle, "utf8"), RESOLVER_REGION);
  assert.equal(
    committed,
    fresh,
    `packages/vscode/dist/extension.bundle.cjs no longer matches ${RESOLVER_SOURCE}; run npm run build:editor`,
  );

  // Contrast case, so the equality above cannot pass vacuously. Edit the
  // fixture's resolver the way a real change would, rebuild, and require the
  // region to move. Without this, the assertion would still pass if
  // `moduleRegion` silently returned the same slice for every input.
  const probeSource = join(fixtureRoot, RESOLVER_SOURCE);
  const original = await readFile(probeSource, "utf8");
  const probed = original.replace(
    "function isPlainObject(value) {",
    'function isPlainObject(value) {\n  if (value === "staleness-probe") return false;',
  );
  assert.notEqual(probed, original, "the probe must actually patch the resolver source");
  await writeFile(probeSource, probed);

  const rebuild = runBuild(fixtureRoot);
  assert.equal(rebuild.status, 0, rebuild.stderr || rebuild.stdout);
  const stale = moduleRegion(await readFile(fixtureBundle, "utf8"), RESOLVER_REGION);
  assert.match(stale, /staleness-probe/u, "the probe must reach the bundled region");
  assert.notEqual(
    stale,
    committed,
    "a resolver change must move the bundled region, or this guard proves nothing",
  );
});

// Contrast case. Without this the inlining assertions above could pass for a
// fixture that never had a chance to externalise anything, and the guard would
// be checking a property nothing can break.
test("a fixture that cannot resolve the provider builds the wrong shape", async (context) => {
  const fixtureRoot = await createFixture({ linkProvider: false });
  context.after(() => rm(fixtureRoot, { recursive: true, force: true }));

  const build = runBuild(fixtureRoot);
  assert.equal(build.status, 0, build.stderr || build.stdout);
  assert.match(
    `${build.stderr}\n${build.stdout}`,
    /UNRESOLVED_IMPORT/u,
    "an unlinked provider is what Rolldown reports as unresolved",
  );

  const bundle = await readFile(
    join(fixtureRoot, "packages/vscode/dist/extension.bundle.cjs"),
    "utf8",
  );
  assert.equal(bundle.includes(RESOLVER_REGION), false);
  assert.match(
    bundle,
    EXTERNALISED_RESOLVER,
    "the degraded build leaves the resolver as a runtime require the VSIX cannot satisfy",
  );
});
