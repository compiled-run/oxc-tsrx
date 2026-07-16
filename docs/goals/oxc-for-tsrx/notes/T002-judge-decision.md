# T002 Judge decision: native OXC for TSRX

Date: 2026-07-15 (America/Chicago)

Decision: **approve the goal and approve a pinned native OXC fork/distribution as
the product architecture.** The broader outcome is not complete. T003 is approved
only as the first native lint vertical slice; all later formatter, Vite+, editor,
packaging, Markless, and clean-room work remains required.

## Why this architecture wins

The user wants `.tsrx` to feel like `.tsx` across oxlint, oxfmt, Vite+, and the
editor while retaining OXC-class speed. Current source evidence eliminates a
configuration-only or ordinary Vite-plugin solution: both oxlint framework loading
and oxfmt external file routing are fixed source-level lists, Vite+ resolves the
installed tool binaries directly, and oxc-vscode omits the TSRX extension/language
selector.

The least invasive architecture that can still satisfy every oracle clause is:

1. Base this repository on pinned OXC source
   `129b759131ec60294bfcc0f388a45264c8740507` (crate line 0.140.0,
   rust-version 1.95.0; current app controls oxlint 1.74.0/oxfmt 0.59.0).
2. Add TSRX as a native whole-file source type and add only the AST nodes/fields
   required to represent original TSRX syntax losslessly.
3. Parse directly into OXC's arena, generate all visitors/AST kinds/transfer tables,
   and teach semantic/CFG behavior explicitly. Native lint and native format each
   parse once and consume that AST; no decoded ESTree or compiled-TSX reparse exists
   in those operations.
4. Extend the normal oxlint and oxfmt CLI/LSP paths and preserve upstream behavior
   for ordinary JS/TS/JSX/TSX by delegating to unchanged branches.
5. Package scoped project-owned distributions whose package layout and `bin` keys
   satisfy Vite+'s exact `oxlint`/`oxfmt` resolution. Test pnpm alias/override
   installation against Markless's Vite+ 0.1.20 and the supported current Vite+.
6. Provide a thin `ripple`/`.tsrx` editor client/selector using the native tool LSPs.
   Keep the existing TSRX Volar service for TS language intelligence.
7. Use Ripple as the normative grammar/compiler oracle, Yuku as a native grammar
   and data-layout reference/differential parser, and the TSRX Prettier suite as a
   formatting behavior oracle. They are not production runtime dependencies.

The repository is a maintained downstream patch stack, not a claim that stock OXC
supports TSRX. Every version string and document must identify both the upstream
OXC revision and the OXC-for-TSRX patch version.

## Explicit stock-OXC boundary

- Upstream/stock `oxlint`, `oxfmt`, `oxc-parser`, and oxc-vscode remain unable to
  parse/select `.tsrx` until corresponding changes land upstream.
- Users receive OXC rules, configuration, diagnostics/fix engine, formatter engine,
  CLI/LSP protocols, and performance architecture through project-owned binaries.
- Vite/Vite+ compilation continues through the framework's existing TSRX Vite
  transform. OXC for TSRX does not pretend that a Vite transform extends lint or
  format.
- Type-aware tsgolint is initially unsupported on raw `.tsrx`. Existing Volar
  virtual TSX remains the type-check/completion path. A future mapped tsgolint path
  may be added only after source-fidelity and performance tests pass; type-aware
  fixes stay disabled until then.

## Rejected alternatives

| Alternative | Decision | Reason |
| --- | --- | --- |
| Compile TSRX to TSX, lint stock output, remap | Rejected as primary | Adds a parse, changes rule-visible control flow, makes fixes/format unsafe, and creates editor/source-map latency. It may exist later only as diagnostics-only fallback with fixes disabled. |
| Yuku packed parser converted into OXC nodes | Rejected as primary | Fast front end, but still creates two AST representations, conversion/allocation work, dual AST maintenance, and source/fix risk before OXC rules/formatter can run. Retain Yuku as oracle/benchmark. |
| Add `.tsrx` to oxlint partial loader | Rejected | Partial loaders only extract already-valid JS/TS slices. TSRX is interleaved and still fails the stock parser. |
| Add TSRX Prettier plugin to oxfmt external bridge | Rejected as final | Could bootstrap formatting but is hard-coded, JS-AST based, 20x+ slower than native controls on the local corpus, and currently crashes on valid BigInt input. |
| Split ESLint/Prettier for TSRX, OXC elsewhere | Rejected as product | Valid incumbent fallback, but does not deliver first-class OXC/Vite+ commands or native formatter/linter performance. |
| Wait for upstream parser plugins | Rejected | No current public hook or delivery timeline and it does not satisfy the requested implementation. |

## Frozen acceptance matrix

Each row needs an owned test/artifact. An upstream test or inferred architecture is
supporting evidence only.

| ID | Oracle clause / claim | Required owned proof |
| --- | --- | --- |
| A01 | Native TSRX grammar and original locations | Parser corpus with all Ripple/Yuku constructs, invalid/recovery cases, UTF-8 byte spans and UTF-16 LSP conversions; provenance-recorded Markless copies; differential snapshots where AST shapes are comparable. |
| A02 | Native semantics | Scope/symbol/reference/CFG tests for code blocks, branches, loops, switch/try, JSX, dynamic tags, lazy patterns, imports, and nested functions; ordinary TSX semantic snapshots unchanged. |
| A03 | Oxlint rules/config/plugins | Black-box CLI and library tests for default/native/JS-plugin rules, nested config, ignores, module resolution, disable directives, visitor keys/tokens, JSON/unix reporters, and exit codes on `.tsrx`. |
| A04 | Diagnostics and safe fixes at TSRX locations | Seeded violations assert rule id/message/severity/byte range/line-column/LSP UTF-16 range; safe fix reparses and changes only expected original bytes; unproven custom-node fixes are suppressed. |
| A05 | Native oxfmt | Snapshots for every custom construct and comment/style edge; parse-after-format, compile-equivalence, first-pass convergence, option handling, stdin/check/write, BigInt regression, and full copied Markless idempotence. |
| A06 | Vite and plugin composition | Real framework fixture builds with existing TSRX plugin plus pre/post plugins, sourcemaps, HMR invalidation, dev/build/Rolldown outputs, and no filename-pattern regression. |
| A07 | Vite+ commands | Fresh installs exercise `vp lint`, `vp fmt`, `vp check`, and `vp pack` on Vite+ 0.1.20 and current supported release; prove the resolved executables are OXC-for-TSRX and ordinary files remain upstream-compatible. |
| A08 | Editor behavior | Protocol tests for both LSPs plus VS Code integration on language id `ripple`: activation, didOpen/didChange diagnostics, format-on-save edits, code action/fix-all, config reload, cancellation, multi-root, and no duplicate formatter registration. |
| A09 | Type-system boundary | Volar/virtual-TSX type diagnostics continue to map correctly; any tsgolint support gets separate mapped diagnostics/fix/performance tests. Documentation clearly states the unsupported raw tsgolint boundary until green. |
| A10 | Installable packaging | Platform package/launcher tests, local tarball install in npm/pnpm/yarn layouts, package alias/override recipe, clean binary discovery, version provenance, license/notices, and no source checkout dependency. |
| A11 | Performance contract | Retained harness/raw JSON/build flags/hardware; upstream controls before/after; parse/lint/format/CLI/NAPI/RSS/editor edit-soak gates below all pass. |
| A12 | Markless real-world proof | Full read-only corpus inventory plus disposable-copy `lint`, safe-fix, format-twice, Vite/Vite+, LSP/editor walkthrough; external Git status fingerprints unchanged. |
| A13 | Honest docs and maintenance | Compatibility table distinguishes stock/project-owned behavior, lists versions/known limits, documents install/update/rebase flow, and maps every performance claim to comparable evidence. |
| A14 | Clean checkout | A new temporary checkout/install runs the documented build and complete acceptance matrices without undeclared local absolute dependencies. |

## Frozen performance contract

### Pins and corpora

- Upstream OXC control: source SHA
  `129b759131ec60294bfcc0f388a45264c8740507`, crates/parser 0.140.0,
  oxlint 1.74.0, oxfmt 0.59.0.
- Language oracles: Ripple
  `03a98fd2a230ab5853808a44ff024568d68142fb`, Yuku
  `bf03e146d97ae2f0c2d4c4ec90456e1e544d2760`, Markless
  `fdcb833616c609385419c6b810069ac7df6ba4dd`.
- Primary real corpus: 176 mutually valid Markless `.tsrx` files, 180,484 bytes.
  Invalid/editor-recovery fixtures are a separate correctness/latency bucket.
- Size buckets: <=2 KiB, 2-10 KiB, 10-100 KiB, >=100 KiB synthetic stress; a
  7,239-byte real Markless file is the warm editor reference.
- Equivalent TSX is compiler output from the pinned authoritative TSRX transform,
  retained with its exact bytes/hash; compare only boundaries producing the same
  class of output.

### Numeric pass/fail budgets

| Gate | Frozen budget |
| --- | --- |
| P01 ordinary OXC fast path | On unchanged JS/TS/JSX/TSX corpora, candidate median throughput may regress <=5% and p95 latency <=8% versus the same-build upstream control for parser, oxlint, and oxfmt. Diagnostic/format output must otherwise match the accepted upstream snapshot. |
| P02 native TSRX parse | Valid Markless corpus >=75 MiB/s median and >=60 MiB/s at p95; candidate throughput also >=50% of equivalent-TSX parse throughput. Warm 10 KiB parse p95 <=1 ms. |
| P03 native TSRX lint | One thread >=35 MiB/s median; default threads >=70 MiB/s median; end-to-end latency <=1.35x equivalent emitted TSX after subtracting neither startup nor I/O. Diagnostic production is included. |
| P04 native TSRX format | Sequential >=15 MiB/s median; batched/default-thread >=100 MiB/s median; >=10x the pinned incumbent TSRX Prettier 1.66 MiB/s median. Formatted string production is included. |
| P05 cold CLI | Oxlint one-file p95 <=50 ms and <=1.25x upstream control. Oxfmt stdin one-file p95 <=110 ms and <=1.25x upstream control. Version/config loading remains in the timing. |
| P06 warm editor | After server init, ordinary <=10 KiB edit-to-diagnostics p50 <=10 ms and p95 <=25 ms; format request p50 <=5 ms and p95 <=15 ms; safe code-action round-trip p95 <=25 ms; initial open-to-diagnostics p95 <=100 ms. |
| P07 memory | Native peak RSS on copied Markless <= min(upstream equivalent-TSX x1.25, upstream +32 MiB). After 1,000 ordinary edits and forced quiescence, steady RSS growth <=8 MiB. No monotonic per-edit growth. |
| P08 Node/NAPI if shipped | Ordinary OXC raw-transfer throughput regression <=8%. Fully materialized TSRX AST >=25 MiB/s median. Lazy APIs are measured both untouched and after a full traversal; labels must not call lazy output a full AST. |
| P09 Vite/build integration overhead | Merely enabling OXC-for-TSRX config/adapter adds <=3% median to the same existing TSRX Vite build; compiler transform cost is reported separately and is not attributed to OXC. |
| P10 forbidden boundaries | Production lint/format/LSP traces show exactly one whole-file native parse per operation, no full-AST JSON serialization, no eager JS AST, and no source-sized duplicate copy except the required owned output string. Any exception requires a new Judge decision and replacement budget. |

### Measurement/noise policy

1. Record host/OS/toolchain, upstream and project SHAs, build profile/features,
   corpus hash, bytes, output boundary, threads, warmups, samples, and raw timings.
2. Use release/LTO settings matching distributed binaries. Debug results never
   satisfy a budget.
3. Sandwich candidate runs between two upstream controls on the same idle host;
   compare to their geometric/median control. Do not compare different machines.
4. Throughput: >=5 warmups and >=15 samples. Editor latency: >=20 warmups and
   >=100 samples. Cold start: >=20 fresh processes. RSS: >=5 fresh processes plus
   the 1,000-edit soak.
5. A result within 3% of a threshold is inconclusive and reruns with >=30 throughput
   samples or >=300 editor samples. A budget fails when two of three full reruns
   fail. Correctness failure always fails immediately.
6. Report p50, p95, p99 where relevant, not only the fastest run. Retain raw JSON;
   never replace historical raw results in place.
7. A benchmark or implementation change invalidating comparison requires a Judge
   note and a new versioned baseline, never a silent budget relaxation.

## Milestone sequence

1. **Foundation/native lint vertical (T003):** import/pin OXC, add failing black-box
   and performance tests, then native source type + representative `@{`/`@if` JSX
   parsing/semantics and observable oxlint diagnostics/safe fix.
2. **Grammar and lint completion:** all TSRX constructs/recovery/comments, semantic
   and CFG coverage, native/JS-plugin rules, imports/config/directives, fix-safety
   classification, complete parser/lint Markless corpus.
3. **Native format:** custom Oxfmt nodes/printer, comments/raw style/options,
   parse/compile/idempotence suite, BigInt fix, CLI/stdin/LSP formatter behavior.
4. **Packaging and Vite+:** scoped platform packages and launchers, pnpm alias/
   override integration, current and 0.1.20 `vp` command matrices, clean tarball
   installs and version provenance.
5. **Editor:** thin `ripple` selector client/VSIX, native lint/format LSP lifecycle,
   format-on-save/diagnostics/fixes, multi-root/config/cancellation, editor latency.
6. **Vite/build and type boundary:** existing framework plugin composition/HMR/
   sourcemaps/build tests; preserve Volar virtual TypeScript; investigate mapped
   tsgolint only if it can meet source-fidelity/performance gates.
7. **Real-world hardening:** copied full Markless corpus, stress/fuzz/error recovery,
   ordinary OXC regression matrix, RSS/edit soak, multi-platform CI, rebase test.
8. **Docs and clean room:** installation/compatibility/maintenance docs, local
   package/VSIX artifacts, temporary-copy Markless walkthrough, full matrices.
9. **Final Judge (T999):** clause-by-clause audit; no completion while any required
   surface remains inferred, queued, or outside budget.

## Exact T003 Worker package

### Objective

Turn the empty target into a reproducibly pinned OXC-for-TSRX source fork and ship
the first native user-observable lint slice. A project-built oxlint CLI must accept
a provenance-recorded representative `.tsrx` fixture containing TypeScript, a
native `@{` function body, JSX, and `@if/@else`; report seeded `no-debugger` and
scope/no-unused-vars diagnostics at original TSRX byte/line locations; apply one
proven-safe standard descendant fix (for example `no-var`) without altering custom
syntax; reparse the fixed file; and preserve ordinary TSX oxlint output and P01/P02/
P03 budgets. Start by committing/running tests that fail against stock OXC.

This slice includes the native parser/AST/semantic/loader work needed for those
constructs and a release benchmark executable. It does not claim grammar-complete
linting, formatter, Vite+, editor, type-aware, or final package support.

### Allowed files

- One-time immutable import of the pinned upstream OXC source tree at repository
  root, preserving existing `docs/goals/**` control files.
- Root fork metadata: `README.md`, `LICENSE`, `THIRD_PARTY_NOTICES.md`,
  `UPSTREAM_OXC_REVISION`, `OXC_TSRX_VERSION`, `.gitignore`, `.gitattributes`,
  `Cargo.toml`, and `Cargo.lock`.
- `crates/oxc_span/**`, `crates/oxc_syntax/**`, `crates/oxc_ast/**`,
  `crates/oxc_ast_visit/**`, `crates/oxc_parser/**`, `crates/oxc_semantic/**`,
  `crates/oxc_cfg/**`, and `crates/oxc_linter/**`.
- Generated outputs required by the touched AST in `crates/oxc_traverse/**` and
  `napi/parser/**`; no unrelated NAPI feature work.
- `apps/oxlint/**`.
- `tasks/ast_tools/**` only for generator compatibility; `tasks/tsrx_benchmark/**`
  for the owned native benchmark executable.
- `tests/tsrx/t003/**`, `benchmarks/tsrx/**`, and `docs/oxc-for-tsrx/**`.
- `docs/goals/**` remains PM-owned and is not a Worker write surface.

### Failing-test-first and final verification

1. After the pinned baseline and tests/harness exist but before parser changes:
   `OXLINT_BIN=/tmp/oxc-tsrx-baseline/node_modules/oxlint/bin/oxlint node --test tests/tsrx/t003/native-lint.test.mjs`
   must fail for unsupported/skipped TSRX and be recorded as the red proof.
2. Before performance-sensitive implementation:
   `cargo run --release -p oxc_tsrx_benchmark -- --corpus tests/tsrx/t003/fixtures --assert benchmarks/tsrx/t003-budgets.json`
   must fail the TSRX parse gate while producing a valid ordinary TSX control.
3. Targeted implementation gates:
   `cargo test -p oxc_span tsrx -- --nocapture`
   `cargo test -p oxc_parser tsrx -- --nocapture`
   `cargo test -p oxc_semantic tsrx -- --nocapture`
   `cargo test -p oxc_linter tsrx -- --nocapture`
4. User-observable green gate:
   `cargo build --release -p oxlint`
   `OXLINT_BIN=target/release/oxlint node --test tests/tsrx/t003/native-lint.test.mjs`
5. Frozen slice performance gate:
   `cargo run --release -p oxc_tsrx_benchmark -- --corpus tests/tsrx/t003/fixtures --assert benchmarks/tsrx/t003-budgets.json`
   plus the upstream-control sandwich recorded under `benchmarks/tsrx/results/`.
6. Touched-engine regressions:
   `cargo test -p oxc_parser -p oxc_semantic -p oxc_linter`
7. Board/receipt health (PM after Worker receipt):
   `node /Users/jacksm5pro/.codex/plugins/cache/goalbuddy/goalbuddy/0.4.0/skills/goal-prep/scripts/check-goal-state.mjs docs/goals/oxc-for-tsrx`

### Stop conditions

- Import would overwrite GoalBuddy control files or cannot be tied exactly to the
  approved OXC SHA/license.
- Required behavior is ambiguous between pinned authoritative Ripple tests and the
  Yuku reference; return the exact syntax/test conflict for another Scout/Judge.
- The representative slice requires full-AST conversion, compiled-TSX reparse,
  source-map fixes, or any other P10-forbidden boundary.
- Original byte spans or the safe-fix edit cannot be proven; disable the fix and
  return for Judge rather than applying an uncertain edit.
- Ordinary TSX behavior exceeds P01 or the native parser/lint exceeds P02/P03 after
  two evidence-based optimization attempts.
- A needed write is outside the allowed paths or outside this repository.
- A dependency/license/API incompatibility appears, or a required verification
  failure repeats after two evidence-based fixes.

No new approval is required to download/import the pinned upstream MIT source or
dependencies into this repository and `/tmp`, build locally, or create local test
artifacts. Publishing packages/VSIX, pushing a branch, opening an upstream PR, or
writing any external checkout remains unauthorized.

## Risks needing later Scout/Judge gates

- Before native formatter implementation, Scout Oxfmt's current comment attachment,
  suppression, raw-text, and embedded-language invariants in detail and choose node
  shapes that do not force a second parse.
- Before package/Vite+ implementation, test pnpm/npm/yarn alias resolution in
  disposable projects and decide whether a tiny Vite+ patch package is needed for
  versions that retain a private nested exact dependency.
- Before editor implementation, choose companion versus full oxc-vscode fork based
  on duplicate-server/selector tests; upstream configurable selectors would reduce
  maintenance.
- Before any tsgolint claim, inspect current virtual-file/provider APIs and prove a
  source-map artifact can be shared without disk churn or unsafe fixes.
- Before release claims, establish supported platform runners and artifact signing/
  provenance. Actual publishing needs fresh user authority.
