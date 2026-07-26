---
title: Upstreaming to OXC
description: A parked, source-complete proposal for upstreaming a projected-source TSRX front end into OXC, with pinned source audits, a transplant matrix, and reproducible evidence. Not submitted and not being pursued.
---

# Upstreaming TSRX to OXC

OXC for TSRX is an independent community project. It is not affiliated with,
endorsed by, or a product of VoidZero or the OXC team. No OXC maintainer
interest or endorsement is claimed here. This page is a technical review map,
not an accepted OXC design or a copy-and-submit contribution.

## Status: a proposal that is not being pursued

Read this first, because it changes what the rest of the page means.

On 2026-07-25 the owner retired upstreaming as a premise for this project and
declined to submit anything. Nothing here has been sent to OXC, and nothing is
planned to be. This page and the patches it describes are a source-complete,
locally verified proposal that is deliberately parked.

Why keep it, then? Because writing it down was already paid for, and throwing it
away would destroy the only honest answer to "what would this take upstream?"
If OXC ever wants a projected-source TSRX front end, the audit, the transplant
matrix, and the reproducible commands are here and pinned to exact revisions.
Until then, this is a record, not a roadmap.

Nothing on this page is how the product works today. OXC for TSRX ships as one
npm package that owns the `oxlint` and `oxfmt` command names, and that name
ownership is the delivery mechanism. See
[Getting Started](../guide/getting-started.md) for what actually runs.

## What TSRX is, and what this page maps

TSRX is the `.tsrx` template syntax this repository lints and formats today by
projecting it into legal TSX and running the canonical OXC toolchain on that
projection.

This page maps what exists, classifies what could move upstream and what could
not, names the seams that are closed at an exact audited commit, and lists the
commands that reproduce every performance and correctness claim. It is written
so that a maintainer could evaluate the design from evidence rather than from
promises, if one ever chose to.

## TL;DR for OXC maintainers

- The reusable core is one crate, `crates/tsrx_syntax`, with exactly one
  direct dependency (`unicode-id-start`) and no OXC API dependency.
- It works by projection: a byte-oriented scan builds a temporary in-memory
  legal-TSX copy of the `.tsrx` file, canonical OXC parses that copy exactly
  once, and results map back onto the authored bytes.
- There is no fork. All twelve OXC dependencies sit behind one adapter crate
  at one exact commit pin.
- A twelve-row transplant matrix below classifies every responsibility as
  direct reuse, adapt or replace, standalone product glue, or upstream-only
  redesign.
- At the audited revision, OXC has no merged whole-file language hook that
  could load this front end. The related RFC, issue, and draft PR are unmerged
  research, and this page treats them that way.

The whole page in one picture. Select a node to read what it is, or step
through the buttons to walk the proposed landing order:

<!-- diagram:upstream-map -->

What this page would support, if it were ever taken up:

- a design review of a projected-source TSRX front end, using the pinned
  source audit, transplant matrix, and reproducible evidence below.

What we are not asking or claiming:

- We are not asking for that review. Submission was declined by the owner on
  2026-07-25, and no request has been made of anyone.
- We do not claim OXC maintainer interest, endorsement, or acceptance.
- We are not presenting a merge-ready patch, and this page is not a request
  to merge one. Whether and how TSRX lands upstream, if ever, is entirely an
  OXC maintainer decision.

## Audit provenance

The comparison was audited on 2026-07-16 (America/Chicago) against:

- this project's canonical OXC pin,
  [`8e0ed2ebb96137fb1611cdbd5742d5cb46037d40`](https://github.com/oxc-project/oxc/commit/8e0ed2ebb96137fb1611cdbd5742d5cb46037d40); and
- then-current OXC `main`,
  [`6fe866af3036127c2236cc1db557f086c4408905`](https://github.com/oxc-project/oxc/commit/6fe866af3036127c2236cc1db557f086c4408905).

The hook-relevant boundaries were rechecked at that exact current-main commit.
They can change later, so a real proposal must repeat the source audit rather
than treating the word “current” on this page as permanent.

## How to review this page

A suggested reading order, with a rough cost per step, so a first pass does
not require reading everything:

<!-- review-route -->

1. Read [What is reviewable today](#what-is-reviewable-today) for the core
   model, the scanner and projection module tree, and the dependency facts.
   Roughly ten minutes.
2. Scan the [transplant matrix](#transplant-matrix) to see which
   responsibilities are candidates for reuse and which are explicitly not.
   Roughly ten minutes; the classification column carries most of the signal.
3. Read [closed seams at the audited revision](#closed-seams-at-the-audited-revision)
   to confirm what this project is not pretending exists upstream. Roughly
   five minutes.
4. Read the [credible landing sequence](#credible-landing-sequence) and
   [performance invariants](#performance-invariants) to judge whether the
   proposed order of work and its performance contract are sound. Roughly ten
   minutes.
5. Optionally, reproduce the evidence with the commands in
   [reproducible evidence](#reproducible-evidence). The test and clippy steps
   take minutes; the full benchmark families take longer machine time but run
   unattended.

## What is reviewable today

The reusable core is `crates/tsrx_syntax`. Its modules are private and its
crate-root API remains deliberately small. The post-refactor tree is exactly:

```text
crates/tsrx_syntax/
  Cargo.toml
  src/
    lib.rs
    diagnostics.rs
    model.rs
    scanner/
      mod.rs
      stack.rs
      control.rs
      header.rs
      jsx.rs
      lexical.rs
      overlay.rs
    projection/
      mod.rs
      mapping.rs
      builder.rs
      lint.rs
      types.rs
      format.rs
      marker.rs
      lift/
        mod.rs
        embedded.rs
        scaffold.rs
        writer.rs
        tokens.rs
        text.rs
  tests/
    architecture.rs
    public_api.rs
```

Two words carry most of this design, so here is what they mean in terms a
Vite user already knows. Projection is a temporary in-memory legal-TSX copy
of the `.tsrx` file, like a Vite plugin transform that never hits disk: the
authored file is untouched, and canonical OXC parses the projected copy. Lift
is the reverse trip for formatting: it maps the formatted output back onto the
authored bytes, and it is checked before anything is written.

The split follows pipeline ownership rather than a line limit:

The exact high-value seams include `scanner/overlay.rs`,
`scanner/lexical.rs`, `projection/lint.rs`, and
`projection/lift/scaffold.rs`.

- `scanner/mod.rs` owns borrowed source, scanner state, construction, finish,
  source identity, and the hot dispatch loop.
- `scanner/lexical.rs` shields strings, comments, regular expressions,
  templates, numbers, identifiers, and trivia using concrete `Scanner`
  methods. There is no lexer trait or dynamic dispatch.
- `scanner/control.rs`, `header.rs`, and `jsx.rs` own TSRX grammar domains;
  `overlay.rs` owns flat-record mutation and rollback; `stack.rs` owns the
  inline/spill delimiter stack.
- `projection/mapping.rs` owns authored/projected affine mappings;
  `builder.rs` owns projection construction; and `lint.rs`, `types.rs`, and
  `format.rs` expose purpose-specific internal paths.
- `projection/marker.rs` owns collision-free marker identity. `lift/*` owns the
  checked formatter protocol, keeping the cohesive scaffold index/edit/render
  model together rather than splitting it cosmetically.
- `diagnostics.rs` owns the existing error contract; `model.rs` owns the
  compact records. Public names and signatures remain curated in `lib.rs`.

`tsrx_syntax` has exactly one direct dependency:
`unicode-id-start = "1"`. It is the standalone Unicode table used to implement
ECMAScript `IdentifierPart` semantics, including the cold non-ASCII/escape
path; it is not an OXC API dependency. All twelve canonical OXC dependencies
remain isolated behind `crates/oxc_adapter` at the one exact pin above.

## Transplant matrix

This table classifies every responsibility in the repository so a reviewer
can see at a glance what is a reuse candidate, what needs adaptation, what is
product glue that should stay out of OXC, and what would be an upstream-only
redesign. The classification matters more than matching directory names.
“Direct reuse” means the algorithm and tests can move with little conceptual
change; it does not promise a patch will compile without adopting OXC types
and conventions. Use the chips to show one classification at a time.

<!-- matrix-filter -->

| Local responsibility | Classification | Credible OXC landing | What changes in tree |
| --- | --- | --- | --- |
| Flat overlay records, checkpoints, spans, and structural fingerprint | **Direct reuse** as an initial projected-source front end | A maintainer-approved `oxc_tsrx`-style crate | Preserve indexed storage; adapt `ByteSpan` to `oxc_span::Span` only if that improves the chosen boundary. |
| Affine authored/projected mapping and lint/type projection | **Direct reuse** | A projected-source result consumed by an explicit Oxlint route | Preserve identity-segment safety; replace project-specific DTO names as needed. |
| Format projection, marker namespace, scaffold validation, and checked lift | **Direct reuse** as the low-risk compatibility strategy | A TSRX-specific crate, potentially paired with an `oxc_formatter_tsrx` strategy | Keep the protocol outside language-agnostic formatter IR; integrate app routing separately. |
| TSRX control/header grammar | **Adapt or replace** | `oxc_parser` grammar modules or a dedicated TSRX front end | Grammar cases and fixtures transfer; parser state, diagnostics, and AST construction follow the selected upstream model. |
| Lexical shielding and Unicode keyword boundaries | **Adapt or replace** | The parser lexer today, or the [incubating `oxc_lexer`](https://github.com/oxc-project/oxc/blob/8e0ed2ebb96137fb1611cdbd5742d5cb46037d40/crates/oxc_lexer/README.md) only after OXC adopts it | Retain common-case/cold-path behavior; do not introduce a generic lexer backend. |
| JSX, dynamic-tag, and raw-style recognition | **Adapt or replace** | JSX parser/lexer integration plus TSRX-owned structure | Tests and normalization rules transfer; AST and embedded-language ownership need maintainer decisions. |
| Error construction | **Adapt or replace** | `oxc_diagnostics` at parser/front-end boundaries | Preserve authored spans and failure conditions while adopting upstream diagnostics. |
| Syntax, formatter, corpus, mapping, Unicode, and layout tests | **Direct reuse** as evidence; adapt harnesses | Parser coverage, formatter conformance, allocation snapshots, and app integration tests | Keep pass/fail inputs and invariants; express them through OXC's fixture/snapshot systems. |
| `oxc_adapter`, config discovery, TypeScript-Go process protocol, and CLI orchestration | **Standalone product glue** | Usually none; small pieces may become Oxlint/Oxfmt app routing | In-tree code calls workspace APIs directly. Keep npm compatibility and release policy outside the grammar core. |
| Vite/Vite+, npm platform packages, and the official-extension LSP multiplexer | **Standalone product glue** | Separate ecosystem packages | These integrations remain useful until OXC exposes shared provider discovery and native `.tsrx` selection. |
| New TSRX AST node kinds, generated visitors, semantic traversal, and a native formatter over those nodes | **Upstream-only redesign** | `oxc_ast`, AST generators, `oxc_semantic`, `oxc_traverse`, and formatter crates | This is a larger alternative to projection, not work this repository can honestly pre-implement. |
| `.tsrx` source dispatch, linter loading, Oxfmt file/LSP routing, and official editor selection | **Upstream-only redesign** | OXC application and editor layers | These closed routes require maintainer-approved APIs and UX; adding an extension alone is insufficient. |

## Closed seams at the audited revision

OXC has no merged whole-file language hook that can load this front end. In the
audited current-main source:

- [`SourceType`](https://github.com/oxc-project/oxc/blob/6fe866af3036127c2236cc1db557f086c4408905/crates/oxc_span/src/source_type.rs)
  models JavaScript/TypeScript syntax choices and known extensions; it is not a
  third-party grammar registry.
- Oxlint's [`PartialLoader`](https://github.com/oxc-project/oxc/blob/6fe866af3036127c2236cc1db557f086c4408905/crates/oxc_linter/src/loader/partial_loader/mod.rs)
  hard-codes framework containers and returns contiguous borrowed script
  regions with fixed offsets. TSRX needs an owned whole-file projection plus
  affine authored identity segments and explicitly unmapped synthetic regions,
  so pretending it is a partial-loader case would lose fidelity.
- Oxfmt's [`FileKind` classification](https://github.com/oxc-project/oxc/blob/6fe866af3036127c2236cc1db557f086c4408905/apps/oxfmt/src/core/support.rs)
  and [Oxfmt LSP routing](https://github.com/oxc-project/oxc/blob/6fe866af3036127c2236cc1db557f086c4408905/apps/oxfmt/src/lsp/mod.rs)
  are closed application routes. Neither loads a native TSRX formatter from
  project configuration.
- [`ParserConfig`](https://github.com/oxc-project/oxc/blob/6fe866af3036127c2236cc1db557f086c4408905/crates/oxc_parser/src/config.rs)
  tunes the canonical parser; it does not replace its grammar.
  The language server's [`ToolBuilder`](https://github.com/oxc-project/oxc/blob/6fe866af3036127c2236cc1db557f086c4408905/crates/oxc_language_server/src/tool.rs)
  is a compile-time Rust embedding seam, not a runtime language loader.

There is relevant design activity, but the
[Language Plugins RFC #21936](https://github.com/oxc-project/oxc/discussions/21936),
[Oxlint custom-template research issue #19918](https://github.com/oxc-project/oxc/issues/19918),
and [draft custom-parser PR #24262](https://github.com/oxc-project/oxc/pull/24262)
are unmerged research, not runtime dependencies. As of 2026-07-24 that draft
has grown well past a bare AST hook: it now covers `parseForESLint`/`parse`
routing, `SourceCode` and token behavior, parser services and scope-manager
integration, fixes and disable directives, editor/LSP routing, and native-rule
coverage through an offset-preserving shadow source, all behind explicit
per-glob opt-in. Even so, it is Oxlint-specific and still does not solve Oxfmt,
type-aware framework files, parse/load caching, generated typed walkers, full
module-graph participation, or first-class language identity. This repository
must continue to use released, capability-probed seams until upstream chooses
and ships something else.

## Provider discovery patches, built locally

The seams above are about the language front end, which is a large design
question. There is a second, much smaller question that can be answered
separately: how does an OXC tool find out that a project has installed a
third-party language provider at all?

This project's answer is a static `oxc.provider` block in the provider
package's own `package.json`, described on the
[editor integration page](../integrations/editor.md). Three adoption patches for
that protocol now exist as source, and two of them have been built and run.

Read the status labels carefully, because they are not the same thing:

- **built and verified locally** means the patch was applied to a clone at a
  pinned public revision, compiled, and exercised by tests on this machine.
- **submitted** would mean sent to an upstream repository. Nothing is submitted,
  and submission was declined by the owner on 2026-07-25.
- **accepted** would mean merged upstream. Nothing is accepted.
- **released** would mean shipped to users. Nothing is released.

| Target | Pinned revision | Size | Diff | Status |
| --- | --- | --- | --- | --- |
| [`oxc-project/oxc`](https://github.com/oxc-project/oxc) Oxlint npm wrapper | [`a065946`](https://github.com/oxc-project/oxc/commit/a065946a8ce95eb3374e08242cd9086ab050314b) | +1463 / -10, of which 43 lines touch existing code | `docs/architecture/patches/oxlint-provider-dispatch.patch` | built, verified locally |
| [`oxc-project/oxc-vscode`](https://github.com/oxc-project/oxc-vscode) document selector | [`beaffb9`](https://github.com/oxc-project/oxc-vscode/commit/beaffb967b06db53907723cbb61712c0fa9d9dea) | +106 / -1 | `docs/architecture/patches/oxc-vscode-provider-selector.patch` | built, verified locally |
| [`voidzero-dev/vite-plus`](https://github.com/voidzero-dev/vite-plus) | [`a24eede`](https://github.com/voidzero-dev/vite-plus/commit/a24eede77ebec23b3e942437bda34f6d34a95cd3) | zero lines | none needed | verified, with a version pin caveat below |

The two diffs are committed in this repository under
`docs/architecture/patches/`, next to a short index at
`docs/architecture/patches/README.md` that repeats the pinned revisions and the
exact `git apply` commands. Each `.patch` file also carries its own target
repository and revision in a plain-text preamble, which `git apply` skips, so a
file that is copied out of the tree on its own is still reproducible.

No upstream source is vendored into this repository. A patch is a diff, not the
source it applies to, and every clone the diffs were built in lives outside this
repository and is temporary.

### What the Oxlint patch does

The npm wrapper does not spawn a native binary. It loads a NAPI addon into the
same Node process and calls `lint(args, ...callbacks)` once, at the end of
`apps/oxlint/src-js/cli.ts`. The patch sits directly above that one call.

When no provider is installed, the wrapper runs that exact statement, unchanged.
When a provider is installed, the wrapper splits the command line: paths Oxlint
already owns stay on the same call, and paths a provider claims are handed to
the capability binary the provider declares. In `--lsp` mode the wrapper instead
composes the canonical language server with each provider's server behind the
one stdio connection the editor opened.

Nothing crosses into Rust. The `lint(...)` signature is untouched, which is what
makes this a JavaScript-only change a reviewer can evaluate on its own.

The patch runs a provider's capability binary in pass-through mode: it hands the
binary the file paths and lets that binary's own output reach the user, then
takes the worst exit code. It does not ask for a machine-readable format and it
merges no reports, because the canonical half is rendered and printed inside
Rust, so the wrapper never holds it as data. That is the contract written down
in `packages/toolchain/README.md` under "Capability calling convention", and the
two now say the same thing.

### Why Vite+ needs nothing, and what that is worth today

Vite+ resolves the `oxlint` package, walks up to the package root, joins
`bin/oxlint`, and executes that path. It never reads file extensions. Its
LSP-only wrapper at `packages/cli/bin/oxlint` does the same and then imports the
result. Both therefore inherit whatever the resolved Oxlint wrapper does, so a
patch to the wrapper reaches Vite+ users with no Vite+ change at all.

That is a claim about Vite+'s code, and it holds. It is not a claim that a
released Vite+ can pick up a provider-aware wrapper today, and it does not
become one. `tests/packaging/vite-plus-provider.test.mjs` runs the real
installed Vite+ 0.2.4 binary against a project holding a provider and records
three things:

1. **Vite+ 0.2.4 pins `"oxlint": "=1.72.0"`, exactly.** An exact pin accepts one
   version and no other, so the Oxlint copy Vite+ installs for itself is frozen
   at 1.72.0. Provider dispatch exists only as a local build at 1.74.0. A
   project that installs Vite+ and nothing else therefore reaches a wrapper that
   has never heard of providers, whatever else it has installed. This is the
   qualification the zero-line claim needs, and it stays true until Vite+ moves
   its pin to a release that carries dispatch.
2. **`vp lint` prefers the project's own Oxlint.** Vite+ resolves with the
   project directory first and its own package second, so a project that
   declares its own `oxlint` is served by that copy even while Vite+'s pinned
   1.72.0 sits nested inside the Vite+ package. In the test the real `vp lint`
   binary lands on the project's wrapper, hands it every named file including
   one with a provider-claimed extension, passes its output straight through,
   and propagates its exit code. So the pass-through shape is real.
3. **The LSP wrapper does not agree with `vp lint`.** `bin/oxlint` resolves
   relative to its own package rather than to the project, so in the same
   layout it runs the pinned 1.72.0 copy while `vp lint` runs the project's.
   An editor reaching Oxlint through Vite+ therefore keeps the pinned wrapper
   even when the command line does not. Anyone proposing this upstream should
   expect that seam to need attention even though the linting seam does not.

The test also records the honest limit of a JavaScript-only dispatch patch:
when the user names a directory rather than files, the wrapper receives the
directory and the native file walker decides what to open, so provider
extensions are never enumerated.

### The strongest evidence so far, and its limit

In a workspace whose only dependency is this project's package, with
`node_modules/.bin` deleted, every tool name shadowed on `PATH`, and no
`oxc.path.*` editor setting of any kind, the released official OXC extension
found the patched Oxlint wrapper by ordinary Node resolution. That wrapper then
read the provider block and started the language server the provider declares,
confirmed against the operating system's own process table.

The limit is exactly one sentence long, and it matters. The wrapper doing the
discovering is a local source build of a patch no OXC maintainer has seen, and
none will, because submission was declined.

So treat that result as a proof that the protocol is implementable, not as a
step toward anything shipping. No released OXC tool reads `oxc.provider`, and on
current plans none will. What ships instead is the `oxlint` and `oxfmt` command
names owned by `oxc-tsrx`, which is a permanent delivery mechanism rather than a
placeholder.

## Credible landing sequence

This is the review sequence the proposal would recommend. It is not a claim that
OXC has accepted the shape, and it is not scheduled work: nothing here is being
pursued.

1. **Review the language core and evidence first.** Start with `model.rs`, the
   scanner domains, public grammar fixtures, Unicode boundary tests, and the
   compact-layout assertions. Decide whether projected legal TSX is an
   acceptable first upstream architecture before discussing app flags.
2. **Prototype a private TSRX front end.** Move or adapt scan, overlay,
   projection, mapping, and diagnostics into a maintainer-owned experimental
   crate. Keep ordinary JS/TS bypass behavior and compare allocations and
   throughput before changing representation.
3. **Add an explicit projected-source contract.** Oxlint needs authored path,
   virtual TSX, complete mapping data, and fix-safety rules. Do not squeeze this
   into `PartialLoader`'s contiguous-slice contract.
4. **Integrate one app at a time.** Prove Oxlint diagnostics/fixes first, then
   add an Oxfmt strategy around the checked format projection/lift. Only after
   those paths exist should `SourceType`, `FileKind`, command routing, and LSP
   capabilities advertise `.tsrx`.
5. **Choose native AST work separately.** If maintainers prefer first-class
   TSRX nodes over projection, scope generated AST, visitor, traversal,
   semantic, formatter, and ecosystem consequences as a redesign. Do not hide
   that work inside a “parser support” patch.
6. **Assume the delivery surfaces stay.** Earlier drafts of this page called the
   explicit package-name facades, the LSP multiplexer, and the optional legacy
   activation client compatibility surfaces that would retire once official
   tools reached parity. That retirement cannot happen now. With upstreaming
   retired as a premise, no official tool is going to cover provider discovery,
   lint, format, source mappings, and `.tsrx` activation on this project's
   behalf, so those surfaces are how the product is delivered and they are
   permanent.

## Performance invariants

An upstream experiment should preserve behavior first and measure every
representation change. The current invariants are:

- ordinary JS/JSX/TS/TSX bypasses TSRX work;
- normal TSRX lint performs one authored structural scan, one contiguous
  legal-TSX projection allocation, and one canonical OXC parse;
- TSRX format performs two structural scanner passes (the initial authored
  scan and a deliberate allocation-light post-lift scan that verifies the
  structural fingerprint) but still only one canonical OXC parse;
- the overlay borrows authored text and stores flat `u32` spans/indices rather
  than per-node owned strings or boxed graphs;
- vector capacities remain source-size based, delimiter state stays inline
  before a spill vector, and sparse dynamic/style tables stay lazy;
- exact-source fixes and formatter lift validate before write;
- ASCII keyword boundaries return inline, while raw Unicode and escapes enter
  one allocation-free cold helper; and
- a module move or upstream adapter adds no new source copies, parses,
  allocations, or dynamic dispatch without profile-backed evidence and an
  explicit new budget.

The local layout tests freeze sixteen hot-record sizes on the qualified 64-bit
toolchain. The release benchmarks separately gate native scan/projection/parse,
complete lint and format, ordinary-source parity, startup, RSS, type-aware
process cost, editor latency, and Vite/Vite+ boundaries. See the
[core performance contract](./rust-oxc-core.md#performance-evidence) and
[acceptance matrix](../acceptance/matrix.md); do not substitute one fast parser
microbenchmark for the full product path.

## Reproducible evidence

The refactor and Unicode correction were test-first and retained their
contracts. A maintainer or reviewer can begin with:

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
```

The optional product-corpus oracle requires a compatible external Markless
checkout and treats it as read-only:

```sh
MARKLESS_ROOT=/path/to/markless pnpm run test:markless-control
```

The architecture contract checks the exact private module tree, public root
API, dependency boundary, and legacy-file removal. Public tests cover the TSRX
grammar and Unicode/escaped identifier suffixes; private tests cover malformed
escape safety. The upstreamability work regenerated all six performance
families after the Unicode semantic fix, so its fresh goal evidence is not
based on pre-refactor Rust. The frozen aggregate selected those exact reports
as the synchronized launch baseline linked from the public performance pages;
the separate exact-source acceptance seal verifies the same source tree and
read-only product corpus.

The read-only Markless corpus is product evidence, not an upstream conformance
suite. An OXC proposal should port relevant inputs into OXC-owned fixtures and
add the parser, formatter, linter, allocation, and application tests appropriate
to the chosen landing points.

## OXC contribution hygiene

OXC's pinned [`AGENTS.md`](https://github.com/oxc-project/oxc/blob/8e0ed2ebb96137fb1611cdbd5742d5cb46037d40/AGENTS.md)
is the source of contribution policy for any real upstream work. It says to
avoid editing generated directories directly, use OXC's generators when a
generated surface must change, and run the relevant repository gates. For this
kind of work that includes focused tests plus `just fmt`, `just test`, parser
or formatter conformance as applicable, `just allocs` for parser allocation
changes, and `just ready` after commits.

That guide also requires contributors to disclose AI use. A human contributor
must review, test, understand, and take responsibility for everything submitted.
The evidence in this repository can reduce discovery cost, but it does not
replace OXC's current policy, maintainer design review, or upstream test suite.

## Primary OXC references

- [Pinned contributor and performance policy](https://github.com/oxc-project/oxc/blob/8e0ed2ebb96137fb1611cdbd5742d5cb46037d40/AGENTS.md)
- [Pinned parser public facade](https://github.com/oxc-project/oxc/blob/8e0ed2ebb96137fb1611cdbd5742d5cb46037d40/crates/oxc_parser/src/lib.rs)
- [Pinned parser grammar modules](https://github.com/oxc-project/oxc/blob/8e0ed2ebb96137fb1611cdbd5742d5cb46037d40/crates/oxc_parser/src/js/mod.rs)
- [Pinned parser lexer modules](https://github.com/oxc-project/oxc/blob/8e0ed2ebb96137fb1611cdbd5742d5cb46037d40/crates/oxc_parser/src/lexer/mod.rs)
- [Pinned identifier semantics](https://github.com/oxc-project/oxc/blob/8e0ed2ebb96137fb1611cdbd5742d5cb46037d40/crates/oxc_syntax/src/identifier.rs)
- [Pinned formatter architecture](https://github.com/oxc-project/oxc/blob/8e0ed2ebb96137fb1611cdbd5742d5cb46037d40/crates/oxc_formatter/AGENTS.md)
- [Audited current `SourceType`](https://github.com/oxc-project/oxc/blob/6fe866af3036127c2236cc1db557f086c4408905/crates/oxc_span/src/source_type.rs)
- [Audited current Oxlint partial-loader dispatch](https://github.com/oxc-project/oxc/blob/6fe866af3036127c2236cc1db557f086c4408905/crates/oxc_linter/src/loader/partial_loader/mod.rs)
- [Audited current Oxfmt file support](https://github.com/oxc-project/oxc/blob/6fe866af3036127c2236cc1db557f086c4408905/apps/oxfmt/src/core/support.rs)
- [Audited current Oxfmt LSP language map](https://github.com/oxc-project/oxc/blob/6fe866af3036127c2236cc1db557f086c4408905/apps/oxfmt/src/lsp/mod.rs)
