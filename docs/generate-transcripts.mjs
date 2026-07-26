// Generates docs/terminal-transcripts.json: the named "See it run" terminal
// walkthroughs embedded on docs pages via <!-- terminal-demo:NAME -->.
// Every transcript is captured by really running the release binaries (and the
// npm wrappers) inside a throwaway sample project, so the output on the site
// is exactly what the tools printed.
// Prereqs:
//   cargo build --release --locked -p oxc_tsrx_cli --bins
//   node scripts/build-parser-native.mjs (parser addon for the parsing demo)
//   pnpm install (for the npm wrappers and the pinned oxlint-tsgolint executable)
//   jq on PATH (the JSON walkthroughs pipe through it for readable output)
import { spawnSync } from 'node:child_process'
import {
  mkdirSync,
  mkdtempSync,
  readFileSync,
  realpathSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from 'node:fs'
import { tmpdir } from 'node:os'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { resolveTsgolintExecutable } from '../scripts/tsgolint-path.mjs'

const docsDir = path.dirname(fileURLToPath(import.meta.url))
const repoRoot = path.join(docsDir, '..')
const lintBin = path.join(repoRoot, 'target', 'release', 'oxc-tsrx')
const formatBin = path.join(repoRoot, 'target', 'release', 'oxc-tsrx')
const npmLintBin = path.join(repoRoot, 'packages', 'toolchain', 'bin', 'oxlint')
const npmFormatBin = path.join(repoRoot, 'packages', 'toolchain', 'bin', 'oxfmt')
const tsgolintBin = resolveTsgolintExecutable(repoRoot)

const baseEnv = {
  ...process.env,
  OXC_TSRX_LINT_BIN: lintBin,
  OXC_TSRX_FORMAT_BIN: formatBin,
  ...(tsgolintBin ? { OXLINT_TSGOLINT_PATH: tsgolintBin } : {}),
}

// ---------- sample project files ----------

const cartTsrx = `export function Cart({ items }: Props) @{
  var total = 0;
  debugger;

  <section class="cart">
    @if (items.length > 0) {
      @for (const item of items; key item.id) {
        <Row item={item} />
      }
    } @else {
      <Empty />
    }
  </section>
}
`

// The Cart file from Getting Started with its two lint warnings fixed, but
// with the kind of messy spacing the formatter exists to clean up.
const messyCartTsrx = `export function Cart({items}:Props) @{
  <section   class="cart">
      @if (items.length>0) {
      @for (const item of items; key item.id) {
          <Row item={item}/>
      }
      } @else {
        <Empty/>
      }
  </section>
}
`

const simpleCounterTsrx = `export function Counter({ start }: { start: number }) @{
  var count = start;
  console.log("mounted");
  debugger;

  <div class="counter">
    <span>{count}</span>
  </div>
}
`

// The type-lane version: an unawaited promise for --type-aware and a wrong
// type annotation for --type-check. The triple-slash reference plus jsx.d.ts
// stand in for the framework types a real project already has installed.
const typedCounterTsrx = `/// <reference path="./jsx.d.ts" />
async function refresh(): Promise<void> {}

export function Counter({ start }: { start: number }) @{
  var count = start;
  const label: string = start;
  console.log("mounted");
  debugger;
  refresh();

  <div class="counter">
    <strong>{label}</strong>
    <span>{count}</span>
  </div>
}
`

const viewTsx = `export function View({ label }: { label: string }) {
  var seen = false
  const title = "section: " + label
  return <p title={title}>{label}</p>
}
`

const viewTsrx = `/// <reference path="./jsx.d.ts" />
import { loadItems } from "./service.tsrx";

export function View({ label }: { label: string }) @{
  const version: number = "0.1.0";
  console.log("render", label);
  debugger;
  loadItems();

  <section class="view">
    <h2>{label}</h2>
    <p>v{version}</p>
  </section>
}
`

const serviceTsrx = `export async function loadItems(): Promise<string[]> {
  return ["alpha", "beta"];
}
`

const jsxShim = `declare namespace JSX {
  interface IntrinsicElements {
    [name: string]: unknown;
  }
}

declare module "react/jsx-runtime" {
  export const Fragment: unknown;
  export function jsx(type: unknown, properties: unknown): unknown;
  export function jsxs(type: unknown, properties: unknown): unknown;
}
`

const tsconfigJson = `{
  "compilerOptions": {
    "allowImportingTsExtensions": true,
    "jsx": "preserve",
    "module": "ESNext",
    "moduleResolution": "Bundler",
    "noEmit": true,
    "strict": true,
    "target": "ESNext"
  },
  "include": ["**/*"]
}
`

// The first example on integrations/configuration.md, extended the way that
// page's type-aware section describes: the typescript plugin, an error-level
// no-floating-promises override for .tsrx files, and the triple-slash style
// rule turned off because the sample project pulls its JSX types through a
// jsx.d.ts reference instead of installed framework types.
const configurationLintConfig = `{
  "plugins": ["react", "typescript"],
  "env": { "browser": true },
  "globals": { "frameworkGlobal": "readonly" },
  "rules": {
    "no-debugger": "error",
    "eqeqeq": ["error", "always"],
    "react/jsx-no-undef": "error",
    "typescript/triple-slash-reference": "off"
  },
  "overrides": [
    {
      "files": ["**/*.tsrx"],
      "rules": {
        "no-console": "warn",
        "typescript/no-floating-promises": "error"
      }
    }
  ],
  "ignorePatterns": ["generated/**"]
}
`

// The triple-slash rule is off because the sample project pulls its JSX types
// through a reference to jsx.d.ts instead of installed framework types.
const lintingConfig = `{
  "rules": {
    "eqeqeq": ["error", "always"],
    "typescript/triple-slash-reference": "off"
  }
}
`

const tripleSlashOffConfig = `{
  "rules": {
    "typescript/triple-slash-reference": "off"
  }
}
`

// Mirrors the format example on integrations/configuration.md.
const formatConfig = `{
  "singleQuote": true,
  "semi": false,
  "printWidth": 100,
  "overrides": [
    {
      "files": ["**/*.tsrx"],
      "options": { "singleAttributePerLine": true }
    }
  ],
  "ignorePatterns": ["generated/**"]
}
`

const simpleFormatConfig = `{
  "singleQuote": true,
  "semi": false,
  "printWidth": 100
}
`

// The parsing guide's sample file. Must stay identical to the tsrx fence on
// docs/guide/parsing.md so the recorded output matches the code on the page.
const parseViewTsrx = `import { Row } from "./Row";

export function View({ items }: { items: Item[] }) @{
  <ul class="list">
    @for (const item of items; key item.id) {
      <Row item={item} />
    } @empty {
      <li>No items yet</li>
    }
  </ul>
}
`

const parseBrokenTsrx = `export function Broken() @{
  <p>hello</p
}
`

// Must stay identical to the js fence on docs/guide/parsing.md.
const parseScript = `import { readFileSync } from "node:fs";
import { parseSync } from "oxc-tsrx/parser";

const source = readFileSync("src/View.tsrx", "utf8");
const result = parseSync("src/View.tsrx", source);

console.log("errors:", result.errors.length);
console.log("imports:", result.module.staticImports.map((s) => s.moduleRequest.value));
console.log("top level:", result.program.body.map((node) => node.type));

function* walk(node) {
  if (Array.isArray(node)) {
    for (const item of node) yield* walk(item);
  } else if (node && typeof node === "object") {
    if (typeof node.type === "string") yield node;
    for (const value of Object.values(node)) yield* walk(value);
  }
}

const forNode = [...walk(result.program)].find((node) => node.type === "JSXForExpression");
console.log("found:", forNode.type);
console.log("its first line, straight from your file:");
console.log(source.slice(forNode.start, forNode.end).split("\\n")[0]);
`

const parseBrokenScript = `import { readFileSync } from "node:fs";
import { parseSync } from "oxc-tsrx/parser";

const source = readFileSync("src/Broken.tsrx", "utf8");
const result = parseSync("src/Broken.tsrx", source);

for (const error of result.errors) {
  console.log(\`\${error.severity}: \${error.message}\`);
  console.log(error.codeframe);
}
`

// ---------- runners ----------

const runners = {
  // One multi-call native binary: no subcommand lints, `fmt` formats.
  lint: { bin: lintBin },
  fmt: { bin: formatBin, prefix: ['fmt'] },
  npxLint: { bin: process.execPath, prefix: [npmLintBin] },
  npxFmt: { bin: process.execPath, prefix: [npmFormatBin] },
  cat: { bin: '/bin/cat' },
  // Plain Node scripts (used by the parser API demo).
  node: { bin: process.execPath },
  // Real shell pipelines (used to pipe JSON reports through jq).
  sh: { bin: '/bin/sh', prefix: ['-c'] },
}

function runEntry(workspace, entry) {
  const runner = runners[entry.runner]
  const result = spawnSync(runner.bin, [...(runner.prefix ?? []), ...entry.args], {
    cwd: workspace,
    encoding: 'utf8',
    env: baseEnv,
    ...(entry.stdinFile
      ? { input: readFileSync(path.join(workspace, entry.stdinFile), 'utf8') }
      : {}),
  })
  if (result.error) throw result.error
  if (result.signal) {
    throw new Error(`${entry.command} exited on signal ${result.signal}`)
  }
  if (result.status !== entry.expectExit) {
    throw new Error(
      `${entry.command} exited ${result.status}, expected ${entry.expectExit}\n${result.stdout}${result.stderr}`,
    )
  }
  return `${result.stdout}${result.stderr}`
}

function sanitize(output, workspace) {
  const real = realpathSync(workspace)
  return output
    .replaceAll(`${real}${path.sep}`, '')
    .replaceAll(`${workspace}${path.sep}`, '')
    .replaceAll(real, '.')
    .replaceAll(workspace, '.')
}

function captureDemo(demo) {
  const workspace = mkdtempSync(path.join(tmpdir(), 'oxc-tsrx-docs-demo-'))
  try {
    for (const [relative, contents] of Object.entries(demo.files)) {
      const absolute = path.join(workspace, relative)
      mkdirSync(path.dirname(absolute), { recursive: true })
      writeFileSync(absolute, contents)
    }
    // Symlinks let a demo resolve real workspace packages (for example
    // node_modules/oxc-tsrx) without copying them into the sample.
    for (const [relative, target] of Object.entries(demo.links ?? {})) {
      const absolute = path.join(workspace, relative)
      mkdirSync(path.dirname(absolute), { recursive: true })
      symlinkSync(target, absolute)
    }
    return {
      caption: demo.caption,
      transcript: demo.entries.map((entry) => ({
        ...(entry.comment ? { comment: entry.comment } : {}),
        command: entry.command,
        output: sanitize(runEntry(workspace, entry), workspace),
      })),
    }
  } finally {
    rmSync(workspace, { recursive: true, force: true })
  }
}

// ---------- demo definitions ----------

const demos = {
  'introduction-commands': {
    caption:
      'Real output, captured at build time. The sample project has a src/Counter.tsrx with a debugger statement and an unformatted src/View.tsx with an unused variable.',
    files: {
      'src/Counter.tsrx': simpleCounterTsrx,
      'src/View.tsx': viewTsx,
    },
    entries: [
      {
        comment: 'Lint .tsrx and ordinary JS/TS with real OXC rules',
        command: 'npx oxlint src/Counter.tsrx src/View.tsx',
        runner: 'npxLint',
        args: ['src/Counter.tsrx', 'src/View.tsx'],
        expectExit: 0,
      },
      {
        comment: 'Format .tsrx and ordinary JS/TS with real Oxfmt layout',
        command: 'npx oxfmt --check src/Counter.tsrx src/View.tsx',
        runner: 'npxFmt',
        args: ['--check', 'src/Counter.tsrx', 'src/View.tsx'],
        expectExit: 1,
      },
    ],
  },

  'getting-started-format-write': {
    caption:
      'Real output, captured at build time. The sample is the src/Cart.tsrx from above with its two warnings fixed but sloppy spacing left behind.',
    files: { 'src/Cart.tsrx': messyCartTsrx },
    entries: [
      {
        comment: 'The warnings are fixed, but look at the spacing',
        command: 'cat src/Cart.tsrx',
        runner: 'cat',
        args: ['src/Cart.tsrx'],
        expectExit: 0,
      },
      {
        comment: 'Rewrite the file in place; printing nothing means it worked',
        command: 'npx oxfmt --write src/Cart.tsrx',
        runner: 'npxFmt',
        args: ['--write', 'src/Cart.tsrx'],
        expectExit: 0,
      },
      {
        comment: 'Same file, now in canonical Oxfmt layout',
        command: 'cat src/Cart.tsrx',
        runner: 'cat',
        args: ['src/Cart.tsrx'],
        expectExit: 0,
      },
    ],
  },

  'getting-started-native': {
    caption:
      'Real output, captured at build time, for the src/Cart.tsrx file above. The native binary prints the whole report as one line of JSON; jq makes it readable here.',
    files: { 'src/Cart.tsrx': cartTsrx },
    entries: [
      {
        comment: 'The report is one line of JSON; jq picks out the diagnostics',
        command: "target/release/oxc-tsrx --format=json src/Cart.tsrx | jq '.diagnostics'",
        runner: 'sh',
        args: [`"${lintBin}" --format=json src/Cart.tsrx | jq '.diagnostics'`],
        expectExit: 0,
      },
      {
        comment: 'And the metadata proves it parsed your file exactly once',
        command:
          "target/release/oxc-tsrx --format=json src/Cart.tsrx | jq '.oxcTsrx.parseCount'",
        runner: 'sh',
        args: [`"${lintBin}" --format=json src/Cart.tsrx | jq '.oxcTsrx.parseCount'`],
        expectExit: 0,
      },
    ],
  },

  'linting-usage': {
    caption:
      'Real output, captured at build time. The sample src/Counter.tsrx has a debugger statement, a var declaration, an unawaited promise, and a wrong type annotation; src/View.tsx has an unused variable.',
    files: {
      'src/Counter.tsrx': typedCounterTsrx,
      'src/View.tsx': viewTsx,
      'src/jsx.d.ts': jsxShim,
      'tsconfig.json': tsconfigJson,
      'config/lint.json': lintingConfig,
      '.oxlintrc.json': tripleSlashOffConfig,
    },
    entries: [
      {
        comment: 'The report is one line of JSON; jq shows the diagnostics readably',
        command:
          "oxc-tsrx --format=json src/Counter.tsrx src/View.tsx \\\n  | jq '.diagnostics'",
        runner: 'sh',
        args: [
          `"${lintBin}" --format=json src/Counter.tsrx src/View.tsx | jq '.diagnostics'`,
        ],
        expectExit: 0,
      },
      {
        comment: 'Explicit configuration plus per-rule severity from the CLI',
        command:
          "oxc-tsrx --format=json --config config/lint.json \\\n  --warn no-console --deny no-debugger src/Counter.tsrx | jq '.diagnostics'",
        runner: 'sh',
        args: [
          `"${lintBin}" --format=json --config config/lint.json --warn no-console --deny no-debugger src/Counter.tsrx | jq '.diagnostics'`,
        ],
        expectExit: 0,
      },
      {
        comment: 'Apply safe fixes; here no-var rewrites var to const',
        command:
          "oxc-tsrx --format=json --deny no-var --fix src/Counter.tsrx | jq '.oxcTsrx.fixes'",
        runner: 'sh',
        args: [
          `"${lintBin}" --format=json --deny no-var --fix src/Counter.tsrx | jq '.oxcTsrx.fixes'`,
        ],
        expectExit: 0,
      },
      {
        comment: 'Opt into the official TypeScript-Go rules',
        command: "oxc-tsrx --format=json --type-aware src/Counter.tsrx | jq '.diagnostics'",
        runner: 'sh',
        args: [`"${lintBin}" --format=json --type-aware src/Counter.tsrx | jq '.diagnostics'`],
        expectExit: 0,
      },
      {
        comment: 'Or add full TypeScript compiler diagnostics on top',
        command: "oxc-tsrx --format=json --type-check src/Counter.tsrx | jq '.diagnostics'",
        runner: 'sh',
        args: [`"${lintBin}" --format=json --type-check src/Counter.tsrx | jq '.diagnostics'`],
        expectExit: 0,
      },
    ],
  },

  'formatting-usage': {
    caption:
      'Real output, captured at build time. The sample src/Counter.tsrx starts with double quotes and no statement semicolons after JSX, so the formatter has work to do.',
    files: {
      'src/Counter.tsrx': simpleCounterTsrx,
      'src/View.tsx': viewTsx,
      'config/format.json': simpleFormatConfig,
    },
    entries: [
      {
        comment: 'Check without modifying files; exits 1 and lists files that differ',
        command: 'oxc-tsrx-fmt --check src/Counter.tsrx',
        runner: 'fmt',
        args: ['--check', 'src/Counter.tsrx'],
        expectExit: 1,
      },
      {
        comment: 'Format and write files (silent on success)',
        command: 'oxc-tsrx-fmt --write src/Counter.tsrx src/View.tsx',
        runner: 'fmt',
        args: ['--write', 'src/Counter.tsrx', 'src/View.tsx'],
        expectExit: 0,
      },
      {
        comment: 'Editor/stdin mode: formatted source goes to stdout',
        command: 'oxc-tsrx-fmt --stdin-filepath=src/Counter.tsrx < src/Counter.tsrx',
        runner: 'fmt',
        args: ['--stdin-filepath=src/Counter.tsrx'],
        stdinFile: 'src/Counter.tsrx',
        expectExit: 0,
      },
      {
        comment: 'Explicit config and worker count',
        command:
          'oxc-tsrx-fmt --write --config config/format.json --threads=4 src/Counter.tsrx',
        runner: 'fmt',
        args: [
          '--write',
          '--config',
          'config/format.json',
          '--threads=4',
          'src/Counter.tsrx',
        ],
        expectExit: 0,
      },
      {
        comment: 'The explicit config switched the file to single quotes, no semicolons',
        command: 'cat src/Counter.tsrx',
        runner: 'cat',
        args: ['src/Counter.tsrx'],
        expectExit: 0,
      },
    ],
  },

  'configuration-lint': {
    caption:
      'Real output, captured at build time by running the release binaries against the sample project described above. The type-aware and type-check runs are filtered to the diagnostics each flag adds.',
    files: {
      'src/View.tsrx': viewTsrx,
      'src/View.tsx': viewTsx,
      'src/service.tsrx': serviceTsrx,
      'src/jsx.d.ts': jsxShim,
      'tsconfig.json': tsconfigJson,
      '.oxlintrc.json': configurationLintConfig,
      'config/lint.json': configurationLintConfig,
    },
    entries: [
      {
        comment: 'Discovered .oxlintrc.json: console is a warning, debugger an error',
        command:
          "oxc-tsrx --format=json src/View.tsrx src/View.tsx \\\n  | jq '.diagnostics'",
        runner: 'sh',
        args: [`"${lintBin}" --format=json src/View.tsrx src/View.tsx | jq '.diagnostics'`],
        expectExit: 0,
      },
      {
        comment: 'Explicit config path plus CLI severity overrides',
        command:
          "oxc-tsrx --format=json --config config/lint.json \\\n  --warn no-console --deny no-debugger src/View.tsrx | jq '.diagnostics'",
        runner: 'sh',
        args: [
          `"${lintBin}" --format=json --config config/lint.json --warn no-console --deny no-debugger src/View.tsrx | jq '.diagnostics'`,
        ],
        expectExit: 0,
      },
      {
        comment:
          'One TypeScript-Go process covers the whole explicit batch; showing only what --type-aware adds',
        command:
          'oxc-tsrx --format=json --type-aware src/View.tsrx src/service.tsrx \\\n  | jq \'[.diagnostics[] | select(.code | startswith("typescript"))]\'',
        runner: 'sh',
        args: [
          `"${lintBin}" --format=json --type-aware src/View.tsrx src/service.tsrx | jq '[.diagnostics[] | select(.code | startswith("typescript"))]'`,
        ],
        expectExit: 0,
      },
      {
        comment:
          '--type-check additionally lands compiler diagnostics on your authored bytes; showing only those',
        command:
          'oxc-tsrx --format=json --type-check src/View.tsrx src/service.tsrx \\\n  | jq \'[.diagnostics[] | select(.code | startswith("typescript(TS"))]\'',
        runner: 'sh',
        args: [
          `"${lintBin}" --format=json --type-check src/View.tsrx src/service.tsrx | jq '[.diagnostics[] | select(.code | startswith("typescript(TS"))]'`,
        ],
        expectExit: 0,
      },
    ],
  },

  'configuration-format': {
    caption:
      'Real output, captured at build time by running the release binaries against the sample project described above.',
    files: {
      'src/View.tsrx': viewTsrx,
      'src/View.tsx': viewTsx,
      'src/service.tsrx': serviceTsrx,
      'src/jsx.d.ts': jsxShim,
      '.oxfmtrc.json': formatConfig,
      'config/format.json': formatConfig,
    },
    entries: [
      {
        comment: 'Both sample files differ from the configured single-quote layout',
        command: 'oxc-tsrx-fmt --check src/View.tsrx src/View.tsx',
        runner: 'fmt',
        args: ['--check', 'src/View.tsrx', 'src/View.tsx'],
        expectExit: 1,
      },
      {
        comment: 'Rewrite one file with the explicit config; silent means success',
        command: 'oxc-tsrx-fmt --write --config config/format.json src/View.tsrx',
        runner: 'fmt',
        args: ['--write', '--config', 'config/format.json', 'src/View.tsrx'],
        expectExit: 0,
      },
      {
        comment: 'Stdin mode prints the formatted source, single quotes and all',
        command: 'oxc-tsrx-fmt --stdin-filepath=src/View.tsrx < src/View.tsrx',
        runner: 'fmt',
        args: ['--stdin-filepath=src/View.tsrx'],
        stdinFile: 'src/View.tsrx',
        expectExit: 0,
      },
    ],
  },

  'parsing-quickstart': {
    caption:
      'Real output, captured at build time. The sample project has the src/View.tsrx and parse.mjs from above, a Broken.tsrx with an unterminated closing tag, and oxc-tsrx installed.',
    files: {
      'src/View.tsrx': parseViewTsrx,
      'src/Broken.tsrx': parseBrokenTsrx,
      'parse.mjs': parseScript,
      'parse-broken.mjs': parseBrokenScript,
    },
    links: {
      'node_modules/oxc-tsrx': path.join(repoRoot, 'packages', 'toolchain'),
    },
    entries: [
      {
        comment: 'Run the parse script from above against the good file',
        command: 'node parse.mjs',
        runner: 'node',
        args: ['parse.mjs'],
        expectExit: 0,
      },
      {
        comment: 'Parse errors land in result.errors and point at your file',
        command: 'node parse-broken.mjs',
        runner: 'node',
        args: ['parse-broken.mjs'],
        expectExit: 0,
      },
    ],
  },
}

const captured = {}
for (const [name, demo] of Object.entries(demos)) {
  captured[name] = captureDemo(demo)
  console.log(`captured ${name} (${captured[name].transcript.length} commands)`)
}

writeFileSync(
  path.join(docsDir, 'terminal-transcripts.json'),
  `${JSON.stringify(
    {
      note: 'Generated by docs/generate-transcripts.mjs by running the real release binaries. Do not edit by hand.',
      demos: captured,
    },
    null,
    2,
  )}\n`,
)
console.log(`wrote docs/terminal-transcripts.json (${Object.keys(captured).length} demos)`)
