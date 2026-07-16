# OXC for TSRX

OXC for TSRX is an independent community project. It is not affiliated with,
endorsed by, or a product of VoidZero or the OXC team.

OXC for TSRX is a Rust-native, non-fork integration that makes `.tsrx` source
usable with canonical OXC engines while OXC remains independently maintained.
It carries no OXC source snapshot, project-local checkout, Cargo patch, fork,
vendor tree, or downstream patch queue. Cargo fetches and compiles one exact
canonical OXC revision as a dependency.

The current vertical slice supports `@{`, `@if`/`@else if`/`@else`,
`@for`/`@empty`, `@switch`/`@case`/`@default`, and
`@try`/`@pending`/`@catch` in statement, direct JSX-child, nested, and
expression positions. Loop support includes `for await`, declaration and
assignment bindings, and `index`/`key` annotations. Catch clauses support the
headerless, error-binding, and error-plus-reset-binding forms. Matched dynamic
JSX tags (`<{expression}>`) and lowercase raw `<style>` elements are also
recognized. The implementation provides:

- one byte-oriented Rust scan into flat indexed overlay records;
- one legal-TSX projection allocation on the TSRX path;
- exactly one canonical OXC parse at pinned commit
  `8e0ed2ebb96137fb1611cdbd5742d5cb46037d40` during normal lint or format;
- dynamic-tag validation by walking that same canonical OXC AST, with no
  second parser;
- mapped real OXC lint diagnostics at original UTF-8 TSRX byte spans;
- an opt-in Rust-native type-semantic projection and one official TypeScript-Go
  process per batch for type-aware rules and compiler diagnostics;
- identity-only safe fixes followed by a validation reparse;
- one native canonical-OXC language server for live authored diagnostics,
  formatting, and validated quick fixes;
- checked Oxfmt layout and lift back to TSRX;
- a direct ordinary JS/TS/JSX/TSX path with no TSRX scan or projection; and
- fail-closed behavior for grammar that is not implemented yet.

All revision-specific OXC calls are isolated in `crates/oxc_adapter`. Installed
artifacts pin one coherent OXC crate set. A deliberate OXC upgrade contains API
changes in that adapter, refreshes the lockfile, distribution metadata, and
legal provenance, and must pass the behavior and performance matrices before
release.

## Install

After the approval-gated 0.1.0 registry launch, install the two command
companions. Their npm package names are project-specific; their executable
names remain `oxlint` and `oxfmt`:

```sh
npm install --save-dev oxlint-tsrx@0.1.0 oxfmt-tsrx@0.1.0
npx oxlint --format=json src/Counter.tsrx src/View.tsx
npx oxfmt --check src/Counter.tsrx src/View.tsx
```

For Vite+, install the same packages under the project-local names Vite+
resolves:

```sh
npm install --save-dev vite-plus@0.2.4 \
  oxlint@npm:oxlint-tsrx@0.1.0 \
  oxfmt@npm:oxfmt-tsrx@0.1.0
```

`@oxc-tsrx/runtime` selects one of eight exact native packages transitively.
There is no lifecycle download and no JavaScript/Wasm fallback. Until npm shows
the complete 0.1.0 set, follow the source-build path in the
[getting-started guide](docs/guide/getting-started.md); local candidate files do
not prove registry publication.

## Current proof

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --all-targets --locked
cargo build --release --locked -p oxc_tsrx_cli --bins
npm test
npm run build:editor
npm run test:editor
npm run test:editor:vscode
npm run test:packaging:unit
npm run test:packaging:clean
npm run test:packaging:matrix
npm run test:packaging:vscode
npm run licenses:check

MARKLESS_ROOT=/Users/jacksm5pro/dev/open-source/markless \
  OXFMT_BIN=target/release/oxc-tsrx-fmt \
  node --test tests/markless-control-corpus.test.mjs

cargo run --release --locked -p oxc_tsrx_benchmark -- \
  --assert benchmarks/native-lint/budgets.json
cargo run --release --locked -p oxc_tsrx_format_benchmark -- \
  --assert benchmarks/native-format/budgets.json

node benchmarks/vite/run.mjs
npm run benchmark:type-aware
npm run benchmark:editor
npm run benchmark:comparative
```

The fresh end-to-end owner run and every frozen performance lane are indexed
in the [release acceptance matrix](docs/acceptance/matrix.md), with links to
the machine-readable clean-room and raw benchmark receipts.

The read-only Markless oracle is pinned to committed revision
`76d0e6a07fa728b9343cc0d342fbe03813c43703`. It proves all 179/179
parser-valid tracked `.tsrx` files format, reparse, and converge, rejects all
12 known parser-invalid completion fixtures, verifies every raw style payload
byte-for-byte, and requires the external worktree fingerprint to remain
identical.

The latest retained Apple M5 Pro lint report is
[`benchmarks/native-lint/results-1784242044684.json`](benchmarks/native-lint/results-1784242044684.json):
262.09 MiB/s scan/project/parse-and-validation, 78.67 MiB/s complete CLI lint,
1.160× CLI latency versus equivalent TSX, and 3.22 ms fresh-process p95.
Its configuration lane records one config load, one parse, and the configured
rule's real diagnostic.

The opt-in type-aware report is
[`benchmarks/type-aware/results-1784242060765.json`](benchmarks/type-aware/results-1784242060765.json):
23.69 ms median / 25.04 ms p95 for one TSRX file and 23.78 / 24.46 ms for a
two-file explicit-`.tsrx` import project. Both use one native tsgolint process.
The same report keeps the default syntax lane at 2.62 ms p95, one OXC parse,
and zero type processes.

The latest formatter report is
[`benchmarks/native-format/results-1784242059253.json`](benchmarks/native-format/results-1784242059253.json):
129.45 MiB/s sequential statement-control formatting, 742.54 MiB/s p95 for a
16 MiB default-thread batch, and 20.14 MiB/s on repeated control flow plus 394
dynamic tags and 197 raw style payloads. The generalized full-versus-half
normalized scaling ratio is 1.014×, fresh stdin p95 is 3.26 ms, and
complete-output RSS is 1.143× canonical same-binary TSX. Every frozen
assertion passes without changing a threshold; the config lane records one
load, two files/two parses, and observed quote/semicolon changes.

The Vite/Vite+ command-boundary report is
[`benchmarks/vite/results-1784242073158.json`](benchmarks/vite/results-1784242073158.json):
mixed companion lint is 61.18 ms p95 (1.868× canonical two-file TSX), mixed
format-check is 137.89 ms p95 (1.331× canonical), and complete Vite+ 0.2.4
mixed lint is 322.65 ms p95. Metadata proves one native parse for the TSRX file
and zero ordinary files entering the project-owned lane.

The native editor report is
[`benchmarks/editor/results-1784242073843.json`](benchmarks/editor/results-1784242073843.json):
2.49 ms median / 2.84 ms p95 from fresh server start/initialize/open to first
diagnostics across 100 measured processes, 0.124 ms edit-to-diagnostics p95,
0.378 ms formatting p95, and 0.195 ms code-action p95 on the retained Markless
fixture plus disposable probes (1,300 bytes). One long-lived canonical OXC
stdio server used 11.14 MiB RSS and grew 0 MiB through a 1,000-edit soak. This
lane is syntax-only. The separate VS Code 1.128 Extension
Host artifact
records automatic OXC for TSRX activation on a real `markless-tsrx` document,
real format-on-save, live diagnostics, a safe quick fix, and identical
external-worktree fingerprints before and after.

The matched cross-tool CLI report is
[`benchmarks/comparative/results-1784242094588.json`](benchmarks/comparative/results-1784242094588.json):
the same byte-identical 1,000-file TSX corpus, one `no-debugger` rule, one
explicit file list, and zero-diagnostic default output measured 660.17 ms for
ESLint + typescript-eslint, 40.87 ms for official Oxlint, and 24.99 ms for OXC
for TSRX after five warmups and twenty measured processes. A separately labeled
paired workload with 20% TSRX measured 26.28 ms (1.052× the product's all-TSX
lane); it is not used as a cross-tool comparison.

## Configuration use

The native commands discover JSON or JSONC Oxlint/Oxfmt configuration once per
session and reuse the compiled state across explicit `.tsrx` and ordinary JS/TS
files:

```sh
target/release/oxc-tsrx --format=json src/Counter.tsrx src/View.tsx
target/release/oxc-tsrx --format=json --config config/lint.json \
  --warn no-console --deny no-debugger src/Counter.tsrx
target/release/oxc-tsrx --format=json --type-aware src/Counter.tsrx
target/release/oxc-tsrx --format=json --type-check src/Counter.tsrx

target/release/oxc-tsrx-fmt --check src/Counter.tsrx src/View.tsx
target/release/oxc-tsrx-fmt --write --config config/format.json src/Counter.tsrx
```

Lint supports built-in rules/plugins and their options, `env`, `globals`,
settings, JSON/JSONC extends, overrides, ignores, warning policy, CLI severity
precedence, and safe fixes. `--type-aware` adds official tsgolint rules;
`--type-check` also reports TypeScript syntactic/semantic diagnostics. Both use
in-memory `.tsrx.tsx` virtual files and preserve authored `.tsrx` override and
import semantics. Formatting supports the public core JS/TSX layout options,
overrides, ignores, stdin, check, and transactional writes. Unsupported JS
plugins, direct-native JS/TS config modules,
`.editorconfig`, and callback-backed formatter features fail loudly instead of
being ignored. The thin npm companions additionally resolve serializable
Vite+ `lint` and `fmt` fields and preserve the authored base for object extends,
overrides, and ignores. See
[the exact configuration matrix](docs/integrations/configuration.md) and
[Vite/Vite+ integration](docs/integrations/vite-plus.md).

## Vite and Vite+

Framework plugins keep complete ownership of TSRX runtime compilation, CSS,
source maps, and HMR. OXC for TSRX deliberately adds no Vite transform or
parser. Real Vite 8.1.5 build/dev/HMR tests pass with the published TSRX React
plugin.

Project-local `oxlint` and `oxfmt` npm aliases can point to `oxlint-tsrx` and
`oxfmt-tsrx`. Vite+ then routes ordinary files to canonical
Oxlint/Oxfmt and `.tsrx` to the native Rust commands. Untouched tarballs now
pass empty-consumer matrices on the supported Vite+ minimum 0.1.24 and current
0.2.4 for literal `vp build`, `vp dev` retransform, lint, format-check, and
`check --fix`, with no source-tree binary override. Markless-pinned 0.1.20
remains an isolated legacy compatibility control and is not part of the
supported or audited dependency graph.

The release manifest defines `oxlint-tsrx`, `oxfmt-tsrx`,
`@oxc-tsrx/runtime`, and one exact `@oxc-tsrx/native-*` optional package for
each of eight macOS, Linux glibc/musl, and Windows targets. Artifact contracts
are checked for all eight targets; the host package and VSIX are built and
executed locally, while the hosted eight-runner candidate build remains a
post-push release gate. Native packages contain the three stripped Rust
commands, checksums, the exact OXC revision, and a generated locked license
inventory; they have no install script. Local package and clean-install
evidence is retained under `tests/packaging`. Registry availability is not
claimed until an approval-gated publication.

## Visual Studio Code

`oxc-tsrx-lsp` hosts the existing Rust lint and format sessions behind
canonical OXC's language-server transport. The thin `packages/vscode`
companion launches that native process and attaches to file-backed `.tsrx`
documents without registering a competing framework language.

In a Markless workspace it coexists with the real `markless-tsrx` extension:
Markless keeps its grammar, TypeScript plugins, completions, navigation, and
runtime compilation, while OXC for TSRX adds format-on-save, live authored-span
diagnostics, and identity-mapped validation-passed quick fixes. Incomplete
source publishes a parse diagnostic, returns no formatting edit, and recovers
when the buffer becomes valid. Fix-all, suggestions, and dangerous actions are
not advertised.

The real VS Code 1.128 Extension Host walkthrough uses a disposable exact copy
of a Markless control-flow fixture. A second packaging walkthrough installs the
actual platform-targeted VSIX, clears native overrides, and proves that its
embedded Rust server supplies authored diagnostics, real format-on-save, and a
safe quick fix. Both record zero external writes in
[`tests/editor/markless-vscode-walkthrough.json`](tests/editor/markless-vscode-walkthrough.json)
and
[`tests/packaging/installed-vsix-report.json`](tests/packaging/installed-vsix-report.json).
See [Editor integration](docs/integrations/editor.md) for settings, architecture,
proof commands, and current packaging boundaries.

## Formatter use

```sh
# Editor/stdin boundary: formatted source is written to stdout.
target/release/oxc-tsrx-fmt --stdin-filepath=src/Counter.tsrx < src/Counter.tsrx

# Check without modifying files; exits 1 and lists differences.
target/release/oxc-tsrx-fmt --check src/Counter.tsrx

# Format explicit files. All reads and formats finish before transactional writes.
target/release/oxc-tsrx-fmt --write src/Counter.tsrx src/View.tsx
```

Ordinary JavaScript and TypeScript files go directly to canonical Oxfmt, and
the black-box contract requires byte-for-byte output parity. Explicit files are
read and formatted in the same parallel pipeline; write mode stages every
successful output before replacing any original.

## Repository map

- `crates/oxc_adapter`: the only OXC revision boundary;
- `crates/tsrx_syntax`: compact native overlay plus distinct syntax-lint,
  type-semantic, and formatter projection/lift paths;
- `crates/tsrx_format`: reusable configured formatting session and
  filesystem-free formatting boundary;
- `crates/tsrx_lint`: lint orchestration, diagnostic translation, and safe fixes;
- `crates/oxc_tsrx_cli`: native `oxc-tsrx`, `oxc-tsrx-fmt`, and
  `oxc-tsrx-lsp` commands;
- `packages/{runtime,oxlint,oxfmt,vscode}`: thin project-local npm, Vite+, and
  editor shells;
- `crates/oxc_tsrx_{benchmark,format_benchmark}`: release performance gates;
- `tests/fixtures/{lint,format,control,editor}`, `tests/native-*.test.mjs`, and
  `tests/editor`: black-box and real Extension Host contracts;
- `benchmarks/native-{lint,format}`, `benchmarks/type-aware`, and
  `benchmarks/{vite,editor}`: frozen native and ecosystem-boundary budgets and
  reports;
- `docs/`: markdown documentation and the vanilla-JS static docs site
  (`npm run docs:build`, `docs:serve`, `docs:verify`); and
- `docs/goals/oxc-for-tsrx`: internal GoalBuddy board and durable receipts.

Rejected JavaScript/Prettier and Zig/Yuku prototypes are absent from the
product tree. Historical measurements remain only in GoalBuddy notes; no Zig
or JavaScript language core is built or shipped.

## Current boundaries

This is not the complete TSRX or Vite+ integration. Oxfmt lays out surrounding
TSRX/JSX, but bytes inside raw `<style>` remain exact and are neither
CSS-formatted nor CSS-validated. The pinned OXC CSS formatter currently needs
downstream allocator source unification through a Cargo patch; this repository
deliberately omits it to preserve the no-patch, one-revision boundary. Dynamic
tag expressions containing nested dynamic JSX are not yet supported, and
malformed or incomplete editor syntax still fails closed. Arbitrary lint-rule
equivalence is not claimed over synthetic control scaffolding; this slice
proves named standard-descendant rules and suppresses synthetic or
cross-segment labels.

JavaScript lint plugins remain deliberately unclaimed until OXC exposes a
stable public one-parse plugin host. Alternate reporters,
nested configs, CLI glob/directory walking, callback-backed formatter features,
and `.editorconfig` also remain. Vite/Vite+ build/HMR and command resolution are
now directly proven. Editor activation, format-on-save, authored diagnostics,
safe code actions, and a real disposable-copy Markless walkthrough are also
proven. Platform npm packages, clean-install discovery, OXC upgrade lanes,
embedded-CSS policy, the launch-ready static artifact, and the complete final
clean-room acceptance run are locally proven.
Registry, Marketplace, repository, website, and social publication remain
separate approval-gated external actions.

See [the Rust/OXC core architecture](docs/architecture/rust-oxc-core.md) for
source fidelity, performance, update isolation, and current boundaries.
