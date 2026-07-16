import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import test from "node:test";
import { pathToFileURL } from "node:url";

const root = resolve(import.meta.dirname, "../..");
const lintBin = process.env.OXC_TSRX_LINT_BIN ?? join(root, "target/release/oxc-tsrx");
const formatBin = process.env.OXC_TSRX_FORMAT_BIN ?? join(root, "target/release/oxc-tsrx-fmt");

function run(command, args, options) {
  return new Promise((resolveRun, rejectRun) => {
    const child = spawn(command, args, { ...options, stdio: ["ignore", "pipe", "pipe"] });
    let stdout = "";
    let stderr = "";
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => (stdout += chunk));
    child.stderr.on("data", (chunk) => (stderr += chunk));
    child.on("error", rejectRun);
    child.on("close", (status) => resolveRun({ status, stdout, stderr }));
  });
}

test("drop-in package roots preserve canonical config APIs and add TSRX formatting", async () => {
  process.env.OXC_TSRX_LINT_BIN = lintBin;
  process.env.OXC_TSRX_FORMAT_BIN = formatBin;

  const oxlint = await import(pathToFileURL(join(root, "packages/oxlint/dist/index.js")).href);
  const oxfmt = await import(pathToFileURL(join(root, "packages/oxfmt/dist/index.js")).href);
  const upstream = await import("oxfmt-current");

  const lintConfig = { rules: { "no-debugger": "error" } };
  const formatConfig = { semi: false };
  assert.equal(oxlint.defineConfig(lintConfig), lintConfig);
  assert.equal(oxfmt.defineConfig(formatConfig), formatConfig);

  const ordinary = "export const value = { double: true };\n";
  assert.deepEqual(
    await oxfmt.format("ordinary.tsx", ordinary, { semi: false }),
    await upstream.format("ordinary.tsx", ordinary, { semi: false }),
  );

  const source = 'export function View( ) @{<div title="proof">TSRX</div>}';
  const formatted = await oxfmt.format("View.tsrx", source);
  assert.deepEqual(formatted.errors, []);
  assert.match(formatted.code, /function View\(\) @\{/);
  assert.match(formatted.code, /<div title="proof">TSRX<\/div>/);
  assert.doesNotMatch(formatted.code, /_t[0-9a-f]+_/);
  assert.deepEqual(await oxfmt.format("View.tsrx", formatted.code), formatted);
});

test("format package reports a missing native artifact instead of silently delegating TSRX", async () => {
  const directory = await mkdtemp(join(tmpdir(), "oxc-tsrx-package-missing-"));
  const previous = process.env.OXC_TSRX_FORMAT_BIN;
  process.env.OXC_TSRX_FORMAT_BIN = join(directory, "missing-oxc-tsrx-fmt");
  try {
    const moduleUrl = pathToFileURL(join(root, "packages/oxfmt/dist/index.js"));
    moduleUrl.searchParams.set("missing-native", String(Date.now()));
    const oxfmt = await import(moduleUrl.href);
    await assert.rejects(
      oxfmt.format("View.tsrx", "function View() @{ <div />; }"),
      /native.*(missing|not found|unavailable)/i,
    );
  } finally {
    if (previous === undefined) delete process.env.OXC_TSRX_FORMAT_BIN;
    else process.env.OXC_TSRX_FORMAT_BIN = previous;
    await rm(directory, { recursive: true, force: true });
  }
});

test("package manifests have Vite+ compatible root and bin shapes", async () => {
  for (const name of ["oxlint", "oxfmt"]) {
    const packageRoot = join(root, "packages", name);
    const manifest = JSON.parse(await readFile(join(packageRoot, "package.json"), "utf8"));
    assert.equal(manifest.name, `${name}-tsrx`);
    assert.equal(manifest.type, "module");
    assert.equal(manifest.main, "./dist/index.js");
    assert.equal(manifest.bin[name], `./bin/${name}`);
    assert.ok(manifest.exports["."]);
    assert.ok(manifest.exports["./package.json"]);
  }
});

test("mixed package lint delegates ordinary TSX and parses each TSRX file once", async () => {
  const fixture = join(root, "tests/fixtures/vite/toolchain/diagnostics");
  const result = await run(
    join(root, "packages/oxlint/bin/oxlint"),
    [
      "--format=json",
      "--config",
      join(fixture, ".oxlintrc.json"),
      join(fixture, "src/ordinary.tsx"),
      join(fixture, "src/view.tsrx"),
    ],
    {
      cwd: root,
      env: { ...process.env, OXC_TSRX_LINT_BIN: lintBin },
    },
  );
  assert.equal(result.status, 1, result.stderr || result.stdout);
  const output = JSON.parse(result.stdout);
  assert.equal(output.number_of_files, 2);
  assert.equal(output.oxcTsrx.parseCount, 1);
  assert.equal(output.oxcTsrx.files.tsrx, 1);
  assert.equal(output.oxcTsrx.files.standard, 0);
  assert.ok(output.diagnostics.some((diagnostic) => diagnostic.filename.endsWith("ordinary.tsx")));
  assert.ok(output.diagnostics.some((diagnostic) => diagnostic.filename.endsWith("view.tsrx")));
});
