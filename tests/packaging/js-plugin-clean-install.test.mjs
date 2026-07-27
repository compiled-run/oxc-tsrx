// The JavaScript plugin lane, proved from packed tarballs installed into an
// empty project.
//
// `tests/plugins/tsrx-js-plugins.test.mjs` covers the lane from this checkout,
// where the native binary comes from `target/release` and the wrapper is read
// out of `packages/toolchain`. That proves the behaviour and proves nothing
// about the published product: the lane spans `bin/oxlint`,
// `dist/lint-cli.js`, `dist/lint-js-plugins.js`, the `oxlint-current`
// dependency, and two argument modes of the native executable, and any one of
// those falling out of a tarball would leave that suite green.
//
// So this file installs. `npm pack` produces the two tarballs a release
// publishes, a local registry serves them as ordinary releases, and `npm
// install` puts them in a directory that has never seen this repository. There
// is deliberately no symlink, no `file:` specifier, no workspace link, and no
// `OXC_TSRX_*` variable: each of those would let the consumer reach back into
// the checkout, and a lane that only works that way is not installed.

import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir, mkdtemp, readFile, realpath, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import test from "node:test";
import { LspClient, pathToFileUri } from "../editor/lsp-client.mjs";
import { parseNpmPackResponse } from "../../scripts/npm-pack-response.mjs";
import { startLocalRegistry } from "./local-registry.mjs";

const root = resolve(import.meta.dirname, "../..");
const npm = process.platform === "win32" ? "npm.cmd" : "npm";

/** The authored `.tsrx` file. Every asserted position below is a position in this string. */
const WIDGET_TSRX = `export function Widget() @{
  const banned = 1;
  debugger;

  <div>{banned}</div>;
}
`;

/** The ordinary file, so the same rule is proved on both halves of a mixed batch. */
const ORDINARY_TSX = `export function ordinary() {
  const banned = 2;
  return banned;
}
`;

/**
 * A plugin a developer could have written. Nothing in it knows what TSRX is: it
 * is the module shape that already runs on `.js`, `.ts`, `.jsx`, and `.tsx`.
 */
const PLUGIN = `const noBannedIdentifier = {
  meta: {
    type: "problem",
    docs: { description: "Ban the identifier \`banned\`" },
    messages: { notAllowed: "\`banned\` is not an allowed identifier." },
    schema: [],
  },
  create(context) {
    return {
      Identifier(node) {
        if (node.name !== "banned") return;
        context.report({ node, messageId: "notAllowed" });
      },
    };
  },
};

export default {
  meta: { name: "house-rules", version: "1.0.0" },
  rules: { "no-banned-identifier": noBannedIdentifier },
};
`;

function hostTarget() {
  if (process.platform === "darwin") {
    return `${process.arch === "arm64" ? "aarch64" : "x86_64"}-apple-darwin`;
  }
  if (process.platform === "win32") {
    return `${process.arch === "arm64" ? "aarch64" : "x86_64"}-pc-windows-msvc`;
  }
  if (process.platform === "linux" && ["arm64", "x64"].includes(process.arch)) {
    const architecture = process.arch === "arm64" ? "aarch64" : "x86_64";
    const libc = process.report?.getReport?.().header?.glibcVersionRuntime ? "gnu" : "musl";
    return `${architecture}-unknown-linux-${libc}`;
  }
  throw new Error(`unsupported clean-install host ${process.platform}-${process.arch}`);
}

function run(executable, args, options = {}) {
  return new Promise((resolveRun, rejectRun) => {
    const child = spawn(executable, args, {
      cwd: options.cwd,
      env: options.env ?? process.env,
      stdio: ["ignore", "pipe", "pipe"],
    });
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

async function mustRun(executable, args, options = {}) {
  const result = await run(executable, args, options);
  assert.equal(result.status, 0, result.stderr || result.stdout);
  return result;
}

async function pack(packageRoot, artifacts, cache) {
  const result = await mustRun(
    npm,
    ["pack", "--json", "--pack-destination", artifacts, resolve(root, packageRoot)],
    { cwd: root, env: { ...process.env, npm_config_cache: cache } },
  );
  const packed = parseNpmPackResponse(result.stdout);
  return {
    ...packed,
    manifest: JSON.parse(await readFile(join(root, packageRoot, "package.json"), "utf8")),
    tarball: join(artifacts, packed.filename),
  };
}

/**
 * The ambient environment with every route back into this checkout removed. The
 * assertions at the end of the test read this object again, so a future edit
 * that reintroduces one of them fails rather than passing quietly.
 */
function consumerEnvironment(consumer, registry) {
  const environment = {
    ...process.env,
    NO_COLOR: "1",
    npm_config_cache: join(consumer, ".npm-cache"),
    npm_config_registry: registry,
  };
  for (const key of Object.keys(environment)) {
    if (key === "NODE_PATH" || key.startsWith("OXC_TSRX_") || key.startsWith("OXLINT_TSGOLINT")) {
      delete environment[key];
    }
  }
  return environment;
}

/** One-based line and column of the nth occurrence of `token` in `source`. */
function authoredPosition(source, token, occurrence = 1) {
  let index = -1;
  for (let found = 0; found < occurrence; found += 1) {
    index = source.indexOf(token, index + 1);
    assert.notEqual(index, -1, `${token} #${occurrence} is not in the authored source`);
  }
  const before = source.slice(0, index);
  const line = before.split("\n").length;
  return { line, column: index - before.lastIndexOf("\n"), offset: index };
}

/**
 * The fully qualified rule code of one LSP diagnostic.
 *
 * The two lanes spell `code` differently on the wire: the native lane publishes
 * the bare rule name and keeps `eslint(no-debugger)` in `data.code`, while the
 * plugin lane publishes the qualified form in both. `data.code` is the field
 * both agree on, so it is what this file compares.
 */
function diagnosticCode(diagnostic) {
  return diagnostic.data?.code ?? diagnostic.code;
}

function labelOf(diagnostic) {
  assert.ok(Array.isArray(diagnostic.labels) && diagnostic.labels.length > 0, diagnostic.message);
  return diagnostic.labels[0].span;
}

function forFile(diagnostics, suffix) {
  return diagnostics.filter((diagnostic) => (diagnostic.filename ?? "").replaceAll("\\", "/").endsWith(suffix));
}

test(
  "an installed oxc-tsrx runs a project's own Oxlint plugin on .tsrx, in the CLI and the editor",
  { timeout: 1_800_000 },
  async (context) => {
    const temporary = await mkdtemp(join(tmpdir(), "oxc-tsrx-js-plugin-install-"));
    const artifacts = join(temporary, "artifacts");
    await mkdir(artifacts, { recursive: true });
    const cache = join(artifacts, ".npm-cache");
    let registry;
    context.after(async () => {
      await registry?.close();
      await rm(temporary, { recursive: true, force: true });
    });

    // Exactly the two tarballs a release publishes for this host: the platform
    // package built from `target/release`, and the public toolchain.
    const nativeResult = await mustRun(
      process.execPath,
      [
        "scripts/package-native.mjs",
        "--allow-missing-parser-addon",
        "--target",
        hostTarget(),
        "--bin-dir",
        "target/release",
        "--out-dir",
        artifacts,
      ],
      { cwd: root, env: { ...process.env, npm_config_cache: cache } },
    );
    const native = JSON.parse(nativeResult.stdout);
    const toolchain = await pack("packages/toolchain", artifacts, cache);
    registry = await startLocalRegistry([
      toolchain,
      {
        manifest: { name: native.packageName, version: native.version },
        tarball: native.tarball,
        integrity: native.integrity,
        shasum: native.shasum,
      },
    ]);

    const consumer = join(temporary, "consumer");
    await mkdir(join(consumer, "src"), { recursive: true });
    await writeFile(
      join(consumer, "package.json"),
      `${JSON.stringify(
        {
          name: "house",
          private: true,
          type: "module",
          // A registry range, not a path. `npm install` has to go to the
          // registry for this, which is the whole point of the file.
          dependencies: { "oxc-tsrx": toolchain.manifest.version },
        },
        null,
        2,
      )}\n`,
    );
    const environment = consumerEnvironment(consumer, registry.url);
    await mustRun(npm, ["install", "--ignore-scripts", "--no-audit", "--no-fund"], {
      cwd: consumer,
      env: environment,
    });

    await writeFile(join(consumer, "house-rules.mjs"), PLUGIN);
    await writeFile(
      join(consumer, ".oxlintrc.json"),
      `${JSON.stringify(
        {
          jsPlugins: ["./house-rules.mjs"],
          rules: { "house-rules/no-banned-identifier": "error", "no-debugger": "warn" },
        },
        null,
        2,
      )}\n`,
    );
    await writeFile(join(consumer, "src/Widget.tsrx"), WIDGET_TSRX);
    await writeFile(join(consumer, "src/Ordinary.tsx"), ORDINARY_TSX);

    // Nothing links back here. `oxc-tsrx` and its platform package are both
    // real directories under the consumer, reached through the registry.
    const consumerRoot = await realpath(consumer);
    const toolchainRoot = join(consumer, "node_modules/oxc-tsrx");
    assert.equal((await realpath(toolchainRoot)).startsWith(consumerRoot), true);
    assert.equal(
      (await realpath(join(consumer, "node_modules", native.packageName))).startsWith(consumerRoot),
      true,
    );

    // The command a project actually gets. It is run through Node by its real
    // path rather than through `node_modules/.bin`, because the shim there is
    // a Windows batch file that cannot be spawned directly.
    const oxlint = join(toolchainRoot, "bin/oxlint");

    const expectedTsrxRule = authoredPosition(WIDGET_TSRX, "banned");
    const expectedTsrxDebugger = authoredPosition(WIDGET_TSRX, "debugger");
    const expectedTsxRule = authoredPosition(ORDINARY_TSX, "banned");

    await context.test("the CLI reports the project's own rule on .tsrx", async () => {
      const lint = await run(process.execPath, [oxlint, "--format=json", "src"], {
        cwd: consumer,
        env: environment,
      });
      assert.equal(lint.status, 1, lint.stderr || lint.stdout);
      const report = JSON.parse(lint.stdout);

      // The extra parse is disclosed, both on stderr and as data.
      assert.match(lint.stderr, /running JS plugins on 1 \.tsrx file/u);
      assert.deepEqual(report.oxcTsrx.jsPluginProjection, { files: 1, extraParses: 1 });

      const widget = forFile(report.diagnostics, "src/Widget.tsrx");
      const rule = widget.filter((diagnostic) => diagnostic.code === "house-rules(no-banned-identifier)");
      assert.equal(rule.length, 2, JSON.stringify(widget, null, 2));

      // The authored position, not the projection's. `banned` is six bytes at
      // line 2 column 9 of the file the developer wrote.
      const declaration = labelOf(rule[0]);
      assert.equal(declaration.line, expectedTsrxRule.line);
      assert.equal(declaration.column, expectedTsrxRule.column);
      assert.equal(declaration.offset, expectedTsrxRule.offset);
      assert.equal(declaration.length, "banned".length);
      assert.equal(
        WIDGET_TSRX.slice(declaration.offset, declaration.offset + declaration.length),
        "banned",
      );

      // Both occurrences, including the one inside the JSX the projection
      // rewrote most heavily.
      const usage = labelOf(rule[1]);
      const expectedUsage = authoredPosition(WIDGET_TSRX, "banned", 2);
      assert.equal(usage.line, expectedUsage.line);
      assert.equal(usage.column, expectedUsage.column);
      assert.equal(
        WIDGET_TSRX.slice(usage.offset, usage.offset + usage.length),
        "banned",
      );

      // A native Rust rule still reports on the same file, alongside the
      // plugin's. Losing these was the shape of the editor regression.
      const debuggerDiagnostics = widget.filter(
        (diagnostic) => diagnostic.code === "eslint(no-debugger)",
      );
      assert.equal(debuggerDiagnostics.length, 1, JSON.stringify(widget, null, 2));
      const debuggerLabel = labelOf(debuggerDiagnostics[0]);
      assert.equal(debuggerLabel.line, expectedTsrxDebugger.line);
      assert.equal(debuggerLabel.column, expectedTsrxDebugger.column);

      // The same rule on an ordinary file, which is the half that already
      // worked and must keep working.
      const ordinary = forFile(report.diagnostics, "src/Ordinary.tsx").filter(
        (diagnostic) => diagnostic.code === "house-rules(no-banned-identifier)",
      );
      assert.equal(ordinary.length, 2, JSON.stringify(ordinary, null, 2));
      const ordinaryLabel = labelOf(ordinary[0]);
      assert.equal(ordinaryLabel.line, expectedTsxRule.line);
      assert.equal(ordinaryLabel.column, expectedTsxRule.column);
    });

    await context.test("the language server publishes both on the same .tsrx file", async () => {
      const client = new LspClient(process.execPath, {
        args: [oxlint, "--lsp"],
        cwd: consumer,
        env: environment,
      });
      const uri = pathToFileUri(join(consumerRoot, "src/Widget.tsrx"));
      try {
        await client.initialize(pathToFileUri(consumerRoot));
        client.notify("textDocument/didOpen", {
          textDocument: { uri, languageId: "ripple", version: 1, text: WIDGET_TSRX },
        });
        // The plugin lane runs after the native one, so the first publish may
        // carry only native diagnostics. Waiting for the developer's own rule
        // is what this test is here to observe; waiting for any publish at all
        // would pass on the regression it exists to catch.
        const published = await client.waitFor(
          (message) =>
            message.method === "textDocument/publishDiagnostics" &&
            message.params.uri === uri &&
            message.params.diagnostics.some(
              (diagnostic) => diagnosticCode(diagnostic) === "house-rules(no-banned-identifier)",
            ),
          60_000,
          "TSRX plugin diagnostics",
        );
        const diagnostics = published.params.diagnostics;
        const withCode = (code) =>
          diagnostics.filter((diagnostic) => diagnosticCode(diagnostic) === code);
        const shown = JSON.stringify(diagnostics, null, 2);

        // The developer's own rule, as a squiggle, on both occurrences. LSP
        // positions are zero-based; the CLI's are one-based.
        const rules = withCode("house-rules(no-banned-identifier)");
        assert.equal(rules.length, 2, shown);
        assert.deepEqual(
          rules.map((diagnostic) => diagnostic.range.start),
          [
            { line: expectedTsrxRule.line - 1, character: expectedTsrxRule.column - 1 },
            {
              line: authoredPosition(WIDGET_TSRX, "banned", 2).line - 1,
              character: authoredPosition(WIDGET_TSRX, "banned", 2).column - 1,
            },
          ],
          shown,
        );

        // And the native rule beside it. A `jsPlugins` config used to remove
        // every diagnostic from this file, this one included.
        const natives = withCode("eslint(no-debugger)");
        assert.equal(natives.length, 1, shown);
        assert.deepEqual(natives[0].range.start, {
          line: expectedTsrxDebugger.line - 1,
          character: expectedTsrxDebugger.column - 1,
        });
      } finally {
        client.terminate();
      }
    });

    // Restated as assertions rather than left to the reader: the lane above ran
    // with no override pointing at this checkout.
    assert.equal(environment.NODE_PATH, undefined);
    assert.equal(
      Object.keys(environment).some((key) => key.startsWith("OXC_TSRX_")),
      false,
    );
    assert.equal(
      Object.keys(environment).some((key) => key.startsWith("OXLINT_TSGOLINT")),
      false,
    );
    const installedManifest = JSON.parse(
      await readFile(join(consumer, "package.json"), "utf8"),
    );
    for (const specifier of Object.values(installedManifest.dependencies)) {
      assert.doesNotMatch(specifier, /^(?:file:|link:|workspace:|portal:)/u);
    }
  },
);
