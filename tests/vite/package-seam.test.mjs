import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import test from "node:test";
import { pathToFileURL } from "node:url";

const root = resolve(import.meta.dirname, "../..");
const lintBin = process.env.OXC_TSRX_LINT_BIN ?? join(root, "target/release/oxc-tsrx");
const formatBin = process.env.OXC_TSRX_FORMAT_BIN ?? join(root, "target/release/oxc-tsrx");

function runProcess(executable, args, options = {}) {
  return new Promise((resolveRun, rejectRun) => {
    const child = spawn(executable, args, {
      cwd: options.cwd,
      env: options.env,
      stdio: [options.input === undefined ? "ignore" : "pipe", "pipe", "pipe"],
    });
    let stdout = "";
    let stderr = "";
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => (stdout += chunk));
    child.stderr.on("data", (chunk) => (stderr += chunk));
    child.on("error", rejectRun);
    child.on("close", (status) => resolveRun({ status, stdout, stderr }));
    if (options.input !== undefined) child.stdin.end(options.input);
  });
}

function run(command, args, options = {}) {
  return runProcess(process.execPath, [command, ...args], options);
}

function lspInitializeRoundTrip(binPath, { timeoutMs = 15_000 } = {}) {
  return new Promise((resolveSession, rejectSession) => {
    const child = spawn(process.execPath, [binPath, "--lsp"], {
      cwd: root,
      env: { ...process.env, OXC_TSRX_LINT_BIN: lintBin, OXC_TSRX_FORMAT_BIN: formatBin },
      stdio: ["pipe", "pipe", "pipe"],
    });
    let stdout = Buffer.alloc(0);
    let settled = false;
    const finish = (error, value) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      const deliver = () => (error ? rejectSession(error) : resolveSession(value));
      if (child.exitCode === null && child.signalCode === null) {
        child.once("close", deliver);
        child.kill();
      } else {
        deliver();
      }
    };
    const timer = setTimeout(() => {
      finish(
        new Error(
          `no LSP initialize response within ${timeoutMs}ms; stdout so far: ${JSON.stringify(stdout.toString("utf8").slice(0, 200))}`,
        ),
      );
    }, timeoutMs);
    child.on("error", (error) => finish(error));
    child.on("close", (status, signal) => {
      finish(new Error(`LSP child exited before responding (status ${status}, signal ${signal})`));
    });
    child.stdout.on("data", (chunk) => {
      stdout = Buffer.concat([stdout, chunk]);
      const header = stdout.toString("utf8").match(/Content-Length: (\d+)\r\n\r\n/);
      if (!header) return;
      const bodyStart = header.index + header[0].length;
      const bodyLength = Number(header[1]);
      if (stdout.length < bodyStart + bodyLength) return;
      try {
        finish(null, JSON.parse(stdout.subarray(bodyStart, bodyStart + bodyLength).toString("utf8")));
      } catch (error) {
        finish(error);
      }
    });
    const message = JSON.stringify({
      jsonrpc: "2.0",
      id: 1,
      method: "initialize",
      params: { processId: null, rootUri: null, capabilities: {} },
    });
    child.stdin.write(`Content-Length: ${Buffer.byteLength(message)}\r\n\r\n${message}`);
  });
}

test("oxlint wrapper keeps a canonical --lsp stdio session alive", async () => {
  const response = await lspInitializeRoundTrip(join(root, "packages/toolchain/bin/oxlint"));
  assert.equal(response.id, 1);
  assert.ok(response.result?.capabilities, `initialize response missing capabilities: ${JSON.stringify(response)}`);
});

test("oxfmt wrapper keeps a canonical --lsp stdio session alive", async () => {
  const response = await lspInitializeRoundTrip(join(root, "packages/toolchain/bin/oxfmt"));
  assert.equal(response.id, 1);
  assert.ok(response.result?.capabilities, `initialize response missing capabilities: ${JSON.stringify(response)}`);
});

test("oxfmt executable forwards TSRX stdin to the native formatter", async () => {
  const source = 'export function View( ) @{<div title="proof">TSRX</div>}';
  const args = ["--stdin-filepath=View.tsrx"];
  const environment = { ...process.env, OXC_TSRX_FORMAT_BIN: formatBin };
  const [actual, expected] = await Promise.all([
    run(join(root, "packages/toolchain/bin/oxfmt"), args, {
      cwd: root,
      env: environment,
      input: source,
    }),
    // The formatter is the `fmt` tool inside the one multi-call native binary,
    // which is exactly what the wrapper is expected to select on its own.
    runProcess(formatBin, ["fmt", ...args], { cwd: root, env: environment, input: source }),
  ]);
  assert.deepEqual(actual, expected);
  assert.notEqual(actual.stdout, "", "formatter output must not be empty");
});

test("drop-in package roots preserve canonical config APIs and add TSRX formatting", async () => {
  process.env.OXC_TSRX_LINT_BIN = lintBin;
  process.env.OXC_TSRX_FORMAT_BIN = formatBin;

  const oxlint = await import(pathToFileURL(join(root, "packages/toolchain/dist/lint.js")).href);
  const oxfmt = await import(pathToFileURL(join(root, "packages/toolchain/dist/format.js")).href);
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
    const moduleUrl = pathToFileURL(join(root, "packages/toolchain/dist/format.js"));
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

// Vite+ resolves the *package* `oxlint` and then derives `<pkgRoot>/bin/oxlint`
// from its manifest, so whichever package answers to those command names has to
// carry a resolvable root and a declared bin for each. That used to be two
// packages, `oxlint-tsrx` and `oxfmt-tsrx`; both were folded into `oxc-tsrx`, so
// the shape is now asserted where it actually lives.
test("package manifests have Vite+ compatible root and bin shapes", async () => {
  const manifest = JSON.parse(
    await readFile(join(root, "packages/toolchain/package.json"), "utf8"),
  );
  assert.equal(manifest.name, "oxc-tsrx");
  assert.equal(manifest.type, "module");
  assert.equal(manifest.main, "./dist/index.js");
  for (const name of ["oxlint", "oxfmt"]) {
    assert.equal(manifest.bin[name], `./bin/${name}`);
  }
  assert.ok(manifest.exports["."]);
  // Vite+ reads the manifest through this subpath, so it has to stay exported.
  assert.ok(manifest.exports["./package.json"]);
});

test("mixed package lint delegates ordinary TSX and parses each TSRX file once", async () => {
  const fixture = join(root, "tests/fixtures/vite/toolchain/diagnostics");
  const traceDirectory = await mkdtemp(join(tmpdir(), "oxc-tsrx-public-route-"));
  const trace = join(traceDirectory, "trace.jsonl");
  try {
    const result = await run(
      join(root, "packages/toolchain/bin/oxlint"),
      [
        "--format=json",
        "--config",
        join(fixture, ".oxlintrc.json"),
        join(fixture, "src/ordinary.tsx"),
        join(fixture, "src/view.tsrx"),
      ],
      {
        cwd: root,
        env: {
          ...process.env,
          OXC_TSRX_LINT_BIN: lintBin,
          OXC_TSRX_TRACE_FILE: trace,
        },
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

    const starts = (await readFile(trace, "utf8"))
      .trim()
      .split("\n")
      .map((line) => JSON.parse(line))
      .filter((event) => event.event === "start");
    assert.ok(
      starts.some(
        (event) =>
          event.executable === process.execPath &&
          // npm installed the alias under its alias name; pnpm resolves the
          // same package through its virtual store, where the directory carries
          // the real published name. Either directory is the public
          // manifest-declared launcher.
          /node_modules\/(?:oxlint|oxlint-current)\/bin\/oxlint$/u.test(
            event.args[0]?.replaceAll("\\", "/") ?? "",
          ),
      ),
      "mixed ordinary files must use the public manifest-declared Oxlint launcher via Node",
    );
    assert.ok(
      starts.some((event) => resolve(event.executable) === resolve(lintBin)),
      "mixed TSRX files must use the native TSRX binary",
    );
    assert.ok(starts.every((event) => !event.executable.startsWith("in-process:")));
  } finally {
    await rm(traceDirectory, { recursive: true, force: true });
  }
});
