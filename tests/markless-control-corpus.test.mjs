import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const here = dirname(fileURLToPath(import.meta.url));
const projectRoot = resolve(here, '..');
const marklessRoot = resolve(
  process.env.MARKLESS_ROOT ?? '/Users/jacksm5pro/dev/open-source/markless',
);
const binary = resolve(process.env.OXFMT_BIN ?? resolve(projectRoot, 'target/release/oxc-tsrx-fmt'));
const revision = '76d0e6a07fa728b9343cc0d342fbe03813c43703';

const invalidFiles = [
  'packages/typescript-plugin/test/fixtures/completion-matrix/catalog.tsrx',
  'packages/typescript-plugin/test/fixtures/completion-matrix/construct-children.tsrx',
  'packages/typescript-plugin/test/fixtures/completion-matrix/constructs.tsrx',
  'packages/typescript-plugin/test/fixtures/completion-matrix/framework.tsrx',
  'packages/typescript-plugin/test/fixtures/completion-matrix/intrinsic-contract-errors.tsrx',
  'packages/typescript-plugin/test/fixtures/completion-matrix/intrinsic-contract.tsrx',
  'packages/typescript-plugin/test/fixtures/completion-matrix/router-contexts.tsrx',
  'packages/typescript-plugin/test/fixtures/completion-matrix/tag-closing-protocol.tsrx',
  'packages/typescript-plugin/test/fixtures/completion-matrix/tag-completions-expression.tsrx',
  'packages/typescript-plugin/test/fixtures/completion-matrix/tag-completions-partial.tsrx',
  'packages/typescript-plugin/test/fixtures/completion-matrix/tag-completions.tsrx',
  'packages/typescript-plugin/test/fixtures/completion-matrix/typescript.tsrx',
];

function git(args, options = {}) {
  return execFileSync('git', ['-C', marklessRoot, ...args], {
    encoding: 'utf8',
    maxBuffer: 32 * 1024 * 1024,
    ...options,
  });
}

function format(path, source) {
  return spawnSync(binary, [`--stdin-filepath=${path}`], {
    cwd: projectRoot,
    encoding: 'utf8',
    input: source,
    maxBuffer: 32 * 1024 * 1024,
  });
}

function collisionFreePrefix(source) {
  for (let nonce = 0; nonce <= 1024; nonce += 1) {
    const prefix = `_t${nonce.toString(16)}_`;
    if (!source.includes(prefix)) return prefix;
  }
  throw new Error('fixture exhausts the formatter marker namespace');
}

function stylePayloads(source) {
  return [...source.matchAll(/<style(?:\s[^>]*)?>([\s\S]*?)<\/style>/g)].map((match) => match[1]);
}

test('formats every parser-valid file at the committed Markless revision without touching Markless', () => {
  const statusBefore = git(['status', '--porcelain=v1', '-z']);
  assert.equal(git(['cat-file', '-t', revision]).trim(), 'commit');

  const requireFromMarkless = createRequire(resolve(marklessRoot, 'packages/compiler/package.json'));
  const coreEntry = requireFromMarkless.resolve('@tsrx/core');
  const corePackage = JSON.parse(readFileSync(resolve(coreEntry, '../..', 'package.json'), 'utf8'));
  assert.equal(corePackage.version, '0.1.32');
  const { parseModule } = requireFromMarkless('@tsrx/core');

  const tracked = git(['ls-tree', '-r', '-z', '--name-only', revision])
    .split('\0')
    .filter((path) => path.endsWith('.tsrx'));
  assert.equal(tracked.length, 191);

  const actualInvalid = [];
  const accepted = [];
  for (const path of tracked) {
    const source = git(['show', `${revision}:${path}`]);
    let valid = true;
    try {
      parseModule(source, path);
    } catch {
      valid = false;
      actualInvalid.push(path);
    }

    const first = format(path, source);
    if (!valid) {
      assert.equal(first.status, 2, `${path}\n${first.stderr}`);
      assert.equal(first.stdout, '', path);
      continue;
    }
    assert.equal(first.signal, null, path);
    assert.equal(first.status, 0, `${path}\n${first.stderr}`);
    assert.equal(first.stderr, '', path);
    if (source.length > 0) assert.notEqual(first.stdout, '', path);
    assert.ok(!first.stdout.includes(collisionFreePrefix(source)), path);
    assert.deepEqual(stylePayloads(first.stdout), stylePayloads(source), path);
    parseModule(first.stdout, path);
    const second = format(path, first.stdout);
    assert.equal(second.status, 0, `${path}\n${second.stderr}`);
    assert.equal(second.stdout, first.stdout, `${path} did not converge`);
    accepted.push(path);
  }

  assert.deepEqual(actualInvalid, invalidFiles);
  assert.equal(accepted.length, 179);
  assert.equal(git(['status', '--porcelain=v1', '-z']), statusBefore);
});
