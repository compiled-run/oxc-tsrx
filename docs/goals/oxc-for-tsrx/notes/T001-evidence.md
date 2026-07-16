# T001 Scout evidence: OXC for TSRX

Date: 2026-07-15 (America/Chicago)

This is a PM-authored receipt note for the read-only Scout task. No implementation
files were changed during T001. Markless, Ripple, Yuku, native-tsrx, OXC, Vite+,
and oxc-vscode were inspected read-only. Disposable npm baselines were installed
only under `/tmp/oxc-tsrx-baseline`.

## Executive finding

The credible end state is a maintained, pinned OXC fork/distribution with TSRX as
a first-class source type in the native parser, AST visitors/semantic analysis,
oxlint loader/CLI/LSP, and oxfmt printer/CLI/LSP. The implementation should port
TSRX grammar behavior into OXC's arena AST, using Ripple as the language oracle and
Yuku's native TSRX branch as a compact parser/performance reference. It should not
put a decoded ESTree, source-map remapper, Prettier bridge, or a second parse in the
production lint/format hot path.

Project-owned drop-in `oxlint` and `oxfmt` packages/binaries are the cleanest Vite+
integration seam today because Vite+ resolves those exact package names. A small
editor integration must add the `ripple`/`.tsrx` document selector and point the
official OXC client protocol at the project-owned binaries. Stock upstream binaries
remain TSRX-blind until equivalent changes land upstream.

## Repository and revision map

| Surface | Revision/version | License | State and relevance |
| --- | --- | --- | --- |
| Target `/Users/jacksm5pro/dev/open-source/oxc-tsrx` | no Git metadata; only GoalBuddy control files at intake | new project | Empty because it is a newly allocated integration workspace, not an existing implementation checkout. |
| Ripple/authoritative TSRX | branch `feat/tsrx-intellisense`, `03a98fd2a230ab5853808a44ff024568d68142fb`; `@tsrx/core` 0.1.33 | MIT | Authoritative Acorn-based grammar, compiler transforms, formatter, ESLint parser, Vite plugins, Volar/TS integration, and VS Code extension. Pre-existing dirty file: `packages/ripple/tests/hydration/components/if.tsrx`. |
| Yuku native TSRX branch | branch `feat/tsrx`, `bf03e146d97ae2f0c2d4c4ec90456e1e544d2760`; base main `728c16d4` | MIT | Complete native TSRX parser/codegen experiment with packed AST transfer and 15 dedicated `.tsrx` fixtures. Clean worktree. |
| native-tsrx experiment | no committed revision; package 0.1.20 | MIT | Uncommitted Zig/N-API packed-AST experiment. Useful design notes, not a dependable source dependency. All files were already untracked. |
| Markless oracle | branch `feat/markless-docs`, `fdcb833616c609385419c6b810069ac7df6ba4dd`; Vite+ 0.1.20, oxlint 1.61.0, oxfmt 0.46.0 | MIT | Read-only real application/tooling corpus. Pre-existing dirty file: `.claude/settings.json`. |
| OXC source baseline | main `129b759131ec60294bfcc0f388a45264c8740507`; crates 0.140.0; rust-version 1.95.0 | MIT | Current native architecture baseline. Latest app release observed: oxlint 1.74.0 and oxfmt 0.59.0. |
| Vite+ current baseline | main `7062bfd849bd5a75be3f95a4f3137ebba1c32281`; release 0.2.4 | MIT | Current package-resolution and Vite plugin behavior reference. Markless remains pinned to 0.1.20 and must also be covered. |
| oxc-vscode | main `94c0fd6c7b629c174ed3386c2f53de91f6d6d8d6`; extension 1.58.0 | MIT | Supports custom oxlint/oxfmt binary paths but hard-codes supported file extensions/language selectors and omits `.tsrx`/`ripple`. |

## Authoritative TSRX architecture

### Grammar and compiler

- `ripple/packages/tsrx/src/parse/index.js` composes Acorn,
  `@sveltejs/acorn-typescript`, the TSRX plugin, and element-closing recovery.
- `ripple/packages/tsrx/src/plugin.js` implements whole-file interleaved TSRX:
  `@{}`, `@if/@else`, `@for/@empty`, `@switch`, `@try`, dynamic tags, raw style
  elements, lazy destructuring, and TypeScript/JSX integration.
- `ripple/packages/tsrx/tests` is the broad behavior oracle, including locations,
  diagnostics, loose/editor parsing, recovery, and custom constructs.
- TSRX is not an SFC container. Directives and template syntax occur inside normal
  TypeScript function bodies, so script-block extraction cannot parse it.
- Framework packages lower the custom AST to ordinary framework-specific TSX/JS.
  That transformation remains the Vite/build boundary; lint and format should work
  on the original native TSRX AST instead of compiling then reparsing.

### Existing lint/format/editor/build surfaces

- `ripple/packages/eslint-parser/src/index.ts` adapts `@tsrx/core` to ESLint. Its
  location normalization repeatedly splits/scans source lines per node, a concrete
  hot-path pattern not to copy.
- `ripple/packages/prettier-plugin/src/index.js` is a mature 6K-line custom printer
  and behavior oracle. Its tests are valuable for formatting semantics and
  idempotence, but production formatting is JS-AST/Prettier based.
- `ripple/packages/vite-plugin` and framework adapters already make normal Vite and
  downstream plugins consume transformed TSX/JS. OXC for TSRX should compose with,
  not replace, that compiler boundary.
- `ripple/packages/typescript-plugin` and language server create virtual `.tsx`
  documents with Volar source mappings. They remain the practical type-system path.
- `ripple/packages/vscode-plugin` owns language id `ripple` for `.tsrx`; its current
  formatter delegates to Prettier. OXC for TSRX needs either a small companion
  extension or an upstreamable change that delegates formatting/linting to the new
  native binaries without changing the language id.

## OXC extension map

### Parser/AST/semantic

- OXC `SourceType::from_path` recognizes only standard JS/TS dialects. The public
  parser entry point accepts source text plus `SourceType`; it has no custom parser
  callback.
- `crates/oxc_parser/src/jsx/mod.rs` parses JSX directly into arena-allocated OXC
  nodes. `crates/oxc_ast/src/ast/jsx.rs` is the source schema; `tasks/ast_tools`
  generates visitors, AST kinds/builders, formatter dispatch, ESTree transfer,
  TypeScript declarations, and traversal code.
- Therefore adding native TSRX nodes is invasive but systematic: source schema +
  parser + semantic/scope behavior + generated outputs. It avoids per-node heap
  objects and conversion into a second AST.
- Yuku's branch demonstrates that the grammar is tractable in a native parser: its
  non-snapshot core changes center on `syntax/tsrx/{root,template,dynamic_tag}.zig`,
  JSX/statement/function hooks, AST nodes, lexer, scope traversal, and codegen.

### Oxlint

- `crates/oxc_linter/src/loader/partial_loader/mod.rs` hard-codes `vue`, `astro`,
  and `svelte`. These loaders return standard JS/TS source slices only.
- `crates/oxc_linter/src/service/runtime.rs` then calls stock `Parser::new` and
  `SemanticBuilder::new_linter`; no external AST/parser injection exists.
- Adding `.tsrx` to a partial-loader list would only make the stock parser fail.
  First-class linting requires native parser/AST/semantic support and a normal
  whole-file loader path.
- Once the AST and visitors exist, native rules, `.oxlintrc`/JS configuration,
  diagnostics, fix machinery, module records, and JS plugin token/source APIs are
  reusable. Custom nodes must expose stable visitor keys and original byte spans.
- Fix policy must be conservative: fixes on standard descendant nodes are allowed
  after span tests; fixes that replace synthetic/custom control-flow constructs are
  disallowed until specifically proven.

### Oxfmt

- `apps/oxfmt/src/core/support.rs` routes standard JS/TS to `oxc_formatter` and
  hard-codes the non-JS Prettier-backed extensions/parsers. `.tsrx` is absent.
- The NAPI external formatter bridge can call fixed Prettier parsers and plugins,
  but there is no generic project parser/plugin option. Adding a TSRX Prettier case
  would be a useful emergency bridge, not the performance/first-class end state.
- Native formatting requires `.tsrx` classification, native parse support, new
  TSRX format nodes/printers, comments/suppression coverage, CLI and LSP tests, and
  config option parity for the options TSRX can honor.

### CLI, Vite+, and editor

- Vite+ 0.1.20 and current Vite+ resolve the exact installed `oxlint` and `oxfmt`
  packages/binaries. Current `resolve-lint.ts`/`resolve-fmt.ts` explicitly suppress
  normal user Vite plugins while those tools load configuration. A Vite transform
  plugin is therefore not a lint/format extension hook.
- A pinned project-owned distribution can satisfy `vp lint`, `vp fmt`, the Vite+
  `oxlint`/`oxfmt` wrappers, and normal direct commands through package-manager
  overrides/aliases or packages with the expected binary contract. This must be
  tested on both Vite+ 0.1.20 and the supported current release.
- oxc-vscode accepts `oxc.path.oxlint` and `oxc.path.oxfmt`, and auto-discovers
  workspace `.bin` entries. However its linter extension list and formatter
  selectors omit `tsrx`/`ripple`, as do activation events. A companion/forked client
  surface is required unless oxc-vscode adds configurable selectors upstream.
- Native oxlint/oxfmt already provide LSP modes, so the editor integration should
  be a thin document-selector/binary-selection client, not a second lint/format
  implementation.

## Markless acceptance map

- Real corpus: 189 `.tsrx` files, 187,285 bytes, 5,135 lines.
- Common custom syntax includes 304 `@{` blocks, 58 `@try`, 52 `@for`, 48
  `@catch`, 35 `@if`, 16 `@else`, 11 `@empty`, 8 `@case`, 5 `@switch`, 5
  `<style>` elements, and component/dynamic element usage.
- 176 files (180,484 bytes) are accepted without diagnostics by both the current
  Acorn TSRX parser and the Yuku branch. Most excluded files are intentionally
  incomplete/invalid editor-completion fixtures and belong in recovery tests.
- The largest representative file is 7,239 bytes. One otherwise-valid 6,002-byte
  BigInt fixture crashes the current TSRX Prettier printer, making it a mandatory
  native formatter regression fixture.
- Root scripts use `vp pack`, `vp check`, `vp fmt`, `vp lint`, and `vp test`.
  Acceptance must copy a curated provenance-recorded subset (and optionally the
  full corpus in a disposable workspace), never modify Markless.

## Compatibility claim matrix

| Surface | Directly reusable | Adaptable | Unsupported by stock | Must build/prove |
| --- | --- | --- | --- | --- |
| OXC native rules/config | Rule implementations, config schema/loaders, diagnostics/fixer | Add TSRX source type/framework context and visitor coverage | Stock parser rejects/skips `.tsrx` | Whole-file native load, rule traversal, JS plugin visitor keys, safe-fix matrix |
| OXC parser/semantic | Arena allocator, lexer/parser architecture, generated visitors, scopes/symbols | Port TSRX grammar/nodes/scope semantics using Ripple/Yuku behavior | No parser hook or `.tsrx` dialect | Native source type, grammar, custom AST, recovery, locations, semantic traversal |
| Oxfmt | IR/printer, comments, config, CLI/LSP, allocator pool | TSRX nodes/printer and file classifier | No `.tsrx` classifier/parser/plugin | Native formatting, idempotence, comments/styles/control flow, semantics check |
| Vite/build | Existing TSRX compiler plugins and Vite transform pipeline | Package/plugin docs and order tests | Vite plugins do not extend `vp lint/fmt` | Vite/Vite+ fixture tests, HMR/plugin ordering, drop-in tool resolution |
| Editor | Native OXC LSP servers; custom binary settings | Thin TSRX document-selector client/companion extension | Official selector omits `.tsrx`/`ripple` | didOpen diagnostics, formatting, code actions, on-save walkthrough |
| Type-aware | Existing Volar virtual TSX and source maps; tsgolint binary | Feed/remap the same virtual artifact if a safe API is available | typescript-go cannot parse raw TSRX | A measured mapped path or an honest documented boundary; never fake native support |
| Packaging | OXC platform packages and JS launchers | Project-owned scoped packages plus Vite+ resolution recipe | Stock npm binaries remain blind | Multi-platform build matrix, tarball install test, version/rebase policy |

## Data layout and boundary-cost map

Recommended production path:

`UTF-8 source -> OXC lexer/parser -> arena OXC+TSRX AST -> semantic model -> native linter or native formatter -> diagnostics/edits/output`

Properties and risks:

- One whole-file parse per lint or format command; no compile-to-TSX reparse.
- Arena nodes and borrowed source slices; no per-node system allocator calls.
- Original byte spans stay on all native/custom nodes. UTF-16 conversion occurs
  only at the LSP edge.
- No ESTree/JSON/ArrayBuffer transfer in native CLI/LSP paths. Raw transfer is
  relevant only if a public Node parser API is later exposed.
- Parser scratch vectors should be reused/amortized; avoid copying raw `<style>` or
  JSX text when a span/slice suffices.
- Generated visitor/formatter/transfer tables must include custom nodes, otherwise
  semantic rules or JS plugins can silently miss descendants.
- Lint and format are separate commands and may each parse once; duplicate parses
  inside a single operation are prohibited. `vp check` orchestration should be
  measured honestly rather than claiming cross-process AST reuse.
- Vite compilation remains its own parser/transform operation. Reuse across Vite
  and lint/format would require a stable artifact/cache protocol and is not a
  prerequisite for first-class tools.

Rejected hot paths:

1. Acorn/ESTree parse -> per-node Rust/OXC conversion -> semantic pass.
2. TSRX compile -> source map -> OXC parse -> remap every diagnostic/fix.
3. Native packed parse -> eager full JS AST -> oxlint JS plugin emulation.
4. Prettier as the permanent oxfmt implementation.

## Local baseline methodology

Raw results are in `T001-local-baselines.json`; reproducible source is in
`T001-benchmark.mjs`.

- Host: Apple M5 Pro, 18 cores, 48 GB; macOS 26.5.1 arm64.
- Node 24.15.0, npm 11.12.1, Rust/Cargo 1.95.0, Zig 0.16.0.
- OXC parser 0.140.0, oxlint 1.74.0, oxfmt 0.59.0 were installed under
  `/tmp/oxc-tsrx-baseline` only.
- Benchmarks use the same local source bytes per compared case, explicit warmups,
  sorted samples, p50/median and p95, and separate packed/native-style work from
  fully decoded JS ASTs.
- Published Yuku numbers are not used as pass/fail evidence.

Headline measurements:

| Boundary/corpus | Median or p50 | p95 | Notes |
| --- | ---: | ---: | --- |
| Acorn TSRX full JS AST, 176 files/180 KB | 4.92 MB/s | 3.75 MB/s | Current language implementation |
| Yuku packed TSRX parse, same corpus | 136.74 MB/s | 123.00 MB/s | ArrayBuffer produced, not decoded |
| Yuku decoded TSRX ESTree, same corpus | 38.35 MB/s | 34.41 MB/s | Full JS object materialization |
| OXC raw-transfer TS ESTree, 592 files/4.23 MB | 127.20 MB/s | 116.12 MB/s | Full eager JS AST, current OXC fast transfer |
| Yuku decoded TS ESTree, same corpus | 100.47 MB/s | 81.68 MB/s | Comparable output boundary |
| OXC legacy JSON TS ESTree, same corpus | 22.30 MB/s | 21.12 MB/s | Demonstrates boundary cost to avoid |
| oxlint CLI, 598 TS files/4.35 MB, default threads | 41.71 ms | 43.72 ms | About 104 MB/s including startup/I/O |
| oxlint CLI, same, one thread | 76.72 ms | 77.46 ms | About 54 MB/s including startup/I/O |
| current TSRX Prettier, 175 files/174 KB | 1.66 MB/s | 1.45 MB/s | All 175 outputs idempotent; BigInt file crashes |
| oxfmt NAPI TS, 598 files/4.35 MB sequential | 37.39 MB/s | 35.17 MB/s | Warm repeated API calls |
| oxfmt NAPI TS, same batched | 315.91 MB/s | 287.46 MB/s | Parallel Promise batch |
| oxfmt CLI `--check`, same, default threads | 121.61 ms | 129.15 ms | Includes about 82 ms Node launcher startup |

Warm single-file editor proxies on a 7.2 KB TSRX file and a 7.3 KB TS file:

| Operation | p50 | p95 | p99 |
| --- | ---: | ---: | ---: |
| Acorn TSRX parse | 1.102 ms | 1.418 ms | 1.528 ms |
| Yuku packed TSRX parse | 0.033 ms | 0.041 ms | 0.046 ms |
| Yuku decoded TSRX AST | 0.141 ms | 0.171 ms | 2.026 ms |
| TSRX Prettier format | 1.825 ms | 2.139 ms | 2.733 ms |
| native oxfmt TS format | 0.156 ms | 0.196 ms | 0.256 ms |

Cold launcher proxies:

| Operation | median | p95 |
| --- | ---: | ---: |
| oxlint `--version` | 22.45 ms | 23.22 ms |
| oxlint one TS file | 29.63 ms | 32.76 ms |
| oxfmt `--version` | 82.43 ms | 83.36 ms |
| oxfmt stdin one TS file | 85.97 ms | 87.00 ms |

Retained JS-object memory is not a native CLI memory baseline, but it proves why
the boundary matters: retained Acorn TSRX ASTs added about 50 MB RSS for 177 files;
Yuku packed buffers added about 5.6 MB and decoded Yuku objects about 7.5 MB for all
189 files. OXC raw-transfer and JSON ASTs on the much larger 4.35 MB TS corpus added
about 157 MB and 121 MB respectively. Native CLI/LSP peak RSS must be measured by
the implementation harness before claims are made.

## Proposed budgets for Judge to freeze

Use ratio gates for portability and absolute editor/startup caps for user impact.
Each throughput result is the median of at least 15 measured samples after at least
5 warmups; latency gates use p50/p95 over at least 100 editor operations. A result
inside 3% of a limit is rerun for 30 samples. CI compares the same commit's upstream
control and TSRX build on the same host; three consecutive confirmed failures block.

1. Ordinary upstream JS/TS/JSX/TSX: no more than 5% median throughput regression
   and no more than 8% p95 latency regression in parse, oxlint, and oxfmt controls.
2. Native TSRX raw parse: at least 75 MB/s median and 60 MB/s p95 on the 176-file
   Markless valid corpus; at least 50% of the same-build equivalent-TSX throughput.
3. Full TSRX lint with default native rule set: at least 35 MB/s one-thread median
   and 70 MB/s default-thread median on the copied corpus, or no more than 35%
   slower than equivalent emitted TSX, whichever is stricter after emitted output
   is frozen.
4. Full native TSRX format: at least 15 MB/s sequential median and 100 MB/s batched
   median on corpus-sized input; at least 10x the incumbent 1.66 MB/s formatter.
5. Cold launcher: project oxlint no more than 1.25x upstream and 50 ms p95;
   project oxfmt no more than 1.25x upstream and 110 ms p95.
6. Warm 10 KB editor file: parse p95 <= 1 ms; diagnostics round-trip p50 <= 10 ms
   and p95 <= 25 ms; format request p50 <= 5 ms and p95 <= 15 ms; safe-fix/code
   action p95 <= 25 ms. Initial LSP open/diagnostics p95 <= 100 ms after server
   initialization.
7. Native peak RSS on the Markless copied corpus: no more than 1.25x the upstream
   equivalent-TSX control or upstream +32 MiB, whichever permits less growth.
   Incremental steady-state RSS must not grow by more than 8 MiB after 1,000 edits.
8. No production operation may contain a measured second whole-file parse, full
   AST JSON serialization, eager JS AST materialization, or source-sized duplicate
   copy unless Judge explicitly replaces the architecture based on stronger data.

## Ranked architecture options

### 1. Pinned native OXC fork/distribution (recommended)

Add TSRX source type and custom nodes directly to OXC, port parser behavior,
generate visitors/transfer tables, teach semantic analysis and formatter, and wire
normal oxlint/oxfmt CLI/LSP classifiers. Distribute drop-in binaries/packages plus a
thin editor selector client. Highest source fidelity and speed; rules/config/fixes
reuse naturally. Costs: OXC AST/parser/formatter churn, generated-code rebases,
multi-platform releases, and an upstream patch discipline.

Maintenance strategy: pin one OXC SHA/release train, keep TSRX changes as a small
ordered patch stack with upstream-control benchmarks, rebase on a scheduled cadence,
and upstream separable source-type/selector/test infrastructure when acceptable.

### 2. Yuku parser -> OXC AST adapter

Yuku already parses TSRX extremely quickly, but native OXC lint/format still needs
OXC arena nodes and semantics. A C ABI/packed buffer plus per-node conversion adds a
second representation, allocation/copy work, span/fix risk, and two upstream ASTs to
track. Useful as a differential oracle or fallback parser benchmark, not preferred.

### 3. Compile-to-TSX then stock OXC, plus source maps

Fastest prototype and compatible with stock rule bodies, but performs another
whole-file parse, makes fixes and formatter output unsafe/hard, changes rule
semantics around custom control flow, and complicates editor latency. Retain only as
an emergency diagnostic-only fallback with fixes disabled.

### 4. Split ESLint/Prettier + stock OXC

Works today and is useful for differential acceptance, but does not deliver OXC
formatter speed, stock Vite+ commands, or first-class native tooling. It is the
incumbent baseline, not the product architecture.

## Verification inventory and clean-room oracle

Existing upstream commands are evidence sources, not owned completion gates:

- Ripple: `pnpm test`, targeted Vitest parser/formatter/language-server suites,
  `pnpm typecheck`, `pnpm format:check`.
- Yuku: `bun test:parser`, `bun test:codegen`, `zig build test-tools`; TSRX branch
  fixtures under `test/parser/misc/tsrx`.
- Markless: `vp pack`, `vp check`, `vp fmt`, `vp lint`, `vp test` plus package
  fixture/box suites. Run only against a disposable copy when writes are possible.

Owned matrix to create:

1. Parser corpus snapshots: Ripple/Yuku grammar cases, invalid/recovery cases,
   comments, byte spans, UTF-8/UTF-16 locations, and provenance-recorded Markless
   copies.
2. Semantic/lint matrix: native rules, JSX/react rules, scope/no-unused-vars,
   imports, directives, JS plugins/visitor keys/tokens, configuration overrides,
   diagnostics, suggestions, and safe/dangerous fix policy.
3. Formatter matrix: every custom construct, comments, raw style, TypeScript,
   options, parse-after-format, one-pass convergence, compiled-output equivalence,
   BigInt regression, and Markless corpus idempotence.
4. CLI/config matrix: direct binaries, `.oxlintrc`/`.oxfmtrc`, stdin, check/write,
   nested config, exit codes, JSON reporters, and unmatched/ignored files.
5. Vite/Vite+ matrix: current and Markless-pinned Vite+, `vp lint/fmt/check`, build,
   pre/post plugin ordering, HMR invalidation, sourcemaps, and framework adapters.
6. LSP/editor matrix: initialize, selectors, didOpen/didChange, pull diagnostics,
   formatting edits, code actions, fix-all, config reload, cancellation, multi-root,
   and a VS Code integration test for language id `ripple`.
7. Packaging matrix: clean local tarball install, platform binding selection,
   workspace/package-manager overrides, no undeclared source checkout, and version
   output identifying both upstream OXC and TSRX patch versions.
8. Performance matrix: retained benchmark source/raw JSON, upstream controls,
   copied Markless and synthetic size/stress buckets, equivalent emitted TSX, cold
   and warm, one/default thread, RSS, 1,000-edit soak, and no duplicate-parse trace.

Final clean-room outline:

1. Create a fresh temporary project and install packed local artifacts.
2. Copy provenance-listed Markless `.tsrx` fixtures into it.
3. Run direct `oxlint`/`oxfmt`, then `vp lint`, `vp fmt --check`, `vp check`, and
   Vite build/plugin-chain tests.
4. Seed a lint violation; assert rule id, message, UTF-16 LSP range, source byte
   range, safe edit, and reparsed result.
5. Run format twice; assert first output parses/compiles equivalently and second is
   byte-identical.
6. Spawn LSPs and VS Code test client; capture diagnostics, formatting, and fix-all
   artifacts for a `ripple` document.
7. Run full correctness/performance matrices and compare to frozen controls.
8. Verify external repo status fingerprints are unchanged.

## Candidate first Worker slice

Objective: convert this workspace into a reproducibly pinned OXC-for-TSRX fork and
deliver one native, user-observable lint vertical slice: a project-owned oxlint CLI
accepts a representative copied `.tsrx` file containing JSX, `@{}`, `@if`, and
TypeScript; reports at least `no-debugger` and scope/no-unused-vars diagnostics at
original byte/line locations; applies one proven safe descendant-node fix; and
ordinary `.tsx` controls remain byte-for-byte/diagnostically compatible and within
the 5% performance budget.

Test-first order:

1. Add copied/provenance fixtures and black-box CLI tests that fail because stock
   oxlint skips/rejects `.tsrx`.
2. Add parser snapshot/location tests and an upstream TSX control benchmark guard.
3. Pin/import OXC 0.140.0 source at the recorded SHA.
4. Add the minimum native source type, AST nodes, parser/semantic traversal, loader,
   and CLI/LSP classification needed for the representative syntax.
5. Regenerate OXC derived files and make black-box tests/performance guards green.

Provisional allowed files: repository scaffolding and lockfiles; pinned upstream OXC
source; `crates/oxc_span`, `crates/oxc_ast`, `crates/oxc_parser`, generated visitor
outputs, `crates/oxc_semantic`, `crates/oxc_linter`, `apps/oxlint`; owned
`tests/fixtures`, `tests/integration`, `benchmarks`, and product docs. GoalBuddy
control files remain PM-owned.

Stop if the OXC pin/license cannot be reproduced, required grammar semantics are
ambiguous between authoritative Ripple tests and Yuku, the slice requires an
external repo write, or native custom nodes cannot preserve original spans without
a second representation. Formatter, Vite+, editor, and publishing are subsequent
required slices; this first slice is not goal completion.

## Open risks for Judge

- Type-aware tsgolint on raw `.tsrx` remains unsupported. The likely correct path is
  a virtual TSX artifact/source-map handoff from the existing Volar transform, with
  type-aware fixes disabled until mapping is proven.
- Custom control-flow nodes need explicit CFG/scope semantics; merely generating a
  visitor is insufficient for all rules.
- Oxfmt comment attachment and group-breaking behavior will be the largest single
  implementation surface. Existing Prettier snapshots are an oracle, not a mandate
  for exact whitespace identity.
- Drop-in package names may require package-manager overrides until upstream Vite+
  supports configurable tool binaries. The install test must cover pnpm's nested
  exact Vite+ dependencies.
- A public npm release, GitHub fork, PR, or external editor modification is outside
  current authority. Local pack/install and local VSIX artifacts are authorized;
  publishing is not.
