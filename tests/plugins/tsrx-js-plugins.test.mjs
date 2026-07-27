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
  jsPluginEditorDisclosure,
  jsPluginUnmappedNote,
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

  // The lane is disclosed in the machine-readable report too, not only on stderr,
  // and `unmapped` says how many of this project's plugin diagnostics had no
  // authored position and were dropped. Zero here: both reports landed.
  assert.deepEqual(parsed.oxcTsrx.jsPluginProjection, {
    files: 1,
    extraParses: 1,
    unmapped: 0,
  });
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

// A rule that reports on the whole `Program` is the shape this lane got wrong for
// longest. Its span is everything Oxlint linted, which on this route is the
// projection with every marker and synthetic wrapper in it, so the all-or-nothing
// mapping cannot place it. The first fix recognised it as `offset == 0`, and a
// `Program` only starts at byte zero when nothing precedes the first token: one
// blank line, comment, or `// @ts-nocheck` above it and the report vanished again,
// on a `.tsrx` that was byte-identical to a `.tsx` where the same rule fired.
// Nothing caught that, because nothing tested leading trivia. This does.
const WHOLE_FILE_PLUGIN = `const wholeFile = {
  meta: {
    type: "problem",
    docs: { description: "Report once on the whole file" },
    schema: [],
  },
  create(context) {
    return {
      Program(node) {
        context.report({ node, message: "this file was reported as a whole" });
      },
    };
  },
};

export default {
  meta: { name: "tsrx-js-whole", version: "0.1.0" },
  rules: { "whole-file": wholeFile },
};
`;

const WHOLE_FILE_CONFIG = {
  jsPlugins: ["./whole-file.mjs"],
  rules: { "tsrx-js-whole/whole-file": "error" },
};

/** The same component, once in TSRX and once in ordinary TSX. */
const WHOLE_FILE_TSRX = `export function Widget() @{
  const label = "hi";

  <p>{label}</p>;
}
`;
const WHOLE_FILE_TSX = `export function Widget() {
  const label = "hi";

  return <p>{label}</p>;
}
`;

/** Everything that can sit above the first token of a file. */
const LEADING_TRIVIA = {
  nothing: "",
  "a blank line": "\n",
  "a line comment": "// a comment above everything\n",
  "a block comment": "/* a block comment above everything */\n",
  "a ts-nocheck": "// @ts-nocheck\n",
};

function trivialSlug(name) {
  return name.replace(/[^a-z]+/giu, "-");
}

function wholeFileFixtures() {
  const files = { "whole-file.mjs": WHOLE_FILE_PLUGIN };
  for (const [name, prefix] of Object.entries(LEADING_TRIVIA)) {
    const slug = trivialSlug(name);
    files[`src/${slug}.tsrx`] = prefix + WHOLE_FILE_TSRX;
    files[`src/${slug}.tsx`] = prefix + WHOLE_FILE_TSX;
  }
  return files;
}

test("a rule reporting on the whole Program fires on .tsrx whatever precedes the first token", async () => {
  await withProject(
    WHOLE_FILE_CONFIG,
    async (project) => {
      for (const [name, prefix] of Object.entries(LEADING_TRIVIA)) {
        const slug = trivialSlug(name);
        const result = await oxlint(project, [
          "--format=json",
          `src/${slug}.tsrx`,
          `src/${slug}.tsx`,
        ]);
        const parsed = report(result);
        const reported = (extension) =>
          (parsed.diagnostics ?? []).filter(
            (diagnostic) =>
              diagnostic.filename.endsWith(`${slug}.${extension}`) &&
              String(diagnostic.code ?? "").startsWith("tsrx-js-whole("),
          );
        const shown = JSON.stringify(parsed.diagnostics, null, 2);
        const onTsrx = reported("tsrx");
        const onTsx = reported("tsx");
        // The ordinary file is the control: whatever the rule does there, the
        // `.tsrx` file with the same trivia above it has to do as well.
        assert.equal(onTsx.length, 1, `${name}: the .tsx control lost the rule\n${shown}`);
        assert.equal(onTsrx.length, 1, `${name}: the rule vanished on .tsrx\n${shown}`);

        // The position is checked against the authored bytes rather than against
        // the other report, so both landing in the same wrong place would fail.
        const source = prefix + WHOLE_FILE_TSRX;
        const expected = locationOf(source, source.indexOf("export"));
        const span = onTsrx[0].labels[0].span;
        assert.equal(span.offset, source.indexOf("export"), name);
        assert.equal(span.offset + span.length, Buffer.byteLength(source), name);
        assert.deepEqual({ line: span.line, column: span.column }, expected, name);
        // And it is where the identical `.tsx` puts it.
        const control = onTsx[0].labels[0].span;
        assert.deepEqual(
          { line: span.line, column: span.column },
          { line: control.line, column: control.column },
          `${name}: the .tsrx report moved away from the .tsx one`,
        );

        // Nothing was lost on the way, and the report says so.
        assert.equal(parsed.oxcTsrx.jsPluginProjection.unmapped, 0, name);
        assert.ok(
          !result.stderr.includes("had no position in the source you wrote"),
          `${name}: a dropped diagnostic was reported where none was dropped`,
        );
      }
    },
    wholeFileFixtures(),
  );
});

test("the editor publishes a whole-file rule where the CLI reports it", async () => {
  await withProject(
    WHOLE_FILE_CONFIG,
    async (project) => {
      // The variant that used to disappear: a comment above the first token.
      const name = "a line comment";
      const slug = trivialSlug(name);
      const relative = `src/${slug}.tsrx`;
      const parsed = report(await oxlint(project, ["--format=json", relative]));
      const fromCli = (parsed.diagnostics ?? []).filter((diagnostic) =>
        String(diagnostic.code ?? "").startsWith("tsrx-js-whole("),
      );
      assert.equal(fromCli.length, 1, JSON.stringify(parsed.diagnostics, null, 2));
      const span = fromCli[0].labels[0].span;
      const source = await readFile(join(project, relative), "utf8");
      const start = locationOf(source, span.offset);
      const end = locationOf(source, span.offset + span.length);

      const uri = pathToFileUri(join(project, relative));
      const client = new LspClient(binary, { args: SERVER_ARGUMENTS, cwd: project });
      try {
        await client.initialize(pathToFileUri(project));
        client.notify("textDocument/didOpen", {
          textDocument: { uri, languageId: "markless-tsrx", version: 1, text: source },
        });
        const published = await client.waitFor(
          (message) =>
            message.method === "textDocument/publishDiagnostics" &&
            message.params.uri === uri &&
            message.params.diagnostics.some((diagnostic) =>
              String(diagnostic.code ?? "").startsWith("tsrx-js-whole("),
            ),
          20000,
          "the editor's whole-file plugin diagnostic",
        );
        const fromEditor = published.params.diagnostics.filter((diagnostic) =>
          String(diagnostic.code ?? "").startsWith("tsrx-js-whole("),
        );
        assert.equal(fromEditor.length, 1, JSON.stringify(published.params.diagnostics, null, 2));
        assert.deepEqual(fromEditor[0].range, {
          start: { line: start.line - 1, character: start.column - 1 },
          end: { line: end.line - 1, character: end.column - 1 },
        });
        await client.close();
      } finally {
        client.terminate();
      }
    },
    wholeFileFixtures(),
  );
});

// The other half of the same rule: a report that really does belong to text the
// projection invented still has nowhere to go, and is still dropped. What changed
// is that the drop is now counted and said out loud instead of leaving a rule that
// looks like it found nothing.
const MARKER_PLUGIN = `export default {
  meta: { name: "tsrx-js-marker", version: "0.1.0" },
  rules: {
    "marker": {
      meta: { type: "problem", schema: [] },
      create(context) {
        return {
          Identifier(node) {
            if (!/^_t\\d+_/u.test(node.name)) return;
            context.report({ node, message: "this identifier exists only in the projection" });
          },
        };
      },
    },
  },
};
`;

test("a report that exists only in the projection is dropped, counted, and disclosed", async () => {
  await withProject(
    { jsPlugins: ["./marker.mjs"], rules: { "tsrx-js-marker/marker": "error" } },
    async (project) => {
      const result = await oxlint(project, ["--format=json", "src/demo.tsrx"]);
      const parsed = report(result);
      // Nothing is reported at a position the developer never wrote.
      assert.equal(
        (parsed.diagnostics ?? []).filter((diagnostic) =>
          String(diagnostic.code ?? "").startsWith("tsrx-js-marker("),
        ).length,
        0,
        JSON.stringify(parsed.diagnostics, null, 2),
      );
      // But the loss is a number in the report and a line on stderr, not silence.
      const unmapped = parsed.oxcTsrx.jsPluginProjection.unmapped;
      assert.ok(unmapped > 0, JSON.stringify(parsed.oxcTsrx, null, 2));
      assert.ok(
        result.stderr.includes(jsPluginUnmappedNote(unmapped)),
        `expected the dropped-diagnostic note, got:\n${result.stderr}`,
      );

      const silent = await oxlint(project, ["--silent", "--format=json", "src/demo.tsrx"]);
      assert.ok(
        !silent.stderr.includes("had no position in the source you wrote"),
        `--silent must suppress the note, got:\n${silent.stderr}`,
      );
    },
    { "marker.mjs": MARKER_PLUGIN },
  );
});

test("the editor says so too when a plugin diagnostic could not be placed", async () => {
  await withProject(
    { jsPlugins: ["./marker.mjs"], rules: { "tsrx-js-marker/marker": "error" } },
    async (project) => {
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
            message.method === "textDocument/publishDiagnostics" &&
            message.params.uri === uri &&
            message.params.diagnostics.some(
              (diagnostic) => diagnostic.code === "js-plugins-unmapped",
            ),
          20000,
          "the editor's dropped-diagnostic warning",
        );
        const note = published.params.diagnostics.find(
          (diagnostic) => diagnostic.code === "js-plugins-unmapped",
        );
        assert.match(note.message, /no position in the source you wrote/u);
        // The native Rust rules are still in the same publish.
        assert.ok(
          published.params.diagnostics.some((diagnostic) => diagnostic.code === "no-debugger"),
          JSON.stringify(published.params.diagnostics, null, 2),
        );
        await client.close();
      } finally {
        client.terminate();
      }
    },
    { "marker.mjs": MARKER_PLUGIN },
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

// The board's own oracle, checked end to end in one test: the same file, the same
// project, the same `.oxlintrc.json`, linted once by the command line and once by
// the language server, and the developer's own rule has to land on identical
// positions in both. A near-miss here is the failure mode this goal exists to
// prevent, so the comparison is between the two reports rather than against a
// number written down in the test.
test("the editor reports the developer's plugin rule at the CLI's own positions", async () => {
  await withProject(BASE_CONFIG, async (project) => {
    const parsed = report(await oxlint(project, ["--format=json", "src/demo.tsrx"]));
    const fromCli = pluginDiagnostics(parsed, "src/demo.tsrx")
      .map((diagnostic) => ({
        code: diagnostic.code,
        offset: diagnostic.labels[0].span.offset,
        length: diagnostic.labels[0].span.length,
      }))
      .sort((left, right) => left.offset - right.offset);
    assert.equal(fromCli.length, 2, JSON.stringify(parsed.diagnostics, null, 2));

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
        20000,
        "editor plugin diagnostics",
      );
      const fromEditor = published.params.diagnostics
        .filter((diagnostic) => String(diagnostic.code ?? "").startsWith("tsrx-js-demo("))
        .sort(
          (left, right) =>
            left.range.start.line - right.range.start.line ||
            left.range.start.character - right.range.start.character,
        );
      assert.equal(
        fromEditor.length,
        fromCli.length,
        JSON.stringify(published.params.diagnostics, null, 2),
      );
      for (const [index, expected] of fromCli.entries()) {
        const start = locationOf(source, expected.offset);
        const end = locationOf(source, expected.offset + expected.length);
        assert.equal(fromEditor[index].code, expected.code);
        assert.deepEqual(fromEditor[index].range, {
          start: { line: start.line - 1, character: start.column - 1 },
          end: { line: end.line - 1, character: end.column - 1 },
        });
      }

      // The native Rust rules are in the same publish. Custom rules arriving at the
      // cost of the built-in ones would be a trade nobody asked for.
      assert.ok(
        published.params.diagnostics.some((diagnostic) => diagnostic.code === "no-debugger"),
        JSON.stringify(published.params.diagnostics, null, 2),
      );
      // The extra parse is disclosed in the editor too, once, with its opt-out.
      const notice = jsPluginEditorDisclosure();
      assert.ok(client.stderr.includes(notice), `expected the disclosure, got:\n${client.stderr}`);
      assert.equal(client.stderr.split(notice).length - 1, 1, client.stderr);
      await client.close();
    } finally {
      client.terminate();
    }
  });
});

// Oxlint reports a rule that threw with an empty `filename`, no `code`, and no
// labels, which is the exact shape every file-matching filter in the lane drops.
// Left alone, a broken plugin is indistinguishable from a plugin that found
// nothing — the same silence this board exists to remove, one level down.
const THROWING_PLUGIN = `export default {
  meta: { name: "tsrx-js-boom", version: "0.0.1" },
  rules: {
    explode: {
      meta: { type: "problem", schema: [] },
      create() {
        return { Identifier() { throw new Error("this rule is broken on purpose"); } };
      },
    },
  },
};
`;
const THROWING_CONFIG = {
  jsPlugins: ["./boom.mjs"],
  rules: { "tsrx-js-boom/explode": "error" },
};

test("a plugin that throws is reported rather than silently dropped", async () => {
  await withProject(
    THROWING_CONFIG,
    async (project) => {
      const result = await oxlint(project, ["src/demo.tsrx"]);
      assert.match(result.stderr, /this rule is broken on purpose/u);
      // The authored file, not the throwaway mirror the developer never opened.
      assert.match(result.stderr, /File path: .*src[/\\]demo\.tsrx$/mu);
      assert.doesNotMatch(result.stderr, /demo\.tsrx\.tsx/u);
      // The native Rust rules still reported, and a broken rule is not a green run.
      assert.match(result.stdout, /no-debugger/u);
      assert.equal(result.code, 1, `${result.stdout}\n${result.stderr}`);
    },
    { "boom.mjs": THROWING_PLUGIN },
  );
});

test("a plugin that throws reaches the editor as a diagnostic of its own", async () => {
  await withProject(
    THROWING_CONFIG,
    async (project) => {
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
          20000,
          "broken-plugin editor diagnostics",
        );
        const reported = published.params.diagnostics.find(
          (diagnostic) => diagnostic.code === "js-plugins-unavailable",
        );
        assert.ok(reported, JSON.stringify(published.params.diagnostics, null, 2));
        assert.match(reported.message, /this rule is broken on purpose/u);
        // Losing a native rule because a plugin threw would be the T005 regression
        // all over again.
        assert.ok(
          published.params.diagnostics.some((diagnostic) => diagnostic.code === "no-debugger"),
          JSON.stringify(published.params.diagnostics, null, 2),
        );
        await client.close();
      } finally {
        client.terminate();
      }
    },
    { "boom.mjs": THROWING_PLUGIN },
  );
});

// The opt-out has to reach the editor's lane as well as the command line's, or a
// project that switched the extra parse off would still be paying for it.
test("the editor lane stays off when the project opted out", async () => {
  await withProject(
    { ...BASE_CONFIG, settings: { oxcTsrx: { jsPluginsOnTsrx: false } } },
    async (project) => {
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
          20000,
          "opted-out editor diagnostics",
        );
        assert.deepEqual(
          published.params.diagnostics.map((diagnostic) => diagnostic.code),
          ["lint-unavailable"],
          JSON.stringify(published.params.diagnostics, null, 2),
        );
        assert.ok(
          !client.stderr.includes("running this project's Oxlint JS plugins"),
          `the opt-out must not start the lane, got:\n${client.stderr}`,
        );
        await client.close();
      } finally {
        client.terminate();
      }
    },
  );
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
  const answeredFile = JSON.parse(result.stdout).files[0];
  const answered = answeredFile.diagnostics;
  assert.deepEqual(
    answered.map((diagnostic) => diagnostic.code),
    ["demo(keeps)"],
  );
  // The two that had no authored position are counted rather than left for the
  // caller to notice by comparing list lengths.
  assert.equal(answeredFile.unmapped, 2);
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
