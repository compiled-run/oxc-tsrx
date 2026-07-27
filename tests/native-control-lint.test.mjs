import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { copyFile, mkdtemp, readFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '..');
const binary = process.env.OXLINT_BIN ?? join(root, 'target/release/oxc-tsrx');
const fixture = join(root, 'tests/fixtures/control/control-lint.tsrx');
const advancedFixture = join(root, 'tests/fixtures/control/control-lint-advanced.tsrx');
const dynamicStyleFixture = join(root, 'tests/fixtures/control/dynamic-style-lint.tsrx');

// Oxlint switches to GitHub's annotation reporter (`##[warning]`, `::error`)
// when it detects Actions. These assertions are about the default human-readable
// format, not about which reporter CI picks, so the detection is turned off here
// and one expected output holds on a laptop and on a runner.
const LINT_ENVIRONMENT = { ...process.env };
delete LINT_ENVIRONMENT.GITHUB_ACTIONS;
delete LINT_ENVIRONMENT.CI;

function run(args) {
  return new Promise((resolvePromise, reject) => {
    const child = spawn(binary, args, {
      cwd: root,
      env: LINT_ENVIRONMENT,
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
  });
}

function outputOf(result) {
  const start = result.stdout.indexOf('{');
  assert.notEqual(start, -1, result.stderr || result.stdout);
  return JSON.parse(result.stdout.slice(start));
}

function byteOffset(source, needle) {
  const offset = source.indexOf(needle);
  assert.notEqual(offset, -1, `missing ${needle}`);
  return Buffer.byteLength(source.slice(0, offset));
}

function diagnostic(output, rule) {
  return output.diagnostics.find((item) => item.rule === rule || item.code.includes(`(${rule})`));
}

test('maps descendant diagnostics through expanded JSX control projection', async () => {
  const source = await readFile(fixture, 'utf8');
  const result = await run([
    '--format=json',
    '--deny',
    'no-debugger',
    '--deny',
    'no-unused-vars',
    fixture,
  ]);
  assert.equal(result.signal, null);
  assert.equal(result.code, 1, result.stderr || result.stdout);
  const output = outputOf(result);

  const debuggerDiagnostic = diagnostic(output, 'no-debugger');
  assert.ok(debuggerDiagnostic, result.stdout);
  assert.deepEqual(debuggerDiagnostic.labels[0].span, {
    offset: byteOffset(source, 'debugger;'),
    length: Buffer.byteLength('debugger;'),
  });

  const unusedDiagnostic = diagnostic(output, 'no-unused-vars');
  assert.ok(unusedDiagnostic, result.stdout);
  assert.equal(
    unusedDiagnostic.labels.some((label) => label.span.offset === byteOffset(source, 'unused = 2')),
    true,
    result.stdout,
  );

  assert.equal(output.oxcTsrx.parseCount, 1);
  assert.equal(output.oxcTsrx.mode, 'mapped_projection');
  assert.ok(output.oxcTsrx.projectionBytes > Buffer.byteLength(source));
  assert.equal(typeof output.oxcTsrx.diagnosticsSuppressed, 'number');
});

test('translates an identity-only no-var fix and validates the original TSRX', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'oxc-tsrx-control-lint-'));
  const path = join(directory, 'control-lint.tsrx');
  await copyFile(fixture, path);
  const before = await readFile(path, 'utf8');

  const result = await run(['--format=json', '--fix', '--deny', 'no-var', path]);
  assert.equal(result.code, 0, result.stderr || result.stdout);
  const output = outputOf(result);
  const after = await readFile(path, 'utf8');

  assert.match(after, /(?:let|const) legacy = 1;/);
  assert.equal(after.replace(/(?:let|const) legacy/, 'var legacy'), before);
  assert.match(after, /@if \(ready\)/);
  assert.match(after, /@else \{/);
  assert.equal(output.oxcTsrx.fixes.applied, 1);
  assert.equal(output.oxcTsrx.reparseCount, 1);
});

test('maps diagnostics through switch and source-order try clauses', async () => {
  const source = await readFile(advancedFixture, 'utf8');
  const result = await run([
    '--format=json',
    '--deny',
    'no-debugger',
    '--deny',
    'no-unused-vars',
    advancedFixture,
  ]);
  assert.equal(result.signal, null);
  assert.equal(result.code, 1, result.stderr || result.stdout);
  const output = outputOf(result);
  const labelOffsets = output.diagnostics.flatMap((item) =>
    item.labels.map((label) => label.span.offset),
  );

  for (const needle of ['debugger;', 'catchUnused = error']) {
    assert.equal(labelOffsets.includes(byteOffset(source, needle)), true, `${needle}\n${result.stdout}`);
  }
  assert.equal(output.oxcTsrx.parseCount, 1);
  assert.equal(output.oxcTsrx.mode, 'mapped_projection');
});

test('applies identity-only no-var fixes inside switch and try bodies', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'oxc-tsrx-advanced-control-lint-'));
  const path = join(directory, 'control-lint-advanced.tsrx');
  await copyFile(advancedFixture, path);
  const before = await readFile(path, 'utf8');

  const result = await run(['--format=json', '--fix', '--deny', 'no-var', path]);
  assert.equal(result.code, 0, result.stderr || result.stdout);
  const output = outputOf(result);
  const after = await readFile(path, 'utf8');

  assert.doesNotMatch(after, /\bvar\b/);
  assert.equal(after.replace(/(?:let|const) legacy/g, 'var legacy'), before);
  assert.match(after, /@switch \(status\)/);
  assert.match(after, /@pending \{/);
  assert.match(after, /@catch \(error, reset\)/);
  assert.equal(output.oxcTsrx.fixes.applied, 2);
  assert.equal(output.oxcTsrx.reparseCount, 1);
});

test('maps dynamic-tag descendants while keeping raw style synthetic', async () => {
  const source = await readFile(dynamicStyleFixture, 'utf8');
  const result = await run([
    '--format=json',
    '--deny',
    'no-debugger',
    '--deny',
    'no-unused-vars',
    dynamicStyleFixture,
  ]);
  assert.equal(result.signal, null);
  assert.equal(result.code, 1, result.stderr || result.stdout);
  const output = outputOf(result);
  const debuggerDiagnostic = diagnostic(output, 'no-debugger');
  assert.ok(debuggerDiagnostic, result.stdout);
  assert.deepEqual(debuggerDiagnostic.labels[0].span, {
    offset: byteOffset(source, 'debugger;'),
    length: Buffer.byteLength('debugger;'),
  });
  assert.equal(output.oxcTsrx.parseCount, 1);
  assert.equal(output.oxcTsrx.mode, 'mapped_projection');
});

test('applies an identity-only fix inside a dynamic tag and reparses style syntax', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'oxc-tsrx-dynamic-style-lint-'));
  const path = join(directory, 'dynamic-style-lint.tsrx');
  await copyFile(dynamicStyleFixture, path);
  const before = await readFile(path, 'utf8');

  const result = await run(['--format=json', '--fix', '--deny', 'no-var', path]);
  assert.equal(result.code, 0, result.stderr || result.stdout);
  const output = outputOf(result);
  const after = await readFile(path, 'utf8');

  assert.match(after, /(?:let|const) legacy = 1;/);
  assert.equal(after.replace(/(?:let|const) legacy/, 'var legacy'), before);
  assert.match(after, /<\{tag\}[\s\S]*class="card"/);
  assert.match(after, /<style>/);
  assert.equal(output.oxcTsrx.fixes.applied, 1);
  assert.equal(output.oxcTsrx.reparseCount, 1);
});
