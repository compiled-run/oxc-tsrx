#!/usr/bin/env node

// Reproduces the principal T001 same-machine parser/formatter measurements.
// It is intentionally read-only. Install disposable OXC controls first:
// npm install --prefix /tmp/oxc-tsrx-baseline --no-audit --no-fund --ignore-scripts \
//   oxc-parser@0.140.0 oxlint@1.74.0 oxfmt@0.59.0

import { execFileSync } from "node:child_process";
import { createRequire } from "node:module";
import { performance } from "node:perf_hooks";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

const roots = {
  markless:
    process.env.MARKLESS_ROOT ?? "/Users/jacksm5pro/dev/open-source/markless",
  ripple: process.env.RIPPLE_ROOT ?? "/Users/jacksm5pro/dev/open-source/ripple",
  yuku: process.env.YUKU_ROOT ?? "/Users/jacksm5pro/dev/open-source/yuku",
  oxcBaseline: process.env.OXC_BASELINE_ROOT ?? "/tmp/oxc-tsrx-baseline",
};

const require = createRequire(import.meta.url);
const importFile = (path) => import(pathToFileURL(path).href);
const [{ parseModule }, { decode }, { parseSync }, { format: oxfmt }, prettier, tsrxPlugin] =
  await Promise.all([
    importFile(join(roots.ripple, "packages/tsrx/src/index.js")),
    importFile(join(roots.yuku, "npm/yuku-parser/decode.js")),
    importFile(join(roots.oxcBaseline, "node_modules/oxc-parser/src-js/index.js")),
    importFile(join(roots.oxcBaseline, "node_modules/oxfmt/dist/index.js")),
    importFile(join(roots.ripple, "node_modules/prettier/index.mjs")),
    importFile(join(roots.ripple, "packages/prettier-plugin/src/index.js")),
  ]);
const yuku = require(join(roots.yuku, "zig-out/lib/yuku-parser.node"));

function files(glob) {
  const output = execFileSync("rg", ["--files", "-g", glob, roots.markless], {
    encoding: "utf8",
  }).trim();
  return output ? output.split("\n") : [];
}

function load(paths) {
  return paths.map((path) => ({ path, source: readFileSync(path, "utf8") }));
}

function bytes(docs) {
  return docs.reduce((total, doc) => total + Buffer.byteLength(doc.source), 0);
}

function quantiles(times) {
  times.sort((a, b) => a - b);
  return {
    medianMs: times[Math.floor(times.length / 2)],
    p95Ms: times[Math.min(times.length - 1, Math.floor(times.length * 0.95))],
    p99Ms: times[Math.min(times.length - 1, Math.floor(times.length * 0.99))],
  };
}

function withRate(result, byteCount) {
  return {
    ...result,
    medianMiBs: byteCount / 1024 / 1024 / (result.medianMs / 1000),
    p95MiBs: byteCount / 1024 / 1024 / (result.p95Ms / 1000),
  };
}

function measureSync(fn, { warmups, samples, byteCount }) {
  for (let index = 0; index < warmups; index++) fn();
  const times = [];
  for (let index = 0; index < samples; index++) {
    if (global.gc) global.gc();
    const start = performance.now();
    fn();
    times.push(performance.now() - start);
  }
  return withRate(quantiles(times), byteCount);
}

async function measureAsync(fn, { warmups, samples, byteCount }) {
  for (let index = 0; index < warmups; index++) await fn();
  const times = [];
  for (let index = 0; index < samples; index++) {
    if (global.gc) global.gc();
    const start = performance.now();
    await fn();
    times.push(performance.now() - start);
  }
  return withRate(quantiles(times), byteCount);
}

const allTsrx = load(files("*.tsrx"));
const validTsrx = [];
for (const doc of allTsrx) {
  try {
    parseModule(doc.source, doc.path);
    const parsed = decode(
      yuku.parse(doc.source, { lang: "tsrx", sourceType: "module" }),
      doc.source,
    );
    if (parsed.diagnostics.length === 0) validTsrx.push(doc);
  } catch {
    // Invalid and recovery fixtures are intentionally excluded from throughput.
  }
}

const allTs = load(files("*.ts"));
const validTs = [];
for (const doc of allTs) {
  const parsed = decode(
    yuku.parse(doc.source, { lang: "ts", sourceType: "module" }),
    doc.source,
  );
  if (parsed.diagnostics.length === 0) validTs.push(doc);
}

let sink = 0;
const tsrxByteCount = bytes(validTsrx);
const tsByteCount = bytes(validTs);
const parserPolicy = { warmups: 8, samples: 30, byteCount: tsrxByteCount };
const tsPolicy = { warmups: 3, samples: 12, byteCount: tsByteCount };

const result = {
  roots,
  corpus: {
    tsrxAll: { files: allTsrx.length, bytes: bytes(allTsrx) },
    tsrxValidIntersection: { files: validTsrx.length, bytes: tsrxByteCount },
    tsValidIntersection: { files: validTs.length, bytes: tsByteCount },
  },
  parser: {
    acornTsrx: measureSync(() => {
      for (const doc of validTsrx) sink += parseModule(doc.source, doc.path).body.length;
    }, parserPolicy),
    yukuTsrxPacked: measureSync(() => {
      for (const doc of validTsrx) {
        sink += yuku.parse(doc.source, { lang: "tsrx", sourceType: "module" }).byteLength;
      }
    }, parserPolicy),
    yukuTsrxDecoded: measureSync(() => {
      for (const doc of validTsrx) {
        sink += decode(
          yuku.parse(doc.source, { lang: "tsrx", sourceType: "module" }),
          doc.source,
        ).program.body.length;
      }
    }, parserPolicy),
    oxcTsRawTransfer: measureSync(() => {
      for (const doc of validTs) {
        const parsed = parseSync(doc.path, doc.source, {
          lang: "ts",
          experimentalRawTransfer: true,
        });
        sink += parsed.program.body.length + parsed.errors.length;
      }
    }, tsPolicy),
    yukuTsDecoded: measureSync(() => {
      for (const doc of validTs) {
        const parsed = decode(
          yuku.parse(doc.source, { lang: "ts", sourceType: "module" }),
          doc.source,
        );
        sink += parsed.program.body.length + parsed.diagnostics.length;
      }
    }, tsPolicy),
  },
};

const prettierOptions = {
  parser: "tsrx",
  plugins: [tsrxPlugin],
  useTabs: true,
  tabWidth: 4,
  printWidth: 100,
};
const formattableTsrx = [];
const prettierFailures = [];
let idempotentFiles = 0;
for (const doc of validTsrx) {
  try {
    const once = await prettier.default.format(doc.source, {
      ...prettierOptions,
      filepath: doc.path,
    });
    const twice = await prettier.default.format(once, {
      ...prettierOptions,
      filepath: doc.path,
    });
    formattableTsrx.push(doc);
    if (once === twice) idempotentFiles++;
  } catch (error) {
    prettierFailures.push({ path: doc.path, message: error.message });
  }
}

result.formatter = {
  prettierTsrx: {
    files: formattableTsrx.length,
    bytes: bytes(formattableTsrx),
    idempotentFiles,
    failures: prettierFailures,
    ...(await measureAsync(
      async () => {
        for (const doc of formattableTsrx) {
          sink += (
            await prettier.default.format(doc.source, {
              ...prettierOptions,
              filepath: doc.path,
            })
          ).length;
        }
      },
      { warmups: 1, samples: 7, byteCount: bytes(formattableTsrx) },
    )),
  },
  oxfmtTsSequential: await measureAsync(
    async () => {
      for (const doc of allTs) {
        const formatted = await oxfmt(doc.path, doc.source, {
          useTabs: true,
          tabWidth: 4,
          printWidth: 100,
        });
        sink += formatted.code.length + formatted.errors.length;
      }
    },
    { warmups: 1, samples: 5, byteCount: bytes(allTs) },
  ),
};

result.sink = sink;
console.log(JSON.stringify(result, null, 2));
