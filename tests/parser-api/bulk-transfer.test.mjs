import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { createRequire } from "node:module";
import { mkdtemp } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import test from "node:test";
import { runInNewContext } from "node:vm";

import {
  PROGRAM_BINARY_TRANSFER_MAGIC,
  PROGRAM_BINARY_TRANSFER_VERSION,
  parseTrustedTsrxProgram,
  parseTsrxProgram,
} from "../../packages/toolchain/dist/tsrx-transfer.js";
import { createTsrxCoreCompat } from "../../packages/tsrx-core-compat/dist/facade.js";
import { scriptNode } from "../helpers/script-node.mjs";
import { removeAddonFixture } from "./addon-fixture.mjs";

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
    await run(scriptNode(), [
      "scripts/build-parser-native.ts",
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
    const parser = await import(`../../packages/toolchain/dist/parser.js?bulk=${Date.now()}`);
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
    assert.equal(Object.hasOwn(eagerProgram, "hashbang"), false);
    assert.equal(Object.hasOwn(eagerProgram.body[0].declarations[0].id, "decorators"), false);
    assert.equal(
      eagerProgram[Symbol.for("@oxc-tsrx/parser/tsrx-core-compat-defaults-stripped")],
      true,
    );

    const paritySource = String.raw`
      "use strict";
      declare class Base<T> {}
      class Box<T> extends Base<T> { value?: T }
      function identity<T>(value: T): T { return value; }
      const result: string = identity<string>("value");
      const pending = import("./dependency");
      function View() @{ <main>{result}</main> }
    `;
    const facade = createTsrxCoreCompat(parser);
    const eagerCompatibilityProgram = facade.parseModule(paritySource, "Parity.tsrx");
    const comments = [];
    const lazyCompatibilityProgram = facade.parseModule(paritySource, "Parity.tsrx", {
      comments,
    });
    assert.deepEqual(lazyCompatibilityProgram, eagerCompatibilityProgram);
  } finally {
    if (previous === undefined) delete process.env.OXC_TSRX_PARSER_ADDON;
    else process.env.OXC_TSRX_PARSER_ADDON = previous;
    await removeAddonFixture(temporary);
  }
});

test("private Program graph decoder rejects malformed envelopes", () => {
  const metadata = '[["type"],["Program"],[]]';
  const words = new Uint32Array([
    PROGRAM_BINARY_TRANSFER_MAGIC,
    PROGRAM_BINARY_TRANSFER_VERSION,
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

  const foreignWords = runInNewContext(`new Uint32Array([${words.join(",")}])`);
  assert.deepEqual(parseTsrxProgram({ metadata, words: foreignWords }), {
    type: "Program",
  });
  assert.deepEqual(parseTrustedTsrxProgram({ metadata, words: foreignWords }), {
    type: "Program",
  });

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
        PROGRAM_BINARY_TRANSFER_MAGIC,
        PROGRAM_BINARY_TRANSFER_VERSION,
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
        PROGRAM_BINARY_TRANSFER_MAGIC,
        PROGRAM_BINARY_TRANSFER_VERSION,
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

test("trusted compatibility transfer omits reference-parser defaults before traversal", () => {
  const metadata = '[[],["Program",null],[]]';
  const words = new Uint32Array([
    PROGRAM_BINARY_TRANSFER_MAGIC,
    PROGRAM_BINARY_TRANSFER_VERSION,
    1,
    2,
    0,
    0,
    1,
    0,
    0,
    2,
    0,
    0,
    0,
    2,
    0x80000005,
    0,
    0x80000002,
    1,
  ]);
  const payload = { metadata, words };

  assert.deepEqual(parseTrustedTsrxProgram(payload), {
    type: "Program",
    hashbang: null,
  });

  const compatible = parseTrustedTsrxProgram(payload, true);
  assert.deepEqual(compatible, { type: "Program" });
  assert.equal(
    compatible[Symbol.for("@oxc-tsrx/parser/tsrx-core-compat-defaults-stripped")],
    true,
  );

  const reorderedPayload = {
    metadata: '[[],["Program",null,"Identifier",false],[]]',
    words: new Uint32Array([
      PROGRAM_BINARY_TRANSFER_MAGIC,
      PROGRAM_BINARY_TRANSFER_VERSION,
      2,
      5,
      1,
      1,
      1,
      0,
      0,
      4,
      0,
      0,
      0,
      3,
      3,
      2,
      0x80000002,
      1,
      0x80000005,
      0,
      0x80000000,
      0x80000000,
      0x8000001c,
      3,
      0x80000005,
      2,
      0,
      1,
      0x40000001,
    ]),
  };
  const reorderedCompatible = parseTrustedTsrxProgram(reorderedPayload, true);
  assert.deepEqual(reorderedCompatible, {
    type: "Program",
    body: [{ type: "Identifier" }],
  });
  assert.equal(
    reorderedCompatible[Symbol.for("@oxc-tsrx/parser/tsrx-core-compat-defaults-stripped")],
    true,
  );
});
