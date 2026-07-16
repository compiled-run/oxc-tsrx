import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtemp, readFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import test from "node:test";

const root = resolve(import.meta.dirname, "../..");
const packageRoot = join(root, "packages/vscode");

function run(executable, args, options = {}) {
  return new Promise((resolveRun, rejectRun) => {
    execFile(executable, args, options, (error, stdout, stderr) => {
      if (error) rejectRun(new Error(stderr || stdout, { cause: error }));
      else resolveRun({ stdout, stderr });
    });
  });
}

test("editor package is additive, workspace-native, bundled, and VSIX-packaged", async () => {
  const manifest = JSON.parse(await readFile(join(packageRoot, "package.json"), "utf8"));
  assert.equal(manifest.name, "oxc-tsrx-vscode");
  assert.deepEqual(manifest.extensionKind, ["workspace"]);
  assert.equal(manifest.capabilities.untrustedWorkspaces.supported, false);
  assert.ok(manifest.activationEvents.includes("onLanguage:markless-tsrx"));
  assert.ok(manifest.activationEvents.includes("workspaceContains:**/*.tsrx"));
  assert.equal(manifest.contributes.languages, undefined);
  assert.equal(manifest.main, "./dist/extension.bundle.cjs");

  const directory = await mkdtemp(join(tmpdir(), "oxc-tsrx-vsix-"));
  const output = join(directory, "oxc-tsrx-vscode.vsix");
  await run(join(root, "node_modules/.bin/vsce"), [
    "package",
    "--no-dependencies",
    "--out",
    output,
  ], { cwd: packageRoot });
  const { stdout: listing } = await run("unzip", ["-Z1", output]);
  assert.match(listing, /extension\/dist\/extension\.bundle\.cjs/);
  assert.match(listing, /extension\/package\.json/);
  assert.match(listing, /extension\/README\.md/i);
  assert.doesNotMatch(listing, /node_modules/);
});
