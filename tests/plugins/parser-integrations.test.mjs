import assert from "node:assert/strict";
import { cp, mkdir, mkdtemp, readFile, readdir, realpath, rm, symlink } from "node:fs/promises";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import test from "node:test";
import { Linter } from "eslint";

import demoPlugin from "../../examples/custom-js-plugins/demo-lint-plugin.mjs";
import tsrxEslintParser, {
  parseForESLint,
} from "../../examples/custom-js-plugins/tsrx-eslint-parser.mjs";
import {
  withTsrxParser,
} from "../../examples/custom-js-plugins/tsrx-parser-service.mjs";
import { tsrxDemoLint } from "../../examples/custom-js-plugins/vite-demo-lint.mjs";

const require = createRequire(import.meta.url);
const root = resolve(import.meta.dirname, "../..");
const fixture = join(root, "tests/fixtures/vite/react");

async function makeViteProject() {
  const project = await mkdtemp(join(tmpdir(), "oxc-tsrx-vite-parser-service-"));
  await cp(fixture, project, { recursive: true });
  const modules = join(project, "node_modules");
  await mkdir(modules, { recursive: true });
  for (const dependency of ["react", "react-dom"]) {
    const packageRoot = dirname(require.resolve(`${dependency}/package.json`));
    await symlink(packageRoot, join(modules, dependency), "dir");
  }
  return realpath(project);
}

async function outputFiles(directory) {
  const files = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) files.push(...(await outputFiles(path)));
    else files.push(path);
  }
  return files;
}

test("the ESLint parser adapter exposes authored TSRX nodes to a custom JavaScript rule", async () => {
  const filename = join(root, "examples/vscode-lints/LintDemo.tsrx");
  const source = await readFile(filename, "utf8");
  const parsed = parseForESLint(source, { filePath: filename, sourceType: "module" });
  assert.ok(parsed.visitorKeys.JSXIfExpression.includes("test"));
  assert.ok(parsed.visitorKeys.JSXIfExpression.includes("consequent"));
  assert.equal(parsed.services.isTsrx, true);
  assert.deepEqual(parsed.ast.tokens, []);

  const linter = new Linter({ configType: "flat" });
  const diagnostics = linter.verify(
    source,
    [
      {
        files: ["**/*.tsrx"],
        languageOptions: {
          parser: tsrxEslintParser,
          parserOptions: { filePath: filename, sourceType: "module" },
        },
        plugins: { "tsrx-demo": demoPlugin },
        rules: { "tsrx-demo/no-tsrx-if": "error" },
      },
    ],
    { filename },
  );

  assert.deepEqual(
    diagnostics.map(({ ruleId, message, line, column }) => ({
      ruleId,
      message,
      line,
      column,
    })),
    [
      {
        ruleId: "tsrx-demo/no-tsrx-if",
        message: "Demo rule: prefer a declarative component over this TSRX @if block.",
        line: 9,
        column: 3,
      },
    ],
  );
});

test("the Vite parser service composes before the existing TSRX React transform", async () => {
  const [{ build }, { tsrxReact }] = await Promise.all([
    import("vite"),
    import("@tsrx/vite-plugin-react"),
  ]);
  const project = await makeViteProject();
  const parsed = [];
  const findings = [];

  try {
    await build({
      root: project,
      appType: "custom",
      configFile: false,
      logLevel: "silent",
      plugins: [
        withTsrxParser(
          tsrxReact(),
          (parser) =>
            tsrxDemoLint(parser, {
              onFinding(finding) {
                findings.push(finding);
              },
            }),
          {
            onParse(observation) {
              parsed.push(observation);
            },
          },
        ),
      ],
      build: {
        minify: false,
        outDir: "dist",
        rolldownOptions: { input: join(project, "src/main.jsx") },
      },
    });

    assert.equal(parsed.length, 1, "the raw TSRX module should be parsed once");
    assert.equal(parsed[0].result.program.type, "Program");
    assert.equal(findings.length, 1);
    assert.equal(findings[0].node.type, "JSXIfExpression");
    assert.match(
      findings[0].sourceText.slice(findings[0].node.start, findings[0].node.end),
      /^@if/u,
    );

    const files = await outputFiles(join(project, "dist"));
    const output = (
      await Promise.all(
        files.filter((file) => file.endsWith(".js")).map((file) => readFile(file, "utf8")),
      )
    ).join("\n");
    assert.match(output, /OXC TSRX BUILD/u);
    assert.doesNotMatch(output, /@if|@\{/u);
  } finally {
    await rm(project, { recursive: true, force: true });
  }
});
