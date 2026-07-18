# T002 OXC parser-binding and upstream conventions

Provenance: read-only `crew run` scout on `gpt-5.6-sol`, PM spot-check of the
pinned source, and PM follow-up against current official OXC contribution pages on
2026-07-17. No repository files outside this GoalBuddy note/board were changed.

All pinned-source citations refer to OXC revision
`8e0ed2ebb96137fb1611cdbd5742d5cb46037d40` under its repository-relative path.
Statements are labeled **Observed**, **Inference**, or **Unresolved**.

## 1. Source identity

**Observed.** The local checkout resolves to the exact revision above. This worktree's
`Cargo.lock` binds all direct OXC crates to that revision (`Cargo.lock:798-800`,
`Cargo.lock:813-815`, `Cargo.lock:929-941`, `Cargo.lock:1096-1098`,
`Cargo.lock:1219-1249`), and `THIRD_PARTY_NOTICES.md:4-5` records the same identity.

## 2. Stable public `oxc-parser` shape

**Observed.** `oxc-parser` is an ESM npm package whose Node entry is
`src-js/index.js` and browser entry is `src-js/wasm.js`
(`napi/parser/package.json:2-4`, `napi/parser/package.json:55-57`). Its typed stable
entry points are:

- `parse(filename, sourceText, options?) -> Promise<ParseResult>`;
- `parseSync(filename, sourceText, options?) -> ParseResult`;
- a lazy `ParseResult` exposing `program`, `module`, `comments`, and `errors` getters
  (`napi/parser/src-js/index.d.ts:43-48`,
  `napi/parser/src-js/index.d.ts:144-157`,
  `napi/parser/src-js/index.d.ts:200-210`).

The JS wrapper materializes each property lazily and JSON-parses the program only on
first access (`napi/parser/src-js/wrap.js:1-20`, `napi/parser/src-js/wrap.js:34-39`).
The public AST types come from and are re-exported with `@oxc-project/types`
(`napi/parser/src-js/index.d.ts:4-9`, `napi/parser/package.json:78-80`).

**Observed.** Stable options are `lang`, `sourceType`, `astType`, `range`,
`preserveParens`, and `showSemanticErrors`
(`napi/parser/src-js/index.d.ts:159-198`). Runtime switches named
`experimentalRawTransfer` and `experimentalLazy` are explicitly experimental
(`napi/parser/src-js/index.js:47-66`, `napi/parser/src-js/index.js:89-98`).
`experimentalTokens` is omitted from generated TypeScript, described as unstable, and
requires raw transfer (`napi/parser/src/types.rs:34-40`).

## 3. Results and errors

**Observed.** Comments are `{ type, value, start, end }`. Parser errors are structured
objects with severity, message, labels, optional help, and optional codeframe
(`napi/parser/src-js/index.d.ts:17-42`). Rust builds those from `OxcDiagnostic` and
filename-bearing source (`crates/oxc_napi/src/error.rs:27-95`). Parse and optional
semantic diagnostics are returned in `ParseResult.errors`, not thrown as JS exceptions
(`napi/parser/src/lib.rs:105-119`, `napi/parser/src/lib.rs:139-144`). Wrapper capability
misuse, such as requesting unsupported raw transfer, may throw
(`napi/parser/src-js/index.js:53-55`, `napi/parser/src-js/index.js:85-87`).

**Design implication.** TSRX strict compatibility may deliberately offer a throwing
facade, but an OXC-convention-aligned core result should retain structured collected
errors. The design must distinguish those layers rather than misdescribe thrown syntax
errors as OXC's parser convention.

## 4. Arena ownership and sync/async behavior

**Observed.** The standard path creates one local `Allocator`, parses borrowed source,
converts diagnostics/comments/module data, serializes ESTree, and returns owned
napi/JSON results (`napi/parser/src/lib.rs:79-144`). No arena-borrowed AST escapes.

`parseSync` is explicitly preferred. Async Rust parsing can run off-thread, but JS-object
deserialization remains on the current thread and is documented as typically costing
3–20 times the asynchronous parse (`napi/parser/src/lib.rs:147-206`, corroborated at
`napi/parser/src-js/index.d.ts:144-155`). OXC recommends worker threads around
`parseSync` for multi-file parallelism.

**Design implication.** TSRX should serialize its OXC and overlay data before the local
arena dies, preserve a strong synchronous API, and benchmark parsing separately from JS
materialization.

## 5. ESTree and raw-transfer architecture

**Observed.** The ordinary transport calls
`program.to_estree_json_with_fixes(include_ts_fields, ranges)`
(`napi/parser/src/lib.rs:139-144`). The wrapper repairs special BigInt/RegExp values via
Rust-generated paths instead of walking the whole AST (`napi/parser/src-js/wrap.js:23-55`).
OXC's internal differentiated AST is therefore separate from its public ESTree
projection (`ARCHITECTURE.md:95-105`).

**Observed.** Raw transfer is a capability-gated fast transport, compiled only for
64-bit little-endian targets (`napi/parser/src/lib.rs:33-46`). It uses a JS-owned 2-GiB
buffer aligned to a 4-GiB boundary, constructs an arena over that buffer, and wraps the
allocator in `ManuallyDrop` so Rust cannot free JS-owned memory
(`napi/parser/src/raw_transfer.rs:29-43`,
`napi/parser/src/raw_transfer.rs:211-225`). Parsing is scoped so arena borrows end before
return, and program/comments/module/errors are packed into generated ESTree records
(`napi/parser/src/raw_transfer.rs:244-267`,
`napi/parser/src/raw_transfer.rs:328-347`,
`napi/parser/src/raw_transfer_types.rs:18-26`,
`napi/parser/src/raw_transfer_types.rs:63-78`,
`napi/parser/src/raw_transfer_types.rs:114-204`).

**Inference.** JSON and raw transfer are two transports for the same public language
model, not separate parser semantics. A TSRX design should specify one AST contract and
permit transport evolution behind explicit capability checks.

## 6. Span and Unicode convention

**Observed.** Rust parses UTF-8, then converts AST, comments, module records, and error
labels to UTF-16 offsets before exposing them to JavaScript
(`crates/oxc_napi/src/lib.rs:12-60`). Raw transfer performs the same conversion
(`napi/parser/src/raw_transfer.rs:296-326`). JS tests use comment offsets directly with
`substring` (`napi/parser/test/parse.test.ts:22-35`).

**Design implication.** OXC convention and the Markless replacement bar agree on UTF-16
half-open JS offsets. Internal byte spans can remain canonical, but JS-visible nodes,
comments, module records, diagnostics, and CSS-relative positions need one coherent
UTF-16 conversion pass. If byte offsets are exposed too, they must be explicitly named
and tested as a second coordinate system.

## 7. Package and target convention

**Observed.** napi-rs generates the native loader and declarations. The public package
ships wrapper code, generated deserializers, lazy constructors/walker, visitor artifacts,
raw-transfer code, and browser/WASI entries (`napi/parser/package.json:25-76`).

The napi manifest defines `@oxc-parser/binding` and a broad target list across Apple,
Android, Windows, GNU/musl Linux, OpenHarmony, FreeBSD, and WASI
(`napi/parser/package.json:97-127`). The generated loader tries an adjacent `.node`, then
the target-specific package, with exact version checks in the loader paths
(`napi/parser/src-js/bindings.js:67-89`,
`napi/parser/src-js/bindings.js:180-227`). The public package currently requires Node
`^20.19.0 || >=22.12.0` (`napi/parser/package.json:129-131`).

**Design implication.** A JS-callable TSRX parser should use a napi addon and
target-specific optional packages, not invent a JSON-over-process parser as the primary
API. oxc-tsrx may initially support its existing eight targets, but its package/version
binding, loader failure behavior, declarations, and capability checks should mirror the
observed napi model.

## 8. Compatibility, tests, and performance evidence

**Observed.** The npm and Rust napi crate versions move together
(`napi/parser/package.json:2-4`, `napi/parser/Cargo.toml:1-3`). OXC records that pre-1.0
crate releases do not follow SemVer (`crates/oxc_parser/CHANGELOG.md:929`) and explicitly
marks public changes as breaking in changelogs (`napi/parser/CHANGELOG.md:397-401`,
`crates/oxc_napi/CHANGELOG.md:46`).

Tests cover sync/async behavior, comments, language/module modes, semantic errors,
JS-versus-TS ESTree fields, declaration types, browsers, raw transfer, fixtures,
ranges/parents, tokens, and lazy mode (`napi/parser/test/parse.test.ts:13-186`,
`napi/parser/test/parse-raw.test.ts:34-177`, `napi/parser/package.json:73-76`).
Benchmarks separately measure standard sync/async, raw sync/async, parse-without-
deserialization, and deserialization (`napi/parser/bench.bench.js:63-108`).

## 9. Current official contribution policy

**Observed from current official OXC documentation on 2026-07-17:**

- prefer smaller PRs, and create an issue or discussion before a PR containing
  architectural changes;
- embrace data-oriented design and keep APIs simple and documented;
- treat runtime and compilation performance issues as bugs;
- minimize dependencies, compile time, heavy macros/generics, and binary-size growth;
- keep documentation as the source of truth;
- use explicit breaking-change commit notation;
- use GitHub Discussions for design questions, issues for bugs/features, and Discord for
  real-time guidance;
- disclose AI assistance and personally understand, validate, review, and test any
  AI-assisted contribution.

Primary sources:

- <https://oxc.rs/docs/contribute/rules>
- <https://oxc.rs/docs/contribute/introduction>
- pinned `CONTRIBUTING.md:1-21`

There is no separate formal RFC procedure established by the current public guide. The
concrete architectural gate is prior issue/discussion. This goal does not contact OXC;
the later implementation/upstream tranche must begin with that discussion rather than
presenting a large unsolicited parser patch.

## 10. Conventions the TSRX design should follow

The following are directly corroborated OXC conventions:

1. Small typed stable JS surface with generated declarations.
2. Lazy `program/module/comments/errors` result materialization.
3. Structured returned parser diagnostics, with transport/capability failures distinct.
4. UTF-16 JS offsets over UTF-8 Rust parsing.
5. Arena ownership kept internal; serialize before return.
6. One public ESTree contract, with ordinary and experimental fast transports behind it.
7. Explicit experimental names and runtime capability probes.
8. napi-rs target packages plus browser/WASI fallback where supported.
9. Parser and semantic passes kept conceptually separate.
10. Fixture/conformance parity between transports and separate parse/materialization
    performance benchmarks.
11. Data-oriented records, simple documented APIs, minimized dependency/compile/binary
    cost, and documentation-first change control.

## 11. Generic upstream pieces versus TSRX-specific ownership

**Inference.** Plausibly generic upstream contributions are narrow mechanisms that keep
OXC's JS/TS grammar intact: reusable ESTree record/serializer support, generalized span
conversion, structured diagnostic helpers, generated raw-transfer record support,
capability/loader improvements, or a carefully designed parser/binding extension seam.
Their acceptability is not established and requires an architectural discussion.

**Inference.** TSRX grammar recognition, TSRX AST discriminants, recovery policy,
language-specific diagnostics, CSS policy, compatibility facade, and Markless semantics
belong in oxc-tsrx unless maintainers explicitly invite them. OXC describes the core
parser as JavaScript/TypeScript (`ARCHITECTURE.md:109-116`,
`crates/oxc_parser/README.md:7-12`).

**Unresolved.** No primary source promises OXC will accept generic custom-language
hooks, nor does a public custom-grammar interface exist in the pinned source. The design
must present upstream-fit as a staged proposal with separable generic pieces, not claim
upstream acceptance.
