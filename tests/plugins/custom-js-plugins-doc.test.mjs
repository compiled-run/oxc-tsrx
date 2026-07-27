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
const rejectionSource = join(root, "crates/oxc_adapter/src/toolchain.rs");

const page = await readFile(pagePath, "utf8");
const transcripts = JSON.parse(await readFile(transcriptsPath, "utf8"));

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

// Oxlint switches to GitHub's annotation reporter (`##[warning]`, `::error`)
// when it detects Actions. These assertions are about the default human-readable
// format, not about which reporter CI picks, so the detection is turned off here
// and one expected output holds on a laptop and on a runner.
const LINT_ENVIRONMENT = { ...process.env };
delete LINT_ENVIRONMENT.GITHUB_ACTIONS;
delete LINT_ENVIRONMENT.CI;

function run(cwd, executable, args, environment = LINT_ENVIRONMENT) {
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

test("the page prints the native refusal exactly as the source writes it", async () => {
  const rust = await readFile(rejectionSource, "utf8");
  const quoted = rust.match(
    /"(JavaScript plugins are not supported by the native TSRX path yet:[^"]*)"/,
  );
  assert.ok(quoted, "the rejection message moved out of crates/oxc_adapter/src/toolchain.rs");
  const message = quoted[1];

  const wall = transcripts.demos?.["custom-plugins-tsrx-wall"];
  assert.ok(wall, "the .tsrx wall demo is missing");
  assert.ok(
    wall.transcript.some((entry) => entry.output.includes(message)),
    "the captured wall transcript no longer contains the native refusal",
  );

  const rendered = join(root, "docs/dist/integrations/custom-js-plugins.md");
  if (existsSync(rendered)) {
    assert.ok(
      (await readFile(rendered, "utf8")).includes(message),
      "the built page no longer shows the native refusal",
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
      assert.match(result.stdout, /src\/TaskList\.tsrx:4:3: warning eslint\(no-debugger\)/u);
      await cp(join(examples, ".oxlintrc.json"), join(project, ".oxlintrc.json"));
    });

    await t.test("oxlint runs the JavaScript plugin on the ordinary .tsx file", async () => {
      const result = await oxlint(project, ["src/TaskRow.tsx"]);
      assert.equal(result.code, 1, result.stderr);
      assert.match(
        result.stdout + result.stderr,
        /src\/TaskRow\.tsx:\d+:\d+: error tsrx-demo\(require-keyed-map\)/u,
      );
    });

    await t.test("oxlint refuses the same plugin on .tsrx with exit 2", async () => {
      const rust = await readFile(rejectionSource, "utf8");
      const message = rust.match(
        /"(JavaScript plugins are not supported by the native TSRX path yet:[^"]*)"/,
      )[1];
      const result = await oxlint(project, ["src/TaskList.tsrx"]);
      assert.equal(result.code, 2, result.stdout);
      assert.ok(
        result.stderr.includes(message),
        `expected the native refusal on stderr, got:\n${result.stderr}`,
      );
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
