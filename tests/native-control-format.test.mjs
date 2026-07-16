import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { readFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '..');
const binary = process.env.OXFMT_BIN ?? join(root, 'target/release/oxc-tsrx-fmt');
const fixtures = join(root, 'tests/fixtures/control');

function run(args, input) {
  return new Promise((resolvePromise, reject) => {
    const child = spawn(binary, args, {
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
    child.once('close', (code, signal) => resolvePromise({ code, signal, stdout, stderr }));
    child.stdin.end(input);
  });
}

async function fixture(name) {
  return readFile(join(fixtures, name), 'utf8');
}

for (const name of [
  'branch',
  'rows',
  'nested',
  'async-expression',
  'switch',
  'try',
  'expressions',
  'dynamic-style',
]) {
  test(`formats and converges for ${name}`, async () => {
    const source = await fixture(`${name}.unformatted.tsrx`);
    const expected = await fixture(`${name}.formatted.tsrx`);
    const first = await run(['--stdin-filepath', `${name}.tsrx`], source);

    assert.equal(first.signal, null);
    assert.equal(first.code, 0, first.stderr || first.stdout);
    assert.equal(first.stderr, '');
    assert.equal(first.stdout, expected);
    assert.doesNotMatch(first.stdout, /__OXC_TSRX|async \*__|async \* __/);

    const second = await run(['--stdin-filepath', `${name}.tsrx`], first.stdout);
    assert.equal(second.code, 0, second.stderr || second.stdout);
    assert.equal(second.stdout, first.stdout);
  });
}

test('preserves @for await rather than inheriting the incumbent formatter loss', async () => {
  const source = await fixture('async-expression.unformatted.tsrx');
  const result = await run(['--stdin-filepath=async-expression.tsrx'], source);
  assert.equal(result.code, 0, result.stderr || result.stdout);
  assert.match(result.stdout, /@for await \(const item of items;/);
});

test('rejects malformed control families without returning partial output', async () => {
  for (const [source, expected] of [
    ['function View() @{ <div>@for (const x of xs) {} empty {}</div> }', /@empty|empty/i],
    ['function View() @{ <div>@try {} pending {}</div> }', /@pending|pending/i],
    ['function View() @{ <div>@switch (x) { @default: {} @default: {} }</div> }', /default/i],
  ]) {
    const result = await run(['--stdin-filepath=invalid.tsrx'], source);
    assert.equal(result.code, 2, result.stderr || result.stdout);
    assert.equal(result.stdout, '');
    assert.match(result.stderr, expected);
  }
});

test('rejects malformed dynamic tags and styles without returning partial output', async () => {
  for (const [source, expected] of [
    ['function View() @{ <{tag}>Hi</{other}> }', /closing|dynamic|match/i],
    ['function View() @{ <{tag()} /> }', /dynamic|tag/i],
    ['function View() @{ <style>.card { color: red; } }', /style|closing/i],
  ]) {
    const result = await run(['--stdin-filepath=invalid.tsrx'], source);
    assert.equal(result.code, 2, result.stderr || result.stdout);
    assert.equal(result.stdout, '');
    assert.match(result.stderr, expected);
  }
});

test('preserves raw style payloads without claiming CSS validation', async () => {
  const payload = '.card { color: red;';
  const source = `function View() @{ <style>${payload}</style> }`;
  const result = await run(['--stdin-filepath=raw-style.tsrx'], source);
  assert.equal(result.code, 0, result.stderr || result.stdout);
  assert.ok(result.stdout.includes(`<style>${payload}</style>`));
});
