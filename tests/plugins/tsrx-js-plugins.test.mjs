// The oracle for "an ordinary Oxlint JavaScript plugin runs on .tsrx".
//
// Every test here runs the real `oxlint` command this package installs, over a
// real `.tsrx` file, with a real user-authored plugin. No ESLint, no second
// linter, no upstream build, and no assertion that stops at "the lane was
// wired up": the positions are checked against the bytes of the authored source
// so a rule reported at the wrong place fails here rather than in someone's
// editor.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { cp, mkdir, mkdtemp, readFile, rm, symlink, writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import test from "node:test";

import { LspClient, SERVER_ARGUMENTS, pathToFileUri } from "../editor/lsp-client.mjs";
import {
  OXLINT_JS_PLUGIN_LANE_BELOW,
  OXLINT_JS_PLUGIN_LANE_MINIMUM,
  installedOxlintVersion,
  jsPluginDisclosure,
  laneSupportsOxlintVersion,
  mirrorRelativePath,
  nativeLaneConfig,
  oxlintVersionRefusal,
  parseOxlintConfigText,
  projectionConfig,
} from "../../packages/toolchain/dist/lint-js-plugins.js";

const root = resolve(import.meta.dirname, "../..");
const fixtures = join(root, "tests/fixtures/lint/js-plugins");
const companion = join(root, "packages/toolchain/bin/oxlint");
const toolchain = join(root, "packages/toolchain");
const binary = resolve(process.env.OXLINT_BIN ?? join(root, "target/release/oxc-tsrx"));

const rejectionSource = join(root, "crates/oxc_adapter/src/toolchain.rs");
const REFUSAL_PATTERN =
  /"(JavaScript plugins are not hosted by the native TSRX lint target itself:[^"]*)"/u;

function run(cwd, executable, args, environment) {
  return new Promise((resolvePromise, reject) => {
    const child = spawn(executable, args, {
      cwd,
      env: environment ?? { ...process.env, OXC_TSRX_LINT_BIN: binary },
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

function oxlint(cwd, args) {
  return run(cwd, process.execPath, [companion, ...args]);
}

function report(result) {
  try {
    return JSON.parse(result.stdout);
  } catch {
    assert.fail(`expected a JSON report, got:\n${result.stdout}\n${result.stderr}`);
  }
}

/**
 * The line and column of `offset` counted in the bytes of `source`, computed
 * here rather than borrowed from the wrapper. This is what makes the position
 * assertions independent of the code that produced them.
 */
function locationOf(source, offset) {
  const bytes = Buffer.from(source, "utf8");
  let line = 1;
  let column = 1;
  for (let index = 0; index < offset; index += 1) {
    if (bytes[index] === 0x0a) {
      line += 1;
      column = 1;
    } else {
      column += 1;
    }
  }
  return { line, column };
}

function pluginDiagnostics(parsed, filename) {
  return (parsed.diagnostics ?? []).filter(
    (diagnostic) =>
      diagnostic.filename.endsWith(filename) &&
      String(diagnostic.code ?? "").startsWith("tsrx-js-demo("),
  );
}

/**
 * A throwaway project: the fixture plugin, one `.tsrx` file, one `.tsx` file,
 * the caller's `.oxlintrc.json`, and a symlinked install of this package. This
 * is the shape a reader's project has.
 */
async function makeProject(config, extra = {}) {
  const project = await mkdtemp(join(tmpdir(), "oxc-tsrx-js-plugin-lane-"));
  await mkdir(join(project, "src"), { recursive: true });
  await cp(join(fixtures, "demo-plugin.mjs"), join(project, "demo-plugin.mjs"));
  await cp(join(fixtures, "demo.tsrx"), join(project, "src/demo.tsrx"));
  await cp(join(fixtures, "ordinary.tsx"), join(project, "src/ordinary.tsx"));
  await writeFile(join(project, ".oxlintrc.json"), `${JSON.stringify(config, null, 2)}\n`);
  for (const [name, contents] of Object.entries(extra)) {
    const target = join(project, name);
    await mkdir(join(target, ".."), { recursive: true });
    await writeFile(target, contents);
  }
  await mkdir(join(project, "node_modules"), { recursive: true });
  await symlink(toolchain, join(project, "node_modules/oxc-tsrx"), "dir");
  return project;
}

async function withProject(config, body, extra = {}) {
  const project = await makeProject(config, extra);
  try {
    return await body(project);
  } finally {
    await rm(project, { recursive: true, force: true });
  }
}

const BASE_CONFIG = {
  jsPlugins: ["./demo-plugin.mjs"],
  rules: { "tsrx-js-demo/no-banned-identifier": "error" },
};

test("the native binary these tests drive is built", () => {
  assert.ok(
    existsSync(binary),
    `missing ${binary}. Build it with:\n  cargo build --release --locked -p oxc_tsrx_cli --bins`,
  );
});

test("a user plugin reports on .tsrx at the authored source's own positions", async () => {
  const result = await oxlint(root, [
    "--format=json",
    "tests/fixtures/lint/js-plugins/demo.tsrx",
  ]);
  const parsed = report(result);
  const source = await readFile(join(fixtures, "demo.tsrx"), "utf8");
  const diagnostics = pluginDiagnostics(parsed, "demo.tsrx");

  assert.equal(diagnostics.length, 2, JSON.stringify(parsed.diagnostics, null, 2));
  // Every reported span must be the six bytes of the word `banned` in the file
  // the user wrote, at the line and column those bytes really sit on. Anything
  // that only checks "a diagnostic arrived" would pass on projection offsets.
  const expected = [...source.matchAll(/banned/gu)].map((match) => match.index);
  assert.equal(expected.length, 2, "the fixture stopped containing two `banned` identifiers");
  for (const [index, diagnostic] of diagnostics.entries()) {
    const span = diagnostic.labels[0].span;
    assert.equal(span.offset, expected[index]);
    assert.equal(span.length, 6);
    assert.equal(source.slice(span.offset, span.offset + span.length), "banned");
    const location = locationOf(source, expected[index]);
    assert.equal(span.line, location.line);
    assert.equal(span.column, location.column);
  }

  // The lane is disclosed in the machine-readable report too, not only on stderr.
  assert.deepEqual(parsed.oxcTsrx.jsPluginProjection, { files: 1, extraParses: 1 });
  // An error-severity plugin rule must not report a green run.
  assert.equal(result.code, 1, result.stderr);
});

test("native Rust rules on .tsrx keep reporting alongside the plugin", async () => {
  const result = await oxlint(root, [
    "--format=json",
    "tests/fixtures/lint/js-plugins/demo.tsrx",
  ]);
  const parsed = report(result);
  const source = await readFile(join(fixtures, "demo.tsrx"), "utf8");
  const native = parsed.diagnostics.find((diagnostic) => diagnostic.rule === "no-debugger");
  assert.ok(native, JSON.stringify(parsed.diagnostics, null, 2));
  const span = native.labels[0].span;
  assert.equal(span.offset, source.indexOf("debugger"));
  assert.equal(span.line, locationOf(source, source.indexOf("debugger")).line);
});

test("the extra parse is disclosed on stderr exactly once, and --silent suppresses it", async () => {
  const result = await oxlint(root, ["tests/fixtures/lint/js-plugins/demo.tsrx"]);
  const notice = jsPluginDisclosure(1);
  assert.ok(result.stderr.includes(notice), `expected the disclosure, got:\n${result.stderr}`);
  assert.equal(result.stderr.split(notice).length - 1, 1, result.stderr);
  assert.ok(
    !result.stdout.includes("running JS plugins on"),
    "the disclosure belongs on stderr, not in the report",
  );

  const silent = await oxlint(root, ["--silent", "tests/fixtures/lint/js-plugins/demo.tsrx"]);
  assert.ok(
    !silent.stderr.includes("running JS plugins on"),
    `--silent must suppress the disclosure, got:\n${silent.stderr}`,
  );
});

test("a mixed .tsrx and .tsx batch runs the same rule on both halves", async () => {
  const result = await oxlint(root, ["--format=json", "tests/fixtures/lint/js-plugins/"]);
  const parsed = report(result);
  assert.ok(pluginDiagnostics(parsed, "demo.tsrx").length > 0, "the .tsrx half lost the plugin");
  assert.ok(
    pluginDiagnostics(parsed, "ordinary.tsx").length > 0,
    "the ordinary half lost the plugin",
  );
  assert.ok(
    parsed.diagnostics.some(
      (diagnostic) => diagnostic.filename.endsWith("demo.tsrx") && diagnostic.rule === "no-debugger",
    ),
    "the native .tsrx rules stopped reporting in a mixed batch",
  );
});

test("an overrides glob written for .tsrx still selects that file's projection", async () => {
  // The mirror names the projection `demo.tsrx.tsx`, which `**/*.tsrx` does not
  // match on its own. The fixture config raises the rule to `error` only inside
  // an override aimed at `.tsrx`, so an override that failed to match would show
  // up here as the base `warn` severity rather than as a missing diagnostic.
  const result = await oxlint(root, [
    "--format=json",
    "tests/fixtures/lint/js-plugins/demo.tsrx",
    "tests/fixtures/lint/js-plugins/ordinary.tsx",
  ]);
  const parsed = report(result);
  for (const diagnostic of pluginDiagnostics(parsed, "demo.tsrx")) {
    assert.equal(diagnostic.severity, "error", "the .tsrx override did not reach the projection");
  }
  for (const diagnostic of pluginDiagnostics(parsed, "ordinary.tsx")) {
    assert.equal(diagnostic.severity, "warning", "the ordinary half picked up the .tsrx override");
  }
});

test("the project's own severities and rule options reach the .tsrx path", async () => {
  await withProject(
    {
      jsPlugins: ["./demo-plugin.mjs"],
      rules: { "tsrx-js-demo/no-banned-identifier": "warn" },
    },
    async (project) => {
      const result = await oxlint(project, ["--format=json", "src/demo.tsrx"]);
      const parsed = report(result);
      const diagnostics = pluginDiagnostics(parsed, "demo.tsrx");
      assert.ok(diagnostics.length > 0, JSON.stringify(parsed.diagnostics, null, 2));
      for (const diagnostic of diagnostics) assert.equal(diagnostic.severity, "warning");
      assert.equal(result.code, 0, result.stderr);
    },
  );

  // A rule the project never enabled must stay off, on `.tsrx` as everywhere else.
  await withProject(BASE_CONFIG, async (project) => {
    const parsed = report(await oxlint(project, ["--format=json", "src/demo.tsrx"]));
    assert.ok(
      !parsed.diagnostics.some((diagnostic) =>
        String(diagnostic.code ?? "").includes("report-filename"),
      ),
      "a rule the project did not enable fired anyway",
    );
  });
});

test("a plugin resolved through extends still runs on .tsrx", async () => {
  await withProject(
    { extends: ["./configs/plugins.json"] },
    async (project) => {
      const parsed = report(await oxlint(project, ["--format=json", "src/demo.tsrx"]));
      assert.ok(
        pluginDiagnostics(parsed, "demo.tsrx").length > 0,
        `an extended config's jsPlugins were dropped:\n${JSON.stringify(parsed.diagnostics, null, 2)}`,
      );
    },
    {
      // Relative to the extending config, which is what makes this a real test of
      // path rewriting: the projection config is read from a different directory.
      "configs/plugins.json": `${JSON.stringify({
        jsPlugins: ["../demo-plugin.mjs"],
        rules: { "tsrx-js-demo/no-banned-identifier": "error" },
      })}\n`,
    },
  );
});

test("the { name, specifier } plugin form Vite+ writes runs on .tsrx too", async () => {
  await withProject(
    {
      // Oxlint accepts an alias form as well as a bare specifier, and this is
      // the one a `vp create` project's lint block is scaffolded with.
      jsPlugins: [{ name: "aliased-demo", specifier: "./demo-plugin.mjs" }],
      rules: { "aliased-demo/no-banned-identifier": "error" },
    },
    async (project) => {
      const parsed = report(await oxlint(project, ["--format=json", "src/demo.tsrx"]));
      const diagnostics = (parsed.diagnostics ?? []).filter((diagnostic) =>
        String(diagnostic.code ?? "").startsWith("aliased-demo("),
      );
      assert.ok(
        diagnostics.length > 0,
        `the aliased plugin form was dropped:\n${JSON.stringify(parsed.diagnostics, null, 2)}`,
      );
    },
  );
});

test("an explicit --config is honoured on the .tsrx path", async () => {
  await withProject(
    { rules: {} },
    async (project) => {
      const parsed = report(
        await oxlint(project, ["--format=json", "-c", "explicit.json", "src/demo.tsrx"]),
      );
      assert.ok(
        pluginDiagnostics(parsed, "demo.tsrx").length > 0,
        `the explicit config's jsPlugins were dropped:\n${JSON.stringify(parsed.diagnostics, null, 2)}`,
      );
    },
    { "explicit.json": `${JSON.stringify(BASE_CONFIG)}\n` },
  );
});

test("context.filename is the projection's path in the mirror, not the authored .tsrx", async () => {
  // Documented in docs/integrations/custom-js-plugins.md as a known difference.
  // Pinning it here is what stops that paragraph becoming a guess.
  await withProject(
    {
      jsPlugins: ["./demo-plugin.mjs"],
      rules: { "tsrx-js-demo/report-filename": "warn" },
    },
    async (project) => {
      const parsed = report(await oxlint(project, ["--format=json", "src/demo.tsrx"]));
      const reported = parsed.diagnostics.find((diagnostic) =>
        String(diagnostic.message ?? "").startsWith("context.filename="),
      );
      assert.ok(reported, JSON.stringify(parsed.diagnostics, null, 2));
      const seen = reported.message.slice("context.filename=".length);
      assert.ok(seen.endsWith(`src${"/"}demo.tsrx.tsx`), seen);
      assert.ok(!seen.startsWith(project), `${seen} must not be inside the project itself`);
      // The diagnostic still lands on the authored file, which is the part that
      // matters to whoever reads the report.
      assert.ok(reported.filename.endsWith("src/demo.tsrx"), reported.filename);
    },
  );
});

test("the opt-out restores the native refusal, in the words the source writes", async () => {
  const rust = await readFile(rejectionSource, "utf8");
  const quoted = rust.match(REFUSAL_PATTERN);
  assert.ok(quoted, "the rejection message moved out of crates/oxc_adapter/src/toolchain.rs");

  await withProject(
    { ...BASE_CONFIG, settings: { oxcTsrx: { jsPluginsOnTsrx: false } } },
    async (project) => {
      const result = await oxlint(project, ["src/demo.tsrx"]);
      assert.equal(result.code, 2, result.stdout);
      assert.ok(
        result.stderr.includes(quoted[1]),
        `expected the native refusal on stderr, got:\n${result.stderr}`,
      );
      assert.ok(
        !result.stderr.includes("running JS plugins on"),
        "the opt-out must not still disclose a lane it did not run",
      );
    },
  );
});

// The editor half of the same lane. `jsPlugins` reaching the native engine is what
// took every `.tsrx` diagnostic away in the editor, including the Rust ones, so the
// language server strips it exactly like the command line does. This checks the two
// halves against each other rather than checking the editor on its own: a strip that
// quietly linted a different configuration would still publish something, and only
// comparing it to what `oxlint` reports for the same file would catch it.
test("the editor reports the same native .tsrx diagnostics as the CLI with jsPlugins on", async () => {
  await withProject(BASE_CONFIG, async (project) => {
    const parsed = report(await oxlint(project, ["--format=json", "src/demo.tsrx"]));
    const fromCli = (parsed.diagnostics ?? [])
      .filter((diagnostic) => diagnostic.rule === "no-debugger")
      .map((diagnostic) => diagnostic.labels[0].span);
    assert.equal(fromCli.length, 1, JSON.stringify(parsed.diagnostics));

    const source = await readFile(join(project, "src/demo.tsrx"), "utf8");
    const uri = pathToFileUri(join(project, "src/demo.tsrx"));
    const client = new LspClient(binary, { args: SERVER_ARGUMENTS, cwd: project });
    try {
      await client.initialize(pathToFileUri(project));
      client.notify("textDocument/didOpen", {
        textDocument: { uri, languageId: "markless-tsrx", version: 1, text: source },
      });
      const published = await client.waitFor(
        (message) =>
          message.method === "textDocument/publishDiagnostics" && message.params.uri === uri,
        5000,
        "editor diagnostics with jsPlugins configured",
      );
      const fromEditor = published.params.diagnostics.filter(
        (diagnostic) => diagnostic.code === "no-debugger",
      );
      assert.equal(fromEditor.length, 1, JSON.stringify(published.params.diagnostics));
      // `oxlint` counts line and column in the authored bytes; the editor answers in
      // zero-based lines and UTF-16 columns. Compare through the authored source so
      // neither number is taken on trust.
      const { line, column } = locationOf(source, fromCli[0].offset);
      assert.equal(fromEditor[0].range.start.line, line - 1);
      assert.equal(fromEditor[0].range.start.character, column - 1);
      await client.close();
    } finally {
      client.terminate();
    }
  });
});

test("the refusal no longer claims the public package has no plugin host", async () => {
  const rust = await readFile(rejectionSource, "utf8");
  assert.ok(
    !rust.includes("does not expose its zero-copy plugin host"),
    "the refusal still asserts a claim this lane disproves",
  );
});

test("ordinary files keep reaching canonical Oxlint untouched", async () => {
  await withProject(BASE_CONFIG, async (project) => {
    const result = await oxlint(project, ["--format=json", "src/ordinary.tsx"]);
    const parsed = report(result);
    assert.ok(pluginDiagnostics(parsed, "ordinary.tsx").length > 0, result.stdout);
    // No `.tsrx` file is in this batch, so nothing was projected and nothing is
    // disclosed.
    assert.ok(!result.stderr.includes("running JS plugins on"), result.stderr);
  });
});

test("a project with no jsPlugins is unchanged", async () => {
  await withProject({ rules: { "no-debugger": "error" } }, async (project) => {
    const result = await oxlint(project, ["--format=json", "src/demo.tsrx"]);
    const parsed = report(result);
    assert.ok(!result.stderr.includes("running JS plugins on"), result.stderr);
    assert.equal(parsed.oxcTsrx.jsPluginProjection, undefined);
    assert.ok(parsed.diagnostics.some((diagnostic) => diagnostic.rule === "no-debugger"));
  });
});

test("the native binary's two plugin modes answer on their own", async () => {
  const emitted = await run(root, binary, [
    "lint",
    "--emit-plugin-projection",
    "tests/fixtures/lint/js-plugins/demo.tsrx",
  ]);
  assert.equal(emitted.code, 0, emitted.stderr);
  const { projections } = JSON.parse(emitted.stdout);
  assert.equal(projections.length, 1);
  const source = await readFile(join(fixtures, "demo.tsrx"), "utf8");
  // The projection is legal TSX, so the TSRX-only syntax is gone from it.
  assert.ok(source.includes("@{"), "the fixture stopped being TSRX");
  assert.ok(!projections[0].projected.includes("@{"), projections[0].projected);
  assert.ok(projections[0].projected.includes("const banned = items.length;"));

  // A label on text the projection inserted has no authored position, so the
  // whole diagnostic is dropped rather than reported somewhere the user can see
  // no such code.
  const markerOffset = projections[0].projected.indexOf("/*_t0_");
  assert.ok(markerOffset > 0);
  const bannedOffset = projections[0].projected.indexOf("banned");

  const request = JSON.stringify({
    files: [
      {
        path: join(fixtures, "demo.tsrx"),
        diagnostics: [
          {
            code: "demo(keeps)",
            message: "authored",
            labels: [{ span: { offset: bannedOffset, length: 6, line: 99, column: 99 } }],
          },
          {
            code: "demo(drops)",
            message: "projection only",
            labels: [{ span: { offset: markerOffset, length: 4 } }],
          },
          { code: "demo(no-labels)", message: "nothing to point at", labels: [] },
        ],
      },
    ],
  });
  const result = await new Promise((resolvePromise, reject) => {
    const child = spawn(binary, ["lint", "--map-plugin-diagnostics"], {
      cwd: root,
      stdio: ["pipe", "pipe", "pipe"],
    });
    let stdout = "";
    let stderr = "";
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => (stdout += chunk));
    child.stderr.on("data", (chunk) => (stderr += chunk));
    child.once("error", reject);
    child.once("close", (code) => resolvePromise({ code, stdout, stderr }));
    child.stdin.end(request);
  });
  assert.equal(result.code, 0, result.stderr);
  const answered = JSON.parse(result.stdout).files[0].diagnostics;
  assert.deepEqual(
    answered.map((diagnostic) => diagnostic.code),
    ["demo(keeps)"],
  );
  const span = answered[0].labels[0].span;
  assert.equal(span.offset, source.indexOf("banned"));
  assert.equal(span.length, 6);
  // Oxlint resolved line and column against the projection; they must not
  // survive, or the wrapper would print a position from the wrong file.
  assert.equal(span.line, undefined);
  assert.equal(span.column, undefined);
});

test("the supported Oxlint range is asserted rather than assumed", () => {
  assert.equal(OXLINT_JS_PLUGIN_LANE_MINIMUM, "1.74.0");
  assert.equal(OXLINT_JS_PLUGIN_LANE_BELOW, "2.0.0");
  assert.ok(laneSupportsOxlintVersion(installedOxlintVersion()), installedOxlintVersion());

  assert.ok(laneSupportsOxlintVersion("1.74.0"));
  assert.ok(laneSupportsOxlintVersion("1.99.3"));
  assert.ok(!laneSupportsOxlintVersion("1.73.9"));
  assert.ok(!laneSupportsOxlintVersion("2.0.0"));
  assert.ok(!laneSupportsOxlintVersion("unknown"));

  // The refusal has to name the range and say why it is refusing, because a
  // reader who sees it needs to know their rules did not run.
  const refusal = oxlintVersionRefusal("2.1.0");
  assert.match(refusal, /oxlint >=1\.74\.0 <2\.0\.0; found 2\.1\.0/u);
  assert.match(refusal, /Refusing rather than silently skipping your rules\./u);
});

test("the projection config keeps the project's rules and turns the built-ins off", () => {
  const projected = projectionConfig(
    {
      $schema: "./node_modules/oxlint/configuration_schema.json",
      jsPlugins: ["./plugin.mjs", "some-package"],
      extends: ["./shared.json"],
      categories: { correctness: "error" },
      plugins: ["react"],
      rules: { "demo/rule": ["error", { option: 1 }] },
      ignorePatterns: ["dist/**"],
      overrides: [{ files: ["**/*.tsrx"], excludeFiles: ["gen/*.tsrx"], rules: {} }],
      settings: { oxcTsrx: { jsPluginsOnTsrx: true } },
    },
    "/project",
  );

  // Every built-in category off: the native lane is the only reporter of
  // built-in rules on `.tsrx`, so leaving one on would print it twice.
  assert.deepEqual(projected.categories, {
    correctness: "off",
    nursery: "off",
    pedantic: "off",
    perf: "off",
    restriction: "off",
    style: "off",
    suspicious: "off",
  });
  // The project's own rule entry survives with its options intact.
  assert.deepEqual(projected.rules, { "demo/rule": ["error", { option: 1 }] });
  assert.deepEqual(projected.plugins, ["react"]);
  assert.deepEqual(projected.settings, { oxcTsrx: { jsPluginsOnTsrx: true } });
  assert.equal(projected.ignorePatterns, undefined);
  assert.equal(projected.$schema, undefined);
  assert.equal(projected.jsPlugins[0], resolve("/project", "./plugin.mjs"));
  assert.equal(projected.extends[0], resolve("/project", "./shared.json"));
  assert.deepEqual(projected.overrides[0].files, ["**/*.tsrx", "**/*.tsrx.tsx"]);
  assert.deepEqual(projected.overrides[0].excludeFiles, ["gen/*.tsrx", "gen/*.tsrx.tsx"]);
});

test("the native lane's config loses jsPlugins and nothing else", () => {
  const stripped = nativeLaneConfig({
    jsPlugins: ["./plugin.mjs"],
    rules: { "no-debugger": "error" },
    ignorePatterns: ["dist/**"],
    overrides: [{ files: ["**/*.tsx"], jsPlugins: ["./other.mjs"], rules: { a: "warn" } }],
  });
  assert.equal(stripped.jsPlugins, undefined);
  assert.equal(stripped.overrides[0].jsPlugins, undefined);
  assert.deepEqual(stripped.rules, { "no-debugger": "error" });
  assert.deepEqual(stripped.ignorePatterns, ["dist/**"]);
  assert.deepEqual(stripped.overrides[0].files, ["**/*.tsx"]);
});

test("JSONC configs are read, and mirror paths stay inside the mirror", () => {
  assert.deepEqual(
    parseOxlintConfigText(`{
      // a line comment
      "jsPlugins": ["./p.mjs"], /* and a block one */
      "rules": { "a/b": "error" }, // trailing comma next
    }`),
    { jsPlugins: ["./p.mjs"], rules: { "a/b": "error" } },
  );
  // A `//` inside a string is not a comment.
  assert.deepEqual(parseOxlintConfigText('{"url": "https://oxc.rs"}'), {
    url: "https://oxc.rs",
  });

  assert.equal(mirrorRelativePath("/project", "/project/src/View.tsrx"), join("src", "View.tsrx.tsx"));
  const outside = mirrorRelativePath("/project", "/elsewhere/View.tsrx");
  assert.ok(outside.startsWith("__outside_cwd__"), outside);
  assert.ok(!outside.includes(".."), outside);
});
