import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { join, resolve } from "node:path";
import test from "node:test";

const root = resolve(import.meta.dirname, "../..");
const version = "0.1.0";
const revision = "8e0ed2ebb96137fb1611cdbd5742d5cb46037d40";

function run(executable, args) {
  return new Promise((resolveRun, rejectRun) => {
    execFile(executable, args, { cwd: root, timeout: 2000 }, (error, stdout, stderr) => {
      if (error) rejectRun(new Error(stderr || stdout, { cause: error }));
      else resolveRun({ stdout, stderr });
    });
  });
}

for (const binary of ["oxc-tsrx", "oxc-tsrx-fmt", "oxc-tsrx-lsp"]) {
  test(`${binary} exposes the package and exact canonical OXC revision`, async () => {
    const executable = join(
      root,
      "target/release",
      process.platform === "win32" ? `${binary}.exe` : binary,
    );
    const { stdout, stderr } = await run(executable, ["--version"]);
    assert.equal(stderr, "");
    assert.equal(stdout, `${binary} ${version} (OXC ${revision})\n`);
  });
}
