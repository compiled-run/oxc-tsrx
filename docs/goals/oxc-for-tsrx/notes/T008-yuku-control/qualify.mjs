import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { readFile, readdir } from 'node:fs/promises';
import { relative, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

import { parseModule } from '@tsrx/core';

import { formatTsrxText } from '../../../../../packages/formatter/src/index.js';

const root = resolve(new URL('../../../../..', import.meta.url).pathname);
const corpus = resolve(process.argv[2]);
const yuku = resolve(process.argv[3] ?? '/Users/jacksm5pro/dev/open-source/yuku');
const require = createRequire(import.meta.url);
const parserBinding = require(resolve(yuku, 'zig-out/lib/yuku-parser.node'));
const codegenBinding = require(resolve(yuku, 'zig-out/lib/yuku-codegen.node'));
const { decode } = await import(pathToFileURL(resolve(yuku, 'npm/yuku-parser/decode.js')));
const { encode } = await import(pathToFileURL(resolve(yuku, 'zig-out/encode.js')));

async function findTsrxFiles(directory) {
  const files = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.name.startsWith('.') || entry.name === 'node_modules') continue;
    const path = resolve(directory, entry.name);
    if (entry.isDirectory()) files.push(...(await findTsrxFiles(path)));
    else if (entry.isFile() && entry.name.endsWith('.tsrx')) files.push(path);
  }
  return files;
}

function yukuFormat(source) {
  const parsed = decode(
    parserBinding.parse(source, {
      lang: 'tsrx',
      sourceType: 'module',
      attachComments: true,
    }),
    source,
  );
  if (parsed.diagnostics.some((diagnostic) => diagnostic.severity === 'error')) {
    throw new Error(parsed.diagnostics[0].message);
  }
  const output = codegenBinding.print(encode(parsed.program), {
    format: 'pretty',
    indent: 2,
    quotes: 'preserve',
    comments: 'all',
  });
  if (output.errors.length > 0) throw new Error(output.errors[0].message);
  return { code: output.code, comments: parsed.comments };
}

function semanticValue(value, key = '') {
  if (
    [
      'start',
      'end',
      'loc',
      'raw',
      'hash',
      'metadata',
      'comments',
      'leadingComments',
      'trailingComments',
      'innerComments',
    ].includes(key)
  ) {
    return undefined;
  }
  if (typeof value === 'bigint') return `${value}n`;
  if (typeof value === 'string' && (key === 'css' || key === 'source')) {
    return value.replace(/\s+/g, '').replace(/;}/g, '}');
  }
  if (Array.isArray(value)) {
    return value.map((item) => semanticValue(item)).filter((item) => item !== undefined);
  }
  if (value && typeof value === 'object') {
    return Object.fromEntries(
      Object.entries(value)
        .map(([childKey, child]) => [childKey, semanticValue(child, childKey)])
        .filter(([, child]) => child !== undefined),
    );
  }
  if (key === 'value' && typeof value === 'string' && /^\s+$/.test(value)) return ' ';
  return value;
}

function normalizedComments(comments) {
  return comments.map(({ type, value }) => ({ type, value }));
}

const paths = (await findTsrxFiles(corpus)).sort();
const report = {
  corpus,
  yuku,
  files: paths.length,
  authoritativeValid: 0,
  yukuValid: 0,
  intersection: 0,
  semanticMismatches: [],
  outputParseFailures: [],
  commentMismatches: [],
  nonIdempotent: [],
  fallbackExactMatches: 0,
  filesWithLeadingTabs: 0,
  filesOverPrintWidth100: 0,
  leadingTabExamples: [],
  overWidthExamples: [],
  yukuFailures: [],
};

for (const path of paths) {
  const source = await readFile(path, 'utf8');
  let before;
  try {
    before = parseModule(source, path);
    report.authoritativeValid += 1;
  } catch {
    continue;
  }

  let formatted;
  try {
    formatted = yukuFormat(source);
    report.yukuValid += 1;
  } catch (error) {
    report.yukuFailures.push({
      file: relative(corpus, path),
      reason: error instanceof Error ? error.message : String(error),
    });
    continue;
  }
  report.intersection += 1;

  let after;
  try {
    after = parseModule(formatted.code, path);
  } catch (error) {
    report.outputParseFailures.push({
      file: relative(corpus, path),
      reason: error instanceof Error ? error.message : String(error),
    });
    continue;
  }
  try {
    assert.deepEqual(semanticValue(after), semanticValue(before));
  } catch {
    report.semanticMismatches.push(relative(corpus, path));
  }

  const reparsed = yukuFormat(formatted.code);
  if (reparsed.code !== formatted.code) report.nonIdempotent.push(relative(corpus, path));
  if (
    JSON.stringify(normalizedComments(formatted.comments)) !==
    JSON.stringify(normalizedComments(reparsed.comments))
  ) {
    report.commentMismatches.push(relative(corpus, path));
  }

  const fallback = await formatTsrxText({ source, filename: path, cwd: root });
  if (fallback.ok && fallback.code === formatted.code) report.fallbackExactMatches += 1;
  if (/^\t+/m.test(formatted.code)) {
    report.filesWithLeadingTabs += 1;
    if (report.leadingTabExamples.length < 8) {
      report.leadingTabExamples.push(relative(corpus, path));
    }
  }
  const widest = formatted.code
    .split(/\r?\n/)
    .reduce((best, line) => (line.length > best.length ? line : best), '');
  if (widest.length > 100) {
    report.filesOverPrintWidth100 += 1;
    if (report.overWidthExamples.length < 8) {
      report.overWidthExamples.push({
        file: relative(corpus, path),
        width: widest.length,
        line: widest,
      });
    }
  }
}

console.log(JSON.stringify(report, null, 2));
