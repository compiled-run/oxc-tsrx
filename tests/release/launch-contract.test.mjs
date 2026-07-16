import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtemp, readFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import test from "node:test";
import { NATIVE_TARGETS, nativePackageName } from "../../packages/runtime/dist/targets.js";

const root = resolve(import.meta.dirname, "../..");
const repository = "https://github.com/thejackshelton/oxc-tsrx";
const homepage = "https://thejackshelton.github.io/oxc-tsrx/";
const publicDirectories = ["runtime", "oxlint", "oxfmt"];

const readJson = async (path) => JSON.parse(await readFile(path, "utf8"));

function run(executable, args, options = {}) {
  return new Promise((resolveRun, rejectRun) => {
    execFile(
      executable,
      args,
      {
        cwd: options.cwd ?? root,
        env: options.env ?? process.env,
        maxBuffer: 32 * 1024 * 1024,
      },
      (error, stdout, stderr) => {
        if (error) rejectRun(new Error(stderr || stdout, { cause: error }));
        else resolveRun({ stdout, stderr });
      },
    );
  });
}

test("root and public packages expose one launch identity", async () => {
  const rootManifest = await readJson(join(root, "package.json"));
  assert.equal(rootManifest.license, "MIT");
  assert.equal(rootManifest.homepage, homepage);
  assert.equal(rootManifest.repository.url, `git+${repository}.git`);
  assert.equal(rootManifest.bugs.url, `${repository}/issues`);
  assert.ok(rootManifest.keywords.includes("tsrx"));

  for (const directory of publicDirectories) {
    const manifest = await readJson(join(root, "packages", directory, "package.json"));
    assert.equal(manifest.version, rootManifest.version, directory);
    assert.equal(manifest.homepage, homepage, directory);
    assert.equal(manifest.repository.url, `git+${repository}.git`, directory);
    assert.equal(manifest.bugs.url, `${repository}/issues`, directory);
    assert.equal(manifest.license, "MIT", directory);
  }

  const vscode = await readJson(join(root, "packages", "vscode", "package.json"));
  assert.equal(vscode.version, rootManifest.version);
  assert.equal(vscode.homepage, homepage);
  assert.equal(vscode.repository.url, `${repository}.git`);
});

test("launch manifest names every byte set and keeps external actions approval-gated", async () => {
  const launch = await readJson(join(root, "docs", "releasing", "v0.1.0-launch.json"));
  assert.equal(launch.schemaVersion, 1);
  assert.equal(launch.version, "0.1.0");
  assert.equal(launch.repository, repository);
  assert.equal(launch.site.url, homepage);
  assert.equal(launch.site.artifact, "docs/dist");
  assert.equal(launch.site.workflow, ".github/workflows/pages.yml");
  assert.equal(launch.site.trigger, "workflow_dispatch");

  const nativeNames = NATIVE_TARGETS.map(nativePackageName);
  assert.deepEqual(launch.npm.publishOrder.slice(0, nativeNames.length), nativeNames);
  assert.deepEqual(launch.npm.publishOrder.slice(nativeNames.length), [
    "@oxc-tsrx/runtime",
    "oxlint-tsrx",
    "oxfmt-tsrx",
  ]);
  assert.deepEqual(launch.vscode.targets, NATIVE_TARGETS.map(({ vscodeTarget }) => vscodeTarget));
  assert.match(launch.social.text, /OXC for TSRX/u);
  assert.match(launch.social.text, /oxlint-tsrx/u);
  assert.match(launch.social.text, /oxfmt-tsrx/u);
  assert.match(launch.social.text, new RegExp(homepage.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")));
  assert.deepEqual(launch.requiredApprovals, [
    "repository-push",
    "npm-publication",
    "vscode-marketplace-publication",
    "website-deployment",
    "social-announcement",
  ]);

  const [notes, runbook, workflow] = await Promise.all([
    readFile(join(root, "docs", "releasing", "v0.1.0.md"), "utf8"),
    readFile(join(root, "docs", "releasing", "launch-runbook.md"), "utf8"),
    readFile(join(root, ".github", "workflows", "pages.yml"), "utf8"),
  ]);
  assert.match(notes, /^# OXC for TSRX 0\.1\.0/mu);
  assert.match(notes, /Known boundaries/u);
  assert.match(runbook, /exact approval/u);
  assert.match(runbook, /COMMIT_SHA/u);
  assert.match(runbook, /RUN_ID/u);
  assert.match(workflow, /workflow_dispatch:/u);
  assert.match(workflow, /npm run test:site:static/u);
  assert.doesNotMatch(workflow, /^\s+push:/mu);
  assert.doesNotMatch(workflow, /npm publish|vsce publish|git push|curl .*social/iu);
});

test("all platform-independent npm payloads pass pack dry-run", async () => {
  const npmCache = await mkdtemp(join(tmpdir(), "oxc-tsrx-pack-cache-"));
  for (const directory of publicDirectories) {
    const { stdout, stderr } = await run("npm", [
      "pack",
      "--dry-run",
      "--json",
      `./packages/${directory}`,
    ], { env: { ...process.env, npm_config_cache: npmCache } });
    assert.equal(stderr, "", directory);
    const [result] = JSON.parse(stdout);
    assert.equal(result.name, (await readJson(join(root, "packages", directory, "package.json"))).name);
    assert.ok(result.files.some(({ path }) => path === "LICENSE"), directory);
    assert.ok(result.files.some(({ path }) => path === "README.md"), directory);
    assert.ok(result.files.some(({ path }) => path === "THIRD_PARTY_NOTICES.md"), directory);
    assert.equal(result.files.some(({ path }) => path.startsWith("test")), false, directory);
  }
});
