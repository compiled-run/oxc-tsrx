# Rust/OXC core architecture

## Decision and ownership

The production language core is Rust and consumes canonical OXC crates without
copying or patching their source. JavaScript provides only thin npm, Vite+,
configuration, and editor launchers. None owns the TSRX syntax model,
projection, diagnostic mapping, or native hot path. Zig/Yuku is a read-only
performance/design oracle, not a product dependency.

`crates/oxc_adapter` is the complete OXC dependency boundary. It pins public
crates from canonical OXC commit
`8e0ed2ebb96137fb1611cdbd5742d5cb46037d40`; no other crate imports OXC. An
upstream upgrade changes this adapter and the lockfile and must pass the
behavior and performance matrices. There is no OXC fork, source snapshot,
Cargo patch, local checkout, or downstream patch queue.

All twelve adapter dependencies deliberately come from that one exact Git
source. The required linter, formatter, formatter-core, and language-server
engines are not all published as a coherent crates.io graph at this revision,
and they exchange allocator, AST, span, and syntax types with their workspace
siblings. Cargo treats the crates.io and Git instances as different packages
even when their names and versions match, so mixing sources would create
incompatible duplicate nominal Rust types at the engine boundary. A future
upgrade may move the whole graph to crates.io only when every required engine
resolves there coherently; it must never mix identities piecemeal.

## Compact TSRX overlay

`tsrx_syntax::scan` performs a byte-oriented structural scan. Its overlay uses
flat indexed vectors of fixed-width records for tokens, nodes, clauses, spans,
parent/child/sibling links, and control-header metadata. Records contain no owned
source strings, boxed AST nodes, or per-node heap graphs. Delimiter recursion
uses small inline stacks with a safe spill path; persistent vectors reserve
amortized capacity from source size.

The current overlay recognizes:

- `@{` function bodies;
- `@if`, `@else if`, and `@else`;
- `@for`, `@for await`, and `@empty`;
- `@switch`, `@case`, and `@default`;
- `@try`, `@pending`, and headerless or bound `@catch` clauses;
- declaration and assignment loop bindings;
- `index` and `key` annotations; and
- matched dynamic JSX opening/closing tags with structurally normalized
  identities;
- lowercase `<style>` elements with opaque raw payload spans; and
- statement, expression, direct JSX-child, and nested contexts.

Strings, comments, regex literals, template text/interpolation, JSX
text/attributes, and Unicode bytes are protected. The scanner rejects stale
same-length overlays by source fingerprint, catches orphan or reordered
clauses and mismatched static or dynamic JSX closing tags, and fails closed on
unsupported or incomplete grammar. Dynamic identity normalization is a bounded
streaming lexical pass: it allocates no per-tag token vector, strips only
enclosing parentheses/trivia, and retains edge comments in one flat span
table.

## Native lint path

```text
ordinary JS/JSX/TS/TSX ───────────────────────────────────────┐
                                                      ▼
TSRX bytes ── linear scan ── mapped legal TSX projection ── OXC parser
                 │                    │                       │
                 │ flat overlay       │ one buffer            │ one arena AST
                 │ original spans     │ affine identity map    ▼
                 └──────────────────────────────── semantic + oxc_linter
                                                              │
                                      identity-only label/fix translation
                                                              │
                                             original TSRX diagnostics
```

Ordinary `.js`, `.jsx`, `.ts`, and `.tsx` source bypasses `tsrx_syntax`; the
original string goes directly to the adapter and metadata proves zero scan,
projection time, and projection bytes.

TSRX control punctuation and required wrappers are synthetic legal TSX.
Authored descendant ranges are copied byte-for-byte and recorded as sorted
affine identity segments. The OXC program retains projected `source_text`, so
AST spans and source-sensitive rules address the same bytes. Every diagnostic
label must map wholly through one identity segment or the complete diagnostic
is suppressed and counted. Cross-segment, synthetic, structural, and boundary
fixes are rejected.

Dynamic names are copied into collision-namespaced JSX attributes. After the
single canonical OXC parse, `oxc_adapter` validates their real expression ASTs
during the reported parse/validation interval; no lexical approximation or
second parser is used. Paired dynamic names map diagnostic labels but reject
one-sided fixes, while self-closing name expressions can remain fixable. Raw
style payloads are replaced with synthetic markers and stay outside the
JavaScript AST, so this slice does not claim CSS linting.

The current black-box proof covers original-byte `no-debugger` and
`no-unused-vars` labels plus identity-only `no-var` fixes across the original
branch/loop controls and the switch/try families. An accepted fix is applied to
original TSRX and then scanned, projected, and reparsed before any write. Normal
non-fix lint parses once; fix validation deliberately adds one post-edit parse.
Arbitrary rule-semantic equivalence over synthetic scaffolding is not claimed.

## Opt-in type-aware lint path

The default path above is unchanged when type-aware linting is disabled: each
TSRX file has one canonical OXC parse and starts no TypeScript-Go process.
`--type-aware` and `--type-check` add a separate project lane:

```text
authored .tsrx ── shared structural scan ── type-semantic projection ─┐
                                                                     │
authored TS/TSX ─────────────────────────────────────────────────────┤
                                                                     ▼
authored-path ConfigStore resolution ── in-memory virtual source batch
                                           (`.tsrx.tsx` for TSRX)
                                                       │
                                                       ▼
                                  one documented tsgolint v2 process
                                                       │
                                                       ▼
                          path-aware labels/fixes ── authored TSRX spans
```

The type-semantic projection is intentionally distinct from the syntax-rule
projection. It retains loop element types and lexical bindings, explicit
`.tsrx` import relationships, catch-parameter context, control-flow scope, and
component callability while emitting legal TypeScript/TSX. Declarations are
appended after authored source so existing byte offsets do not move.

Each rule set and path-sensitive override resolves against the authored file
name before TSRX receives its virtual `.tsrx.tsx` script identity. The whole
mixed project is transferred through stdin as one documented protocol-v2
batch; no generated source is written to the consumer project. Ordinary
TS/TSX can join that type project but still bypasses the TSRX scanner and
projection.

The pinned public `TsGoLintState::lint_source` source-override API proves the
single-source seam. Its returned `Message` currently discards `file_path`, so
the production multi-file lane uses tsgolint's documented path-preserving
protocol v2 rather than guessing which file owns a diagnostic. The adapter
locates and verifies the official platform binary from `oxlint-tsgolint`
0.24.0. A missing or mismatched binary fails actionably; there is no silent
downgrade.

Diagnostic endpoints must map through complete authored coverage. A protocol
fix additionally has to be marked safe and occupy one exact affine identity
range, after which the edited TSRX is rescanned and reparsed before write.
Suggestions and synthetic or cross-segment edits remain visible only as
non-applicable diagnostics. This keeps type-aware fix safety identical in
spirit to the built-in Rust lint path.

## Native formatter path

```text
ordinary JS/JSX/TS/TSX ───────────────────────────────────┐
                                                             ▼
TSRX bytes ── linear scan ── marked legal TSX projection ── Oxfmt
                 │                    │                       │
                 │ flat overlay       │ one buffer            │ one arena AST
                 │ source fingerprint │ collision-free IDs    │ one parse
                 └─────────────────────────────────────────────┘
                                                                  │
                                           indexed one-forward-pass lift
                                                                  │
                                                        formatted TSRX
```

`tsrx_format::format_text` is the default-options filesystem-free library
boundary; `tsrx_format::FormatSession::format_text` is the configured
editor/batch boundary.
Ordinary files call `oxc_adapter::format` directly. Supported TSRX is projected
to one legal TSX buffer containing delimiter-safe identifiers and typed comment
markers for structural tokens, wrapper boundaries, annotated headers, dynamic
names, closing-tag comments, and opaque style payloads.

Projection construction is linear: wrapper start/end events are emitted from
the flat nesting stack, structural tokens and annotated headers are already
source-ordered, and a fixed-width action merge drives the output builder
without a general sort. Canonical Oxfmt parses and formats that buffer exactly
once. Checked end-sentinel attributes bound formatted dynamic expressions, so
regex/template braces never require a partial ad hoc expression lexer.

Lift first indexes every collision-free scaffold occurrence in one scan. It
validates identifier uniqueness, numeric identity, wrapper method/object/call
adjacency, header helper order, marker sides, nesting, and expected sentinels.
A single forward event renderer then:

- skips proven wrapper/header scaffold ranges;
- keeps authored payload in source order;
- composes dedent for nested wrappers;
- protects template raw bytes while handling `${...}` code;
- preserves UTF-8 bytes; and
- writes one output allocation.

A second forward token pass restores `@{`, `@if`, `@else`, `@for`, `@empty`,
`@switch`, `@case`, and `@default`, requiring markers to be complete, unique,
and ordered. The same indexed renderer consumes the collision-namespaced
object-method scaffold for `@try`/`@pending`/`@catch`, preserves authored catch
bindings, and restores those clauses in source order. The lifted source must
contain no namespace residue and must rescan to the same compact structural
fingerprint. OXC is not reparsed during lift.

An embedded lift pass reconstructs dynamic closing names from the formatted
opening expression, restores closing-only comments in source order, and
borrows each raw style payload directly from the original source. Files with
neither dynamic tags nor raw/scoped style skip that pass entirely; this avoids
a whole-file copy on the common path. Raw CSS bytes are preserved, not parsed,
validated, or reformatted. The
pinned `oxc_formatter_css` dependency graph currently requires a downstream
allocator Cargo patch, so it is deliberately excluded until OXC exposes a
clean one-allocator public package boundary.

The generalized performance regression is deliberately retained. The old
per-scaffold search/string-shift lift formatted 262 KiB at 0.324 MiB/s with a
1.928× normalized full/half scaling ratio. The latest indexed-renderer run
reaches 20.14 MiB/s and 1.014× while also exercising 394 dynamic tags and 197
raw style payloads. The statement-control fast path is 129.45 MiB/s.

`oxc-tsrx-fmt` supports stdin, check, transactional write, explicit multi-file
input, and optional thread count. Explicit files are read and formatted in one
parallel pipeline. All results succeed before write staging begins; recoverable
rename/staging failures restore originals, and symbolic links are rejected.

## Configured session layer

Lint and format configuration lives above the source hot path. A command or
editor host constructs configured `LintSession` and `FormatSession` values,
discovers or loads JSON/JSONC configuration once, and reuses the compiled OXC
config store or resolved formatter options for every file. The editor keeps a
diagnostic lint session separate from its fix-enabled action session so a
request can never enter a disk-writing CLI path:

```text
JSON/JSONC config ── discover/merge/compile once ── session
                                                    │
                       ┌────────────────────────────┼──────────────────────┐
                       ▼                            ▼                      ▼
                    file 1                       file 2                 file N
                 scan/project/parse           direct OXC path      scan/project/parse
```

`oxc_adapter::LintEngine` owns the canonical `ConfigStore`, `Linter`, and
ignore matcher. It resolves per-path overrides without rebuilding rules.
`tsrx_format::FormatSession` owns project-stable option values, compiled glob
sets, and the ignore matcher; the adapter alone converts them to the exact
pinned `JsFormatOptions` types. Normal source metadata excludes configuration
time, while aggregate lint metadata and dedicated benchmark summaries report
it separately.

The thin npm/Vite+ host may resolve a `vite.config.*` through Vite+'s public
`resolveConfig` API once, extract a serializable `lint` or `fmt` field, and
provide disposable JSON plus the authored config base to the native TSRX
process. Canonical ordinary-file tools reuse that JSON when the field has no
path-sensitive options; for `extends`, overrides, ignores, or plugin paths they
load the authored Vite config through the canonical supported path. This
configuration work remains outside every per-file hot path, creates no consumer
project files, and is removed after the batch.

Unsupported capabilities are rejected before source work: direct-native JS/TS
config modules, JavaScript lint plugins, `.editorconfig`, and callback-backed
or unknown TSRX-affecting formatter options. Type-aware lint is accepted only
with the explicit CLI opt-in; a direct native config cannot start the extra
process by itself. The Vite+ companion turns its resolved serializable
`typeAware` or `typeCheck` option into that explicit opt-in. This prevents a
fast path from achieving its numbers by silently dropping requested behavior.
The public/private boundary and complete option matrix are documented in
`docs/integrations/configuration.md` and `docs/integrations/vite-plus.md`.

## Performance evidence

The release-only harnesses retain applicable host/toolchain/OXC identity,
corpus hashes, raw nanosecond/RSS arrays, sample policy, distributions, and
every assertion.

Latest lint report:
`benchmarks/native-lint/results-1784242044684.json`.

| Boundary | Result | Budget |
| --- | ---: | ---: |
| Ordinary product/OXC median | 1.001× | ≤1.05× |
| Ordinary product/OXC p95 | 0.995× | ≤1.08× |
| TSRX scan/project/parse+validate median | 262.09 MiB/s | ≥75 MiB/s |
| TSRX scan/project/parse+validate p95 | 236.20 MiB/s | ≥60 MiB/s |
| TSRX/equivalent-TSX parse throughput | 0.585× | ≥0.50× |
| Warm 10 KiB scan/project/parse p95 | 0.044 ms | ≤1 ms |
| Complete CLI lint | 78.67 MiB/s | ≥35 MiB/s |
| Complete CLI TSRX/TSX latency | 1.160× | ≤1.35× |
| Fresh-process lint p95 | 3.22 ms | ≤50 ms |
| Configured batch invariant | 1 load / 1 file / 1 parse | exact |

The dedicated opt-in report is
`benchmarks/type-aware/results-1784242060765.json`.

| Boundary | Result | Budget |
| --- | ---: | ---: |
| Default syntax lint p95 | 2.62 ms | ≤10 ms |
| Single-file type-aware median / p95 | 23.69 / 25.04 ms | p95 ≤60 ms |
| Two-file project median / p95 | 23.78 / 24.46 ms | p95 ≤70 ms |
| Type-aware/default p95 ratio | 9.572× | ≤12× |
| Default path | 1 OXC parse / 0 type processes | exact |
| Type-aware batch | 1 type process | exact |

Latest formatter report:
`benchmarks/native-format/results-1784242059253.json`.

| Boundary | Result | Budget |
| --- | ---: | ---: |
| Ordinary product/Oxfmt median | 0.986× | ≤1.05× |
| Ordinary product/Oxfmt p95 | 1.004× | ≤1.08× |
| Statement-control sequential format | 129.45 MiB/s | ≥15 MiB/s |
| Generalized dynamic/style median | 20.14 MiB/s | ≥15 MiB/s |
| Generalized dynamic/style p95 | 18.37 MiB/s | ≥12 MiB/s |
| Generalized normalized scaling | 1.014× | ≤1.35× |
| Default-thread 16 MiB check p95 | 742.54 MiB/s | ≥100 MiB/s |
| Fresh stdin p95 | 3.26 ms | ≤110 ms |
| Complete-output TSRX/TSX RSS | 1.143× | ≤1.15× |
| Configured batch invariant | 1 load / 2 files / 2 parses | exact |

The formatter report also retains 30 raw samples for each sequential phase.
Their p95 times are 0.880 ms scan, 0.330 ms projection, 0.870 ms parse,
5.382 ms canonical format, and 1.123 ms checked lift. Because the RSS result is
inside the policy's 3% inconclusive band, three explicit fresh reports were
run: `results-1784242049439.json`, `results-1784242054518.json`, and
`results-1784242059253.json` all passed at 1.143076–1.143166×. The 1.15× limit did
not change.

`memory-stats` is linked only into benchmark executables, never the distributed
CLI.

The separate Vite boundary report
`benchmarks/vite/results-1784242073158.json` measures fresh companion processes:
mixed lint is 61.18 ms p95 and 1.868× canonical two-file TSX; mixed format-check
is 137.89 ms and 1.331× canonical; Vite+ 0.2.4 mixed lint is 322.65 ms. It
also asserts one native TSRX parse and zero ordinary files in the project-owned
lane. Runtime Vite build/HMR has no OXC for TSRX transform at all.

The syntax-only editor report
`benchmarks/editor/results-1784242073843.json` measures 100 fresh canonical OXC
stdio server start/initialize/open cycles plus one long-lived server over the
retained 1,300-byte Markless fixture and disposable probes. Fresh open is
2.49 ms median / 2.84 ms p95; edit-to-diagnostics, formatting, and code-action
p95 are 0.124, 0.378, and 0.195 ms. RSS is 11.14 MiB with 0 MiB growth through
a 1,000-edit soak. This boundary excludes TypeScript-Go work and VS Code
rendering.

The matched cross-tool report
`benchmarks/comparative/results-1784242094588.json` uses one byte-identical
1,000-file TSX corpus, one `no-debugger` rule, the same explicit file list, and
zero-diagnostic default output. After five warmups and twenty measured fresh
processes, median times are 660.17 ms for ESLint + typescript-eslint, 40.87 ms
for official Oxlint, and 24.99 ms for OXC for TSRX. Its separate paired 20%
TSRX workload is 26.28 ms / 1.052× the product's all-TSX lane and is not a
cross-tool comparison.

## Current boundary

The pinned read-only Markless oracle proves all 179/179 parser-valid tracked
files format, reparse, and converge, with all 12 known parser-invalid
completion fixtures rejected. It also compares every raw `<style>` payload
byte-for-byte and verifies the external Markless worktree remains unchanged.

This proves the pinned tracked grammar/formatter corpus, not Markless
compilation/runtime behavior. Separately, real Vite build, dev watcher/HMR, and
literal Vite+ 0.1.24/0.2.4 `vp build` and `vp dev` retransform matrices pass,
alongside mixed lint/format/check commands, without an OXC for TSRX runtime
transform. A real VS Code Extension Host walkthrough also proves automatic OXC
for TSRX activation on a `markless-tsrx` document, authored diagnostics,
live config-path refresh, configured format-on-save, one validation-passed
quick fix, and no external Markless writes. The retained direct-protocol matrix
separately proves malformed-buffer diagnostics and recovery.
Nested dynamic JSX inside a dynamic-name expression remains unsupported, and
raw CSS remains unformatted/unvalidated until the canonical CSS formatter has
a clean downstream package graph. JSON/JSONC configuration is implemented;
remaining compatibility includes JavaScript plugins, direct-native JS/TS
config modules, nested config, and alternate reporters. Platform artifact
contracts, deliberate OXC upgrade lanes, and public-launch preparation are
locally proven. The host artifact has been executed locally; hosted production
of all eight platform candidates, external publication, and deployment remain
approval-gated release operations.
