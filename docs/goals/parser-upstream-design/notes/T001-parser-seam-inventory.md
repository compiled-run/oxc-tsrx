# T001 parser-seam inventory

Provenance: read-only `crew run` scout on `gpt-5.6-sol`, independently spot-checked by
the PM against the cited source. No product files changed and no builds or tests ran.

## 1. Current parse and projection pipeline

- `tsrx_syntax::scan(source)` runs the crate-private scanner and returns
  `Result<Overlay, ProjectionError>`. The crate publicly re-exports the overlay,
  structural token/span types, projection result types, and scan/project/lift functions
  (`crates/tsrx_syntax/src/lib.rs:3-23`).
- The crate has four projection purposes: equal-width legacy projection, mapped lint
  projection, type-semantic projection, and formatter projection/lift
  (`crates/tsrx_syntax/src/lib.rs:7-11`). The mapped lane records authored affine
  segments; the type lane adds synthetic declarations; the formatter lane builds a
  private lift manifest (`crates/tsrx_syntax/src/projection.rs:755-810`,
  `crates/tsrx_syntax/src/projection.rs:877-930`).
- Projection construction merges source-ordered wrapper, try, structural-token, header,
  and embedded actions, and rejects stale overlays by source length/fingerprint
  (`crates/tsrx_syntax/src/projection.rs:947-1028`,
  `crates/tsrx_syntax/src/projection.rs:2673-2679`).
- OXC parsing happens only inside `oxc_adapter`: formatter parsing uses
  `parse_for_format`; lint creates an OXC parser, validates dynamic tags, builds
  semantics, and runs the linter (`crates/oxc_adapter/src/lib.rs:141-176`,
  `crates/oxc_adapter/src/lib.rs:588-669`).

Design consequence: a TSRX-aware JS AST must combine the compact TSRX overlay with the
pinned OXC parse. Neither stage alone currently yields a serializable TSRX AST.

## 2. Public versus private data

- Public `ByteSpan` is a UTF-8 byte range with public `start`/`end`
  (`crates/tsrx_syntax/src/model.rs:5-30`).
- Public `StructuralKind` has eleven variants, but `projected_token` is crate-private.
  Public `StructuralToken` exposes `kind` and `span`; its node `owner` is crate-private
  (`crates/tsrx_syntax/src/model.rs:32-69`).
- `ControlKind`, `ControlContext`, `ClauseRole`, `EmbeddedKind`, `EmbeddedToken`,
  `DynamicTag`, `StyleBlock`, `ForHeader`, `Clause`, and `SyntaxNode` are all
  crate-private (`crates/tsrx_syntax/src/model.rs:71-160`). Their fields retain control
  contexts, clause roles, tree links, authored headers/bindings, dynamic tag identities,
  closing-comment indices, and style spans, but callers cannot enumerate them.
- Public `Overlay` hides source identity, tokens, nodes, clauses, embedded records,
  dynamic tags/comments, style blocks, and root indices. Its public API exposes only
  structural tokens, source length, three counts, and an identity-range predicate
  (`crates/tsrx_syntax/src/model.rs:162-217`).
- Public `ProjectionError` preserves structured Rust variants and fields, but no syntax
  model derives serialization and `tsrx_syntax` has no dependencies
  (`crates/tsrx_syntax/src/model.rs:220-306`,
  `crates/tsrx_syntax/Cargo.toml:1-10`).
- `MappedProjection`, `TypeProjection`, and `FormatProjection` are public containers with
  private fields. Their public methods expose projected source, range queries, and a few
  counts/contracts, not complete records (`crates/tsrx_syntax/src/projection.rs:18-146`,
  `crates/tsrx_syntax/src/projection.rs:207-249`).
- `MapSegment { projected, original_start, fixable }` and every formatter manifest are
  module-private (`crates/tsrx_syntax/src/projection.rs:11-16`,
  `crates/tsrx_syntax/src/projection.rs:148-223`).

## 3. Mapping and authored data retention

- A map segment stores its projected byte span, authored start, and fixability.
  Byte-for-byte copied ranges are coalesced into compatible affine segments
  (`crates/tsrx_syntax/src/projection.rs:11-16`,
  `crates/tsrx_syntax/src/projection.rs:350-390`).
- Mapped lint diagnostics/fixes map only within one affine segment; fixes additionally
  require `fixable`. Type diagnostics may map independently anchored monotonic endpoints
  across synthetic text, but type fixes still require one segment
  (`crates/tsrx_syntax/src/projection.rs:40-146`).
- Dynamic opening expressions are copied, but paired names reject one-sided fixes.
  Closing expressions become synthetic identifiers; only closing-comment spans survive
  as markers. Raw style contents become a marker in projected TSX
  (`crates/tsrx_syntax/src/projection.rs:680-749`).
- The private overlay still retains authored opening/closing spans, closing-comment
  indices, self-closing state, style payload spans, control/clause/header spans, loop
  bindings, and node tree links (`crates/tsrx_syntax/src/model.rs:114-176`).
- Generated-control authored spans are retained privately, while callers get only an
  intersection predicate (`crates/tsrx_syntax/src/projection.rs:114-121`,
  `crates/tsrx_syntax/src/projection.rs:1022-1028`).

Design consequence: complete source maps and reconstructed TSRX nodes cannot be derived
from current query methods. The binding needs enumerable DTOs for mapping segments and
overlay records, while formatter-only lift manifests can remain internal unless the JS
contract explicitly needs them.

## 4. OXC adapter and allocator lifetime

- `oxc_adapter` is the sole workspace crate importing the twelve OXC crates at one pinned
  Git revision (`crates/oxc_adapter/Cargo.toml:9-21`). The architecture rejects mixed
  identities because arena AST/span/syntax types cross engine boundaries
  (`docs/architecture/rust-oxc-core.md:11-26`).
- Format and lint requests borrow projected/original source. Both paths create a local
  OXC `Allocator`; the parsed program is consumed before the function returns
  (`crates/oxc_adapter/src/lib.rs:80-176`,
  `crates/oxc_adapter/src/lib.rs:276-285`,
  `crates/oxc_adapter/src/lib.rs:588-669`).
- No allocator-backed AST escapes. Formatter results own code/timings; lint maps OXC
  messages into owned diagnostics/fixes (`crates/oxc_adapter/src/lib.rs:119-130`,
  `crates/oxc_adapter/src/lib.rs:480-519`).
- `program.source_text` is reset to projected input so OXC spans and source-sensitive
  rules refer to that buffer (`crates/oxc_adapter/src/lib.rs:609-614`).

Design consequence: the JS seam must serialize/copy while the allocator and projected
source are alive, or introduce an owned lifetime container. It should not leak pinned OXC
Rust types across the adapter boundary.

## 5. Existing CLI, LSP, and JS surfaces

- The Rust package has lint, formatter, and LSP binaries only
  (`crates/oxc_tsrx_cli/Cargo.toml:9-19`).
- The lint CLI emits aggregate lint output and exposes no scan/projection/AST operation
  (`crates/oxc_tsrx_cli/src/main.rs:28-175`).
- The LSP supplies diagnostics, whole-document formatting, and quick fixes. It has no
  parser/AST/custom parse request (`crates/oxc_tsrx_cli/src/bin/oxc-tsrx-lsp.rs:12-94`,
  `crates/oxc_tsrx_cli/src/bin/oxc-tsrx-lsp.rs:163-275`).
- The stable editor boundary deliberately excludes OXC/LSP implementation types and
  exposes owned authored ranges, diagnostics, edits, actions, and tool traits
  (`crates/oxc_adapter/src/editor.rs:1-5`,
  `crates/oxc_adapter/src/editor.rs:27-225`).
- The npm runtime resolves only `lint`, `format`, and `server`; there is no JS parser
  export or parser executable (`packages/runtime/dist/index.js:13-26`,
  `packages/runtime/dist/index.js:101-123`).

## 6. Native distribution surface

- The target table covers macOS arm64/x64, Linux GNU arm64/x64, Linux musl arm64/x64,
  and Windows MSVC arm64/x64 (`packages/runtime/dist/targets.js:1-62`).
- Runtime optional dependencies enumerate those eight packages
  (`packages/runtime/package.json:35-43`).
- Native packaging requires only `oxc-tsrx`, `oxc-tsrx-fmt`, and `oxc-tsrx-lsp`, then
  binds their target, hashes, version, OXC revision, and protocol into the package
  (`scripts/package-native.mjs:17-23`, `scripts/package-native.mjs:230-305`).
- Runtime loading validates runtime/native version, protocol, host target, OXC revision,
  and declared executable (`packages/runtime/dist/index.js:63-99`).

Any native parser binding or protocol addition therefore touches every target package,
the optional dependency contract, package metadata/checksums, and runtime validation.

## 7. Philosophy and compliance constraints

- Rust owns syntax/projection/mapping and hot paths; JavaScript remains thin. OXC is one
  canonical pinned graph with no fork, patch queue, or copied source
  (`docs/architecture/rust-oxc-core.md:3-26`).
- The overlay is compact, flat, indexed, allocation-light, and source-borrowing rather
  than an owned object graph (`docs/architecture/rust-oxc-core.md:28-35`).
- Scanning is fail-closed for unsupported/incomplete grammar
  (`docs/architecture/rust-oxc-core.md:51-58`).
- Embedded CSS is recorded as `KEEP_RAW`: byte-exact, without a CSS parser, formatter,
  subprocess, or Cargo patch. Requalification requires a coherent upstream allocator
  graph and fidelity/failure/convergence/performance gates
  (`compliance/css-boundary.json:1-29`).
- Style payloads are outside the JS AST and formatter lifting borrows them from the
  original source without parsing or rewriting
  (`docs/architecture/rust-oxc-core.md:88-94`,
  `docs/architecture/rust-oxc-core.md:203-211`).

Exposing raw style spans/bytes is consistent with policy. Returning a CSS AST or adding
recovery is an explicit policy ruling for T003, not a hidden serializer detail.

## 8. Markless control-corpus gate

- The gate pins Markless commit `76d0e6a07fa728b9343cc0d342fbe03813c43703`
  and `@tsrx/core` 0.1.32 (`tests/markless-control-corpus.test.mjs:11-15`,
  `tests/markless-control-corpus.test.mjs:61-70`).
- It classifies 191 tracked `.tsrx` files with Markless `parseModule`, expects twelve
  named invalid completion fixtures, and accepts 179
  (`tests/markless-control-corpus.test.mjs:17-30`,
  `tests/markless-control-corpus.test.mjs:71-108`).
- Each valid file must format, preserve style payload bytes, reparse with Markless, and
  converge; invalid inputs must fail with no output. The Markless worktree must remain
  unchanged (`tests/markless-control-corpus.test.mjs:61-110`).
- This proves formatter/reparse/convergence compatibility, not AST equivalence, error
  identity, offset semantics, or runtime behavior
  (`docs/architecture/rust-oxc-core.md:355-364`).

A replacement claim therefore needs a separate AST/error/offset corpus oracle.

## 9. Candidate exposure points

These are evidence-backed exposure candidates, not a selected API:

1. Convert private overlay records—nodes, clauses, embedded tokens, dynamic tags and
   comments, style blocks, root indices—into public binding DTOs, or deliberately make
   their Rust model public (`crates/tsrx_syntax/src/model.rs:71-176`).
2. Include structural-token ownership and indexed tree/clause relationships
   (`crates/tsrx_syntax/src/model.rs:63-69`,
   `crates/tsrx_syntax/src/model.rs:138-176`).
3. Serialize control kind/context/role, loop header fields, dynamic identity/comment
   metadata, and style spans (`crates/tsrx_syntax/src/model.rs:71-160`).
4. Expose a complete mapping DTO equivalent to private `MapSegment`; query methods alone
   cannot enumerate synthetic gaps/fixability (`crates/tsrx_syntax/src/projection.rs:11-146`).
5. Preserve structured `ProjectionError` variants rather than making downstream code
   parse display text. The current LSP does exactly that lossy parsing
   (`crates/oxc_tsrx_cli/src/bin/oxc-tsrx-lsp.rs:279-308`).
6. Serialize the OXC program inside `oxc_adapter` while its allocator lives, converting
   into an externally stable DTO instead of exposing revision-specific OXC types
   (`crates/oxc_adapter/src/lib.rs:141-176`,
   `crates/oxc_adapter/src/lib.rs:588-669`).
7. Extend all eight native packages and runtime integrity checks for the chosen binding
   artifact or protocol (`packages/runtime/dist/targets.js:1-80`,
   `scripts/package-native.mjs:17-23`, `scripts/package-native.mjs:267-305`).
8. Decide whether serialization belongs in a binding/adapter DTO layer or adds public
   records/serialization dependencies to `tsrx_syntax`.

## 10. Contradictions and T003 decisions

- The board named `crates/tsrx_syntax/src/projection/mapping.rs`, but that path does not
  exist in this worktree. The authoritative code is the monolithic
  `crates/tsrx_syntax/src/projection.rs`, selected by `lib.rs`
  (`crates/tsrx_syntax/src/lib.rs:3-11`).
- `docs/tools/projection-dump` is unpublished and outside the workspace. It can dump only
  projected source, public structural tokens/counts, and type-projected source—not
  hidden nodes, mapping segments, clauses, embedded records, or structured errors
  (`docs/tools/projection-dump/Cargo.toml:1-13`,
  `docs/tools/projection-dump/src/main.rs:1-64`, `Cargo.toml:1-10`).
- T003 must rule: binding DTOs versus public syntax records; UTF-8 bytes versus UTF-16 or
  dual offsets; raw-only CSS versus optional adapter-owned CSS parsing; strict production
  parse versus a separate loose/collect lane; serialization inside the local OXC arena;
  native-addon versus protocol/executable distribution; and the missing AST/error/offset
  Markless replacement oracle.
