import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import test from "node:test";
import { pathToFileURL } from "node:url";

const root = resolve(import.meta.dirname, "../..");

// The pinned host-qualified parser accepted this exact shape at 12,288 owners;
// parsing itself became the limiting stage above that. The lower fixed depth
// leaves margin and isolates materialization instead of claiming unlimited depth.
const DEEP_JSX_DEPTH = 8_192;
const MATERIALIZER_STACK_MIB = 1;

const workerSource = String.raw`
  import { parentPort, workerData } from "node:worker_threads";

  const { parse } = await import(workerData.parserUrl);
  const source = "function View() @{ "
    + "<A>".repeat(workerData.depth)
    + "<Leaf/>"
    + "</A>".repeat(workerData.depth)
    + " }";

  // Async parsing keeps pinned OXC parsing on its task thread. Accessing
  // program then exercises lazy native materialization on this worker's
  // deliberately bounded stack.
  const result = await parse("Deep.tsrx", source);
  const program = result.program;
  const pending = [[program, 0]];
  const seen = new Set();
  let jsxElements = 0;
  let maxJsxNesting = 0;

  while (pending.length > 0) {
    const [value, parentDepth] = pending.pop();
    if (value === null || typeof value !== "object" || seen.has(value)) continue;
    seen.add(value);
    const currentDepth = value.type === "JSXElement" ? parentDepth + 1 : parentDepth;
    if (value.type === "JSXElement") {
      jsxElements += 1;
      maxJsxNesting = Math.max(maxJsxNesting, currentDepth);
    }
    const children = Array.isArray(value) ? value : Object.values(value);
    for (const child of children) pending.push([child, currentDepth]);
  }

  parentPort.postMessage({
    errors: result.errors.length,
    programType: program.type,
    cachedProgram: result.program === program,
    jsxElements,
    maxJsxNesting,
    stackSizeMb: workerData.stackSizeMb,
  });
`;

function run(executable, args, options = {}) {
  return new Promise((resolveRun, rejectRun) => {
    execFile(
      executable,
      args,
      {
        cwd: options.cwd ?? root,
        env: options.env ?? process.env,
        maxBuffer: 16 * 1024 * 1024,
      },
      (error, stdout, stderr) => {
        if (error) rejectRun(new Error(stderr || stdout, { cause: error }));
        else resolveRun({ stdout, stderr });
      },
    );
  });
}

test("deep TSRX materializes iteratively on a bounded worker stack", async () => {
  const temporary = await mkdtemp(join(tmpdir(), "oxc-tsrx-materializer-stack-"));
  try {
    const addon = join(temporary, "parser.node");
    await run(process.execPath, [
      "scripts/build-parser-native.mjs",
      "--skip-build",
      "--out",
      addon,
    ]);

    const workerPath = join(temporary, "materialize-worker.mjs");
    await writeFile(workerPath, workerSource, "utf8");

    const childSource = String.raw`
      import { Worker } from "node:worker_threads";

      const workerUrl = new URL(process.argv[1]);
      const parserUrl = process.argv[2];
      const depth = Number(process.argv[3]);
      const stackSizeMb = Number(process.argv[4]);

      const worker = new Worker(workerUrl, {
        execArgv: [],
        workerData: { parserUrl, depth, stackSizeMb },
        resourceLimits: { stackSizeMb },
      });
      const observation = await new Promise((resolveObservation, rejectObservation) => {
        let message;
        worker.once("message", (value) => {
          message = value;
        });
        worker.once("error", rejectObservation);
        worker.once("exit", (code) => {
          if (code !== 0) {
            rejectObservation(new Error("deep materializer worker exited " + code));
          } else if (message === undefined) {
            rejectObservation(new Error("deep materializer worker exited without a result"));
          } else {
            resolveObservation(message);
          }
        });
      });
      process.stdout.write(JSON.stringify(observation));
    `;

    const environment = {
      ...process.env,
      OXC_TSRX_PARSER_ADDON: addon,
    };
    const { stdout, stderr } = await run(
      process.execPath,
      [
        "--input-type=module",
        "-e",
        childSource,
        pathToFileURL(workerPath).href,
        new URL("../../packages/parser/index.js", import.meta.url).href,
        String(DEEP_JSX_DEPTH),
        String(MATERIALIZER_STACK_MIB),
      ],
      { env: environment },
    );

    assert.equal(stderr, "");
    assert.deepEqual(JSON.parse(stdout), {
      errors: 0,
      programType: "Program",
      cachedProgram: true,
      jsxElements: DEEP_JSX_DEPTH + 1,
      maxJsxNesting: DEEP_JSX_DEPTH + 1,
      stackSizeMb: MATERIALIZER_STACK_MIB,
    });
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
});
