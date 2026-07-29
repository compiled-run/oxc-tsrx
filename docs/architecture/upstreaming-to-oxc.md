---
title: Upstreaming to OXC
description: A transplant guide for OXC maintainers and contributors: what a TSRX front end would reuse, what upstream does not expose yet, and the order it could land in. Nothing has been submitted.
---

# Upstreaming TSRX to OXC

This page is for OXC maintainers and contributors evaluating whether `.tsrx`
support belongs upstream, and for anyone who wants to attempt it. It maps what
would transfer, what would have to be rewritten, and what upstream would have to
open up first, against pinned revisions.

Nothing here has been submitted to OXC and nothing is scheduled. OXC for TSRX is
an independent community project, not affiliated with or endorsed by VoidZero or
the OXC team, and no maintainer interest is claimed. What ships today is one npm
package owning the `oxlint` and `oxfmt` command names, which does not depend on
any of this.

Four facts set up the rest:

- TSRX works by making a **copy**: a scan builds a temporary in-memory TSX
  version of a file, OXC parses that copy exactly once, and results map back
  onto the authored bytes. It is the shape of a Vite plugin transform, except
  the copy never reaches disk.
- The reusable core is one crate, `crates/tsrx_syntax`. Its one direct
  dependency, `unicode-id-start = "1"`, is the standalone Unicode table behind
  ECMAScript `IdentifierPart` semantics, not an OXC API dependency.
- There is no fork. All twelve OXC dependencies sit behind `crates/oxc_adapter`
  at one exact commit pin.
- At the audited revision, OXC has no merged whole-file language hook that could
  load this front end. Audited 2026-07-16 against the pin,
  [`8e0ed2e`](https://github.com/oxc-project/oxc/commit/8e0ed2ebb96137fb1611cdbd5742d5cb46037d40),
  and then-current OXC `main`,
  [`6fe866a`](https://github.com/oxc-project/oxc/commit/6fe866af3036127c2236cc1db557f086c4408905).
  Boundaries move, so a real proposal would repeat that audit.

Select a node to read what it is, or step the buttons through the order:

<!-- diagram:upstream-map -->

## The reusable core

`crates/tsrx_syntax` keeps its modules private and its public API small. Two
ideas run through it: the **copy**, a temporary legal-TSX version OXC reads
while your file is never touched, and **mapping back**, the return trip that
puts OXC's output into TSRX and checks itself before anything is written.

| Module | Owns |
| --- | --- |
| `scanner/mod.rs` | the borrowed source, scanner state, and the main loop |
| `scanner/lexical.rs` | strings, comments, regular expressions, templates, numbers, and identifiers, as plain methods with no dynamic dispatch |
| `scanner/control.rs`, `header.rs`, `jsx.rs`, `overlay.rs`, `stack.rs` | the TSRX grammar. `scanner/overlay.rs` records scanner state and can undo it |
| `projection/mapping.rs`, `builder.rs`, `marker.rs` | which byte of the copy came from which byte of your file, building the copy, and naming its placeholders |
| `projection/lint.rs`, `types.rs`, `format.rs`, `lift/*` | one path per job. `projection/lift/scaffold.rs` is the formatter's return trip: it maps output back into TSRX and verifies it |
| `diagnostics.rs`, `model.rs` | the error shapes, and the compact records |

## What could move upstream

The classification is the point, not the directory names. "Direct reuse" means
the code and its tests could move with little conceptual change, not that a
patch would compile as-is.

<!-- matrix-filter -->

| Local responsibility | Classification | Where it could land |
| --- | --- | --- |
| Scanner records, checkpoints, spans, shape fingerprint | **Direct reuse** | An `oxc_tsrx`-style crate |
| The mapping between your file and the copy | **Direct reuse** | An Oxlint route that accepts a copy and tracks which ranges are yours |
| The formatting copy and its checked return trip | **Direct reuse** | A TSRX crate beside the language-agnostic formatter |
| TSRX control and header grammar | **Adapt or replace** | `oxc_parser` grammar modules. Test cases transfer, the rest follows upstream |
| Lexical shielding and Unicode boundaries | **Adapt or replace** | The parser lexer, or [`oxc_lexer`](https://github.com/oxc-project/oxc/blob/8e0ed2ebb96137fb1611cdbd5742d5cb46037d40/crates/oxc_lexer/README.md) once OXC adopts it |
| JSX, dynamic tags, raw `<style>` | **Adapt or replace** | JSX parser and lexer, once maintainers decide who owns the AST |
| Error construction | **Adapt or replace** | `oxc_diagnostics`, if errors keep pointing at your file |
| The test suites | **Direct reuse** as evidence | OXC's fixture and snapshot systems |
| `oxc_adapter`, config discovery, type-aware process protocol, CLI | **Standalone product glue** | Usually nowhere |
| Vite+, npm platform packages, the LSP multiplexer | **Standalone product glue** | Separate ecosystem packages |
| Native TSRX AST nodes, visitors, semantics, formatter | **Upstream-only redesign** | `oxc_ast` and friends. The alternative to the copy, and not something this repository can build in advance |
| `.tsrx` dispatch, linter loading, Oxfmt routing, editor selection | **Upstream-only redesign** | OXC application and editor layers |

## What OXC does not expose today

Every route a third-party language would need is closed in the audited source:

| Upstream boundary | What it actually is |
| --- | --- |
| [`SourceType`](https://github.com/oxc-project/oxc/blob/6fe866af3036127c2236cc1db557f086c4408905/crates/oxc_span/src/source_type.rs) | JavaScript and TypeScript syntax choices plus known extensions, not a registry a third-party grammar can join |
| Oxlint's [`PartialLoader`](https://github.com/oxc-project/oxc/blob/6fe866af3036127c2236cc1db557f086c4408905/crates/oxc_linter/src/loader/partial_loader/mod.rs) | Hard-coded framework containers returning contiguous borrowed script regions at fixed offsets |
| Oxfmt's [`FileKind` classification](https://github.com/oxc-project/oxc/blob/6fe866af3036127c2236cc1db557f086c4408905/apps/oxfmt/src/core/support.rs) and [Oxfmt LSP routing](https://github.com/oxc-project/oxc/blob/6fe866af3036127c2236cc1db557f086c4408905/apps/oxfmt/src/lsp/mod.rs) | Closed application routes. Neither loads a native TSRX formatter from project configuration |
| [`ParserConfig`](https://github.com/oxc-project/oxc/blob/6fe866af3036127c2236cc1db557f086c4408905/crates/oxc_parser/src/config.rs) | Tuning for the existing parser, not a way to replace its grammar |
| The language server's [`ToolBuilder`](https://github.com/oxc-project/oxc/blob/6fe866af3036127c2236cc1db557f086c4408905/crates/oxc_language_server/src/tool.rs) | A compile-time Rust embedding seam, not a runtime language loader |

The `PartialLoader` row matters most. It hands the linter one contiguous slice
of a file, which suits a `<script>` block inside HTML. TSRX is not like that:
the whole file becomes a copy in which some ranges are yours and some are
placeholders the tool wrote, and the two have to stay told apart. Treating it as
a partial-loader case would quietly lose accuracy.

Three unmerged research threads exist. They are research,
not runtime dependencies: [Language Plugins RFC #21936](https://github.com/oxc-project/oxc/discussions/21936),
[custom-template issue #19918](https://github.com/oxc-project/oxc/issues/19918),
and [draft custom-parser PR #24262](https://github.com/oxc-project/oxc/pull/24262),
which as of 2026-07-24 reaches past a bare AST hook into editor routing but is
still Oxlint-only.

## Provider discovery patches, built locally

A smaller question stands on its own: how would an OXC tool find out that a
project installed a third-party language provider at all? The proposed answer is
a static [`oxc.provider` block](./provider-protocol.md), and three adoption
patches exist as source. "Built, verified locally" means the patch was applied
to a clone at a pinned revision, compiled, and run against tests.

| Target | Pinned revision | Size | Diff | Status |
| --- | --- | --- | --- | --- |
| [`oxc-project/oxc`](https://github.com/oxc-project/oxc) Oxlint npm wrapper | [`a065946`](https://github.com/oxc-project/oxc/commit/a065946a8ce95eb3374e08242cd9086ab050314b) | +1463 / -10, of which 43 lines touch existing code | `docs/architecture/patches/oxlint-provider-dispatch.patch` | built, verified locally |
| [`oxc-project/oxc-vscode`](https://github.com/oxc-project/oxc-vscode) document selector | [`beaffb9`](https://github.com/oxc-project/oxc-vscode/commit/beaffb967b06db53907723cbb61712c0fa9d9dea) | +106 / -1 | `docs/architecture/patches/oxc-vscode-provider-selector.patch` | built, verified locally |
| [`voidzero-dev/vite-plus`](https://github.com/voidzero-dev/vite-plus) | [`a24eede`](https://github.com/voidzero-dev/vite-plus/commit/a24eede77ebec23b3e942437bda34f6d34a95cd3) | zero lines | none needed | verified, with a version pin caveat below |

The diffs live under `docs/architecture/patches/`, with the pinned revisions and
the exact `git apply` commands. No source is vendored here.

The Oxlint one is the substantive patch, and it stays in JavaScript: it sits
directly above the single `lint(args, ...callbacks)` call at the end of
`apps/oxlint/src-js/cli.ts`, so nothing crosses into Rust. With no provider
installed the wrapper is unchanged. With one, it sends the paths that provider
claims to its binary in pass-through mode and takes the worst exit code, and in
`--lsp` mode composes both language servers behind the editor's one stdio
connection. See [the calling
convention](./provider-protocol.md#capability-calling-convention).

Two results worth knowing before repeating the work. With `node_modules/.bin`
deleted, every tool name shadowed on `PATH`, and no `oxc.path.*` setting, the
released official OXC extension still found the patched wrapper by ordinary Node
resolution and started the provider's language server, which proves the protocol
implementable and nothing more. And Vite+ needs no patch of its own, because it
resolves the `oxlint` package and runs `bin/oxlint` from it without reading file
extensions, so patching the resolved wrapper reaches Vite+ users. That second
one holds only while Vite+ pins `"oxlint": "=1.72.0"` exactly;
`tests/packaging/vite-plus-provider.test.mjs` records the measurements, and the
limit of a JavaScript-only patch, which is that a directory argument goes to the
native walker and never enumerates provider extensions.

## Landing order

The order this proposal would suggest, if a maintainer first accepted reading a
TSX copy as a starting design. None of it is scheduled or agreed to.

1. **Prototype a private TSRX front end.** Scanner, copy builder, mapping, and
   errors in an experimental crate, with ordinary JS and TS skipping all of it,
   measured for speed and memory before anything else.
2. **Give Oxlint a real way to accept a copy.** It needs your file path, the TSX
   copy, the full mapping, and the rules for when a fix is safe. `PartialLoader`
   cannot carry that.
3. **Integrate one tool at a time.** Oxlint diagnostics and fixes, then
   formatting. Only then should `SourceType`, `FileKind`, and the language
   server advertise `.tsrx`.
4. **Decide about native AST nodes separately.** First-class TSRX redesigns node
   types, visitors, semantic analysis, and the formatter. Hiding that inside a
   "parser support" patch would be dishonest.

## Budgets any experiment has to keep

Ordinary JS, JSX, TS, and TSX files skip TSRX work entirely. Beyond that, four
budgets:

- a normal TSRX lint does one scan, one TSX copy, and one OXC parse;
- TSRX format does two structural scans, the authored one plus a cheap check
  that the result came back the same shape, and still one OXC parse;
- the scanner borrows your text and stores numbers rather than copying strings
  per node, and fixes and formatting check themselves before writing;
- moving a module upstream adds no new copies, parses, allocations, or dynamic
  dispatch without measurements and an agreed new budget.

Tests freeze the size of sixteen hot data records, and release benchmarks gate
every stage. See the [core performance
contract](./rust-oxc-core.md#performance-evidence) and the [acceptance
matrix](../acceptance/matrix.md).

## Reproducible evidence

A reviewer can rerun everything from a clean checkout:

```sh
cargo test --locked -p tsrx_syntax --test architecture
cargo test --locked -p tsrx_syntax --all-targets
cargo test --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets -- -D warnings
pnpm test
pnpm run test:packaging:unit
pnpm run benchmark:native-lint
pnpm run benchmark:native-format
pnpm run benchmark:type-aware
pnpm run benchmark:editor
node benchmarks/vite/run.mjs
pnpm run benchmark:comparative
# optional, needs a compatible external Markless checkout, read-only:
MARKLESS_ROOT=/path/to/markless pnpm run test:markless-control
```

The architecture test checks the private module tree, the public API, and the
dependency boundary. All six benchmark families were regenerated after the
Unicode fix.

## OXC contribution hygiene

Real upstream work would follow OXC's own
[`AGENTS.md`](https://github.com/oxc-project/oxc/blob/8e0ed2ebb96137fb1611cdbd5742d5cb46037d40/AGENTS.md):
its generators, its repository gates, and its rule that a human contributor
reviews, tests, understands, discloses AI use, and takes responsibility for
everything submitted. Nothing here replaces maintainer design review.
