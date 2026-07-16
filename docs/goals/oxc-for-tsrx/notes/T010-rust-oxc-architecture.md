# T010 — non-fork Rust/OXC qualification

Date: 2026-07-15  
Role: Scout  
Outcome: qualified; activate a Rust lint vertical slice, not another wrapper prototype

## Decision

OXC for TSRX will be a separate Rust workspace that consumes canonical OXC crates at an exact
official commit behind one project-owned adapter. It will not copy, vendor, patch, subtree, or fork
OXC. Ordinary JavaScript/TypeScript paths enter OXC directly. A native TSRX structural front-end
adds only the work unique to TSRX, then hands a standard arena-backed OXC `Program` to OXC semantic,
lint, and formatting infrastructure.

The initial pin is canonical OXC commit
`8e0ed2ebb96137fb1611cdbd5742d5cb46037d40` (the 0.140.0 parser/AST/semantic generation and the
co-released 1.74.0 linter / 0.59.0 formatter generation). The project minimum and current lane are
the same exact revision until the first release has enough history to support two revisions.

This preserves the owner's update-resilience requirement in the useful sense:

- installed prebuilt artifacts contain an immutable, tested OXC generation and cannot be broken by
  a new crates.io/npm release;
- all OXC-facing calls live in `oxc_adapter`, so a deliberate upgrade changes one boundary;
- candidate OXC revisions compile and run correctness/performance matrices before a release;
- consumers receive newer OXC behavior by upgrading OXC for TSRX, not by silently changing the
  engine under an existing install.

It does **not** mean that arbitrary future OXC behavior is inherited without qualification. That
would contradict the requirement that upstream changes cannot break installed users.

## Exact upstream qualification

All repositories below were inspected read-only.

| Surface | Revision / generation | Publication state | Qualified use |
|---|---|---|---|
| canonical OXC source | `8e0ed2ebb96137fb1611cdbd5742d5cb46037d40` | MIT | exact-revision Cargo source only; no local source copy |
| `oxc_allocator` | 0.140.0 | crates.io published | arena ownership |
| `oxc_ast` / `AstBuilder` | 0.140.0 | crates.io published | standard AST and external arena AST construction |
| `oxc_parser` | 0.140.0 | crates.io published | one full standard projection parse |
| `oxc_semantic` | 0.140.0 | crates.io published | `SemanticBuilder::new_linter().build(&Program)` |
| `oxc_span` / `oxc_syntax` | 0.140.0 | crates.io published | byte spans, source types, syntax module record |
| `oxc_linter` | 1.74.0 | `publish = false` | canonical exact-git dependency; real built-in rules via `Linter::run` |
| `oxc_formatter` | 0.59.0 | `publish = false` | canonical exact-git dependency; full standard-program Oxfmt formatting |
| `oxc_formatter_core` | 0.59.0 | `publish = false` | canonical exact-git dependency; public arena IR/layout/printer for custom syntax |
| Vite+ | `7062bfd849bd5a75be3f95a4f3137ebba1c32281` | official repository | standard Vite plugin plus project-local `oxlint`/`oxfmt` package resolution |
| OXC VS Code extension | `94c0fd6c7b629c174ed3386c2f53de91f6d6d8d6` | official repository | project binary discovery/path config; companion `.tsrx` selectors required |

The earlier charter assumption that every engine is crates.io-published is false. At this revision,
the linter, JavaScript formatter, and formatter core explicitly set `publish = false`. Cargo can
depend on those public workspace crates from the canonical OXC Git repository at an exact `rev`.
That is an official-source dependency, not a fork: no OXC source exists in this repository, no patch
is applied, and Cargo verifies the pinned Git object. Rejecting all non-crates.io engines would force
either a process wrapper or a fork and would fail the owner's intended in-process OXC integration.

The boundary policy is therefore corrected to: use crates.io packages where OXC publishes them;
use an exact canonical Git revision for a required public workspace crate that OXC deliberately does
not publish; never use a local path, copied file, private generated layout, patch section, or fork.

## Disposable compile/run proof

The spike lives only at `/tmp/oxc-tsrx-t010` and is not product code. Its exact inputs after the
formatter-core and marker-lift proofs are:

- `Cargo.toml`: SHA-256 `464dbe8d8de3a7b3de07fec5f97f6d1f0c40ed81b2c26970de96870feb8e5998`
- `Cargo.lock`: SHA-256 `bdef8c426bf9d5fb05ff096b326e669bfa9ce765de7e298e0891a2fdec33a4a1`
- `src/main.rs`: SHA-256 `ac014ea393f4b76e655bddb8618f20db38656802e1e1115f4bcb16eb51e5985f`
- `src/bin/normalize_bench.rs`: SHA-256 `df66b4a428f3a916328e05a8bae50813a051b7fb242b264d15f6a445ada5afa1`

Commands:

```text
cargo check
cargo run --locked --bin oxc-tsrx-t010
```

Observed output:

```text
stock_text_format="const view = <main>{answer}</main>;\n"
same_width_normalized_lint_span=33..42 source="debugger;"
external_ast_lint_rule=no-debugger span=14..23 source="debugger;"
external_ast_format_projection="debugger;\n"
custom_tsrx_format_via_oxc_core="@if (ready) {\n  debugger;\n}"
full_oxfmt_marker_lift="function View() @{\n  @if (ready) {\n    debugger;\n  }\n}\n"
```

This proves five separate boundaries:

1. Canonical Oxfmt formats standard TSX text normally.
2. A same-width native structural normalization can retain original byte spans. After parsing the
   normalized view, real OXC semantic analysis and the real `no-debugger` rule report the exact
   original `debugger;` bytes in TSRX.
3. A program built with canonical `AstBuilder` and original TSRX byte spans is accepted by real OXC
   semantic and linter engines. This is the escape hatch for custom constructs that require AST
   surgery after the one standard projection parse.
4. `oxc_formatter_core` accepts a project-owned TSRX format context and prints custom `@if` syntax
   through OXC's arena document IR and native line-breaking/indent printer.
5. The full JavaScript Oxfmt printer can format one marked standard projection once, after which a
   linear native lift restores `@{` and `@if`. This is the preferred formatter direction because it
   retains Oxfmt's standard TS/JSX language printer rather than reimplementing it.

`format_program` alone is not a custom-syntax formatter: when given an external AST containing only
a projected `DebuggerStatement`, it correctly prints only `debugger;`. Custom syntax must therefore
survive in the lossless TSRX overlay and be restored by validated markers, or be emitted through the
formatter core. No claim of stock `oxfmt` parsing `.tsrx` is made.

### Actual linter entry points proved

The spike uses the public Rust items directly:

- `SemanticBuilder::new_linter().build(&program)`;
- `oxc_linter::ModuleRecord::new(path, syntax_module_record, semantic)`;
- `ConfigStoreBuilder::all().build(...)` and `ConfigStore::new(...)`;
- `ContextSubHost::new(...)`;
- `Linter::run(...)`.

The proof runs the canonical built-in `no-debugger` rule, not a simulated rule. Built-in native rules
are qualified. JavaScript plugins remain a separate acceptance lane: OXC's external-plugin raw
transfer path assumes allocator ownership/order and parser-token availability. The TSRX front-end can
eventually satisfy those conditions, but T010 does not claim JS-plugin compatibility yet.

## Production data path and ownership

```text
borrowed/owned source bytes
  -> one Rust structural scan
       -> compact TSRX overlay (kind + original byte spans + marker/edit records)
       -> one contiguous legal TSX projection when required
  -> exactly one OXC whole-file parse for lint/semantic OR one Oxfmt parse for format
       -> arena-backed standard OXC Program
       -> optional in-arena AST adjustments for custom construct semantics/scopes
  -> OXC semantic/linter or Oxfmt document printer
  -> diagnostics/fixes by numeric span, or formatted marked text
  -> one linear checked map/lift against original TSRX
  -> CLI/N-API/LSP scalar result (never a JavaScript AST)
```

Owned objects per operation:

- original UTF-8 source remains available for diagnostic rendering and edit preconditions;
- one arena owns OXC AST/semantic/lint/formatter data;
- a compact indexed overlay owns only custom TSRX tokens, construct nesting, and edits;
- an optional source-sized native projection buffer is permitted only on `.tsrx` and is measured;
- formatted output is necessarily one owned output string;
- JavaScript receives strings, diagnostics, edits, and timings—not nodes.

Ordinary `.js/.jsx/.ts/.tsx` bypass the structural scan and projection allocation and enter canonical
OXC directly. This keeps P01 meaningful and prevents TSRX support from taxing standard files.

### Projection-buffer measurement

`src/bin/normalize_bench.rs` in the disposable spike measured the memory-traffic model on the
ignore-aware, read-only Markless TypeScript corpus using a release build, 8 warmups, and 30 samples:

```text
files=598 bytes=4352922
parse_median_ms=13.4093 parse_median_mib_s=309.58
copy_scan_parse_median_ms=14.2311 copy_scan_parse_median_mib_s=291.70
copy_scan_median_ms=0.5973 copy_scan_median_mib_s=6950.64
copy_scan_parse_ratio=1.0613
```

This is a memory-bound scan/copy control, not a claim that the real TSRX recognizer is implemented.
It shows one native projection buffer costs about 0.60 ms over 4.35 MB and leaves parse throughput at
94.2% of the direct control on this machine. P01 still requires zero such overhead for standard
files. T011 retains P02/P03 and measures the real recognizer; if the copy causes a gate failure, the
first optimization is owned-buffer in-place masking plus a compact restore log.

## Source fidelity and safe fixes

The lint projection uses equal-width masking wherever possible. OXC node spans then address original
TSRX UTF-8 bytes directly. The overlay records every non-identity region. A fix is safe only when:

1. its range is wholly inside identity-mapped standard syntax;
2. the mapping is contiguous and one-to-one;
3. the original bytes still match the diagnostic/fix precondition;
4. it does not cross or touch a structural marker/custom token;
5. the edited TSRX reparses successfully and the retained construct map remains valid.

All other fixes are reported without an edit. Complex custom constructs may be represented by
length-preserving blocks/placeholders and then adjusted in the OXC arena before semantic analysis.
The external-AST proof demonstrates that OXC accepts that representation; grammar-wide correctness
remains incremental Worker work, not a T010 claim.

## Formatter direction

The preferred full formatter path is:

1. scan TSRX once and create a legal standard projection containing collision-resistant structured
   comment markers for custom constructs;
2. call canonical `oxc_formatter` once on the full projection;
3. verify each marker occurs exactly once and in legal nesting/order;
4. lift markers back to TSRX syntax in one native scan;
5. reparse the lifted TSRX, compare normalized structure/semantics/comments, and require second-call
   byte identity before a file write.

Marker creation preflights the source for collisions. Missing, duplicated, reordered, or malformed
markers fail closed with no write. The retained TDD matrix must cover comment attachment, ignored
regions, strings/template literals, source comments that resemble markers, width changes, all custom
constructs, Unicode, styles, and real Markless files. `oxc_formatter_core` is a qualified native
fallback for constructs whose syntax cannot be represented safely by the marker projection, but a
hybrid cannot be selected merely because the small proof compiles.

## Vite and Vite+ integration

### Build/dev/HMR/plugin ecosystem

The npm package exposes a normal Vite plugin with an `enforce: "pre"` transform for `.tsrx`. The
transform produces ordinary framework-target TSX/JS and a source map. Vite 8/Vite+ build, Rolldown,
HMR, React/framework refresh, and downstream Vite plugins then operate on the standard output. The
plugin shell is JavaScript/TypeScript; the TSRX language work it invokes is native Rust.

### Literal `vp lint`, `vp fmt`, and `vp check`

Current Vite+ does more than merely bundle fixed binaries:

- `packages/cli/src/utils/constants.ts` resolves packages with search paths
  `[process.cwd(), import.meta.dirname]`;
- `resolve-lint.ts` resolves project package `oxlint`, derives its `bin/oxlint`, and passes that path
  to the Rust CLI core;
- `resolve-fmt.ts` does the same for `oxfmt` / `bin/oxfmt`;
- `packages/cli/binding/src/check/mod.rs` implements `vp check` by resolving its `Fmt` and `Lint`
  subcommands, so the same package selection applies to the composite command.

A disposable project-local resolution probe returned:

```text
oxlint=/private/tmp/oxc-tsrx-t010/vp-probe/node_modules/oxlint/bin/oxlint
oxfmt=/private/tmp/oxc-tsrx-t010/vp-probe/node_modules/oxfmt/bin/oxfmt
```

Packaging can therefore install one scoped CLI under npm aliases named `oxlint` and `oxfmt`, with
the expected `dist/index.js`, `bin/oxlint`, and `bin/oxfmt` shape. Literal Vite+ commands then execute
OXC for TSRX without a Vite+ fork. This is a source-proven package-resolution seam, not a declared
checker-plugin API, so minimum/current/canary Vite+ tests and a runtime capability probe are required.
If a future Vite+ changes resolution, the supported adapter must fail with an actionable message or
use a documented Vite task; it must never silently skip `.tsrx`.

Vite+ deliberately skips user Vite plugin factories while resolving lint/fmt config metadata. The
Vite transform plugin alone is therefore not the static-tool integration; the package-resolved
native binaries are.

## Editor boundary

The official OXC VS Code extension already discovers project `node_modules/.bin/oxlint` and
`node_modules/.bin/oxfmt` before global binaries and exposes `oxc.path.oxlint` / `oxc.path.oxfmt`.
The package aliases therefore supply the correct server binaries automatically.

However, the current extension's lint selector hard-codes
`astro,cjs,cts,js,jsx,mjs,mts,svelte,ts,tsx,vue`; formatter selectors and activation events likewise
omit `.tsrx`. A thin companion extension must register the TSRX language/selectors and connect the
same native LSP binaries. It does not fork the OXC extension or own parsing/formatting. An upstream
configurable-selector change could later remove this companion surface.

## Performance contract retained/revised

The numeric P01–P09 budgets in `T002-judge-decision.md` remain frozen:

- P01 standard OXC median regression <=5%, p95 <=8%; direct standard bypass is mandatory.
- P02 TSRX parse >=75 MiB/s median, >=60 MiB/s p95, >=50% equivalent TSX; warm 10 KiB p95 <=1 ms.
- P03 lint >=35 MiB/s one thread, >=70 MiB/s default, <=1.35x equivalent TSX end to end.
- P04 format >=15 MiB/s sequential, >=100 MiB/s default, >=10x pinned Prettier.
- P05 cold lint <=50 ms and format-stdin <=110 ms, each <=1.25x upstream control.
- P06 warm editor diagnostics p95 <=25 ms and format p95 <=15 ms.
- P07 RSS <= min(upstream x1.25, upstream +32 MiB), edit-soak growth <=8 MiB.
- P08 applies if N-API is shipped.
- P09 merely enabling the Vite adapter adds <=3% median.

P10 is refined for the public non-fork parser boundary: one whole-file native OXC parse per
operation; no compiled-TSX second parse, full-AST conversion, JSON AST, eager JS AST, per-node heap
graph, or JS-owned source map. One contiguous native TSRX projection buffer is allowed because the
public OXC parser accepts `&str`, only on the TSRX path, and only while P02/P03/P04/P07 remain green.
Its scan/copy time and allocation are reported separately. Standard paths may not allocate it.

Every benchmark records native parse/lower, OXC engine, output production, process/N-API boundary,
cold start, RSS, and editor incremental time separately. Yuku remains a read-only high-performance
control; it is not linked, distributed, or used as the production parser.

## Rejected alternatives

- **OXC source fork/vendor/patch:** violates the owner's invariant and makes upgrades a merge queue.
- **Zig/Yuku production core:** fast control, wrong product architecture; retain only until Rust
  correctness/performance evidence supersedes it.
- **JS compiler + stock-process remapping as final core:** already useful fallback evidence but loses
  native throughput and cannot provide a native syntax-preserving formatter.
- **Full custom Rust TypeScript parser:** duplicates OXC and makes compatibility/performance harder.
- **Whole AST over N-API/JSON:** erases arena locality and violates the boundary contract.
- **Stock binaries magically parsing `.tsrx`:** false; no stock custom parser/formatter hook exists.
- **Vite transform plugin as lint/format hook:** false for current Vite+ metadata/check execution.
- **Immediate JS-plugin compatibility claim:** unsafe until token/allocator raw-transfer assumptions
  are tested directly.

## Next largest safe Worker slice (T011)

Implement the first real Rust lint vertical slice:

- create the Rust workspace and `oxc_adapter` pinned to the exact canonical revision;
- retain a black-box test that stock Oxlint fails on a representative `.tsrx` fixture first;
- implement a token-aware structural scan for representative `@{` and `@if/@else` around real
  TypeScript and JSX;
- make one equal-width native projection and one OXC parse;
- run real semantic analysis and real OXC built-in rules;
- report `no-debugger` and `no-unused-vars` at original TSRX bytes;
- apply one identity-proven `no-var` fix, then reparse;
- delegate ordinary `.tsx` directly with output/performance regression tests;
- measure the actual scan/projection, parse, lint, CLI, and memory boundaries against P01/P02/P03.

The slice stops if it needs copied OXC source, an OXC patch, a second whole-file parse, a JS AST,
source-map-based fixes, or cannot keep original spans and budgets after two measured optimizations.
Formatter, complete grammar, JS plugins, Vite+, editor, packaging, and Markless clean-room acceptance
remain mandatory later slices and may not be inferred from T011.

## External-write audit

No external repository was modified. OXC, Vite+, OXC VS Code, Yuku, Ripple/TSRX, and Markless were
read-only. Disposable compilation and probes are confined to `/tmp/oxc-tsrx-t010`; durable writes are
only this goal's allowed note/receipt/board/charter files.
