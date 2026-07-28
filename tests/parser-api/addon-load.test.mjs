import assert from "node:assert/strict";
import { readFile, stat } from "node:fs/promises";
import { resolve } from "node:path";
import test from "node:test";
import { nativeTargetForHost } from "../../packages/toolchain/dist/native-targets.js";

// Every other suite in this directory builds its own copy of the addon from
// `target/release` and then points the loader at that copy. That proves the
// host can build and load an addon, which is why it only ever ran where the
// build already happened: on linux-x64-gnu. The release matrix builds one
// `parser.node` per target and, until this file existed, seven of the eight
// were packaged without anything ever opening them. That is the exact class of
// defect that shipped 0.1.0 with `parser.node` missing from all eight packages.
//
// So this suite deliberately does the opposite: it builds nothing, and it
// honours `OXC_TSRX_PARSER_ADDON` instead of setting it. The caller names the
// addon it wants exercised, and this asserts that that exact file loads through
// the shipped loader and produces real parser output on this host.
//
// A missing override is a failure rather than a skip. If it silently fell back
// the loader would open whatever sits beside `packages/toolchain/dist`, and the
// lane would report success for an addon nobody asked about.
const requested = process.env.OXC_TSRX_PARSER_ADDON;

function linuxLibc() {
  if (process.platform !== "linux") return undefined;
  return process.report?.getReport?.().header?.glibcVersionRuntime ? "glibc" : "musl";
}

test("the addon named by OXC_TSRX_PARSER_ADDON loads and parses on this host", async () => {
  assert.ok(
    requested,
    "OXC_TSRX_PARSER_ADDON must name the parser addon this lane is meant to exercise",
  );
  const addon = resolve(requested);
  const addonStat = await stat(addon).catch(() => null);
  assert.ok(addonStat?.isFile(), `parser addon is not a file: ${addon}`);

  // The loader reads its identity record from `<addon>.json` and refuses to
  // open a file whose bytes, SHA-256, or object identity disagree with it, so
  // reading the record here is only for the log line below.
  const record = JSON.parse(await readFile(`${addon}.json`, "utf8"));
  const host = nativeTargetForHost(process.platform, process.arch, linuxLibc());
  assert.ok(host, `no published native target for ${process.platform}-${process.arch}`);
  assert.equal(
    record.target,
    host.target,
    "the addon under test must be built for the host it is being loaded on",
  );
  console.log(
    `parser addon under test: ${addon} (${record.target}, ${record.bytes} bytes, sha256 ${record.sha256})`,
  );

  // A fresh module instance per run: the loader caches its binding for the
  // lifetime of the module, so a plain import could hand back a binding opened
  // from a different path by an earlier suite in the same process.
  const parser = await import(
    `../../packages/toolchain/dist/parser.js?addon-load=${Date.now()}`
  );

  const source = String.raw`
    const bytes = 9007199254740993n;
    const matcher = /addon\/load/gi;
    function View() @{ <main>{bytes}</main> }
  `;
  const result = parser.parseSync("AddonLoad.tsrx", source);
  const program = result.program;
  assert.equal(program.type, "Program");
  // Lazy identity: the second read must hand back the same materialized graph
  // rather than decoding the payload again.
  assert.equal(result.program, program);
  assert.equal(program.body[0].declarations[0].init.value, 9007199254740993n);
  assert.deepEqual(program.body[1].declarations[0].init.value, /addon\/load/gi);
  assert.deepEqual(result.errors, []);

  // Real diagnostics come out of the same addon, not out of a JavaScript
  // fallback. A parse that only ever succeeds cannot tell the two apart.
  const broken = parser.parseSync("AddonLoad.tsrx", "function View() @{ <main> }\n");
  assert.ok(Array.isArray(broken.errors));
  assert.ok(broken.errors.length > 0, "a malformed TSRX view must produce a diagnostic");
  console.log(`parser addon diagnostic: ${broken.errors[0].message}`);
});
