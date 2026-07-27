// Generates docs/type-error-example.json: the self-contained TSRX snippet with
// an unawaited Promise, plus the REAL oxc-tsrx --type-aware (tsgolint /
// TypeScript-Go) diagnostics for it.
//
// --type-aware, not --type-check, on purpose. --type-check would add the
// TypeScript compiler's own syntactic and semantic errors, and those are what
// an editor's language server already shows. The finding worth demonstrating
// is the type-aware lint rule, which needs type information oxlint does not
// have on its own.
//
// Why this is pre-generated: the type lane needs the tsgolint executable, and
// the published site runs the engine as WebAssembly in the browser, which
// cannot host it. Without a committed report the home hero's "Type-aware
// lint" example has nothing to show and goes silently dead. The site labels the
// replayed result as pre-generated; live runs on the local development server
// still call the real binary.
//
// Prereqs:
//   cargo build --release --locked -p oxc_tsrx_cli --bins
//   pnpm install   (provides node_modules/@oxlint-tsgolint/<platform>/tsgolint)
// Run: node docs/generate-type-error.mjs
import { execFileSync } from 'node:child_process'
import { existsSync, mkdtempSync, rmSync, writeFileSync, mkdirSync } from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { TYPE_AWARE_ANCHOR, typeAwareCode } from './demo-sources.mjs'
import {
  DEMO_TSCONFIG,
  JSX_CONTRACT,
  TYPE_PREFIX,
  TYPE_PREFIX_BYTES,
  normalizeDiagnostics,
} from './demo-type-lane.mjs'
import { resolveTsgolintExecutable } from '../scripts/tsgolint-path.mjs'

const docsDir = path.dirname(fileURLToPath(import.meta.url))
const repoRoot = path.join(docsDir, '..')
const lintBin = process.env.OXC_TSRX_LINT_BIN ?? path.join(repoRoot, 'target', 'release', 'oxc-tsrx')
const tsgolintBin = resolveTsgolintExecutable(repoRoot)

if (!existsSync(lintBin)) {
  throw new Error(`oxc-tsrx is not built at ${lintBin}: cargo build --release -p oxc_tsrx_cli --bins`)
}
if (!tsgolintBin) {
  throw new Error('tsgolint is not resolvable under node_modules/@oxlint-tsgolint: run pnpm install')
}

// The type lane infers a TypeScript program from the file's directory, so the
// scratch directory lives under the repo root rather than the OS temp dir.
const demoTmpDir = path.join(repoRoot, '.docs-demo-tmp')
mkdirSync(demoTmpDir, { recursive: true })
const requestDir = mkdtempSync(path.join(demoTmpDir, 'typegen-'))
const file = path.join(requestDir, 'demo.tsrx')

let report
try {
  writeFileSync(path.join(requestDir, 'jsx.d.ts'), JSX_CONTRACT)
  writeFileSync(path.join(requestDir, 'tsconfig.json'), DEMO_TSCONFIG)
  writeFileSync(file, TYPE_PREFIX + typeAwareCode)
  try {
    report = JSON.parse(
      execFileSync(lintBin, ['--format=json', '--type-aware', file], {
        cwd: requestDir,
        encoding: 'utf8',
      }),
    )
  } catch (error) {
    // Exit 1 means diagnostics were found. The JSON report is still complete.
    if (!error.stdout?.trim()) throw error
    report = JSON.parse(error.stdout)
  }
} finally {
  rmSync(requestDir, { recursive: true, force: true })
}

const diagnostics = normalizeDiagnostics(report.diagnostics, TYPE_PREFIX_BYTES)

// Refuse to write a report that does not demonstrate what it claims to. A
// silently empty one would put the site right back where it started, and a
// report carrying only compiler errors would be the language-server output
// this example exists to be distinct from.
if (diagnostics.length === 0) {
  throw new Error('the type-aware lane reported nothing: the example would be silent again')
}
if (!report.oxcTsrx?.typeAware) {
  throw new Error('the report is not type-aware: tsgolint did not run')
}
// Compiler diagnostics surface under the synthetic "parse-error" rule; real
// tsgolint findings carry their own rule name.
const ruleFindings = diagnostics.filter((diagnostic) => diagnostic.rule !== 'parse-error')
if (ruleFindings.length !== diagnostics.length) {
  throw new Error(
    'the report contains TypeScript compiler diagnostics; this example must show type-aware lint rules only',
  )
}
const anchorOffset = typeAwareCode.indexOf(TYPE_AWARE_ANCHOR)
const flagsAnchor = ruleFindings.some((diagnostic) =>
  diagnostic.labels.some(
    (label) =>
      label.span.offset >= anchorOffset &&
      label.span.offset < anchorOffset + TYPE_AWARE_ANCHOR.length,
  ),
)
if (!flagsAnchor) {
  throw new Error('no type-aware finding covers the unawaited call: the underline would land elsewhere')
}

const example = {
  generatedBy:
    'docs/generate-type-error.mjs, from a real oxc-tsrx --type-aware (tsgolint) run. Do not edit by hand.',
  // The chip's explanation ships with the example rather than in the eagerly
  // loaded bundle, so the budgeted home page only pays for it on click.
  note: 'saveTask returns a Promise nobody awaits. Only types reveal that, so tsgolint\u2019s no-floating-promises catches it and plain oxlint cannot.',
  pregeneratedNote:
    'saveTask returns a Promise nobody awaits, which only types reveal. tsgolint cannot run in the browser, so this is the real --type-aware report, replayed from the build.',
  tsrx: typeAwareCode,
  ruleCount: report.number_of_rules ?? null,
  parseCount: report.oxcTsrx?.parseCount ?? null,
  diagnostics,
}
writeFileSync(
  path.join(docsDir, 'type-error-example.json'),
  `${JSON.stringify(example, null, 2)}\n`,
)
console.log(
  `wrote docs/type-error-example.json (${diagnostics.length} diagnostic${diagnostics.length === 1 ? '' : 's'}: ${diagnostics.map((d) => d.code).join(', ')})`,
)
