import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import {
  chmod,
  copyFile,
  mkdir,
  mkdtemp,
  readFile,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { delimiter, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { installPhysicalToolPackages } from "../vite/physical-consumer.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "../..");
const binary = resolve(process.env.OXLINT_BIN ?? join(root, "target/release/oxc-tsrx"));
const tsgolint = join(root, "node_modules/.bin/tsgolint");
const singleRoot = join(root, "tests/fixtures/type-aware/single");
const singleSource = join(singleRoot, "View.tsrx");
const projectRoot = join(root, "tests/fixtures/type-aware/project");
const projectView = join(projectRoot, "View.tsrx");
const projectService = join(projectRoot, "service.tsrx");
const typeCheckRoot = join(root, "tests/fixtures/type-aware/type-check");
const typeCheckSource = join(typeCheckRoot, "View.tsrx");
const fixRoot = join(root, "tests/fixtures/type-aware/fix");
const fixSource = join(fixRoot, "View.tsrx");
const controlRoot = join(root, "tests/fixtures/type-aware/control");
const controlSource = join(controlRoot, "View.tsrx");
const componentRoot = join(root, "tests/fixtures/type-aware/component-project");
const componentView = join(componentRoot, "View.tsrx");
const componentApp = join(componentRoot, "App.tsx");

function run(cwd, args, env = process.env, executable = binary) {
  return new Promise((resolvePromise, reject) => {
    const child = spawn(executable, args, {
      cwd,
      env,
      stdio: ["ignore", "pipe", "pipe"],
    });
    let stdout = "";
    let stderr = "";
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => (stdout += chunk));
    child.stderr.on("data", (chunk) => (stderr += chunk));
    child.once("error", reject);
    child.once("close", (code, signal) => {
      resolvePromise({ code, signal, stdout, stderr });
    });
  });
}

function parseJson(result) {
  assert.equal(result.signal, null);
  const start = result.stdout.indexOf("{");
  assert.notEqual(start, -1, result.stderr || result.stdout);
  return JSON.parse(result.stdout.slice(start));
}

function byteOffset(source, needle) {
  const characterOffset = source.indexOf(needle);
  assert.notEqual(characterOffset, -1, `Missing ${needle}`);
  return Buffer.byteLength(source.slice(0, characterOffset));
}

test("type-aware lint maps a real tsgolint diagnostic to authored TSRX", async () => {
  const source = await readFile(singleSource, "utf8");
  const result = await run(singleRoot, ["--format=json", "--type-aware", singleSource]);

  assert.equal(result.code, 1, result.stderr || result.stdout);
  const output = parseJson(result);
  const diagnostic = output.diagnostics.find(
    (item) => item.rule === "no-floating-promises",
  );
  assert.ok(diagnostic, result.stdout);
  assert.equal(diagnostic.filename, singleSource);
  assert.equal(diagnostic.code, "typescript(no-floating-promises)");
  assert.equal(diagnostic.severity, "error");
  assert.equal(
    diagnostic.labels.some(
      (label) =>
        label.span.offset === byteOffset(source, "save();") &&
        label.span.length === Buffer.byteLength("save();"),
    ),
    true,
    result.stdout,
  );
  assert.equal(output.oxcTsrx.parseCount, 1);
  assert.equal(output.oxcTsrx.typeAware, true);
  assert.equal(output.oxcTsrx.typeAwareFiles, 1);
  assert.equal(output.oxcTsrx.typeAwareProcesses, 1);
});

test("a project batch preserves explicit .tsrx imports and authored override intent", async () => {
  const source = await readFile(projectView, "utf8");
  const result = await run(projectRoot, [
    "--format=json",
    "--type-aware",
    projectView,
    projectService,
  ]);

  assert.equal(result.code, 1, result.stderr || result.stdout);
  const output = parseJson(result);
  const diagnostics = output.diagnostics.filter(
    (item) => item.rule === "no-floating-promises",
  );
  assert.equal(diagnostics.length, 1, result.stdout);
  assert.equal(diagnostics[0].filename, projectView);
  assert.equal(diagnostics[0].labels[0].span.offset, byteOffset(source, "save();"));
  assert.equal(output.number_of_files, 2);
  assert.equal(output.oxcTsrx.parseCount, 2);
  assert.equal(output.oxcTsrx.typeAwareFiles, 2);
  assert.equal(output.oxcTsrx.typeAwareProcesses, 1);
});

test("--type-check maps TypeScript compiler diagnostics to authored bytes", async () => {
  const source = await readFile(typeCheckSource, "utf8");
  const result = await run(typeCheckRoot, [
    "--format=json",
    "--type-check",
    typeCheckSource,
  ]);

  assert.equal(result.code, 1, result.stderr || result.stdout);
  const output = parseJson(result);
  const diagnostic = output.diagnostics.find((item) => item.code === "typescript(TS2322)");
  assert.ok(diagnostic, result.stdout);
  assert.equal(diagnostic.filename, typeCheckSource);
  assert.equal(
    diagnostic.labels.some(
      (label) => label.span.offset === byteOffset(source, "count: number"),
    ),
    true,
    result.stdout,
  );
  assert.equal(output.oxcTsrx.typeCheck, true);
  assert.equal(output.oxcTsrx.typeAwareProcesses, 1);
});

test("--fix applies an identity-safe type-aware edit and validates TSRX", async () => {
  const cwd = await mkdtemp(join(tmpdir(), "oxc-tsrx-type-fix-"));
  const sourcePath = join(cwd, "View.tsrx");
  await copyFile(fixSource, sourcePath);
  await copyFile(join(fixRoot, ".oxlintrc.json"), join(cwd, ".oxlintrc.json"));
  await copyFile(join(fixRoot, "tsconfig.json"), join(cwd, "tsconfig.json"));
  const before = await readFile(sourcePath, "utf8");
  const result = await run(
    cwd,
    ["--format=json", "--type-aware", "--fix", sourcePath],
    { ...process.env, OXLINT_TSGOLINT_PATH: tsgolint },
  );

  assert.equal(result.code, 0, result.stderr || result.stdout);
  const output = parseJson(result);
  const after = await readFile(sourcePath, "utf8");
  assert.match(before, /identity<string>/);
  assert.doesNotMatch(after, /identity<string>/);
  assert.match(after, /identity\("saved"\)/);
  assert.match(after, /export function View\(\) @\{/);
  assert.equal(output.oxcTsrx.fixes.applied, 1);
  assert.equal(output.oxcTsrx.reparseCount, 1);
});

test("--fix rejects type-aware suggestions that may change meaning", async () => {
  const cwd = await mkdtemp(join(tmpdir(), "oxc-tsrx-type-suggestion-"));
  const sourcePath = join(cwd, "View.tsrx");
  await copyFile(singleSource, sourcePath);
  await copyFile(join(singleRoot, ".oxlintrc.json"), join(cwd, ".oxlintrc.json"));
  await copyFile(join(singleRoot, "tsconfig.json"), join(cwd, "tsconfig.json"));
  const before = await readFile(sourcePath, "utf8");
  const result = await run(
    cwd,
    ["--format=json", "--type-aware", "--fix", sourcePath],
    { ...process.env, OXLINT_TSGOLINT_PATH: tsgolint },
  );

  assert.equal(result.code, 1, result.stderr || result.stdout);
  const output = parseJson(result);
  assert.equal(await readFile(sourcePath, "utf8"), before);
  assert.ok(output.oxcTsrx.fixes.rejected > 0, result.stdout);
  assert.ok(output.diagnostics.some((item) => item.rule === "no-floating-promises"));
});

test("missing and unsupported tsgolint binaries fail without a silent downgrade", async () => {
  const missing = await run(
    singleRoot,
    ["--format=json", "--type-aware", singleSource],
    { ...process.env, OXLINT_TSGOLINT_PATH: join(tmpdir(), "missing-tsgolint") },
  );
  assert.equal(missing.code, 2);
  assert.equal(missing.stdout, "");
  assert.match(missing.stderr, /OXLINT_TSGOLINT_PATH|executable/i);

  const packageRoot = await mkdtemp(join(tmpdir(), "oxc-tsrx-old-tsgolint-"));
  const binDirectory = join(packageRoot, "bin");
  const executable = join(binDirectory, "tsgolint");
  await mkdir(binDirectory, { recursive: true });
  await writeFile(
    join(packageRoot, "package.json"),
    JSON.stringify({ name: "oxlint-tsgolint", version: "0.23.0" }),
  );
  await writeFile(executable, "#!/bin/sh\nexit 0\n");
  await chmod(executable, 0o755);
  const unsupported = await run(
    singleRoot,
    ["--format=json", "--type-aware", singleSource],
    { ...process.env, OXLINT_TSGOLINT_PATH: executable },
  );
  assert.equal(unsupported.code, 2);
  assert.equal(unsupported.stdout, "");
  assert.match(unsupported.stderr, /unsupported.*0\.23\.0.*0\.24\.0/i);
});

test("type projection preserves loop and branch scopes without synthetic type errors", async () => {
  const source = await readFile(controlSource, "utf8");
  const result = await run(controlRoot, [
    "--format=json",
    "--type-check",
    controlSource,
  ]);

  assert.equal(result.code, 1, result.stderr || result.stdout);
  const output = parseJson(result);
  const floating = output.diagnostics.filter((item) => item.rule === "no-floating-promises");
  assert.equal(floating.length, 2, result.stdout);
  assert.ok(floating.every((item) => item.filename === controlSource));
  assert.deepEqual(
    floating.map((item) => item.labels[0].span.offset).sort((left, right) => left - right),
    [byteOffset(source, "item.save();"), byteOffset(source, "saveAll();")],
  );
  const compilerDiagnostics = output.diagnostics.filter((item) =>
    item.code.startsWith("typescript(TS"),
  );
  assert.deepEqual(compilerDiagnostics, [], result.stdout);
});

test("cross-file component inference remains usable from ordinary TSX", async () => {
  const result = await run(componentRoot, [
    "--format=json",
    "--type-check",
    componentView,
    componentApp,
  ]);

  assert.equal(result.code, 1, result.stderr || result.stdout);
  const output = parseJson(result);
  assert.deepEqual(
    output.diagnostics.filter((item) => item.code.startsWith("typescript(TS")),
    [],
    result.stdout,
  );
  const floating = output.diagnostics.filter((item) => item.rule === "no-floating-promises");
  assert.equal(floating.length, 1, result.stdout);
  assert.equal(floating[0].filename, componentApp);
  assert.equal(output.oxcTsrx.typeAwareProcesses, 1);
  assert.equal(output.oxcTsrx.parseCount, 2);
});

test("the oxlint-tsrx package enables type awareness from resolved Vite+ config", async () => {
  const cwd = await mkdtemp(join(tmpdir(), "oxc-tsrx-type-vite-plus-"));
  const source = join(cwd, "View.tsrx");
  const modules = join(cwd, "node_modules");
  await mkdir(modules, { recursive: true });
  await installPhysicalToolPackages(modules, "vite-plus-current");
  await copyFile(singleSource, source);
  await copyFile(join(singleRoot, "tsconfig.json"), join(cwd, "tsconfig.json"));
  await copyFile(join(singleRoot, "vite.config.ts"), join(cwd, "vite.config.ts"));
  const result = await run(
    cwd,
    ["--format=json", source],
    {
      ...process.env,
      OXC_TSRX_LINT_BIN: binary,
      OXLINT_TSGOLINT_PATH: tsgolint,
      NODE_PATH: [modules, join(root, "node_modules")].join(delimiter),
      VP_COMMAND: "lint",
      VP_VERSION: "0.2.4",
    },
    join(modules, "oxlint/bin/oxlint"),
  );

  assert.equal(result.code, 1, result.stderr || result.stdout);
  const output = parseJson(result);
  assert.ok(output.diagnostics.some((item) => item.rule === "no-floating-promises"));
  assert.equal(output.oxcTsrx.typeAware, true);
  assert.equal(output.oxcTsrx.typeAwareProcesses, 1);
});
