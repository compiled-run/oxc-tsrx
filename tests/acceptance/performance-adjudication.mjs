function relativeMargin(assertion) {
  return Math.abs(assertion.threshold - assertion.observed) / Math.abs(assertion.threshold)
}

export function normalizeAssertion(assertion, { context = 'performance contract' } = {}) {
  const invariant =
    assertion?.comparison === 'required boolean invariant' || assertion?.comparison === '=='
  if (invariant) return { ...assertion, invariant: true, operator: '==' }
  const operator =
    ['<=', '>='].includes(assertion?.operator)
      ? assertion.operator
      : ['<=', '>='].includes(assertion?.comparison)
        ? assertion.comparison
        : null
  if (!operator) {
    throw new Error(
      `${context}: ${assertion?.name ?? 'assertion'} lacks an explicit numeric comparison operator`,
    )
  }
  return { ...assertion, invariant: false, operator }
}

export function isAcceptedBenchmarkExit(
  { status, signal, reportPassed },
  { allowAssertionFailure = false } = {},
) {
  if (signal !== null) return false
  if (status === 0) return true
  return allowAssertionFailure && status === 1 && reportPassed === false
}

function normalizedBudgetPressure(assertion) {
  if (assertion.operator === '<=') return assertion.observed / assertion.threshold
  if (assertion.operator === '>=') return assertion.threshold / assertion.observed
  throw new Error(`cannot calculate budget pressure for ${assertion.name}: ${assertion.operator}`)
}

function validateAssertion(assertion, reportPath) {
  if (!assertion || typeof assertion !== 'object' || typeof assertion.name !== 'string') {
    throw new Error(`${reportPath}: malformed performance assertion`)
  }
  if (typeof assertion.pass !== 'boolean') {
    throw new Error(`${reportPath}: ${assertion.name} lacks a boolean result`)
  }
  if (assertion.invariant) {
    if (assertion.operator !== '==') {
      throw new Error(`${reportPath}: ${assertion.name} invariant does not use ==`)
    }
    if (!Number.isFinite(assertion.observed) || !Number.isFinite(assertion.threshold)) {
      throw new Error(`${reportPath}: ${assertion.name} has non-finite equality operands`)
    }
    if (assertion.pass !== (assertion.observed === assertion.threshold)) {
      throw new Error(`${reportPath}: ${assertion.name} result contradicts its equality comparison`)
    }
    return
  }
  if (!['<=', '>='].includes(assertion.operator)) {
    throw new Error(`${reportPath}: ${assertion.name} lacks a numeric comparison operator`)
  }
  if (
    !Number.isFinite(assertion.observed) ||
    !Number.isFinite(assertion.threshold) ||
    assertion.observed <= 0 ||
    assertion.threshold <= 0
  ) {
    throw new Error(`${reportPath}: ${assertion.name} has a non-positive or non-finite value`)
  }
  const expectedPass =
    assertion.operator === '<='
      ? assertion.observed <= assertion.threshold
      : assertion.observed >= assertion.threshold
  if (assertion.pass !== expectedPass) {
    throw new Error(`${reportPath}: ${assertion.name} result contradicts its numeric comparison`)
  }
}

function validateReport(report) {
  if (!report || typeof report !== 'object' || typeof report.path !== 'string') {
    throw new Error('performance adjudication received a report without a path')
  }
  if (!Array.isArray(report.assertions) || report.assertions.length === 0) {
    throw new Error(`${report.path}: missing performance assertions`)
  }
  const names = report.assertions.map(({ name }) => name)
  if (new Set(names).size !== names.length) {
    const duplicate = names.find((name, index) => names.indexOf(name) !== index)
    throw new Error(`${report.path}: duplicate performance assertion: ${duplicate}`)
  }
  for (const assertion of report.assertions) validateAssertion(assertion, report.path)

  const actualReportPassed = report.assertions.every(({ pass }) => pass)
  const actualInvariantsPassed = report.assertions
    .filter(({ invariant }) => invariant)
    .every(({ pass }) => pass)
  if (report.reportPassed !== actualReportPassed) {
    throw new Error(`${report.path}: aggregate report result contradicts its assertions`)
  }
  if (report.invariantsPassed !== actualInvariantsPassed) {
    throw new Error(`${report.path}: aggregate invariant result contradicts its assertions`)
  }
  if (!actualInvariantsPassed) {
    throw new Error(`${report.path}: correctness invariant failed; performance tolerance cannot override it`)
  }
  if (!report.identity || typeof report.identity !== 'object') {
    throw new Error(`${report.path}: missing adjudication identity`)
  }
  if (Object.keys(report.identity).length === 0) {
    throw new Error(`${report.path}: empty adjudication identity`)
  }
  for (const [key, value] of Object.entries(report.identity)) {
    if (typeof value !== 'string' || value.length === 0) {
      throw new Error(`${report.path}: incomplete adjudication identity: ${key}`)
    }
  }
}

function requireCoherentReports(reports) {
  const first = reports[0]
  const firstIdentityKeys = Object.keys(first.identity).sort()
  for (const report of reports.slice(1)) {
    const identityKeys = [...new Set([...firstIdentityKeys, ...Object.keys(report.identity)])].sort()
    for (const key of identityKeys) {
      if (report.identity[key] !== first.identity[key]) {
        throw new Error(`performance adjudication identity drifted: ${key}`)
      }
    }

    if (report.assertions.length !== first.assertions.length) {
      throw new Error(`${report.path}: performance assertion set drifted`)
    }
    for (const firstAssertion of first.assertions) {
      const assertion = report.assertions.find(({ name }) => name === firstAssertion.name)
      if (
        !assertion ||
        assertion.threshold !== firstAssertion.threshold ||
        assertion.operator !== firstAssertion.operator ||
        assertion.invariant !== firstAssertion.invariant
      ) {
        throw new Error(`${report.path}: performance assertion drifted: ${firstAssertion.name}`)
      }
    }
  }
}

function validateBandFraction(bandFraction) {
  if (!Number.isFinite(bandFraction) || bandFraction <= 0) {
    throw new Error('performance adjudication requires a positive confidence band')
  }
}

export function findSingleFreshReport(before, after, family) {
  const previous = new Set(before)
  const fresh = [...new Set(after)].filter((name) => !previous.has(name)).sort()
  if (fresh.length !== 1) {
    throw new Error(`expected exactly one fresh ${family} report; found ${fresh.length}`)
  }
  return fresh[0]
}

export function planAdjudication(first, { bandFraction = 0.03 } = {}) {
  validateBandFraction(bandFraction)
  validateReport(first)
  const triggeredBy = first.assertions
    .filter((assertion) => !assertion.invariant && relativeMargin(assertion) <= bandFraction)
    .map(({ name }) => name)
  return {
    triggered: triggeredBy.length > 0,
    triggeredBy,
    requiredReports: triggeredBy.length > 0 ? 3 : 1,
  }
}

export function adjudicateReports(reports, { bandFraction = 0.03 } = {}) {
  validateBandFraction(bandFraction)
  if (!Array.isArray(reports) || reports.length === 0) {
    throw new Error('performance adjudication requires at least one report')
  }
  for (const report of reports) validateReport(report)
  if (new Set(reports.map(({ path }) => path)).size !== reports.length) {
    throw new Error('performance adjudication requires distinct report paths')
  }

  const first = reports[0]
  const { triggered, triggeredBy, requiredReports } = planAdjudication(first, {
    bandFraction,
  })
  if (reports.length !== requiredReports) {
    throw new Error(
      `performance adjudication requires exactly ${requiredReports} report${requiredReports === 1 ? '' : 's'}; received ${reports.length}`,
    )
  }
  requireCoherentReports(reports)

  const assertionDecisions = first.assertions.map((firstAssertion) => {
    const assertionTriggered = triggeredBy.includes(firstAssertion.name)
    const samples = reports.map((report) => {
      const assertion = report.assertions.find(({ name }) => name === firstAssertion.name)
      return {
        path: report.path,
        observed: assertion.observed,
        threshold: assertion.threshold,
        operator: assertion.operator,
        pass: assertion.pass,
        relativeMargin: assertion.invariant ? null : relativeMargin(assertion),
        budgetPressure: assertion.invariant ? null : normalizedBudgetPressure(assertion),
      }
    })
    const failures = samples.filter(({ pass }) => !pass)
    const definitiveFailure =
      !firstAssertion.invariant &&
      failures.some(({ relativeMargin: margin }) => margin > bandFraction)
    const passed = firstAssertion.invariant
      ? failures.length === 0
      : assertionTriggered
        ? !definitiveFailure && failures.length < 2
        : failures.length === 0
    return {
      name: firstAssertion.name,
      invariant: firstAssertion.invariant,
      triggered: assertionTriggered,
      passCount: samples.length - failures.length,
      failCount: failures.length,
      definitiveFailure,
      decision: passed ? 'passed' : 'failed',
      samples,
    }
  })

  const pressureFor = (report) => {
    if (!triggered) return 0
    return Math.max(
      ...triggeredBy.map((name) =>
        normalizedBudgetPressure(report.assertions.find((assertion) => assertion.name === name)),
      ),
    )
  }
  const orderedReports = reports
    .map((report) => ({ path: report.path, budgetPressure: pressureFor(report) }))
    .sort(
      (left, right) =>
        left.budgetPressure - right.budgetPressure || left.path.localeCompare(right.path),
    )
  const selectedReport = orderedReports[Math.floor(orderedReports.length / 2)].path
  const selected = reports.find(({ path }) => path === selectedReport)
  const assertionsPassed = assertionDecisions.every(({ decision }) => decision === 'passed')
  const decision = assertionsPassed && selected.reportPassed ? 'passed' : 'failed'

  return {
    bandFraction,
    triggered,
    triggeredBy,
    requiredReports,
    decision,
    selectedReport,
    selectionPolicy: {
      kind: 'median-normalized-budget-pressure',
      assertions: triggeredBy,
      aggregation: 'maximum-pressure-across-triggering-assertions',
      ordering: 'ascending-pressure',
      tieBreak: 'report-path-ascending',
      selectedIndex: Math.floor(orderedReports.length / 2),
      orderedReports,
    },
    reports: reports.map(({ path, reportPassed, identity }) => ({
      path,
      reportPassed,
      budgetPressure: pressureFor(reports.find((report) => report.path === path)),
      ...identity,
    })),
    assertionDecisions,
  }
}
