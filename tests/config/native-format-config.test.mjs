import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir, mkdtemp, readFile, realpath, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "../..");
// One multi-call native binary carries the linter, the formatter, and the
// language server; `fmt` selects the formatter.
const binary = resolve(process.env.OXFMT_BIN ?? join(root, "target/release/oxc-tsrx"));
// pnpm installs `oxfmt-current` under the package that declares it, so it is
// resolved from this file's own package instead of from a hoisted
// repository-root `node_modules`.
const stock = join(
  dirname(createRequire(import.meta.url).resolve("oxfmt-current/package.json")),
  "bin/oxfmt",
);

function run(executable, cwd, args, input = null) {
  return new Promise((resolvePromise, reject) => {
    const child = spawn(executable, args, {
      cwd,
      env: process.env,
      stdio: ["pipe", "pipe", "pipe"],
    });
    let stdout = "";
    let stderr = "";
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => (stdout += chunk));
    child.stderr.on("data", (chunk) => (stderr += chunk));
    child.once("error", reject);
    child.once("close", (code, signal) => resolvePromise({ code, signal, stdout, stderr }));
    child.stdin.end(input ?? undefined);
  });
}

function runFormat(cwd, args, input = null) {
  return run(binary, cwd, ["fmt", ...args], input);
}

test("discovers JSONC Oxfmt options for TSRX and preserves ordinary TSX parity", async () => {
  const cwd = await mkdtemp(join(tmpdir(), "oxc-tsrx-format-config-"));
  await writeFile(
    join(cwd, ".oxfmtrc.jsonc"),
    `{
      // Public core JS/TSX options are shared with Oxfmt.
      "useTabs": false,
      "tabWidth": 4,
      "printWidth": 48,
      "singleQuote": true,
      "jsxSingleQuote": true,
      "semi": false,
      "trailingComma": "none",
      "arrowParens": "avoid"
    }\n`,
  );
  const tsrx =
    'export function View({label}:{label:string}) @{ const message="hello"; <button title="world">{label}{message}</button>; <style>.button{color:red}</style>; }\n';
  const tsx =
    'export function View({label}:{label:string}) { const message="hello"; return <button title="world">{label}{message}</button>; }\n';

  const [tsrxResult, candidateTsx, stockTsx] = await Promise.all([
    runFormat(cwd, ["--stdin-filepath=View.tsrx"], tsrx),
    runFormat(cwd, ["--stdin-filepath=View.tsx"], tsx),
    run(stock, cwd, ["--stdin-filepath=View.tsx"], tsx),
  ]);
  assert.equal(tsrxResult.code, 0, tsrxResult.stderr || tsrxResult.stdout);
  assert.match(tsrxResult.stdout, /const message = 'hello'/);
  assert.match(tsrxResult.stdout, /title='world'/);
  assert.doesNotMatch(tsrxResult.stdout, /const message = 'hello';/);
  assert.match(tsrxResult.stdout, /\n {4}const message/);
  assert.match(tsrxResult.stdout, /<style>\.button\{color:red\}<\/style>/);
  assert.equal(candidateTsx.code, 0, candidateTsx.stderr || candidateTsx.stdout);
  assert.equal(stockTsx.code, 0, stockTsx.stderr || stockTsx.stdout);
  assert.equal(candidateTsx.stdout, stockTsx.stdout);

  const converged = await runFormat(cwd, ["--stdin-filepath=View.tsrx"], tsrxResult.stdout);
  assert.equal(converged.code, 0, converged.stderr || converged.stdout);
  assert.equal(converged.stdout, tsrxResult.stdout);
});

test("an explicit config applies per-file TSRX overrides without changing ordinary TSX options", async () => {
  const cwd = await mkdtemp(join(tmpdir(), "oxc-tsrx-format-override-"));
  const config = join(cwd, "custom-format.json");
  await writeFile(
    config,
    `{
      "singleQuote": false,
      "jsxSingleQuote": false,
      "semi": true,
      "overrides": [{
        "files": ["**/*.tsrx"],
        "options": { "singleQuote": true, "jsxSingleQuote": true, "semi": false }
      }]
    }\n`,
  );
  const tsrx = 'export function View() @{ const label="hello"; <p title="world">{label}</p>; }\n';
  const tsx =
    'export function View() { const label="hello"; return <p title="world">{label}</p>; }\n';

  const [tsrxResult, candidateTsx, stockTsx] = await Promise.all([
    runFormat(cwd, ["--config", config, "--stdin-filepath=src/View.tsrx"], tsrx),
    runFormat(cwd, ["--config", config, "--stdin-filepath=src/View.tsx"], tsx),
    run(stock, cwd, ["--config", config, "--stdin-filepath=src/View.tsx"], tsx),
  ]);
  assert.equal(tsrxResult.code, 0, tsrxResult.stderr || tsrxResult.stdout);
  assert.match(tsrxResult.stdout, /'hello'/);
  assert.match(tsrxResult.stdout, /title='world'/);
  assert.doesNotMatch(tsrxResult.stdout, /const label = 'hello';/);
  assert.equal(candidateTsx.stdout, stockTsx.stdout);
  assert.match(candidateTsx.stdout, /"hello";/);
  assert.match(candidateTsx.stdout, /title="world"/);
});

test("a materialized Vite format config keeps overrides and ignores rooted at its authored base", async () => {
  const cwd = await realpath(await mkdtemp(join(tmpdir(), "oxc-tsrx-format-materialized-base-")));
  const materialized = await realpath(
    await mkdtemp(join(tmpdir(), "oxc-tsrx-format-materialized-config-")),
  );
  await mkdir(join(cwd, "src"), { recursive: true });
  const active = join(cwd, "src/active.tsrx");
  const ignored = join(cwd, "src/ignored.tsrx");
  const config = join(materialized, ".oxfmtrc.json");
  const source = 'export function View() @{const value="hello";<p>{value}</p>}\n';
  await writeFile(
    config,
    JSON.stringify({
      semi: true,
      singleQuote: false,
      ignorePatterns: ["src/ignored.tsrx"],
      overrides: [
        {
          files: ["src/**/*.tsrx"],
          options: { semi: false, singleQuote: true },
        },
      ],
    }),
  );
  await writeFile(active, source);
  await writeFile(ignored, source);

  const result = await runFormat(cwd, [
    "--write",
    "--config",
    config,
    "--config-base",
    cwd,
    active,
    ignored,
  ]);
  assert.equal(result.code, 0, result.stderr || result.stdout);
  const formatted = await readFile(active, "utf8");
  assert.match(formatted, /const value = 'hello'/);
  assert.doesNotMatch(formatted, /const value = 'hello';/);
  assert.equal(await readFile(ignored, "utf8"), source);

  const outside = join(materialized, "outside.tsrx");
  await writeFile(outside, source);
  const outsideResult = await runFormat(cwd, [
    "--check",
    "--config",
    config,
    "--config-base",
    cwd,
    outside,
  ]);
  assert.equal(outsideResult.signal, null, outsideResult.stderr || outsideResult.stdout);
  assert.equal(outsideResult.code, 1, outsideResult.stderr || outsideResult.stdout);
});

test("remaining public JS/TSX layout options retain stock parity and apply in one TSRX pass", async () => {
  const cwd = await mkdtemp(join(tmpdir(), "oxc-tsrx-format-core-options-"));
  await writeFile(
    join(cwd, ".oxfmtrc.json"),
    `{
      "endOfLine": "crlf",
      "printWidth": 40,
      "quoteProps": "consistent",
      "trailingComma": "all",
      "bracketSpacing": false,
      "bracketSameLine": true,
      "objectWrap": "collapse",
      "singleAttributePerLine": true,
      "htmlWhitespaceSensitivity": "ignore",
      "insertFinalNewline": false
    }\n`,
  );
  const tsx =
    'const data={plain:1,"needs-dash":2}; export function View({first,second}:{first:string;second:string}) { const props={plain:data.plain}; return <section alpha="one" beta="two" gamma="three"><span>{first}</span> <span>{second}</span>{props.plain}</section>; }\n';
  const tsrx =
    'const data={plain:1,"needs-dash":2}; export function View({first,second}:{first:string;second:string}) @{ const props={plain:data.plain}; <section alpha="one" beta="two" gamma="three"><span>{first}</span> <span>{second}</span>{props.plain}</section>; }\n';

  const [candidateTsx, stockTsx, candidateTsrx] = await Promise.all([
    runFormat(cwd, ["--stdin-filepath=View.tsx"], tsx),
    run(stock, cwd, ["--stdin-filepath=View.tsx"], tsx),
    runFormat(cwd, ["--stdin-filepath=View.tsrx"], tsrx),
  ]);
  assert.equal(candidateTsx.code, 0, candidateTsx.stderr || candidateTsx.stdout);
  assert.equal(stockTsx.code, 0, stockTsx.stderr || stockTsx.stdout);
  assert.equal(candidateTsrx.code, 0, candidateTsrx.stderr || candidateTsrx.stdout);
  assert.equal(candidateTsx.stdout, stockTsx.stdout);
  assert.match(candidateTsrx.stdout, /\r\n/);
  assert.doesNotMatch(candidateTsrx.stdout, /[\r\n]$/);
  assert.match(candidateTsrx.stdout, /\{plain:/);
  assert.match(candidateTsrx.stdout, /<section\r\n\s+alpha=/);
});

test("ignorePatterns leave ignored files byte-identical while formatting the rest transactionally", async () => {
  const cwd = await mkdtemp(join(tmpdir(), "oxc-tsrx-format-ignore-"));
  const included = join(cwd, "included.tsrx");
  const ignored = join(cwd, "ignored.tsrx");
  const source = 'export function View() @{const value="hello";<p>{value}</p>}\n';
  await writeFile(
    join(cwd, ".oxfmtrc.json"),
    '{ "singleQuote": true, "ignorePatterns": ["ignored.tsrx"] }\n',
  );
  await writeFile(included, source);
  await writeFile(ignored, source);

  const result = await runFormat(cwd, ["--write", included, ignored]);
  assert.equal(result.code, 0, result.stderr || result.stdout);
  assert.notEqual(await readFile(included, "utf8"), source);
  assert.match(await readFile(included, "utf8"), /'hello'/);
  assert.equal(await readFile(ignored, "utf8"), source);
});

test("unsupported callback-backed options, editorconfig, and JS config modules fail before output or writes", async () => {
  const cases = [
    {
      name: "tailwind",
      configName: ".oxfmtrc.json",
      config: '{ "sortTailwindcss": true }\n',
      pattern: /sortTailwindcss|Tailwind|callback|unsupported/i,
    },
    {
      name: "embedded-language",
      configName: ".oxfmtrc.json",
      config: '{ "embeddedLanguageFormatting": "auto" }\n',
      pattern: /embeddedLanguageFormatting|embedded-language|callback|unsupported/i,
    },
    {
      name: "editorconfig",
      configName: ".editorconfig",
      config: "root = true\n[*]\nindent_size = 2\n",
      pattern: /editorconfig|unsupported|silently ignored/i,
    },
    {
      name: "js-config",
      configName: "oxfmt.config.js",
      config: "export default { singleQuote: true };\n",
      pattern: /JavaScript|TypeScript|config.*module|JSON/i,
    },
  ];

  for (const fixture of cases) {
    const cwd = await mkdtemp(join(tmpdir(), `oxc-tsrx-format-${fixture.name}-`));
    const source = 'export function View() @{const value="hello";<p>{value}</p>}\n';
    await writeFile(join(cwd, fixture.configName), fixture.config);
    const result = await runFormat(cwd, ["--stdin-filepath=View.tsrx"], source);
    assert.equal(result.code, 2, `${fixture.name}: ${result.stderr || result.stdout}`);
    assert.equal(result.stdout, "", fixture.name);
    assert.match(result.stderr, fixture.pattern, fixture.name);
  }
});
