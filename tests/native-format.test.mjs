import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { createRequire } from 'node:module';
import { basename, dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '..');
// One multi-call native binary carries the linter, the formatter, and the
// language server. `fmt` selects the formatter.
const binary = process.env.OXFMT_BIN ?? join(root, 'target/release/oxc-tsrx');
// `oxfmt-current` is declared by tests/package.json, so it is resolved from
// this file rather than from a hoisted repository-root `node_modules`. pnpm
// installs it under tests/node_modules and nowhere else.
const stockBinary = join(
  dirname(createRequire(import.meta.url).resolve('oxfmt-current/package.json')),
  'bin/oxfmt',
);
const fixtures = join(root, 'tests/fixtures/format');

function run(executable, args, input = null) {
  return new Promise((resolvePromise, reject) => {
    const child = spawn(executable, args, {
      cwd: root,
      env: process.env,
      stdio: ['pipe', 'pipe', 'pipe'],
    });
    let stdout = '';
    let stderr = '';
    child.stdout.setEncoding('utf8');
    child.stderr.setEncoding('utf8');
    child.stdout.on('data', (chunk) => (stdout += chunk));
    child.stderr.on('data', (chunk) => (stderr += chunk));
    child.once('error', reject);
    child.once('close', (code, signal) => {
      resolvePromise({ code, signal, stdout, stderr });
    });
    child.stdin.end(input ?? undefined);
  });
}

function runFormat(args, input = null) {
  return run(binary, ['fmt', ...args], input);
}

async function fixture(name) {
  return readFile(join(fixtures, name), 'utf8');
}

test('formats a Markless-derived TSRX component from stdin and is idempotent', async () => {
  const source = await fixture('markless-counter.unformatted.tsrx');
  const expected = await fixture('markless-counter.formatted.tsrx');
  const first = await runFormat(['--stdin-filepath=Counter.tsrx'], source);

  assert.equal(first.signal, null);
  assert.equal(first.code, 0, first.stderr || first.stdout);
  assert.equal(first.stdout, expected);
  assert.equal(first.stderr, '');
  assert.match(first.stdout, /export function Counter\(\) @\{/);

  const second = await runFormat(['--stdin-filepath=Counter.tsrx'], first.stdout);
  assert.equal(second.code, 0, second.stderr || second.stdout);
  assert.equal(second.stdout, first.stdout);
});

test('preserves lexical @ text while formatting nested statement control flow', async () => {
  const source = await fixture('conditional.unformatted.tsrx');
  const expected = await fixture('conditional.formatted.tsrx');
  const result = await runFormat(['--stdin-filepath', 'conditional.tsrx'], source);

  assert.equal(result.code, 0, result.stderr || result.stdout);
  assert.equal(result.stdout, expected);
  assert.match(result.stdout, /\/@if\\s\+\\\/\/gu/);
  assert.match(result.stdout, /`Crème 🚀 \$\{label\}`/);
  assert.match(result.stdout, />@if is text, not control<\/p>/);
  assert.match(result.stdout, /\/\* @if \(comment\) \{\} \*\//);
  assert.match(result.stdout, /"@else@example\.com"/);
  assert.equal((result.stdout.match(/@if \(ready\)/g) ?? []).length, 1);
  assert.equal((result.stdout.match(/@else \{/g) ?? []).length, 1);
});

test('check and write converge without touching an unformatted file during check', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'oxc-tsrx-format-'));
  const path = join(directory, 'Counter.tsrx');
  const before = await fixture('markless-counter.unformatted.tsrx');
  const expected = await fixture('markless-counter.formatted.tsrx');
  await writeFile(path, before);

  const checkBefore = await runFormat(['--check', path]);
  assert.equal(checkBefore.code, 1, checkBefore.stderr || checkBefore.stdout);
  assert.match(checkBefore.stdout, new RegExp(`${basename(path)}|${path.replaceAll('\\', '\\\\')}`));
  assert.equal(await readFile(path, 'utf8'), before);

  const write = await runFormat(['--write', path]);
  assert.equal(write.code, 0, write.stderr || write.stdout);
  assert.equal(await readFile(path, 'utf8'), expected);

  const checkAfter = await runFormat(['--check', path]);
  assert.equal(checkAfter.code, 0, checkAfter.stderr || checkAfter.stdout);
});

test('a multi-file write is fail-atomic when one file contains malformed TSRX', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'oxc-tsrx-format-atomic-'));
  const validPath = join(directory, 'valid.tsrx');
  const malformedPath = join(directory, 'malformed.tsrx');
  const validBefore = await fixture('markless-counter.unformatted.tsrx');
  const malformedBefore = 'export function List() @{ <style>p { color: red } }\n';
  await writeFile(validPath, validBefore);
  await writeFile(malformedPath, malformedBefore);

  const result = await runFormat(['--write', validPath, malformedPath]);
  assert.equal(result.code, 2, result.stderr || result.stdout);
  assert.match(result.stderr, /unterminated|closing|style|structural/i);
  assert.equal(await readFile(validPath, 'utf8'), validBefore);
  assert.equal(await readFile(malformedPath, 'utf8'), malformedBefore);
});

test('invalid TSRX returns no formatted output', async () => {
  const source = 'export function Broken() @{ const value = ; }\n';
  const result = await runFormat(['--stdin-filepath=broken.tsrx'], source);
  assert.equal(result.code, 2, result.stderr || result.stdout);
  assert.equal(result.stdout, '');
  assert.match(result.stderr, /parse|expected|unexpected/i);
});

test('ordinary JS, JSX, TS, and TSX take the canonical format path byte-for-byte', async () => {
  const cases = {
    'ordinary.js': 'export function value( ){return {answer:1}}\n',
    'ordinary.jsx': 'export function View( ){return <main data-x="1">ok</main>}\n',
    'ordinary.ts': 'export function value(input:number):number{return input+1}\n',
    'ordinary.tsx': 'type P={label:string};export function View({label}:P){return <main>{label}</main>}\n',
  };
  for (const [name, source] of Object.entries(cases)) {
    const [candidate, stock] = await Promise.all([
      runFormat([`--stdin-filepath=${name}`], source),
      run(stockBinary, [`--stdin-filepath=${name}`], source),
    ]);
    assert.equal(candidate.code, 0, candidate.stderr || candidate.stdout);
    assert.equal(stock.code, 0, stock.stderr || stock.stdout);
    assert.equal(candidate.stdout, stock.stdout, name);
  }
});

test('Unicode decorator identifiers format identically through TSRX and canonical TSX paths', async () => {
  const cases = {
    'raw-unicode': '@ifπ\nclass Decorated{method( ){return 1}}\n',
    'escaped-unicode': '@for\\u03c0\nclass Decorated{method( ){return 1}}\n',
    'escaped-astral': '@try\\u{1D49C}\nclass Decorated{method( ){return 1}}\n',
  };

  for (const [name, source] of Object.entries(cases)) {
    const [tsrx, tsx] = await Promise.all([
      runFormat([`--stdin-filepath=${name}.tsrx`], source),
      runFormat([`--stdin-filepath=${name}.tsx`], source),
    ]);
    assert.equal(tsx.code, 0, tsx.stderr || tsx.stdout);
    assert.equal(tsrx.code, 0, tsrx.stderr || tsrx.stdout);
    assert.equal(tsrx.stdout, tsx.stdout, name);
    assert.equal(tsrx.stderr, '');
  }
});
