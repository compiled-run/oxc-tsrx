// The oracle for docs/integrations/custom-js-plugins.md.
//
// The page is a tutorial, so it is only correct if a reader can run it. This
// file executes its commands against the files it tells the reader to create
// and refuses the two ways such a page rots: a printed transcript nobody ran,
// and a code fence that has drifted from the runnable copy in examples/.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { cp, mkdir, mkdtemp, readFile, rm, symlink, writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import test from "node:test";

const require = createRequire(import.meta.url);
const root = resolve(import.meta.dirname, "../..");
const examples = join(root, "examples/custom-js-plugins");
const pagePath = join(root, "docs/integrations/custom-js-plugins.md");
const transcriptsPath = join(root, "docs/terminal-transcripts.json");
const companion = join(root, "packages/toolchain/bin/oxlint");
const toolchain = join(root, "packages/toolchain");
// Same resolution order as tests/config/native-lint-config.test.mjs.
const binary = resolve(process.env.OXLINT_BIN ?? join(root, "target/release/oxc-tsrx"));
const eslintCli = join(
  dirname(require.resolve("eslint/package.json", { paths: [join(root, "tests")] })),
  "bin/eslint.js",
);

// The single source of truth for the refusal the page documents.
const rejectionSource = join(root, "crates/oxc_adapter/src/toolchain/config.rs");

const page = await readFile(pagePath, "utf8");
const transcripts = JSON.parse(await readFile(transcriptsPath, "utf8"));

// The lane has an editor half, and it was documented one page late. These are
// the pages a reader lands on before the tutorial, so they are held to the same
// standard as the tutorial itself.
const editorPagePath = join(root, "docs/integrations/editor.md");
const readmePath = join(root, "README.md");

function fences(markdown) {
  const found = [];
  const pattern = /^```([\w-]*)\r?\n([\s\S]*?)^```[ \t]*$/gm;
  for (const match of markdown.matchAll(pattern)) {
    found.push({
      lang: match[1],
      body: match[2],
      index: match.index,
      bodyIndex: match.index + match[0].indexOf("\n") + 1,
    });
  }
  return found;
}

function run(cwd, executable, args, environment = process.env) {
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

function oxlint(cwd, args) {
  return run(cwd, process.execPath, [companion, ...args], {
    ...process.env,
    OXC_TSRX_LINT_BIN: binary,
  });
}

// The page's commands carry no `--format`, so Oxlint picks a reporter from the
// environment: the compact `agent` form inside a coding agent, GitHub's
// annotations when GITHUB_ACTIONS is exactly `true`, and the graphical form with
// source excerpts otherwise. The rule is restated here independently of
// packages/toolchain/dist/lint-cli.js, which reproduces it so a composed `.tsrx`
// batch and an ordinary one answer in the same shape. These assertions ask for
// the diagnostic in whichever reporter this environment selects rather than
// pinning one, because a divergence between the two is a real defect on a
// runner. The graphical reporter cannot be rebuilt from JSON, so a composed
// batch falls back to the compact form for it, and so does this expectation.
const AGENT_ENVIRONMENT_VARIABLES = [
  "AI_AGENT",
  "CLAUDECODE",
  "CLAUDE_CODE",
  "CODEX_SANDBOX",
  "CODEX_THREAD_ID",
  "COPILOT_CLI",
  "CURSOR_AGENT",
  "GEMINI_CLI",
  "JUNIE_DATA",
  "JUNIE_SHIM_PATH",
  "OPENCODE",
  "REPL_ID",
];

function ambientReporter(env = process.env) {
  if (AGENT_ENVIRONMENT_VARIABLES.some((name) => (env[name] ?? "") !== "")) return "agent";
  if ((env.EDITOR ?? "").includes("devin")) return "agent";
  if (env.TERM_PROGRAM === "kiro") return "agent";
  if (env.GITHUB_ACTIONS === "true") return "github";
  return "default";
}

// A batch canonical Oxlint answers by itself uses the environment's reporter; a
// batch that had to be composed here falls back from the graphical one.
const canonicalReporter = ambientReporter();
const composedReporter = canonicalReporter === "default" ? "agent" : canonicalReporter;

function diagnosticPattern(reporter, { file, line = "\\d+", column = "\\d+", severity, code }) {
  const escaped = file.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&");
  const title = code.replace(/[()/]/gu, "\\$&");
  if (reporter === "github") {
    return new RegExp(
      `^::${severity} file=${escaped},line=${line},endLine=\\d+,col=${column},endColumn=\\d+,title=${title}::`,
      "mu",
    );
  }
  if (reporter === "agent") {
    return new RegExp(`^${escaped}:${line}:${column}: ${severity} ${title}: `, "mu");
  }
  // The graphical reporter prints the code above a `,-[file:line:col]` rule.
  return new RegExp(`${title}:[\\s\\S]*?,-\\[${escaped}:${line}:${column}\\]`, "u");
}

// The page's project is the examples directory plus an installed oxc-tsrx. The
// only edit is the parser adapter's import specifier, which the page itself
// tells the reader to change; docs/generate-transcripts.mjs makes the same swap.
async function makeProject() {
  const project = await mkdtemp(join(tmpdir(), "oxc-tsrx-custom-plugins-doc-"));
  await cp(examples, project, { recursive: true });
  const adapter = join(project, "tsrx-eslint-parser.mjs");
  const source = await readFile(adapter, "utf8");
  const swapped = source.replace(
    '"../../packages/toolchain/dist/parser.js"',
    '"oxc-tsrx/parser"',
  );
  assert.notEqual(swapped, source, "the adapter's relative parser import moved");
  await writeFile(adapter, swapped);
  await mkdir(join(project, "node_modules"), { recursive: true });
  await symlink(toolchain, join(project, "node_modules/oxc-tsrx"), "dir");
  return project;
}

test("the native binary the page's commands need is built", () => {
  assert.ok(
    existsSync(binary),
    `missing ${binary}. Build it with:\n  cargo build --release --locked -p oxc_tsrx_cli --bins\nor point OXLINT_BIN at an existing binary.`,
  );
});

test("every terminal-demo marker on the page resolves to a captured transcript", () => {
  const names = [...page.matchAll(/<!-- terminal-demo:([a-z0-9-]+) -->/g)].map(
    (match) => match[1],
  );
  assert.ok(names.length > 0, "the page prints no captured commands at all");
  for (const name of names) {
    const demo = transcripts.demos?.[name];
    assert.ok(demo, `docs/terminal-transcripts.json has no demo named ${name}`);
    assert.ok(
      Array.isArray(demo.transcript) && demo.transcript.length > 0,
      `demo ${name} has an empty transcript`,
    );
    for (const entry of demo.transcript) {
      assert.ok(entry.command?.length > 0, `demo ${name} has an entry with no command`);
      assert.ok(entry.output?.trim().length > 0, `demo ${name} entry printed nothing`);
    }
  }
});

test("every file the page tells you to save matches examples/custom-js-plugins", async () => {
  // Tolerate the sentence wrapping across lines. A single-line-only pattern
  // silently drops coverage for any instruction that reflows.
  const instructions = [...page.matchAll(/Save\s+this\s+as\s+`([^`]+)`:\r?\n\r?\n```/g)];
  assert.ok(instructions.length >= 7, "the page stopped telling readers to save files");
  for (const instruction of instructions) {
    const name = instruction[1];
    const fence = fences(page).find((entry) => entry.index > instruction.index);
    const expected = await readFile(join(examples, name), "utf8");
    assert.equal(
      fence.body,
      expected,
      `the fence for ${name} differs from examples/custom-js-plugins/${name}`,
    );
  }
});

test("the page prints no hand-typed terminal output", () => {
  const pmInstall = new Set(
    [...page.matchAll(/<!-- pm-install -->\r?\n```sh\r?\n/g)].map(
      (match) => match.index + match[0].length,
    ),
  );
  for (const fence of fences(page)) {
    if (fence.lang === "sh") {
      assert.ok(
        pmInstall.has(fence.bodyIndex),
        `a shell fence on the page is not an install block:\n${fence.body}`,
      );
      continue;
    }
    assert.ok(
      ["js", "json", "jsonc", "tsrx", "tsx", "ts", "rust"].includes(fence.lang),
      `fence with language "${fence.lang}" looks like pasted terminal output; use <!-- terminal-demo:NAME -->`,
    );
  }
});

// The refusal did not disappear when JS plugins started running on `.tsrx`; it
// moved. It is now what you get if you switch the projection lane off, or if you
// drive the native lint target directly instead of through `oxlint`. The page
// still has to print it in the exact words the source writes, because a reader
// who sees it needs to recognise it.
const REFUSAL_PATTERN =
  /"(JavaScript plugins are not hosted by the native TSRX lint target itself:[^"]*)"/;

test("the page prints the native refusal exactly as the source writes it", async () => {
  const rust = await readFile(rejectionSource, "utf8");
  const quoted = rust.match(REFUSAL_PATTERN);
  assert.ok(
    quoted,
    "the rejection message moved out of crates/oxc_adapter/src/toolchain/config.rs",
  );
  const message = quoted[1];

  const optOut = transcripts.demos?.["custom-plugins-tsrx-opt-out"];
  assert.ok(optOut, "the .tsrx opt-out demo is missing");
  assert.ok(
    optOut.transcript.some((entry) => entry.output.includes(message)),
    "the captured opt-out transcript no longer contains the native refusal",
  );

  const rendered = join(root, "docs/dist/integrations/custom-js-plugins.md");
  if (existsSync(rendered)) {
    assert.ok(
      (await readFile(rendered, "utf8")).includes(message),
      "the built page no longer shows the native refusal",
    );
  }
});

test("the page no longer claims a JavaScript plugin cannot run on .tsrx", async () => {
  const rust = await readFile(rejectionSource, "utf8");
  assert.ok(
    !rust.includes("does not expose its zero-copy plugin host"),
    "the refusal still asserts a claim the projection lane disproves",
  );
  assert.ok(
    !page.includes("a JavaScript plugin does not run on `.tsrx`"),
    "the page still describes the removed wall",
  );
  assert.ok(
    transcripts.demos?.["custom-plugins-tsrx-wall"] === undefined,
    "the captured wall transcript outlived the wall",
  );
});

test("the editor page documents the editor half of the lane", async () => {
  const editor = await readFile(editorPagePath, "utf8");

  // The four facts a reader needs before their own rule surprises them.
  assert.match(editor, /jsPlugins/u, "the editor page never mentions jsPlugins");
  assert.match(
    editor,
    /parses every linted \.tsrx file once more|extra parse of each `\.tsrx` file/u,
    "the editor page does not disclose the extra parse",
  );
  assert.match(editor, /jsPluginsOnTsrx/u, "the editor page omits the opt-out key");
  assert.match(
    editor,
    /context\.filename/u,
    "the editor page omits the mirror-path difference",
  );
  assert.match(
    editor,
    /js-plugins-unavailable/u,
    "the editor page does not say a failing plugin is surfaced",
  );

  // The stale claim this lane disproved, in the words the page used to use.
  assert.doesNotMatch(
    editor,
    /runs no JavaScript rules\)/u,
    "the editor page still says the native path runs no JavaScript rules",
  );
});

test("no page still claims a plain install serves .tsrx before activation", async () => {
  for (const path of [editorPagePath, readmePath]) {
    const text = await readFile(path, "utf8");
    // Every page that promises editor diagnostics has to name the activation
    // step in the same breath, because Ripple's extension owns `.tsrx` as the
    // language id `ripple` and the official OXC extension activates on neither.
    assert.match(
      text,
      /activation event|activationEvents|onLanguage/u,
      `${path} promises editor diagnostics without naming the activation step`,
    );
    assert.match(text, /ripple/iu, `${path} does not name the extension that owns .tsrx`);
  }
});

test("the pages that describe the .tsrx lint boundary no longer wall off plugins", async () => {
  const claims = [
    // Each entry is a page and a phrase that was true before this lane shipped.
    [join(root, "docs/reference/limitations.md"), /JavaScript lint plugins do not run in the native TSRX CLI/u],
    [join(root, "docs/integrations/configuration.md"), /JavaScript plugins \(`jsPlugins`\) on the `\.tsrx` lane\./u],
    [join(root, "packages/toolchain/README.md"), /They are not a\s+host that executes one against `\.tsrx`/u],
    [readmePath, /On `\.tsrx` they fail loudly/u],
  ];
  for (const [path, stale] of claims) {
    const text = await readFile(path, "utf8");
    assert.doesNotMatch(text, stale, `${path} still describes the removed wall`);
    assert.match(
      text,
      /jsPluginsOnTsrx|extra parse|once more/u,
      `${path} describes the lane without its cost`,
    );
  }
});

test("the page's commands really do what the page says", async (t) => {
  if (!existsSync(binary)) {
    assert.fail(
      `missing ${binary}. Build it with: cargo build --release --locked -p oxc_tsrx_cli --bins`,
    );
  }
  const project = await makeProject();
  try {
    await t.test("oxlint lints .tsrx with a built-in rule and no config", async () => {
      await rm(join(project, ".oxlintrc.json"));
      const result = await oxlint(project, ["src/TaskList.tsrx"]);
      assert.equal(result.code, 0, result.stderr);
      assert.match(
        result.stdout,
        diagnosticPattern(composedReporter, {
          file: "src/TaskList.tsrx",
          line: 4,
          column: 3,
          severity: "warning",
          code: "eslint(no-debugger)",
        }),
        result.stdout,
      );
      await cp(join(examples, ".oxlintrc.json"), join(project, ".oxlintrc.json"));
    });

    await t.test("oxlint runs the JavaScript plugin on the ordinary .tsx file", async () => {
      const result = await oxlint(project, ["src/TaskRow.tsx"]);
      assert.equal(result.code, 1, result.stderr);
      assert.match(
        result.stdout + result.stderr,
        // Canonical Oxlint answers an ordinary file on its own, so this is its
        // own reporter rather than the composed one.
        diagnosticPattern(canonicalReporter, {
          file: "src/TaskRow.tsx",
          severity: "error",
          code: "tsrx-demo(require-keyed-map)",
        }),
        result.stdout + result.stderr,
      );
    });

    await t.test("oxlint runs the same plugin on .tsrx, and says what it cost", async () => {
      const result = await oxlint(project, ["src/TaskList.tsrx"]);
      // `require-keyed-map` looks for `.map()`, and TaskList.tsrx has an `@for`
      // block, so the rule runs and finds nothing. The page says exactly that.
      assert.equal(result.code, 0, result.stdout + result.stderr);
      assert.match(
        result.stderr,
        /^oxlint \(oxc-tsrx\): running JS plugins on 1 \.tsrx file\(s\)/mu,
        `expected the extra-parse disclosure on stderr, got:\n${result.stderr}`,
      );
      assert.match(
        result.stdout + result.stderr,
        diagnosticPattern(composedReporter, {
          file: "src/TaskList.tsrx",
          line: 4,
          column: 3,
          severity: "warning",
          code: "eslint(no-debugger)",
        }),
        "the native Rust rules stopped reporting once a plugin was configured",
      );
    });

    await t.test("the page's own src/TaskFeed.tsrx really reports the plugin rule", async () => {
      // The fence the page tells you to add is executed here rather than
      // trusted, because it is the one file on the page that is not mirrored in
      // examples/custom-js-plugins.
      const fence = page.match(/Add this as `src\/TaskFeed\.tsrx`:\r?\n\r?\n```tsrx\r?\n([\s\S]*?)^```/mu);
      assert.ok(fence, "the page stopped telling readers to add src/TaskFeed.tsrx");
      await writeFile(join(project, "src/TaskFeed.tsrx"), fence[1]);
      const result = await oxlint(project, ["src/TaskFeed.tsrx"]);
      assert.equal(result.code, 1, result.stdout + result.stderr);
      assert.match(
        result.stdout + result.stderr,
        diagnosticPattern(composedReporter, {
          file: "src/TaskFeed.tsrx",
          line: 4,
          column: 36,
          severity: "error",
          code: "tsrx-demo(require-keyed-map)",
        }),
        result.stdout + result.stderr,
      );
      await rm(join(project, "src/TaskFeed.tsrx"));
    });

    await t.test("the settings opt-out restores the native refusal with exit 2", async () => {
      const rust = await readFile(rejectionSource, "utf8");
      const message = rust.match(REFUSAL_PATTERN)[1];
      const config = JSON.parse(await readFile(join(project, ".oxlintrc.json"), "utf8"));
      await writeFile(
        join(project, ".oxlintrc.json"),
        JSON.stringify({ ...config, settings: { oxcTsrx: { jsPluginsOnTsrx: false } } }, null, 2),
      );
      try {
        const result = await oxlint(project, ["src/TaskList.tsrx"]);
        assert.equal(result.code, 2, result.stdout);
        assert.ok(
          result.stderr.includes(message),
          `expected the native refusal on stderr, got:\n${result.stderr}`,
        );
      } finally {
        await writeFile(join(project, ".oxlintrc.json"), JSON.stringify(config, null, 2));
      }
    });

    await t.test("the ESLint escape hatch reports both tsrx-demo rules", async () => {
      const result = await run(project, process.execPath, [eslintCli, "src/TaskList.tsrx"]);
      assert.equal(result.code, 1, result.stderr);
      assert.match(result.stdout, /warning.+tsrx-demo\/no-tsrx-if/u);
      assert.match(result.stdout, /error.+tsrx-demo\/require-keyed-for/u);
    });
  } finally {
    await rm(project, { recursive: true, force: true });
  }
});
