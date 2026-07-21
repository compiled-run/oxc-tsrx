import assert from 'node:assert/strict'
import test from 'node:test'

import {
  adjudicateReports,
  findSingleFreshReport,
  isAcceptedBenchmarkExit,
  normalizeAssertion,
  planAdjudication,
} from './performance-adjudication.mjs'

const commonIdentity = Object.freeze({
  oxcRevision: '8e0ed2ebb96137fb1611cdbd5742d5cb46037d40',
  corpus: 'paired-corpus-v1',
  budgets: 'frozen-budgets-v1',
  host: 'test-host-v1',
  build: 'release-build-v1',
  versions: 'tool-versions-v1',
  route: 'public-route-v1',
})

function numeric(name, observed, threshold, operator) {
  return {
    name,
    observed,
    threshold,
    operator,
    comparison: operator,
    invariant: false,
    pass: operator === '<=' ? observed <= threshold : observed >= threshold,
  }
}

function invariant(name, pass = true) {
  return {
    name,
    observed: pass ? 1 : 0,
    threshold: 1,
    operator: '==',
    comparison: 'required boolean invariant',
    invariant: true,
    pass,
  }
}

function report(path, mixedRatio, options = {}) {
  const assertions = [
    numeric(
      'nearOxlintParity',
      options.nearOxlintParity ?? 1,
      1.25,
      '<=',
    ),
    numeric(
      'fasterThanEslint',
      options.fasterThanEslint ?? 10,
      2,
      '>=',
    ),
    numeric('mixedNoBlowup', mixedRatio, 1.5, '<='),
    invariant('publicRouteInvariant', options.invariant ?? true),
  ]
  return {
    path,
    assertions,
    reportPassed: assertions.every(({ pass }) => pass),
    invariantsPassed: assertions.filter(({ invariant: required }) => required).every(({ pass }) => pass),
    identity: { ...commonIdentity, ...options.identity },
  }
}

test('fresh report discovery rejects stale or ambiguous filesystem evidence', () => {
  const before = new Set(['results-100.json'])
  assert.equal(
    findSingleFreshReport(before, new Set(['results-100.json', 'results-101.json']), 'comparative'),
    'results-101.json',
  )
  assert.throws(
    () => findSingleFreshReport(before, new Set(before), 'comparative'),
    /exactly one fresh comparative report; found 0/u,
  )
  assert.throws(
    () =>
      findSingleFreshReport(
        before,
        new Set(['results-100.json', 'results-101.json', 'results-102.json']),
        'comparative',
      ),
    /exactly one fresh comparative report; found 2/u,
  )
})

test('benchmark exit validation admits only success or a documented red assertion exit', () => {
  assert.equal(isAcceptedBenchmarkExit({ status: 0, signal: null, reportPassed: true }), true)
  assert.equal(
    isAcceptedBenchmarkExit(
      { status: 1, signal: null, reportPassed: false },
      { allowAssertionFailure: true },
    ),
    true,
  )
  for (const outcome of [
    { status: 1, signal: null, reportPassed: true },
    { status: 2, signal: null, reportPassed: false },
    { status: 137, signal: null, reportPassed: false },
    { status: null, signal: 'SIGKILL', reportPassed: false },
  ]) {
    assert.equal(
      isAcceptedBenchmarkExit(outcome, { allowAssertionFailure: true }),
      false,
      JSON.stringify(outcome),
    )
  }
  assert.equal(
    isAcceptedBenchmarkExit(
      { status: 1, signal: null, reportPassed: false },
      { allowAssertionFailure: false },
    ),
    false,
  )
})

test('native descriptive comparisons require explicit stable operators', () => {
  assert.equal(
    normalizeAssertion({
      name: 'rss ratio',
      comparison: 'candidate RSS / canonical RSS',
      operator: '<=',
      observed: 1.14,
      threshold: 1.15,
      pass: true,
    }).operator,
    '<=',
  )
  assert.equal(
    normalizeAssertion({
      name: 'throughput floor',
      comparison: 'complete format throughput',
      operator: '>=',
      observed: 20,
      threshold: 15,
      pass: true,
    }).operator,
    '>=',
  )
  assert.equal(
    normalizeAssertion({
      name: 'failed ceiling',
      comparison: 'candidate / canonical',
      operator: '<=',
      observed: 1.16,
      threshold: 1.15,
      pass: false,
    }).operator,
    '<=',
  )
  assert.equal(
    normalizeAssertion({
      name: 'correctness',
      comparison: 'required boolean invariant',
      observed: 1,
      threshold: 1,
      pass: true,
    }).operator,
    '==',
  )
  assert.throws(
    () =>
      normalizeAssertion({
        name: 'missing operator',
        comparison: 'descriptive comparison',
        observed: 1.14,
        threshold: 1.15,
        pass: true,
      }),
    /lacks an explicit numeric comparison operator/u,
  )
})

test('near-threshold adjudication selects the median pressure report, not a favorable tail', () => {
  assert.deepEqual(planAdjudication(report('reports/planning.json', 1.49), { bandFraction: 0.03 }), {
    triggered: true,
    triggeredBy: ['mixedNoBlowup'],
    requiredReports: 3,
  })
  const result = adjudicateReports(
    [
      report('reports/first-pass.json', 1.49),
      report('reports/near-fail.json', 1.51),
      report('reports/last-better-pass.json', 1.45),
    ],
    { bandFraction: 0.03 },
  )

  assert.equal(result.triggered, true)
  assert.deepEqual(result.triggeredBy, ['mixedNoBlowup'])
  assert.equal(result.requiredReports, 3)
  assert.equal(result.decision, 'passed')
  assert.equal(result.selectedReport, 'reports/first-pass.json')
  assert.equal(result.selectionPolicy.kind, 'median-normalized-budget-pressure')
  assert.deepEqual(result.selectionPolicy.assertions, ['mixedNoBlowup'])
  assert.equal(result.selectionPolicy.tieBreak, 'report-path-ascending')
  assert.deepEqual(
    result.selectionPolicy.orderedReports.map(({ path }) => path),
    [
      'reports/last-better-pass.json',
      'reports/first-pass.json',
      'reports/near-fail.json',
    ],
  )
  assert.equal(
    result.assertionDecisions.find(({ name }) => name === 'mixedNoBlowup').passCount,
    2,
  )
})

test('pressure ordering handles floor budgets and stable path ties', () => {
  const floorResult = adjudicateReports(
    [
      report('reports/floor-first.json', 1.3, { fasterThanEslint: 2.01 }),
      report('reports/floor-near-fail.json', 1.3, { fasterThanEslint: 1.99 }),
      report('reports/floor-better.json', 1.3, { fasterThanEslint: 2.1 }),
    ],
    { bandFraction: 0.03 },
  )
  assert.deepEqual(floorResult.triggeredBy, ['fasterThanEslint'])
  assert.equal(floorResult.decision, 'passed')
  assert.equal(floorResult.selectedReport, 'reports/floor-first.json')
  assert.deepEqual(
    floorResult.selectionPolicy.orderedReports.map(({ path }) => path),
    [
      'reports/floor-better.json',
      'reports/floor-first.json',
      'reports/floor-near-fail.json',
    ],
  )

  const tieResult = adjudicateReports(
    [
      report('reports/tie-c.json', 1.49),
      report('reports/tie-a.json', 1.49),
      report('reports/tie-b.json', 1.49),
    ],
    { bandFraction: 0.03 },
  )
  assert.deepEqual(
    tieResult.selectionPolicy.orderedReports.map(({ path }) => path),
    ['reports/tie-a.json', 'reports/tie-b.json', 'reports/tie-c.json'],
  )
  assert.equal(tieResult.selectedReport, 'reports/tie-b.json')
})

test('two near failures fail the triggering assertion', () => {
  const result = adjudicateReports(
    [
      report('reports/near-fail-1.json', 1.51),
      report('reports/near-fail-2.json', 1.52),
      report('reports/pass.json', 1.49),
    ],
    { bandFraction: 0.03 },
  )

  assert.equal(result.decision, 'failed')
  assert.equal(
    result.assertionDecisions.find(({ name }) => name === 'mixedNoBlowup').failCount,
    2,
  )
})

test('one failure outside the confidence band is definitive even with two passes', () => {
  const result = adjudicateReports(
    [
      report('reports/pass-1.json', 1.49),
      report('reports/far-fail.json', 1.56),
      report('reports/pass-2.json', 1.45),
    ],
    { bandFraction: 0.03 },
  )
  const decision = result.assertionDecisions.find(({ name }) => name === 'mixedNoBlowup')

  assert.equal(result.decision, 'failed')
  assert.equal(decision.definitiveFailure, true)
})

test('a passing vote cannot publish a median representative whose raw report is red', () => {
  const result = adjudicateReports(
    [
      report('reports/both-pass.json', 1.49, { nearOxlintParity: 1.24 }),
      report('reports/near-only-fail.json', 1.48, { nearOxlintParity: 1.26 }),
      report('reports/mixed-only-fail.json', 1.51, { nearOxlintParity: 1.23 }),
    ],
    { bandFraction: 0.03 },
  )

  assert.ok(
    result.assertionDecisions
      .filter(({ triggered }) => triggered)
      .every(({ decision }) => decision === 'passed'),
  )
  assert.equal(result.selectedReport, 'reports/mixed-only-fail.json')
  assert.equal(result.decision, 'failed')
})

test('only assertions triggered by the first report receive two-of-three tolerance', () => {
  const result = adjudicateReports(
    [
      report('reports/trigger.json', 1.49),
      report('reports/other-budget-fails.json', 1.48, { nearOxlintParity: 1.26 }),
      report('reports/pass.json', 1.47),
    ],
    { bandFraction: 0.03 },
  )
  const otherDecision = result.assertionDecisions.find(
    ({ name }) => name === 'nearOxlintParity',
  )

  assert.equal(otherDecision.triggered, false)
  assert.equal(otherDecision.failCount, 1)
  assert.equal(otherDecision.decision, 'failed')
  assert.equal(result.decision, 'failed')
})

test('a report outside the confidence band requires exactly one sample', () => {
  const single = report('reports/single.json', 1.3)
  const result = adjudicateReports([single], { bandFraction: 0.03 })

  assert.equal(result.triggered, false)
  assert.equal(result.requiredReports, 1)
  assert.equal(result.selectedReport, single.path)
  assert.equal(result.decision, 'passed')
  assert.throws(
    () => adjudicateReports([single, report('reports/unrequested.json', 1.31)], { bandFraction: 0.03 }),
    /requires exactly 1 report; received 2/u,
  )
})

test('identity drift and correctness invariant failures fail closed', () => {
  const contradictoryInvariant = report('reports/contradictory-invariant.json', 1.3)
  contradictoryInvariant.assertions.find(({ invariant: required }) => required).observed = 0
  assert.throws(
    () => adjudicateReports([contradictoryInvariant], { bandFraction: 0.03 }),
    /publicRouteInvariant result contradicts its equality comparison/u,
  )

  const duplicateAssertions = report('reports/duplicate-assertions.json', 1.3)
  duplicateAssertions.assertions.push({ ...duplicateAssertions.assertions[0] })
  assert.throws(
    () => adjudicateReports([duplicateAssertions], { bandFraction: 0.03 }),
    /duplicate performance assertion: nearOxlintParity/u,
  )

  assert.throws(
    () =>
      adjudicateReports(
        [report('reports/missing-identity.json', 1.3, { identity: { oxcRevision: undefined } })],
        { bandFraction: 0.03 },
      ),
    /incomplete adjudication identity: oxcRevision/u,
  )
  for (const key of Object.keys(commonIdentity)) {
    assert.throws(
      () =>
        adjudicateReports(
          [
            report('reports/first.json', 1.49),
            report('reports/drift.json', 1.48, { identity: { [key]: `${key}-drift` } }),
            report('reports/third.json', 1.47),
          ],
          { bandFraction: 0.03 },
        ),
      new RegExp(`identity drifted: ${key}`, 'u'),
    )
  }

  const assertionDrift = report('reports/assertion-drift.json', 1.48)
  assertionDrift.assertions[0].threshold = 1.3
  assert.throws(
    () =>
      adjudicateReports(
        [
          report('reports/first.json', 1.49),
          assertionDrift,
          report('reports/third.json', 1.47),
        ],
        { bandFraction: 0.03 },
      ),
    /performance assertion drifted: nearOxlintParity/u,
  )
  assert.throws(
    () =>
      adjudicateReports(
        [
          report('reports/first.json', 1.49),
          report('reports/invariant-fail.json', 1.48, { invariant: false }),
          report('reports/third.json', 1.47),
        ],
        { bandFraction: 0.03 },
      ),
    /correctness invariant failed/u,
  )
})
