import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { copyFile, mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '..');
const binary = process.env.OXLINT_BIN ?? join(root, 'target/release/oxc-tsrx');
const stockBinary = join(root, 'node_modules/oxlint-current/bin/oxlint');
const tsrxFixture = join(root, 'tests/fixtures/lint/native-lint.tsrx');
const tsxFixture = join(root, 'tests/fixtures/lint/ordinary.tsx');

function run(args) {
  return new Promise((resolvePromise, reject) => {
    const child = spawn(binary, args, {
      cwd: root,
      env: process.env,
      stdio: ['ignore', 'pipe', 'pipe'],
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
  });
}

function parseJsonOutput(result) {
  const start = result.stdout.indexOf('{');
  assert.notEqual(start, -1, result.stderr || result.stdout);
  return JSON.parse(result.stdout.slice(start));
}

function byteOffset(source, needle) {
  const characterOffset = source.indexOf(needle);
  assert.notEqual(characterOffset, -1, `Missing ${needle}`);
  return Buffer.byteLength(source.slice(0, characterOffset));
}

function diagnosticFor(output, rule) {
  return output.diagnostics.find((diagnostic) =>
    diagnostic.rule === rule || diagnostic.code?.includes(`(${rule})`));
}

function comparableDiagnostics(output) {
  return output.diagnostics.map(({ code, message, severity, labels }) => ({
    code,
    message,
    severity,
    labels: labels.map(({ span }) => ({
      span: { offset: span.offset, length: span.length },
    })),
  }));
}

test('runs real OXC rules once and reports original TSRX byte spans', async () => {
  const source = await readFile(tsrxFixture, 'utf8');
  const result = await run([
    '--format=json',
    '--deny',
    'no-debugger',
    '--deny',
    'no-unused-vars',
    tsrxFixture,
  ]);

  assert.equal(result.signal, null);
  assert.equal(result.code, 1, result.stderr || result.stdout);
  const output = parseJsonOutput(result);

  const debuggerDiagnostic = diagnosticFor(output, 'no-debugger');
  assert.ok(debuggerDiagnostic, result.stdout);
  assert.equal(debuggerDiagnostic.filename, tsrxFixture);
  assert.equal(debuggerDiagnostic.severity, 'error');
  assert.deepEqual(debuggerDiagnostic.labels[0].span, {
    offset: byteOffset(source, 'debugger;\n    <main'),
    length: Buffer.byteLength('debugger;'),
  });

  const unusedDiagnostic = diagnosticFor(output, 'no-unused-vars');
  assert.ok(unusedDiagnostic, result.stdout);
  assert.equal(unusedDiagnostic.filename, tsrxFixture);
  assert.equal(
    unusedDiagnostic.labels.some((label) =>
      label.span.offset === byteOffset(source, 'unused = 1')),
    true,
    result.stdout,
  );

  assert.equal(output.oxcTsrx.native, true);
  assert.equal(output.oxcTsrx.engine, 'oxc_linter');
  assert.equal(output.oxcTsrx.oxcRevision, '8e0ed2ebb96137fb1611cdbd5742d5cb46037d40');
  assert.equal(output.oxcTsrx.parseCount, 1);
  assert.equal(output.oxcTsrx.files.tsrx, 1);
  for (const field of ['scanNs', 'projectionNs', 'parseNs', 'semanticNs', 'lintNs']) {
    assert.equal(typeof output.oxcTsrx.timings[field], 'number');
  }
});

test('applies only an identity-mapped no-var fix and reparses TSRX', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'oxc-tsrx-t011-'));
  const file = join(directory, 'native-lint.tsrx');
  await copyFile(tsrxFixture, file);
  const before = await readFile(file, 'utf8');

  const result = await run(['--format=json', '--fix', '--deny', 'no-var', file]);
  assert.equal(result.code, 0, result.stderr || result.stdout);
  const output = parseJsonOutput(result);
  const after = await readFile(file, 'utf8');

  assert.match(after, /\b(?:let|const) legacy = 2;/);
  assert.doesNotMatch(after, /\bvar legacy = 2;/);
  assert.equal(after.replace(/(?:let|const) legacy/, 'var legacy'), before);
  assert.match(after, /export function View\(\{ ready \}: Props\) @\{/);
  assert.match(after, /const contact = "@if@example\.com";/);
  assert.match(after, /\/\/ @if \(false\) \{ debugger; \}/);
  assert.equal(output.oxcTsrx.fixes.applied, 1);
  assert.equal(output.oxcTsrx.fixes.rejected, 0);
  assert.equal(output.oxcTsrx.reparseCount, 1);
});

test('ordinary TSX bypasses the TSRX scan and projection allocation', async () => {
  const result = await run(['--format=json', '--deny', 'no-debugger', tsxFixture]);
  assert.equal(result.code, 1, result.stderr || result.stdout);
  const output = parseJsonOutput(result);
  assert.ok(diagnosticFor(output, 'no-debugger'), result.stdout);
  assert.equal(output.oxcTsrx.mode, 'direct');
  assert.equal(output.oxcTsrx.files.standard, 1);
  assert.equal(output.oxcTsrx.timings.scanNs, 0);
  assert.equal(output.oxcTsrx.timings.projectionNs, 0);
  assert.equal(output.oxcTsrx.projectionBytes, 0);
});

test('ordinary JS, JSX, TS, and TSX match canonical Oxlint and bypass every TSRX stage', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'oxc-tsrx-direct-lint-'));
  const cases = {
    'ordinary.js': 'export function run() { debugger; return 1; }\n',
    'ordinary.jsx': 'export function View() { debugger; return <main>{1}</main>; }\n',
    'ordinary.ts': 'export function run(value: number): number { debugger; return value; }\n',
    'ordinary.tsx': 'export function View(props: { value: number }) { debugger; return <main>{props.value}</main>; }\n',
  };

  for (const [name, source] of Object.entries(cases)) {
    const file = join(directory, name);
    await writeFile(file, source);
    const [candidateResult, stockResult] = await Promise.all([
      run(['--format=json', '--deny', 'no-debugger', file]),
      new Promise((resolvePromise, reject) => {
        const child = spawn(stockBinary, ['--format=json', '--deny', 'no-debugger', file], {
          cwd: root,
          env: process.env,
          stdio: ['ignore', 'pipe', 'pipe'],
        });
        let stdout = '';
        let stderr = '';
        child.stdout.setEncoding('utf8');
        child.stderr.setEncoding('utf8');
        child.stdout.on('data', (chunk) => (stdout += chunk));
        child.stderr.on('data', (chunk) => (stderr += chunk));
        child.once('error', reject);
        child.once('close', (code, signal) => resolvePromise({ code, signal, stdout, stderr }));
      }),
    ]);
    assert.equal(candidateResult.code, stockResult.code, name);
    const candidate = parseJsonOutput(candidateResult);
    const stock = parseJsonOutput(stockResult);
    assert.deepEqual(comparableDiagnostics(candidate), comparableDiagnostics(stock), name);
    assert.equal(candidate.oxcTsrx.mode, 'direct', name);
    assert.equal(candidate.oxcTsrx.files.standard, 1, name);
    assert.equal(candidate.oxcTsrx.timings.scanNs, 0, name);
    assert.equal(candidate.oxcTsrx.timings.projectionNs, 0, name);
    assert.equal(candidate.oxcTsrx.projectionBytes, 0, name);
  }
});
