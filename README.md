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

You need Node.js 20.19 or newer. Install the one public toolchain package:

```sh
npm install --save-dev oxc-tsrx
npx oxlint --format=json src/Counter.tsrx src/View.tsx
npx oxfmt --check src/Counter.tsrx src/View.tsx
```

The package also exposes the parser as `oxc-tsrx/parser`, lint and format APIs,
helpers for authoring custom JavaScript lint plugins (helpers to write one, not
a native host that runs one against `.tsrx`; see
[Custom JavaScript plugins](docs/integrations/custom-js-plugins.md)), and
`oxc-tsrx-lsp`.

The tools are written in Rust. `oxc-tsrx` is one package that carries the
parser, lint, format, and language-server entry points itself, and it lists
eight platform packages as `optionalDependencies` so that a normal install
downloads exactly one prebuilt native binary, the one matching your platform.
You do not need Rust, no package runs an install script, and no command
downloads anything later. There is no JavaScript/Wasm
fallback. Until npm shows the complete 0.1.0 set from the approval-gated
registry launch, follow the source-build path in the
[getting-started guide](docs/guide/getting-started.md); local candidate files
do not prove registry publication.

### The minimum steps, per host

Measured against the published `oxc-tsrx` 0.1.0 installed from the registry into
a clean project on darwin-arm64:

| Where you use it | Steps | What you run |
| --- | --- | --- |
| Command line (`oxlint`, `oxfmt`) | 1 | `npm install --save-dev oxc-tsrx` |
| Editor, through released `oxc.oxc-vscode` | 1 | the same install, and nothing else |
| Vite+ (`vp lint`, `vp fmt`) | 2 | the same install, then `oxc-tsrx setup` |

The Vite+ second step repeats after every clean dependency install. No row asks
for a config file, an ignore file, or a lifecycle script.

Three things a first run is likely to raise, none of which is a broken install:

- **A bare `npx oxlint` also lints `node_modules`.** In a scratch project made
  with `npm init -y` that measured 9260 warnings, 9257 of them from
  `node_modules`. Official Oxlint from the same install produces the same wall,
  so this is canonical Oxlint parity. A `.gitignore` containing `node_modules`
  removes it completely with no git repository required, and naming a path
  (`npx oxlint src`) avoids it too. The same scope decides what `--fix` may
  rewrite: measured at 15 files inside `node_modules` for this package and 13
  for official Oxlint. `oxfmt` skips `node_modules` unless you pass
  `--with-node-modules`.
- **`npx oxc-tsrx status` prints three `missing` lines and exits 0.** It
  inspects the Vite+ compatibility facades only, so that is the correct state
  for command-line and editor users. Use `npx oxc-tsrx providers` to check that
  TSRX support is wired up.
- **A `tsgolint` command appears in `node_modules/.bin`.** It is not this
  project's. It comes with the `oxlint-tsgolint` dependency, the official
  type-aware runner behind `--type-aware`, and you never call it directly.

[Getting Started](docs/guide/getting-started.md) has the same material with the
full transcripts.

### Install-only provider discovery

That install is the whole consumer action. `oxc-tsrx` declares a static
`oxc.provider` block in its own `package.json`, and a host that performs
provider discovery reads that JSON to find which files this package owns
(`.tsrx`) plus the parser, linter, formatter, and language server to use for
them.

There is no second step. No activation command, no dependency alias, no root
`overrides` block, no install script, no `PATH` entry, and nothing written into
`node_modules` after the install finishes. Delete `node_modules`, run a frozen
reinstall, and discovery works again for the same reason it worked the first
time: `oxc-tsrx` is still a direct dependency in your `package.json`. Nothing is
imported or spawned to be discovered; a host only resolves and reads
`package.json` files.

Inspect what a host would find in your project, without changing anything:

```sh
npx oxc-tsrx providers --json
```

**Status: local reference implementation and proof.** The hosts that read this
metadata are the ones in this repository: the `oxlint --lsp` multiplexer
`oxc-tsrx` ships, and this repository's own VS Code client. Of the four declared
capabilities only `lsp` has a host today, so do not read the declaration as four
working integrations.

Discovery itself is proven from clean consumers on npm 11.12.1, pnpm 10.33.2,
Bun 1.3.14, and Yarn Berry 4.9.2 on both the node-modules and Plug'n'Play
linkers, in `tests/packaging/provider-matrix.test.mjs`. Every lane deletes its
install tree, reinstalls frozen, and must reproduce a byte-identical index.
Windows and Yarn Classic are not covered.

**`oxc.provider` is a protocol proposed to OXC.** It is a source-complete
proposal and nothing more: nothing has been submitted upstream, nothing has been
accepted, and no released Oxlint, Oxfmt, Vite+, or `oxc.oxc-vscode` build reads
`oxc.provider` metadata. It is recorded because it is the right long-term shape,
not because anything depends on it. The full contract, for TSRX or for any other
provider, is in [the package README](packages/toolchain/README.md).

### How the install actually reaches released OXC tools

The command names are the mechanism, not a stopgap.

**The `oxlint` and `oxfmt` command names.** `oxc-tsrx` declares bins under those
names because released Vite+ and the released OXC editor extension select tools
by literal package and binary name. That name ownership is how `npx oxlint` and
`npx oxfmt` above reach TSRX from a plain install, and it is what the released
official OXC extension follows too.

This is deliberate and it is what ships. Earlier drafts of this README described
those names as debt to delete once a released host discovered providers. That is
no longer an honest description. Upstream patching was retired as a premise, so
no released host is going to start reading `oxc.provider`, and the command names
are what delivers the product.

One consequence to know about: a project that pins official `oxlint` itself gets
official Oxlint for that command name. `.tsrx` is then reachable through
`oxc-tsrx-lint` and `oxc-tsrx-fmt`, which are always installed. Breaking a
pinned setup would be worse than that extra name.

**Editors need no setup step.** With `oxc-tsrx` installed and nothing else done,
the released `oxc.oxc-vscode` extension (measured at 1.59.0) gives `.tsrx`
diagnostics that refresh on unsaved edits, formatting, and applied native quick
fixes, while ordinary TypeScript stays on canonical Oxlint. Measured on
darwin-arm64 in a real editor session.

**`oxc-tsrx setup` is only for Vite+.** Vite+ resolves the *package* name
`oxlint`, not a command, and a bin cannot satisfy that. This project cannot
legitimately publish a package under that name, so Vite+ users install normally
and then run one explicit command:

```sh
pnpm add -D vite-plus oxc-tsrx
pnpm exec oxc-tsrx setup
```

Run both lines with your own package manager. `npx` is right only when npm *is*
your package manager, because `vp create` writes a `devEngines.packageManager`
block and npm exits with `EBADDEVENGINES` in a project that names a different
one. Any modern package manager version works; Corepack is not required.

`setup` is explicit, idempotent, reversible with `oxc-tsrx remove`, and
never edits `package.json`. Because `node_modules` is disposable, run it again
after a clean dependency install. This one command is a real limitation of the
Vite+ path, and it is the only place an install alone is not enough.

## Current proof

Run `pnpm run build:native` before the lanes that need a release binary. It runs
the same `cargo build --release --locked -p oxc_tsrx_cli --bins` and then writes
`target/release/oxc-tsrx-fmt` as a verified copy of it. The crate builds one
executable named `oxc-tsrx`, which picks its tool from `argv[0]` or a leading
subcommand, so that copy is how a caller that can only name a file still reaches
the formatter. A bare `cargo build` does not create it.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --all-targets --locked
pnpm run build:native
pnpm test
pnpm run build:editor
pnpm run test:editor
pnpm run test:editor:official-toolchain
pnpm run test:editor:vscode
pnpm run test:packaging:unit
pnpm run test:packaging:clean
pnpm run test:packaging:matrix
pnpm run test:packaging:vscode
pnpm run licenses:check

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
the ordinary `oxfmt` command supplied by `oxc-tsrx` is 103.26 ms median /
113.44 ms p95 versus canonical Oxfmt's 100.99 / 103.01 ms (1.101× p95).
Mixed toolchain lint is 57.91 ms
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
public `oxlint` command supplied by `oxc-tsrx`. The ordinary-only command
imports the exact declared
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
files. With `oxc-tsrx` installed:

```sh
npx oxlint --format=json src/Counter.tsrx src/View.tsx
npx oxlint --format=json --config config/lint.json \
  --warn no-console --deny no-debugger src/Counter.tsrx
npx oxlint --format=json --type-aware src/Counter.tsrx
npx oxlint --format=json --type-check src/Counter.tsrx

npx oxfmt --check src/Counter.tsrx src/View.tsx
npx oxfmt --write --config config/format.json src/Counter.tsrx
```

The same invocations work against the native Rust binary directly (an internal
detail useful when building from source). One executable carries all three
tools, and a leading `fmt` or `lsp` selects one; with neither, it lints:

```sh
target/release/oxc-tsrx --format=json src/Counter.tsrx src/View.tsx
target/release/oxc-tsrx fmt --check src/Counter.tsrx src/View.tsx
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
being ignored. The toolchain's thin internal command adapters additionally
resolve serializable Vite+ `lint` and `fmt` fields and preserve the authored
base for object extends, overrides, and ignores. See
[the exact configuration matrix](docs/integrations/configuration.md) and
[Vite/Vite+ integration](docs/integrations/vite-plus.md).

## Vite and Vite+

Framework plugins keep complete ownership of TSRX runtime compilation, CSS,
source maps, and HMR. OXC for TSRX deliberately adds no Vite transform or
parser. Real Vite 8.1.5 build/dev/HMR tests pass with the published TSRX React
plugin.

Vite+ is the one integration an install alone cannot serve. Released Vite+
resolves its lint and format tools by literal *package* name and reads no
`oxc.provider` metadata, so `oxc-tsrx setup` creates exact, reversible
project-local facades for its current `oxlint` and `oxfmt` resolvers. Vite+ then
routes ordinary files to canonical Oxlint/Oxfmt and `.tsrx` to the native Rust
commands. Rerun `setup` after a clean dependency install, because `node_modules`
is disposable. The step would stop being needed only if Vite+ resolved tools by
command name or read provider metadata, and neither is something this project
controls.

Untouched tarballs pass empty-consumer matrices on the supported
Vite+ minimum 0.1.24 and current 0.2.4 for literal `vp build`, `vp dev`
retransform, lint, format-check, and `check --fix`, with `oxc-tsrx` as the only
direct TSRX dependency and no source-tree binary override.

The release manifest defines nine npm packages: the public `oxc-tsrx` package
and one `@oxc-tsrx/native-*` package for each of eight macOS, Linux glibc/musl,
and Windows targets. Artifact contracts are checked for all eight targets;
hosted candidate production remains a post-push release gate. Each native
package contains one stripped multi-call Rust executable that serves lint,
format, and the language server, plus checksums, the exact OXC revision, and a
generated locked license inventory; they have no install script. Registry
availability is not claimed until an approval-gated publication. The publish
procedure is [the publish runbook](docs/releasing/publish-runbook.md).

## Visual Studio Code

Install the released official OXC extension (`oxc.oxc-vscode`). It selects the
project-local `oxlint` command supplied by `oxc-tsrx` by that literal name, not
by reading provider metadata. That name selection is how this path works, and it
is what ships.

For `oxlint --lsp`, the public package multiplexes canonical Oxlint for ordinary
JS/TS and the native `oxc-tsrx-lsp` server for `.tsrx`, then dynamically
registers TSRX
document sync, diagnostics, formatting, and quick fixes with the official
client. Request IDs and document traffic remain isolated between the two
servers. No companion or forked extension is required.

That multiplexer is one of the hosts that does read `oxc.provider`: it registers
only the extensions discovered from your installed providers, and with no
provider installed it is a plain passthrough to canonical Oxlint. So the
released extension reaches it by name, and it routes by discovery.

The current official extension does not include `.tsrx` in its activation
events. In a TSRX-only workspace, open any JavaScript, TypeScript, or JSON file
once to activate it. The older `packages/vscode` client remains an optional
legacy path for automatic `.tsrx`-only activation, not the primary install.

In a Markless workspace it coexists with the real `markless-tsrx` extension:
Markless keeps its grammar, TypeScript plugins, completions, navigation, and
runtime compilation, while OXC for TSRX adds format-on-save, live authored-span
diagnostics, and identity-mapped validation-passed quick fixes. Incomplete
source publishes a parse diagnostic, returns no formatting edit, and recovers
when the buffer becomes valid. Fix-all, suggestions, and dangerous actions are
not advertised.

The released-official-extension proof installs untouched tarballs into a clean
consumer that declares only `oxc-tsrx`; it verifies canonical TypeScript
diagnostics plus native TSRX diagnostics, unsaved edits, formatting, and a safe
quick fix with the legacy companion absent. Separate retained legacy-client
walkthroughs use a disposable Markless fixture and record zero external writes
in
[`tests/editor/markless-vscode-walkthrough.json`](tests/editor/markless-vscode-walkthrough.json)
and
[`tests/packaging/installed-vsix-report.json`](tests/packaging/installed-vsix-report.json).
See [Editor integration](docs/integrations/editor.md) for settings, architecture,
proof commands, and current packaging boundaries.

## Formatter use

With `oxc-tsrx` installed:

```sh
# Editor/stdin boundary: formatted source is written to stdout.
npx oxfmt --stdin-filepath=src/Counter.tsrx < src/Counter.tsrx

# Check without modifying files; exits 1 and lists differences.
npx oxfmt --check src/Counter.tsrx

# Format explicit files. All reads and formats finish before transactional writes.
npx oxfmt --write src/Counter.tsrx src/View.tsx
```

The native binary accepts the same flags directly under its `fmt` subcommand,
for example `target/release/oxc-tsrx fmt --check src/Counter.tsrx`.

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
- `crates/oxc_tsrx_cli`: one native executable, `oxc-tsrx`, that carries the
  linter, formatter, and language server and selects one from `argv[0]` or a
  leading `lint`/`fmt`/`lsp` subcommand;
- `packages/toolchain`: the single public `oxc-tsrx` package. It owns the
  parser, lint, format, and language-server entry points, every published
  command name, and the compatibility `setup` command;
- `packages/native`: the source of the eight `@oxc-tsrx/native-*` platform
  packages, which are the only other published names;
- `packages/tsrx-core-compat`: an unpublished `@tsrx/core` facade used by the
  Markless drop-in tests;
- `packages/vscode`: optional legacy editor client; the primary path uses the
  released official OXC extension;
- `crates/oxc_tsrx_{benchmark,format_benchmark}`: release performance gates;
- `tests/fixtures/{lint,format,control,editor}`, `tests/native-*.test.mjs`, and
  `tests/editor`: black-box and real Extension Host contracts;
- `benchmarks/native-{lint,format}`, `benchmarks/type-aware`, and
  `benchmarks/{vite,editor}`: frozen native and ecosystem-boundary budgets and
  reports;
- `docs/`: markdown documentation and the vanilla-JS static docs site
  (`pnpm run docs:build`, `docs:serve`, `docs:verify`).

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
