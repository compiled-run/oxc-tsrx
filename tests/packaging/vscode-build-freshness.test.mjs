import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { cp, mkdir, mkdtemp, readFile, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import test from "node:test";

const root = resolve(import.meta.dirname, "../..");

async function linkPackage(fixtureRoot, packageName, source) {
  const destination = join(fixtureRoot, "node_modules", ...packageName.split("/"));
  await mkdir(dirname(destination), { recursive: true });
  await symlink(source, destination, process.platform === "win32" ? "junction" : "dir");
}

async function createFixture() {
  const fixtureRoot = await mkdtemp(join(tmpdir(), "oxc-tsrx-vscode-build-"));
  await mkdir(join(fixtureRoot, "packages"), { recursive: true });
  await Promise.all([
    cp(join(root, "packages/vscode"), join(fixtureRoot, "packages/vscode"), {
      recursive: true,
    }),
    cp(join(root, "packages/runtime"), join(fixtureRoot, "packages/runtime"), {
      recursive: true,
    }),
  ]);
  await Promise.all([
    linkPackage(fixtureRoot, "rolldown", join(root, "node_modules/rolldown")),
    linkPackage(
      fixtureRoot,
      "vscode-languageclient",
      join(root, "node_modules/vscode-languageclient"),
    ),
    linkPackage(
      fixtureRoot,
      "@oxc-tsrx/runtime",
      join(fixtureRoot, "packages/runtime"),
    ),
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

  const freshCheck = runBuild(fixtureRoot, "--check");
  assert.equal(freshCheck.status, 0, freshCheck.stderr || freshCheck.stdout);
  assert.equal(await readFile(bundlePath, "utf8"), freshBundle);
});
