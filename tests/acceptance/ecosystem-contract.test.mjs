import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import path from 'node:path'
import test from 'node:test'

import {
  assertOwnerSourceUnchanged,
  shouldExcludeSourcePath,
} from './clean-room-utils.mjs'

const root = path.resolve(import.meta.dirname, '..', '..')

test('both supported physical Vite+ consumers prove build and dev through the framework plugin', async () => {
  const report = JSON.parse(
    await readFile(path.join(root, 'tests/packaging/vite-plus-matrix-report.json'), 'utf8'),
  )
  assert.deepEqual(
    report.lanes.map((lane) => lane.vitePlus.version),
    ['0.1.24', '0.2.4'],
  )
  for (const lane of report.lanes) {
    assert.equal(lane.proof.viteBuild, true, `${lane.lane}: vp build`)
    assert.equal(lane.proof.viteDev, true, `${lane.lane}: vp dev`)
    assert.equal(lane.proof.viteDevRetransform, true, `${lane.lane}: dev retransform`)
  }
})

test('the clean-room oracle excludes nested build artifacts and detects concurrent owner edits', async () => {
  assert.equal(shouldExcludeSourcePath('target/release/oxc-tsrx'), true)
  assert.equal(shouldExcludeSourcePath('docs/tools/projection-dump/target/release/projection-dump'), true)
  assert.equal(shouldExcludeSourcePath('packages/editor/node_modules/pkg/index.js'), true)
  assert.equal(shouldExcludeSourcePath('benchmarks/comparative/.corpus-123/component.tsx'), true)
  assert.equal(shouldExcludeSourcePath('benchmarks/native-lint/latest.json'), true)
  assert.equal(shouldExcludeSourcePath('docs/dist/assets/app.js'), true)
  assert.equal(
    shouldExcludeSourcePath('docs/acceptance/clean-room-report.json', { excludeReport: true }),
    true,
  )
  assert.equal(shouldExcludeSourcePath('crates/targeted/src/lib.rs'), false)
  assert.doesNotThrow(() => assertOwnerSourceUnchanged('same', 'same'))
  assert.throws(
    () => assertOwnerSourceUnchanged('before', 'after'),
    /owner source changed during clean-room verification/u,
  )

  const runner = await readFile(path.join(root, 'tests/acceptance/run.mjs'), 'utf8')
  assert.match(runner, /finalSourceHash\s*=\s*await treeHash/u)
  assert.match(runner, /assertOwnerSourceUnchanged\(initialSourceHash, finalSourceHash\)/u)
})
