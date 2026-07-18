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

You need Node.js 20.19 or newer. Install both packages as dev dependencies.
The npm package names are project-specific, but the commands you get are the
familiar `oxlint` and `oxfmt`:

```sh
npm install --save-dev oxlint-tsrx oxfmt-tsrx
npx oxlint --format=json src/Counter.tsrx src/View.tsx
npx oxfmt --check src/Counter.tsrx src/View.tsx
```

For Vite+, install the same packages under the project-local names Vite+
resolves:

```sh
npm install --save-dev vite-plus \
  oxlint@npm:oxlint-tsrx \
  oxfmt@npm:oxfmt-tsrx
```

That is the whole setup. The tools are written in Rust; a normal npm install
pulls in `@oxc-tsrx/runtime`, which picks the one prebuilt native binary that
matches your platform out of eight exact native packages. You do not need
Rust on your machine, the packages run no install scripts, and the commands
never download anything later. There is no JavaScript/Wasm fallback. Until
npm shows the complete 0.1.0 set from the approval-gated registry launch,
follow the source-build path in the
[getting-started guide](docs/guide/getting-started.md); local candidate files
do not prove registry publication.

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

node tests/acceptance/run-performance.mjs
```

The fresh end-to-end owner run and every frozen performance lane are indexed
in the [release acceptance matrix](docs/acceptance/matrix.md), with links to
the machine-readable clean-room, aggregate adjudication, and raw benchmark
receipts. Individual benchmark commands remain available for diagnostic raw
runs, but the aggregate runner is the authoritative release command.

The read-only Markless oracle is pinned to committed revision
`76d0e6a07fa728b9343cc0d342fbe03813c43703`. It proves all 179/179
parser-valid tracked `.tsrx` files format, reparse, and converge, rejects all
12 known parser-invalid completion fixtures, verifies every raw style payload
byte-for-byte, and requires the external worktree fingerprint to remain
identical.

The aggregate-selected Apple M5 Pro lint report is
[`benchmarks/native-lint/results-1784321646022.json`](benchmarks/native-lint/results-1784321646022.json):
260.28 MiB/s scan/project/parse-and-validation, 80.24 MiB/s complete CLI lint,
1.141× CLI latency versus equivalent TSX, and 3.16 ms fresh-process p95.
Its configuration lane records one config load, one parse, and the configured
rule's real diagnostic.

The opt-in type-aware report is
[`benchmarks/type-aware/results-1784321661795.json`](benchmarks/type-aware/results-1784321661795.json):
22.63 ms median / 24.78 ms p95 for one TSRX file and 22.95 / 24.04 ms for a
two-file explicit-`.tsrx` import project. Both use one native tsgolint process.
The same report keeps the default syntax lane at 2.65 ms p95, one OXC parse,
and zero type processes.

The aggregate-selected formatter report is
[`benchmarks/native-format/results-1784321655592.json`](benchmarks/native-format/results-1784321655592.json):
134.78 MiB/s sequential statement-control formatting, 823.10 MiB/s p95 for a
16 MiB default-thread batch, and 21.79 MiB/s on repeated control flow plus 394
dynamic tags and 197 raw style payloads. The generalized full-versus-half
normalized scaling ratio is 1.000×, fresh stdin p95 is 3.16 ms, and
complete-output RSS is 1.143× canonical same-binary TSX. Every frozen
assertion passes without changing a threshold; the config lane records one
load, two files/two parses, and observed quote/semicolon changes.

The Vite/Vite+ command-boundary report is
[`benchmarks/vite/results-1784321678410.json`](benchmarks/vite/results-1784321678410.json):
ordinary `oxfmt-tsrx` is 103.26 ms median / 113.44 ms p95 versus canonical
Oxfmt's 100.99 / 103.01 ms (1.101× p95). Mixed companion lint is 57.91 ms
p95 (1.813× canonical two-file TSX), mixed format-check is 127.11 ms p95
(1.234× canonical), and complete Vite+ 0.2.4 mixed lint is 237.08 ms p95.
Metadata proves one native parse for the TSRX file and zero ordinary files
entering the project-owned lane.

The native editor report is
[`benchmarks/editor/results-1784321679056.json`](benchmarks/editor/results-1784321679056.json):
2.40 ms median / 2.59 ms p95 from fresh server start/initialize/open to first
diagnostics across 100 measured processes, 0.115 ms edit-to-diagnostics p95,
0.125 ms formatting p95, and 0.114 ms code-action p95 on the retained Markless
fixture plus disposable probes (1,300 bytes). One long-lived canonical OXC
stdio server used 10.98 MiB RSS and grew 0 MiB through a 1,000-edit soak. This
lane is syntax-only. The separate VS Code 1.128 Extension
Host artifact
records automatic OXC for TSRX activation on a real `markless-tsrx` document,
real format-on-save, live diagnostics, a safe quick fix, and identical
external-worktree fingerprints before and after.

The matched cross-tool CLI report is
[`benchmarks/comparative/results-1784321699288.json`](benchmarks/comparative/results-1784321699288.json):
the same byte-identical 1,000-file TSX corpus, one `no-debugger` rule, one
explicit file list, and zero-diagnostic default output, with every lane
launched through its npm CLI entry point the way projects invoke it. After
five warmups and twenty measured processes the medians were 609.06 ms for
ESLint + typescript-eslint, 40.68 ms for official Oxlint, and 45.92 ms for the
`oxlint-tsrx` command. The ordinary-only command imports the exact declared
official Oxlint launcher in the same Node process, without entering the TSRX
dispatch path. A separately labeled mixed-file-types workload (20% TSRX by
file count) measured 68.38 ms (1.489× the product's all-TSX lane); exactly one public canonical
Node child and one native TSRX child start in parallel, with zero private
adapter children. It is not used as a cross-tool comparison. The mixed ratio
landed inside the unchanged 3% near-threshold band, so the aggregate retained
three coherent runs and selected this median-pressure report rather than the
fastest sample.

## Configuration use

The commands discover JSON or JSONC Oxlint/Oxfmt configuration once per
session and reuse the compiled state across explicit `.tsrx` and ordinary JS/TS
files. With the `oxlint-tsrx` and `oxfmt-tsrx` npm packages installed:

```sh
npx oxlint --format=json src/Counter.tsrx src/View.tsx
npx oxlint --format=json --config config/lint.json \
  --warn no-console --deny no-debugger src/Counter.tsrx
npx oxlint --format=json --type-aware src/Counter.tsrx
npx oxlint --format=json --type-check src/Counter.tsrx

npx oxfmt --check src/Counter.tsrx src/View.tsx
npx oxfmt --write --config config/format.json src/Counter.tsrx
```

The same invocations work against the native Rust binaries directly (an
internal detail useful when building from source):

```sh
target/release/oxc-tsrx --format=json src/Counter.tsrx src/View.tsx
target/release/oxc-tsrx-fmt --check src/Counter.tsrx src/View.tsx
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

The companion also coexists with the official OXC VS Code extension. It
attaches only to `.tsrx` documents and talks to the project-owned
`oxc-tsrx-lsp` server, while the official extension keeps serving ordinary
JS/TS files. That split holds even when a project aliases `oxlint`/`oxfmt` to
the `oxlint-tsrx`/`oxfmt-tsrx` wrapper packages, because the wrappers load the
packages' declared canonical launchers in the same Node process for `--lsp`,
preserving the upstream stdio session. The official extension still cannot
select `.tsrx` files; the
companion exists to close exactly that gap.

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

With the `oxfmt-tsrx` npm package installed:

```sh
# Editor/stdin boundary: formatted source is written to stdout.
npx oxfmt --stdin-filepath=src/Counter.tsrx < src/Counter.tsrx

# Check without modifying files; exits 1 and lists differences.
npx oxfmt --check src/Counter.tsrx

# Format explicit files. All reads and formats finish before transactional writes.
npx oxfmt --write src/Counter.tsrx src/View.tsx
```

The native binary accepts the same flags directly, for example
`target/release/oxc-tsrx-fmt --check src/Counter.tsrx`.

Ordinary JavaScript and TypeScript files use the exact manifest-declared
canonical Oxfmt launcher in the wrapper's Node process, and the black-box
contract requires byte-for-byte output parity with zero TSRX dispatch. Mixed
batches keep that public canonical lane alongside the native TSRX child.
Explicit files are read and formatted in the same parallel pipeline; write
mode stages every successful output before replacing any original.

## Repository map

- `crates/oxc_adapter`: the only OXC revision boundary;
- `crates/tsrx_syntax`: compact native overlay plus private scanner,
  mapping, syntax-lint, type-semantic, and formatter projection/lift modules;
  the [upstream transplant map](docs/architecture/upstreaming-to-oxc.md)
  classifies what can move directly, what must adapt, and what requires an
  upstream-only redesign;
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
  (`npm run docs:build`, `docs:serve`, `docs:verify`).

Rejected JavaScript/Prettier and Zig/Yuku prototypes are absent from the
product tree; no Zig or JavaScript language core is built or shipped.

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
source fidelity, performance, update isolation, and current boundaries. The
[maintainer-facing upstream map](docs/architecture/upstreaming-to-oxc.md)
records the exact current module tree, OXC landing points, closed hooks, and
review sequence without implying upstream endorsement.
