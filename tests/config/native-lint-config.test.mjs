import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir, mkdtemp, readFile, realpath, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

import { resolveOxlintBytePositions } from "../../packages/toolchain/dist/lint-cli.js";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "../..");
const binary = resolve(process.env.OXLINT_BIN ?? join(root, "target/release/oxc-tsrx"));
// pnpm installs `oxlint-current` under the package that declares it, so it is
// resolved from this file's own package instead of from a hoisted
// repository-root `node_modules`.
const stock = join(
  dirname(createRequire(import.meta.url).resolve("oxlint-current/package.json")),
  "bin/oxlint",
);
const companion = resolve(join(root, "packages/toolchain/bin/oxlint"));

function run(cwd, args, executable = binary, environment = process.env) {
  return new Promise((resolvePromise, reject) => {
    const child = spawn(executable, args, {
      cwd,
      env: environment,
      stdio: ["ignore", "pipe", "pipe"],
    });
    let stdout = "";
    let stderr = "";
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => (stdout += chunk));
    child.stderr.on("data", (chunk) => (stderr += chunk));
    child.once("error", reject);
    child.once("close", (code, signal) => resolvePromise({ code, signal, stdout, stderr }));
  });
}

function runCompanion(cwd, args) {
  return run(cwd, [companion, ...args], process.execPath, {
    ...process.env,
    OXC_TSRX_LINT_BIN: binary,
  });
}

function json(result) {
  assert.equal(result.signal, null);
  const start = result.stdout.indexOf("{");
  assert.notEqual(start, -1, result.stderr || result.stdout);
  return JSON.parse(result.stdout.slice(start));
}

// Stock Oxlint prints a diagnostic filename relative to its working directory
// whenever the linted file sits under it, while the TSRX binary echoes back the
// path it was handed. Passing `base` compares the two spellings as the same
// file instead of as the same string. macOS hides the difference because
// `mkdtemp` returns a symlinked `/var/folders/...` path that Oxlint cannot
// strip its real `/private/var/folders/...` working directory from, so it falls
// back to the absolute argument; on Linux CI `/tmp` is real and the same run
// reports `view.tsx`.
function diagnostic(output, filename, rule, base) {
  const sameFile = base
    ? (candidate) => resolve(base, candidate) === resolve(base, filename)
    : (candidate) => candidate === filename;
  return output.diagnostics.find(
    (item) => sameFile(item.filename) && (item.rule === rule || item.code.includes(`(${rule})`)),
  );
}

test("Oxlint byte-position resolver handles CR, LF, CRLF, EOF, duplicates, and bad offsets", () => {
  const bytes = Buffer.from("a\rβ\r\n🍄\nz");
  const positions = resolveOxlintBytePositions(
    bytes,
    [12, 0, 5, 6, 2, 1, 10, 4, 11, 6],
    "edge.tsrx",
  );
  assert.deepEqual(
    Object.fromEntries(positions),
    {
      0: { line: 1, column: 1 },
      1: { line: 1, column: 2 },
      2: { line: 2, column: 1 },
      4: { line: 2, column: 3 },
      5: { line: 3, column: 1 },
      6: { line: 3, column: 1 },
      10: { line: 3, column: 5 },
      11: { line: 4, column: 1 },
      12: { line: 4, column: 2 },
    },
  );
  for (const offset of [-1, 1.5, bytes.length + 1, Number.MAX_SAFE_INTEGER]) {
    assert.throws(
      () => resolveOxlintBytePositions(bytes, [offset], "edge.tsrx"),
      /invalid diagnostic byte offset/u,
    );
  }
  for (const offset of [3, 7, 8, 9]) {
    assert.throws(
      () => resolveOxlintBytePositions(bytes, [offset], "edge.tsrx"),
      /splits UTF-8/u,
    );
  }
});

test("npm wrapper matches canonical Oxlint line and UTF-8 byte columns after Unicode", async () => {
  const cwd = await realpath(await mkdtemp(join(tmpdir(), "oxc-tsrx-lint-unicode-location-")));
  const unicode = [
    "export function View() @{",
    '  const prior = "π";',
    "  void prior;",
    '  void "🍄"; var total = 0; debugger; void total;',
    "}",
    "",
  ].join("\n");
  const cases = [
    { name: "unicode", source: unicode },
    { name: "ascii", source: unicode.replace("π", "p").replace("🍄", "m") },
  ];
  for (const fixture of cases) {
    fixture.sourcePath = join(cwd, `${fixture.name}.tsrx`);
    fixture.controlPath = join(cwd, `${fixture.name}.tsx`);
    fixture.controlFilename = `${fixture.name}.tsx`;
    await writeFile(fixture.sourcePath, fixture.source);
    await writeFile(fixture.controlPath, fixture.source.replace("@{", "{"));
  }

  const expected = new Map([
    ["no-var", { token: "var", label: "var" }],
    ["no-debugger", { token: "debugger", label: "debugger;" }],
  ]);
  const ruleArgs = ["--deny", "no-var", "--deny", "no-debugger"];
  const args = [...ruleArgs, ...cases.map((fixture) => fixture.sourcePath)];

  const structured = await runCompanion(cwd, ["--format=json", ...args]);
  assert.equal(structured.code, 1, structured.stderr || structured.stdout);
  const output = json(structured);
  const controlResult = await run(
    cwd,
    ["--format=json", ...ruleArgs, ...cases.map((fixture) => fixture.controlPath)],
    stock,
  );
  assert.equal(controlResult.code, 1, controlResult.stderr || controlResult.stdout);
  const control = json(controlResult);

  const human = await runCompanion(cwd, args);
  assert.equal(human.code, 1, human.stderr || human.stdout);
  for (const fixture of cases) {
    for (const [rule, location] of expected) {
      const item = diagnostic(output, fixture.sourcePath, rule);
      const controlItem = diagnostic(control, fixture.controlFilename, rule);
      assert.ok(item, `${fixture.name}: ${rule}`);
      assert.ok(controlItem, `${fixture.name} control: ${rule}`);
      const span = item.labels[0].span;
      const controlSpan = controlItem.labels[0].span;
      const characterIndex = fixture.source.indexOf(location.token);
      assert.equal(
        span.offset,
        Buffer.byteLength(fixture.source.slice(0, characterIndex)),
        `${fixture.name} ${rule}: byte offset`,
      );
      assert.equal(
        span.length,
        Buffer.byteLength(location.label),
        `${fixture.name} ${rule}: byte length`,
      );
      assert.deepEqual(
        { line: span.line, column: span.column },
        { line: controlSpan.line, column: controlSpan.column },
        `${fixture.name} ${rule}: canonical line/byte-column parity`,
      );
      assert.match(
        human.stdout,
        new RegExp(
          `^${fixture.name}\\.tsrx:${controlSpan.line}:${controlSpan.column}: error eslint\\(${rule}\\) `,
          "mu",
        ),
        `${fixture.name}: ${rule}`,
      );
    }
  }
});

test("npm wrapper matches canonical Oxlint across CR, LF, and CRLF line starts", async () => {
  const cwd = await realpath(await mkdtemp(join(tmpdir(), "oxc-tsrx-lint-line-endings-")));
  const sourcePath = join(cwd, "lines.tsrx");
  const controlPath = join(cwd, "lines.tsx");
  const source =
    'export function Lines() @{ void "π";\rdebugger;\r\nvar value = 0;\nvoid value;\r}';
  await writeFile(sourcePath, source);
  await writeFile(controlPath, source.replace("@{", "{"));
  const ruleArgs = ["--deny", "no-debugger", "--deny", "no-var"];
  const wrappedResult = await runCompanion(cwd, ["--format=json", ...ruleArgs, sourcePath]);
  const controlResult = await run(cwd, ["--format=json", ...ruleArgs, controlPath], stock);
  assert.equal(wrappedResult.code, 1, wrappedResult.stderr || wrappedResult.stdout);
  assert.equal(controlResult.code, 1, controlResult.stderr || controlResult.stdout);
  const wrapped = json(wrappedResult);
  const control = json(controlResult);
  for (const [rule, exact] of [
    ["no-debugger", { line: 2, column: 1 }],
    ["no-var", { line: 3, column: 1 }],
  ]) {
    const span = diagnostic(wrapped, sourcePath, rule)?.labels[0].span;
    const controlSpan = diagnostic(control, "lines.tsx", rule)?.labels[0].span;
    assert.deepEqual({ line: span?.line, column: span?.column }, exact, rule);
    assert.deepEqual(
      { line: span?.line, column: span?.column },
      { line: controlSpan?.line, column: controlSpan?.column },
      `${rule}: canonical line-ending parity`,
    );
  }
});

test("discovers one JSONC Oxlint config and applies rules, severities, and per-file overrides", async () => {
  const cwd = await realpath(await mkdtemp(join(tmpdir(), "oxc-tsrx-lint-config-")));
  const tsrx = join(cwd, "view.tsrx");
  const tsx = join(cwd, "view.tsx");
  const config = join(cwd, ".oxlintrc.jsonc");
  await writeFile(
    config,
    `{
      // The same config governs TSRX and ordinary TSX.
      "plugins": ["react"],
      "rules": {
        "no-debugger": "error",
        "no-console": "warn",
        "eqeqeq": ["error", "always"],
        "no-undef": "error",
        "react/jsx-no-undef": "error"
      },
      "env": { "browser": true },
      "globals": { "projectGlobal": "readonly" },
      "settings": { "react": { "version": "19.0.0" } },
      "overrides": [{
        "files": ["**/*.tsx"],
        "rules": { "no-debugger": "off" }
      }]
    }\n`,
  );
  await writeFile(
    tsrx,
    "export function Tsrx(value: unknown) @{ debugger; console.log(window, projectGlobal, missingGlobal, value == null); <Missing />; }\n",
  );
  await writeFile(
    tsx,
    "export function Tsx(value: unknown) { debugger; console.log(value == null); return <Missing />; }\n",
  );

  const result = await run(cwd, ["--format=json", tsrx, tsx]);
  assert.equal(result.code, 1, result.stderr || result.stdout);
  const output = json(result);

  assert.equal(diagnostic(output, tsrx, "no-debugger")?.severity, "error");
  assert.equal(diagnostic(output, tsrx, "no-console")?.severity, "warning");
  assert.equal(diagnostic(output, tsrx, "eqeqeq")?.severity, "error");
  const undefinedDiagnostics = output.diagnostics.filter(
    (item) => item.filename === tsrx && item.rule === "no-undef",
  );
  assert.ok(undefinedDiagnostics.some((item) => /missingGlobal/.test(item.message)));
  assert.ok(undefinedDiagnostics.every((item) => !/window|projectGlobal/.test(item.message)));
  assert.equal(diagnostic(output, tsrx, "jsx-no-undef")?.severity, "error");
  assert.equal(diagnostic(output, tsx, "jsx-no-undef")?.severity, "error");
  assert.equal(diagnostic(output, tsx, "no-debugger"), undefined);
  assert.equal(diagnostic(output, tsx, "no-console")?.severity, "warning");
  assert.equal(diagnostic(output, tsx, "eqeqeq")?.severity, "error");
  assert.equal(output.number_of_files, 2);
  assert.equal(output.oxcTsrx.files.tsrx, 1);
  assert.equal(output.oxcTsrx.files.standard, 1);
  assert.equal(output.oxcTsrx.parseCount, 2);
  assert.equal(output.oxcTsrx.configLoads, 1);
  assert.equal(output.oxcTsrx.configPath, await realpath(config));

  const stockTsxResult = await run(cwd, ["--format=json", tsx], stock);
  assert.equal(stockTsxResult.code, 1, stockTsxResult.stderr || stockTsxResult.stdout);
  const stockTsx = json(stockTsxResult);
  assert.equal(diagnostic(stockTsx, tsx, "no-debugger", cwd), undefined);
  for (const rule of ["no-console", "eqeqeq", "no-undef", "jsx-no-undef"]) {
    const candidate = diagnostic(output, tsx, rule);
    const control = diagnostic(stockTsx, tsx, rule, cwd);
    assert.ok(control, `${rule}: stock Oxlint control diagnostic`);
    assert.equal(candidate?.severity, control?.severity, rule);
    assert.equal(candidate?.labels[0]?.span.offset, control?.labels[0]?.span.offset, rule);
    assert.equal(candidate?.labels[0]?.span.length, control?.labels[0]?.span.length, rule);
  }

  const stockTsrxResult = await run(cwd, ["--format=json", tsrx], stock);
  assert.equal(stockTsrxResult.code, 1);
  assert.equal(json(stockTsrxResult).number_of_files, 0);
});

test("explicit config extends files and CLI filters override configured severities", async () => {
  const cwd = await mkdtemp(join(tmpdir(), "oxc-tsrx-lint-extends-"));
  const source = join(cwd, "source.tsrx");
  const base = join(cwd, "base.json");
  const config = join(cwd, "lint.json");
  await writeFile(base, '{ "rules": { "no-var": "error", "no-debugger": "off" } }\n');
  await writeFile(config, '{ "extends": ["./base.json"], "rules": { "no-debugger": "warn" } }\n');
  await writeFile(source, "export function View() @{ var value = 1; debugger; void value; }\n");

  const denied = await run(cwd, [
    "--format=json",
    "--config",
    config,
    "--allow",
    "no-debugger",
    "--deny",
    "no-var",
    source,
  ]);
  assert.equal(denied.code, 1, denied.stderr || denied.stdout);
  const deniedOutput = json(denied);
  assert.equal(diagnostic(deniedOutput, source, "no-debugger"), undefined);
  assert.equal(diagnostic(deniedOutput, source, "no-var")?.severity, "error");

  const warned = await run(cwd, [
    "--format=json",
    "-c",
    config,
    "--warn",
    "no-debugger",
    "--allow",
    "no-var",
    source,
  ]);
  assert.equal(warned.code, 0, warned.stderr || warned.stdout);
  const warnedOutput = json(warned);
  assert.equal(diagnostic(warnedOutput, source, "no-var"), undefined);
  assert.equal(diagnostic(warnedOutput, source, "no-debugger")?.severity, "warning");
});

test("a materialized Vite config keeps object extends, overrides, and ignores rooted at its authored base", async () => {
  const cwd = await realpath(await mkdtemp(join(tmpdir(), "oxc-tsrx-lint-materialized-base-")));
  const materialized = await realpath(
    await mkdtemp(join(tmpdir(), "oxc-tsrx-lint-materialized-config-")),
  );
  await mkdir(join(cwd, "src"), { recursive: true });
  const active = join(cwd, "src/active.tsrx");
  const ignored = join(cwd, "src/ignored.tsrx");
  const config = join(materialized, ".oxlintrc.json");
  await writeFile(
    config,
    JSON.stringify({
      extends: [{ rules: { "no-debugger": "error" } }],
      ignorePatterns: ["src/ignored.tsrx"],
      overrides: [
        {
          files: ["src/**/*.tsrx"],
          rules: { "no-console": "error" },
        },
      ],
    }),
  );
  await writeFile(
    active,
    'export function Active() @{ debugger; console.log("active"); <div />; }\n',
  );
  await writeFile(
    ignored,
    'export function Ignored() @{ debugger; console.log("ignored"); <div />; }\n',
  );

  const result = await run(cwd, [
    "--format=json",
    "--config",
    config,
    "--config-base",
    cwd,
    active,
    ignored,
  ]);
  assert.equal(result.code, 1, result.stderr || result.stdout);
  const output = json(result);
  assert.equal(output.number_of_files, 1);
  assert.equal(diagnostic(output, active, "no-debugger")?.severity, "error");
  assert.equal(diagnostic(output, active, "no-console")?.severity, "error");
  assert.equal(
    output.diagnostics.some((item) => item.filename === ignored),
    false,
  );
});

test("ignorePatterns filter a batch and configured safe fixes reparse both TSRX and TSX", async () => {
  const cwd = await mkdtemp(join(tmpdir(), "oxc-tsrx-lint-fix-config-"));
  const tsrx = join(cwd, "fix.tsrx");
  const tsx = join(cwd, "fix.tsx");
  const ignored = join(cwd, "ignored.tsrx");
  await writeFile(
    join(cwd, ".oxlintrc.json"),
    `{
      "ignorePatterns": ["ignored.tsrx"],
      "rules": { "no-var": "error", "no-debugger": "error" }
    }\n`,
  );
  await writeFile(tsrx, "export function Tsrx() @{ var value = 1; void value; }\n");
  await writeFile(tsx, "export function Tsx() { var value = 1; void value; }\n");
  await writeFile(ignored, "export function Ignored() @{ debugger; }\n");

  const result = await run(cwd, ["--format=json", "--fix", tsrx, tsx, ignored]);
  assert.equal(result.code, 0, result.stderr || result.stdout);
  const output = json(result);
  assert.equal(output.number_of_files, 2);
  assert.equal(output.diagnostics.length, 0);
  assert.equal(output.oxcTsrx.fixes.applied, 2);
  assert.equal(output.oxcTsrx.reparseCount, 2);
  assert.doesNotMatch(await readFile(tsrx, "utf8"), /\bvar\b/);
  assert.doesNotMatch(await readFile(tsx, "utf8"), /\bvar\b/);
  assert.match(await readFile(ignored, "utf8"), /debugger/);
});

test("denyWarnings and maxWarnings preserve Oxlint warning exit policy", async () => {
  const cwd = await mkdtemp(join(tmpdir(), "oxc-tsrx-lint-warning-policy-"));
  const source = join(cwd, "source.tsrx");
  const denyWarnings = join(cwd, "deny-warnings.json");
  const maxWarnings = join(cwd, "max-warnings.json");
  await writeFile(source, "export function View() @{ debugger; }\n");
  await writeFile(
    denyWarnings,
    '{ "rules": { "no-debugger": "warn" }, "options": { "denyWarnings": true } }\n',
  );
  await writeFile(
    maxWarnings,
    '{ "rules": { "no-debugger": "warn" }, "options": { "maxWarnings": 0 } }\n',
  );

  for (const config of [denyWarnings, maxWarnings]) {
    const result = await run(cwd, ["--format=json", "--config", config, source]);
    assert.equal(result.code, 1, result.stderr || result.stdout);
    assert.equal(diagnostic(json(result), source, "no-debugger")?.severity, "warning");
  }
});

test("unsupported JS plugins, type-aware mode, and JS config modules fail before linting", async () => {
  const cases = [
    {
      name: "js-plugin",
      configName: ".oxlintrc.json",
      config: '{ "jsPlugins": ["./plugin.js"], "rules": {} }\n',
      pattern: /JS|JavaScript.*plugin|plugin.*unsupported/i,
    },
    {
      name: "type-aware",
      configName: ".oxlintrc.json",
      config: '{ "options": { "typeAware": true }, "rules": {} }\n',
      pattern: /type.?aware|tsgolint/i,
    },
    {
      name: "js-config",
      configName: "oxlint.config.ts",
      config: 'export default { rules: { "no-debugger": "error" } };\n',
      pattern: /JavaScript|TypeScript|config.*module|JSON/i,
    },
  ];

  for (const fixture of cases) {
    const cwd = await mkdtemp(join(tmpdir(), `oxc-tsrx-lint-${fixture.name}-`));
    const source = join(cwd, "source.tsrx");
    await writeFile(join(cwd, fixture.configName), fixture.config);
    await writeFile(source, "export function View() @{ debugger; }\n");
    const result = await run(cwd, ["--format=json", source]);
    assert.equal(result.code, 2, `${fixture.name}: ${result.stderr || result.stdout}`);
    assert.equal(result.stdout, "", fixture.name);
    assert.match(result.stderr, fixture.pattern, fixture.name);
  }
});
