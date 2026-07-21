import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { createRequire } from "node:module";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import test from "node:test";

import { parseTsrxProgram } from "../../packages/parser/tsrx-transfer.js";

const root = resolve(import.meta.dirname, "../..");
const require = createRequire(import.meta.url);

function run(executable, args) {
  return new Promise((resolveRun, rejectRun) => {
    execFile(
      executable,
      args,
      { cwd: root, env: process.env, maxBuffer: 16 * 1024 * 1024 },
      (error, stdout, stderr) => {
        if (error) rejectRun(new Error(stderr || stdout, { cause: error }));
        else resolveRun({ stdout, stderr });
      },
    );
  });
}

test("TSRX crosses Node-API as one versioned Program payload", async () => {
  const temporary = await mkdtemp(join(tmpdir(), "oxc-tsrx-bulk-transfer-"));
  const previous = process.env.OXC_TSRX_PARSER_ADDON;
  try {
    const addon = join(temporary, "parser.node");
    await run(process.execPath, [
      "scripts/build-parser-native.mjs",
      "--skip-build",
      "--out",
      addon,
    ]);

    const native = require(addon);
    const source = String.raw`
      const integer = 9007199254740993n;
      const matcher = /bulk\/transfer/gi;
      function View() @{ <main>{integer}</main> }
    `;
    const nativeResult = native.parseSync("Bulk.tsrx", "", source, {}, 6);
    const payload = nativeResult.program;

    assert.equal(typeof payload, "string");
    const envelope = JSON.parse(payload);
    assert.equal(envelope.version, 1);
    assert.equal(envelope.node.type, "Program");
    assert.ok(Array.isArray(envelope.fixes));
    assert.ok(envelope.fixes.length >= 2);

    const eagerResult = native.parseSync("Bulk.tsrx", "", source, {}, 7);
    assert.equal(typeof eagerResult.metadata, "string");
    assert.ok(eagerResult.words instanceof Uint32Array);
    assert.equal(parseTsrxProgram(eagerResult).type, "Program");

    process.env.OXC_TSRX_PARSER_ADDON = addon;
    const parser = await import(`../../packages/parser/index.js?bulk=${Date.now()}`);
    const result = parser.parseSync("Bulk.tsrx", source);
    const program = result.program;
    assert.equal(program.type, "Program");
    assert.equal(result.program, program);
    assert.equal(program.body[0].declarations[0].init.value, 9007199254740993n);
    assert.deepEqual(program.body[1].declarations[0].init.value, /bulk\/transfer/gi);

    const eagerOptions = Object.defineProperty(
      { lang: "tsrx", sourceType: "module", astType: "js", preserveParens: false },
      Symbol.for("@oxc-tsrx/parser/tsrx-core-compat-eager"),
      { value: true },
    );
    const eagerProgram = parser.parseSync("Bulk.tsrx", source, eagerOptions);
    assert.equal(eagerProgram.type, "Program");
    assert.equal(Object.hasOwn(eagerProgram, "program"), false);
  } finally {
    if (previous === undefined) delete process.env.OXC_TSRX_PARSER_ADDON;
    else process.env.OXC_TSRX_PARSER_ADDON = previous;
    await rm(temporary, { recursive: true, force: true });
  }
});

test("private Program graph decoder rejects malformed envelopes", () => {
  const metadata = '[["type"],["Program"],[]]';
  const words = new Uint32Array([
    0x42525354,
    1,
    1,
    1,
    0,
    0,
    1,
    0,
    1,
    1,
    0,
    0,
    0,
    1,
    0,
    0,
  ]);
  assert.deepEqual(parseTsrxProgram({ metadata, words }), { type: "Program" });

  const malformed = [
    { metadata, words: words.slice(0, -1) },
    { metadata, words: Uint32Array.from(words, (word, index) => (index === 0 ? 0 : word)) },
    {
      metadata,
      words: Uint32Array.from(words, (word, index) => (index === 15 ? 9 : word)),
    },
    {
      metadata: '[["self"],[],[]]',
      words: new Uint32Array([
        0x42525354,
        1,
        1,
        1,
        0,
        0,
        1,
        0,
        1,
        0,
        0,
        0,
        0,
        1,
        0,
        0x40000000,
      ]),
    },
    {
      metadata: "[[],[],[]]",
      words: new Uint32Array([
        0x42525354,
        1,
        2,
        0,
        0,
        0,
        1,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
      ]),
    },
  ];
  for (const payload of malformed) {
    assert.throws(() => parseTsrxProgram(payload), TypeError);
  }
});
