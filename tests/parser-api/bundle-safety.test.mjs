import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import test from "node:test";
import { build } from "rolldown";

const root = resolve(import.meta.dirname, "../..");

function runNode(path) {
  return new Promise((resolveRun, rejectRun) => {
    execFile(process.execPath, [path], { cwd: root }, (error, stdout, stderr) => {
      if (error) rejectRun(new Error(stderr || stdout, { cause: error }));
      else resolveRun({ stdout, stderr });
    });
  });
}

test("the parser can be bundled without a runtime package.json dependency", async () => {
  const temporary = await mkdtemp(join(tmpdir(), "oxc-tsrx-parser-bundle-"));
  try {
    const entry = join(temporary, "entry.mjs");
    const bundle = join(temporary, "parser-bundle.cjs");
    const parser = join(root, "packages/parser/index.js");
    await writeFile(
      entry,
      `import { createRequire } from "node:module";\n` +
        `import { capabilities } from ${JSON.stringify(parser)};\n` +
        `const require = createRequire(import.meta.url);\n` +
        `const loadedAddons = Object.keys(require.cache).filter((path) => path.endsWith("parser.node"));\n` +
        `process.stdout.write(JSON.stringify({ apiVersion: capabilities.apiVersion, lazy: capabilities.lazy, loadedAddons }));\n`,
      "utf8",
    );
    await build({
      input: entry,
      platform: "node",
      output: { file: bundle, format: "cjs", codeSplitting: false, sourcemap: false },
    });

    const bundledSource = await readFile(bundle, "utf8");
    assert.doesNotMatch(bundledSource, /require\(["']\.\/package\.json["']\)/u);
    const result = await runNode(bundle);
    assert.equal(result.stderr, "");
    assert.deepEqual(JSON.parse(result.stdout), { apiVersion: 1, lazy: true, loadedAddons: [] });
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
});
