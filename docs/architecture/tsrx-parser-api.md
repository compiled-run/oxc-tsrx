# TSRX parser API architecture

## Status, scope, and non-goals

Covered criteria: UF-04, UF-05, UF-08, UF-12, UF-13.

This document is the implementation contract for a JavaScript-callable TSRX parser in
oxc-tsrx. Its status is **approved design input, not implemented behavior**. The design has
two deliberately separate products:

1. `@oxc-tsrx/parser`, the canonical OXC-shaped parser API; and
2. `@oxc-tsrx/tsrx-core-compat`, an exact `@tsrx/core@0.1.32` compatibility facade for
   Markless.

The canonical parser preserves the existing production invariants: one pinned OXC revision,
no fork or patch queue, fail-closed syntax recognition, compact overlays, raw embedded CSS,
and no JavaScript or process hop in the parse hot path. The facade adds synchronous throwing,
reference recovery, CSS materialization, event helpers, and caller-array behavior without changing
core acceptance. A raw-only canonical parser may qualify and ship while the compatibility facade
remains withheld. The compatibility product is also withheld unless a separate approved compliance
decision and every frozen dependency, compile-time, size, performance, fidelity, and failure gate
authorize its isolated CSS implementation.

This design does not implement Rust, JavaScript, declarations, tests, packages, generated files,
or release automation. It does not change Markless, contact OXC maintainers, publish packages,
or assert that OXC will accept any contribution. It does not add a public OXC grammar-plugin
interface, browser or WASI support, CSS formatting, CSS linting, source-map generation, a printer,
or compiler-pass changes. Markless must not require a compiler rewrite: the compatibility boundary
emits its consumed shape.

## Evidence base and convention/divergence ledger

Covered criteria: UF-01, UF-02, UF-03, UF-04, UF-08, UF-09, UF-10, UF-12, UF-14.

The binding rulings come from the [T003 tenets and rubric](../goals/parser-upstream-design/notes/T003-design-tenets-and-rubric.md),
grounded by the [parser seam inventory](../goals/parser-upstream-design/notes/T001-parser-seam-inventory.md),
the [OXC convention inventory](../goals/parser-upstream-design/notes/T002-oxc-upstream-conventions.md),
the [Rust/OXC architecture](rust-oxc-core.md), and the
[CSS compliance record](../../compliance/css-boundary.json). The Markless contract receipt is
summarized in the goal's [intake and oracle](../goals/parser-upstream-design/goal.md); its complete
consumed inventory is transcribed into Appendix A so implementation does not depend on a local
path outside this repository.

Every major choice is classified here. “OXC convention” means observed in the pinned official
OXC parser binding or current contribution rules. “Deliberate divergence” means a bounded TSRX or
compatibility requirement that is not presented as OXC behavior.

| Major choice | Classification | Evidence and ruling |
| --- | --- | --- |
| Typed `parse` and `parseSync` entry points | OXC convention | The pinned parser exposes the pair and recommends the synchronous path; see the [OXC convention inventory](../goals/parser-upstream-design/notes/T002-oxc-upstream-conventions.md) and pinned [declarations](https://github.com/oxc-project/oxc/blob/8e0ed2ebb96137fb1611cdbd5742d5cb46037d40/napi/parser/src-js/index.d.ts). |
| Direct ordinary-language binding before TSRX conversion | OXC convention | `.js`, `.jsx`, `.ts`, `.tsx`, and `.d.ts`/`dts` preserve the pinned binding's argument conversion, parse, UTF-16 conversion, serialization, lazy wrapper, and result shapes; only `.tsrx` enters the local lossless bridge and reconstruction pipeline. |
| Lazy `program`, `module`, `comments`, and `errors` | OXC convention | OXC materializes these result properties lazily; see the [pinned wrapper](https://github.com/oxc-project/oxc/blob/8e0ed2ebb96137fb1611cdbd5742d5cb46037d40/napi/parser/src-js/wrap.js). |
| Structured syntax diagnostics returned by the canonical API | OXC convention | OXC returns parse diagnostics and reserves throws for wrapper/capability faults; see [T002](../goals/parser-upstream-design/notes/T002-oxc-upstream-conventions.md). |
| UTF-16 JavaScript offsets derived from internal UTF-8 spans | OXC convention | The pinned binding converts AST, comments, module data, and diagnostics; see [OXC conversion code](https://github.com/oxc-project/oxc/blob/8e0ed2ebb96137fb1611cdbd5742d5cb46037d40/crates/oxc_napi/src/lib.rs). |
| `dts`, `commonjs`, and `undefined`/`null` options | OXC convention | The canonical declarations and argument normalization reproduce the pinned option surface. `tsrx` is the only added language. |
| Complete `EcmaScriptModule`, nullable diagnostic fields, and `Advice` severity | OXC convention | The canonical types reproduce the pinned nested import/export value-span records and diagnostic nullability rather than a lossy local summary. |
| Own enumerable configurable lazy getters | OXC convention | The ordinary result uses the pinned object-literal descriptor shape and one cache per property; nullable TSRX values use explicit initialized sentinels. |
| Allocator-contained serialization into an owned public representation | OXC convention | OXC serializes before its local allocator dies; the current adapter already contains OXC lifetimes in [its engine boundary](../../crates/oxc_adapter/src/lib.rs). |
| Ordinary lazy transport plus explicitly experimental raw transfer | OXC convention | The pinned package capability-gates raw transfer and keeps one semantic AST; see [raw transfer](https://github.com/oxc-project/oxc/blob/8e0ed2ebb96137fb1611cdbd5742d5cb46037d40/napi/parser/src/raw_transfer.rs). |
| Node-API ESM wrapper and target-specific optional packages | OXC convention | OXC's generated loader uses native target packages; the local eight-target contract is [enumerated here](../../packages/runtime/dist/targets.js). |
| Authored TSRX nodes reconstructed from a compact indexed overlay | Deliberate divergence | OXC parses JavaScript and TypeScript, while TSRX projection contains synthetic scaffolding. Authored reconstruction is required to prevent that scaffolding from becoming API. |
| `Program | null` on a canonical fail-closed syntax result | Deliberate divergence | `tsrx_syntax` can reject before an OXC program exists. Returning structured diagnostics without inventing a partial production AST preserves the [fail-closed core](rust-oxc-core.md). |
| `tsrx` and explicit recovery/capability options | Deliberate divergence | These are local language/product controls absent from pinned OXC. Recovery is isolated from production/compiler acceptance; capability controls fail before parse when unavailable. |
| One frozen `capabilities` object; no stable visitor API in v1 | Deliberate divergence | It replaces pinned `rawTransferSupported()` with complete validated native identity. `Visitor`, `visitorKeys`, and `experimentalGetLazyVisitor` are outside this parser tranche rather than falsely re-exported. |
| Throwing synchronous `parseModule` facade | Deliberate divergence | It exactly matches the first consumer but remains above the canonical returned-error surface. |
| Dedicated compatibility CSS implementation | Deliberate divergence | The shipping core remains `KEEP_RAW`. If separately approved, an isolated data-oriented Rust crate reproduces pinned CSS semantics for the facade, including the reference `TextEncoder` hash bytes; this does not reinterpret the current compliance record. |
| Eight targets in version 1, with browser and WASI deferred | Deliberate divergence | OXC supports broader targets, but oxc-tsrx version 1 follows its existing [eight native targets](../../packages/runtime/dist/targets.js) because the first consumer is Node tooling. |
| Prior issue or Discussion and small separable upstream proposals | OXC convention | Current official rules require discussion before architectural changes and favor small, measured contributions; see [contribution rules](https://oxc.rs/docs/contribute/rules) and [contribution introduction](https://oxc.rs/docs/contribute/introduction). |

## Requirements and ranked invariants

Covered criteria: UF-03, UF-04, UF-05, UF-06, UF-07, UF-08, UF-09, UF-10, UF-11.

The following invariants are ordered. A lower item cannot weaken a higher item.

1. One exact canonical OXC revision is imported only by `crates/oxc_adapter`. No fork,
   snapshot, vendor tree, Cargo patch, mixed revision, allocator borrow, or OXC Rust type crosses
   that boundary.
2. Production parsing is fail-closed. Recovery, CSS parsing, compatibility errors, helpers, and
   caller-array mutation cannot change compiler acceptance.
3. There is one authored-source enumerable ESTree/TypeScript/JSX/TSRX `Program` contract across
   ordinary, lazy, and experimental transports. Projection scaffolding is forbidden.
4. Every public coordinate names its domain. Stable version 1 uses original-source UTF-16
   half-open offsets; CSS descendants use payload-relative UTF-16 half-open offsets. Bytes remain
   internal.
5. The compatibility facade is exact at the consumed boundary: three runtime exports, synchronous
   `parseModule`, helper edge behavior, error and array semantics, CSS topology, ESM, and both type
   subpaths.
6. `KEEP_RAW` remains authoritative in scanning, projection, formatting, diagnostics, fixes, and
   the canonical parser. CSS trees are derived compatibility data and never replace `css`.
7. The overlay and owned transfer remain data-oriented: compact records, indices, string tables,
   bounded copies, and lazy JavaScript objects rather than an eager Rust node graph.
8. Packaging is correctness. A facade release requires all eight targets and mutual
   wrapper/addon/target/ABI/checksum/OXC-revision/Node-engine validation.
9. Differential semantics and separated performance baselines are frozen before implementation
   approval. A budget is never relaxed merely to admit a regression.

Violation of invariants 1–6 is an architectural stop. Violation of invariants 7–9 blocks the
affected stage or package release.

## Architecture and ownership boundaries

Covered criteria: UF-02, UF-03, UF-04, UF-06, UF-08, UF-14.

The dependency graph and parse path are acyclic. `tsrx_tape_schema` is a small,
OXC-independent leaf crate containing record tags, indices, spans, table layouts, schema versions,
and completeness flags. It has no parser, Node-API, OXC, CSS, serde, or JSON dependency. Both
`oxc_adapter` and `tsrx_parser_engine` consume it; `oxc_adapter` remains the only OXC-dependent
crate. Bindings depend downward on engines and never on one another:

```text
parser_napi_binding -> oxc_adapter, tsrx_parser_engine
tsrx_parser_engine -> tsrx_syntax, oxc_adapter, tsrx_tape_schema
oxc_adapter -> pinned OXC crates, tsrx_tape_schema
compat_napi_binding -> tsrx_compat_engine
tsrx_compat_engine -> tsrx_parser_engine, tsrx_compat_css, tsrx_tape_schema
```

`tsrx_compat_css` is OXC-independent. No leaf depends upward, no binding is callable from another
binding, and no cycle is possible. The TSRX-only data path is:

```text
authored source
  -> tsrx_syntax scan -> borrowed compact overlay
  -> legal TSX projection + affine byte map
  -> oxc_adapter parse -> allocator-borrowed OXC Program
  -> oxc_adapter serialization -> owned projected tape
  -> tsrx_parser_engine destructive reconstruction -> owned authored tape + UTF-16 index
  -> Node-API result handle -> independently lazy enumerable JS objects
```

`tsrx_syntax` continues to own recognition, structural byte spans, source fingerprinting, and
projection. It must expose a borrowed, compact view sufficient for reconstruction, without adding
`napi`, `serde`, JSON, or a general serialization dependency. The Rust seam is:

```rust
pub struct OverlayView<'a> {
    pub source_len: u32,
    pub tokens: &'a [OverlayToken],
    pub nodes: &'a [OverlayNode],
    pub clauses: &'a [OverlayClause],
    pub embedded: &'a [OverlayEmbedded],
    pub dynamic_tags: &'a [OverlayDynamicTag],
    pub dynamic_comments: &'a [ByteSpan],
    pub style_blocks: &'a [OverlayStyleBlock],
    pub first_root: u32,
}

pub struct ProjectionView<'a> {
    pub source: &'a str,
    pub segments: &'a [ProjectionSegment],
}
```

The view records expose the existing `kind`, `context`, `role`, byte spans, `owner`,
`parent`/`first_child`/`last_child`/`next_sibling`, `first_clause`/`last_clause`/`next`,
for-header `left`/`right`/`index`/`key`/`annotated`/`await`, catch binding count, dynamic opening
and closing spans, closing-comment range, `self_closing`, style content span, `first_root`, and
projection `projected`/`original_start`/`fixable`. Roots are not a contiguous range: iteration
starts at the actual `Overlay.first_root` and follows each root node's `next_sibling` until `NONE`;
child lists use the same first-child/next-sibling chain. `last_root` and `last_child` remain
construction/invariant aids. Missing indices use the existing `u32::MAX` sentinel at this
Rust-only seam. These are `Copy`-style borrowed records and accessors over existing vectors, not a
second owned syntax graph. Formatter-only lift manifests remain private.

`tsrx_tape_schema` owns the revision-neutral projected-record vocabulary.
`tsrx_parser_engine` owns authored reconstruction, UTF-16 indices, string/list tables,
public-contract planning, and transport version. Binding crates own only argument conversion,
result handles, and direct JS materialization.

The `@oxc-tsrx/parser` ESM wrapper owns the one source-family dispatch. After applying the pinned
experimental-option branch order, its source-family branch classifies an explicit `lang` first and
otherwise uses the pinned filename inference, including `.d.ts`; it forwards the original
filename, source string, and options object without rewriting them. Ordinary `.js`, `.jsx`, `.ts`,
`.tsx`, and `.d.ts`/`dts` calls select dedicated sync or async exports whose Node-API signatures and
argument converters are the pinned OXC signatures. This is the **direct OXC ordinary-language
binding path**. The native ordinary export calls `oxc_adapter::parse_ordinary` directly and
preserves pinned `get_source_type`, `Parser`, semantic pass, UTF-8-to-UTF-16 conversion, ESTree JSON
plus special-value fixes, module/comments/errors conversion, and `wrap` result construction. It
does not call `tsrx_parser_engine` or `tsrx_syntax` and does not allocate a TSRX tape.

Only explicit `lang: "tsrx"`, inferred `.tsrx`, and the compatibility facade select the TSRX
binding export. That export alone copies the original JavaScript string into `Vec<u16>` and owns
the scan, projection, boundary map, rare surrogate prepass/fixups, authored reconstruction, and
optional recovery/CSS compatibility lanes. Native entry assertions reject a wrapper/binding route
mismatch before parsing. Route fixtures cover explicit language overriding the filename, every
ordinary extension, `.d.ts`, extensionless pinned defaults, `.tsrx`, sync/async, null/undefined
options, and experimental-option ordering.

`oxc_adapter` remains the sole owner of `Allocator`, `ParserReturn`, OXC AST, source type,
diagnostics, comments, and module-record conversion. `tsrx_parser_engine` owns joining the serialized
projected tree to the overlay, deleting every synthetic lane, and emitting authored nodes.
The Node-API binding owns argument validation, async task scheduling, result handles, capability
errors, native route assertions, and JS materialization. The compatibility package owns strict
throwing, arrays, CSS, helpers, and type aliases.

## Canonical JavaScript API

Covered criteria: UF-01, UF-07, UF-09, UF-10.

`@oxc-tsrx/parser` is ESM. Except for the explicitly labeled additions below, declarations match
the pinned OXC surface. `Program` is defined in the next section.

```ts
export type Language = "js" | "jsx" | "ts" | "tsx" | "dts" | "tsrx";
export type SourceType = "script" | "module" | "commonjs" | "unambiguous";
export type AstType = "js" | "ts";

export interface ParserOptions {
  lang?: Language;
  sourceType?: SourceType | undefined;
  astType?: AstType;
  range?: boolean;
  preserveParens?: boolean;
  showSemanticErrors?: boolean;
  /** Deliberate TSRX divergence; production callers use none. */
  recovery?: "none" | "editor";
}

export interface Span { start: number; end: number }
export interface ValueSpan extends Span { value: string }
export interface Comment extends Span { type: "Line" | "Block"; value: string }

export interface ErrorLabel extends Span { message: string | null }
export declare const enum Severity { Error = "Error", Warning = "Warning", Advice = "Advice" }
export interface OxcError {
  severity: Severity;
  message: string;
  labels: ErrorLabel[];
  helpMessage: string | null;
  codeframe: string | null;
}

export declare const enum ImportNameKind {
  Name = "Name", NamespaceObject = "NamespaceObject", Default = "Default"
}
export interface ImportName {
  kind: ImportNameKind;
  name: string | null;
  start: number | null;
  end: number | null;
}
export interface StaticImportEntry {
  importName: ImportName;
  localName: ValueSpan;
  isType: boolean;
}
export interface StaticImport extends Span {
  moduleRequest: ValueSpan;
  entries: StaticImportEntry[];
}

export declare const enum ExportImportNameKind {
  Name = "Name", All = "All", AllButDefault = "AllButDefault", None = "None"
}
export declare const enum ExportExportNameKind { Name = "Name", Default = "Default", None = "None" }
export declare const enum ExportLocalNameKind { Name = "Name", Default = "Default", None = "None" }
export interface ExportImportName {
  kind: ExportImportNameKind;
  name: string | null;
  start: number | null;
  end: number | null;
}
export interface ExportExportName {
  kind: ExportExportNameKind;
  name: string | null;
  start: number | null;
  end: number | null;
}
export interface ExportLocalName {
  kind: ExportLocalNameKind;
  name: string | null;
  start: number | null;
  end: number | null;
}
export interface StaticExportEntry extends Span {
  moduleRequest: ValueSpan | null;
  importName: ExportImportName;
  exportName: ExportExportName;
  localName: ExportLocalName;
  isType: boolean;
}
export interface StaticExport extends Span { entries: StaticExportEntry[] }
export interface DynamicImport extends Span { moduleRequest: Span }
export interface EcmaScriptModule {
  hasModuleSyntax: boolean;
  staticImports: StaticImport[];
  staticExports: StaticExport[];
  dynamicImports: DynamicImport[];
  importMetas: Span[];
}

export declare class ParseResult {
  get program(): Program | null;
  get module(): EcmaScriptModule | null;
  get comments(): Comment[];
  get errors(): OxcError[];
}

export interface ParserCapabilities {
  readonly apiVersion: 1;
  readonly languages: readonly Language[];
  readonly target: string;
  readonly nodeApi: number;
  readonly nodeEngine: "^20.19.0 || >=22.12.0";
  readonly oxcRevision: string;
  readonly lazy: true;
  readonly async: true;
  readonly editorRecovery: boolean;
  readonly cssMaterialization: false;
  readonly rawTransfer: boolean;
}

export type ParserOperationalErrorCode =
  | "ERR_TSRX_INVALID_ARGUMENT"
  | "ERR_TSRX_UNSUPPORTED_TARGET"
  | "ERR_TSRX_NATIVE_NOT_INSTALLED"
  | "ERR_TSRX_NATIVE_INTEGRITY"
  | "ERR_TSRX_NATIVE_VERSION"
  | "ERR_TSRX_CAPABILITY_RECOVERY"
  | "ERR_TSRX_CAPABILITY_CSS"
  | "ERR_TSRX_CAPABILITY_RAW_TRANSFER"
  | "ERR_TSRX_RESOURCE_EXHAUSTED"
  | "ERR_TSRX_CANCELLED";

export class ParserOperationalError extends Error {
  readonly name: "ParserOperationalError";
  readonly code: ParserOperationalErrorCode;
}

export const capabilities: Readonly<ParserCapabilities>;

export function parseSync(
  filename: string,
  sourceText: string,
  options?: Readonly<ParserOptions> | undefined | null,
): ParseResult;

export function parse(
  filename: string,
  sourceText: string,
  options?: Readonly<ParserOptions> | undefined | null,
): Promise<ParseResult>;
```

`EcmaScriptModule` has no invented module-level span, source type, or flattened entry list.
`staticImports`/`staticExports` preserve statement spans and nested entries; all value spans carry
their exact string and span; nullable import/export name components remain null; dynamic import
records carry the expression span and module-request span; and `importMetas` are spans. Every span
is converted to authored UTF-16. `hasModuleSyntax` follows pinned OXC (static import/export and
`import.meta`, not dynamic import alone). Synthetic projection imports/exports are removed during
authored reconstruction.

`filename` is required and is used for language inference, diagnostics, and module identity; it is
not read from disk. `sourceText` is parsed exactly as supplied. Options are read once and never
mutated; omitted, explicit `undefined`, and explicit `null` all select defaults exactly as pinned
OXC does. An invalid option type or enum value, an unavailable experimental transport, an
unsupported host, a mismatched native
artifact, or allocation/transport failure throws a typed operational error with stable `name`,
`code`, and `message`. Grammar and semantic diagnostics do not throw: they populate `errors`.

The ESM wrapper's source-family discriminator dispatches before any TSRX source bridge. Ordinary
`.js`, `.jsx`, `.ts`, `.tsx`, and `.d.ts` filenames and explicit `lang: "js" | "jsx" | "ts" |
"tsx" | "dts"` use the direct OXC ordinary-language binding path and forward the original three
arguments to pinned-shape sync/async native exports. That route retains pinned argument conversion,
parse, serialization, special-value repair, lazy result wrapper, result shapes, and ordinary error
behavior. It performs no TSRX scan, projection, UTF-16 source copy, boundary-map construction,
surrogate prepass, tape reconstruction, recovery, or CSS work. Only explicit or inferred `tsrx`
selects the lossless UTF-16 bridge described below. Route selection is identical for `parseSync`
and `parse`; it changes transport ownership, not the public signature.

The canonical stable declaration intentionally does not advertise `experimentalLazy`,
`experimentalRawTransfer`, or `experimentalTokens`. The first two remain runtime-only pinned-OXC
experiments accessed through separately documented capability APIs if implemented; tokens are not
part of this TSRX design. This removes the draft's unsupported claim that both transport switches
were stable typed options. `tsrx`, `recovery`, capabilities, and nullable Program/module results
are the honest TSRX divergences.

`parseSync` performs native parsing on the calling thread. `parse` schedules the same native
operation off-thread and resolves to the same semantic tape; accessing a lazy getter materializes
JavaScript objects on the thread that accesses it. The async promise rejects only for the same
argument, loader, capability, transport, cancellation-before-start, or resource failures that
would make `parseSync` throw. Tests compare all four lazy properties for sync/async parity.

Each ordinary `ParseResult` is the pinned object-literal wrapper over the direct ordinary native
result, with four own accessor descriptors. For
`program`, `module`, `comments`, and `errors`, `enumerable === true`, `configurable === true`, the
getter is present, the setter is absent, and `writable` is not a descriptor member. Each getter
materializes once, caches by identity, and is independent: reading `errors` cannot build `program`.
Unlike pinned OXC's truthiness cache, the TSRX wrapper uses a separate initialized boolean per
property because `program` and `module` can validly be null. Thus even null is fetched/transferred
once and cached. A fatal
fail-closed syntax result has `program === null` and `module === null`, authored comments that were
lexically complete, and at least one error. A successful result has a `Program` and authored module
record; semantic errors do not erase either. Stable version 1 exposes ordinary lazy transport.
Runtime experimental transports, if present, must reproduce these descriptors and values and
callers must branch on `capabilities`.

## `@tsrx/core` compatibility facade

Covered criteria: UF-05, UF-06, UF-07, UF-08, UF-09, UF-10, UF-11.

`@oxc-tsrx/tsrx-core-compat` is a thin ESM package with exactly the three consumed runtime named
exports and no default export requirement:

```ts
import type { CommentWithLocation, CompileError, ParseOptions } from "./types";
import type { Program } from "./types/estree";

export function parseModule(
  source: string,
  filename?: string,
  options?: Readonly<ParseOptions>,
): Program;

export function isEventAttribute(name: string): boolean;
export function normalizeEventName(name: string): string;

export type { CommentWithLocation, CompileError, ParseOptions };
```

`parseModule` is always synchronous. It parses ES2022 modules with TypeScript, JSX, and TSRX,
allows return outside a function, and returns the authored enumerable `Program` directly. The
compatibility binding invokes the same core engine crates once and directly materializes the
compatibility graph; it does not call the canonical addon or translate a cached canonical graph.
Strict never retries as loose.
Filename is passed through as semantic input. Style-free input may omit it; any `<style>` requiring
the compatibility scope identity without a filename throws at the style start. Scope hashes,
selector rewriting inputs, path normalization, and case sensitivity must match
`@tsrx/core@0.1.32` exactly for the supplied filename.

The exact compatibility options are:

```ts
export interface ParseOptions {
  collect?: boolean;
  loose?: boolean;
  errors?: CompileError[];
  comments?: CommentWithLocation[];
}

export interface Position { line: number; column: number }
export interface SourceLocation { start: Position; end: Position }

export interface CompileError extends Error {
  code: string | undefined;
  pos: number | undefined;
  raisedAt: number | undefined;
  end: number | undefined;
  loc: SourceLocation | undefined;
  fileName: string | null;
  type: "fatal" | "usage";
}

export interface CommentWithLocation {
  type: "Line" | "Block";
  value: string;
  start: number;
  end: number;
  loc: SourceLocation;
  context?: CommentMetaData | null;
}

export interface CommentMetaData {
  containerId: number;
  childIndex: number;
  beforeMeaningfulChild: boolean;
}
```

The facade computes `collection = !!(collect || loose)`. Only in collection mode is an error sink
passed to the parser: the caller's `errors` array by identity when supplied, otherwise a private
array. Supplying `errors` or `comments` alone does not enable collection. Strict supplied arrays
remain untouched. A supplied comments array is used only in collection mode and is appended in
source order only after the native parse, including CSS parsing, returns successfully. A throw at
any point leaves that comments array untouched. No array is cleared, replaced, sorted,
deduplicated, frozen, or modified at an existing index.

Parser-generated Acorn failures are `SyntaxError`-like and may carry numeric `pos`, `raisedAt`, and
`loc`. Reference `error(...)` diagnostics are actual `Error` instances with the enumerable fields
shown above. Their own enumerable key order is `pos`, `raisedAt`, `fileName`, `code`, `end`, `loc`,
`type`; `message` and `stack` retain normal non-enumerable `Error` descriptors. The enumerable keys
exist even when their value is undefined. Collected instances have `type: "usage"`, and thrown
instances have `type: "fatal"`.
The missing-style-filename check and direct CSS parser failures are plain `Error` throws. Therefore
the facade never promises a universal error constructor, name, code, or defined numeric field.
Representable diagnostics append in observation order in collection mode, but an unrecoverable
parser or CSS failure still throws and may follow earlier error appends. Exact fixture behavior is
fixed by the mode matrix below.

`isEventAttribute` is exactly equivalent to
`name.startsWith("on") && name.length > 2 && name[2] === name[2].toUpperCase()`. It does not trim,
restrict the third code unit to ASCII, or validate the suffix. Therefore `onClick`, `on1`, and an
uncased third character qualify, while `onclick` does not.

`normalizeEventName` removes the first two UTF-16 code units unconditionally. It removes an
exact-case terminal `Capture` unless the lowercased remainder is `gotpointercapture` or
`lostpointercapture`, then lowercases the result. It does not call `isEventAttribute` first.
Consequently `onClickCapture` becomes `click`, while `onGotPointerCapture` and
`onLostPointerCapture` retain `capture`.

The package exports types-only `./types` and `./types/estree` subpaths. A source-compatible trial
installs an exact version alias such as
`"@tsrx/core": "npm:@oxc-tsrx/tsrx-core-compat@0.x.y"`; Markless import specifiers and compiler
passes remain unchanged. The alias is a qualification mechanism, not a publication or migration
performed by this design.

## AST and ParseResult contract

Covered criteria: UF-01, UF-02, UF-05, UF-06, UF-07.

There is one authored semantic tree, with two explicitly different materializations. The canonical
API uses the OXC-shaped authored `Program` and raw style nodes. The facade materializes the exact
`@tsrx/core@0.1.32` compatibility graph, including Acorn locations, comments, metadata, parsed CSS,
and reference key order. This is a deliberate compatibility view, not a second parse grammar and
not a claim that the smaller canonical graph is structurally identical.

In the compatibility graph every emitted node is an ordinary JavaScript object. Reference-required
properties are own enumerable writable configurable data properties in reference insertion order;
arrays are ordinary arrays in source order. Each Acorn node carries `type`, `start`, `end`, and
`loc: { start: { line, column }, end: { line, column } }`, with one-based lines and zero-based
columns. `range` is present only where the reference emits it. `Literal` includes enumerable
`value` and source-exact `raw`; regex and bigint values follow reference behavior. Comment objects
include `type`, `value`, `start`, `end`, `loc`, and `context` (normally null, otherwise the exact
parser metadata object), and attached `leadingComments`, `trailingComments`, `innerComments`,
`comments`, and `metadata` properties are present only when reference source creates them. The
materializer does not fill absent optional fields with null or undefined unless the reference does.

Projection wrappers, collision prefixes, helper declarations, sentinels, marker comments,
synthetic identifiers, synthetic attributes, projected offsets, affine maps, and projected
`sourceText` are forbidden in the public graph. Reconstructed controls replace their entire
scaffold subtree. Authored expressions and statements under a control are selected through affine
segments and overlay ownership, mapped to original spans, and reparented under the TSRX node.
Dynamic opening and closing expressions use overlay spans and parsed authored expression nodes;
raw style payloads come from the original source.

The complete custom compatibility declarations are:

```ts
interface Position { line: number; column: number }
interface SourceLocation { start: Position; end: Position }
interface Node {
  type: string;
  start?: number;
  end?: number;
  loc?: SourceLocation;
  range?: [number, number];
  metadata?: BaseNodeMetaData;
  comments?: Comment[];
  leadingComments?: Comment[];
  trailingComments?: Comment[];
  innerComments?: Comment[];
}
interface BaseNodeMetaData {
  scoped?: boolean; path: Node[]; has_template?: boolean; source_name?: string;
  source_length?: number; module_keyword?: "module" | "namespace";
  is_capitalized?: boolean; commentContainerId?: number; parenthesized?: boolean;
  native_tsrx?: boolean; native_tsrx_template_block?: boolean; dynamicElement?: boolean;
  templateMode?: "script" | "template"; script_only?: boolean;
  tsrxDirective?: "if" | "for" | "switch" | "try"; ts_name?: string;
  delegated?: unknown; returned_tsrx_return?: ReturnStatement; styleScopeHash?: string;
  css?: { scopedClasses: Map<string, { start: number; end: number; selector: unknown }>;
    hash: string };
  elementLeadingComments?: Comment[]; returns?: ReturnStatement[]; has_return?: boolean;
  has_throw?: boolean; has_continue?: boolean; is_reactive?: boolean; lone_return?: boolean;
  regular_js?: boolean; returned_tsrx_child?: boolean; forceMapping?: boolean;
  generated_loop_skip_if?: boolean; lazy_id?: string; disable_verification?: boolean;
  lazy_param_binding_mappings?: Array<{ source: Identifier; generated: Identifier | Literal }>;
}

interface TSRXExpression extends Node {
  type: "TSRXExpression";
  expression: Expression;
  metadata: BaseNodeMetaData;
}

interface JSXCodeBlock extends Node {
  type: "JSXCodeBlock";
  body: Statement[];
  render: Node | null;
  metadata: BaseNodeMetaData;
  innerComments?: Comment[];
}

interface JSXIfExpression extends Node {
  type: "JSXIfExpression";
  statementType: "IfStatement";
  test: Expression;
  consequent: Statement;
  alternate: Statement | null;
  metadata: BaseNodeMetaData;
}

interface JSXForExpression extends Node {
  type: "JSXForExpression";
  statementType: "ForStatement" | "ForInStatement" | "ForOfStatement";
  body: Statement;
  init?: VariableDeclaration | Expression | null;
  test?: Expression | null;
  update?: Expression | null;
  left?: VariableDeclaration | Pattern;
  right?: Expression;
  await?: boolean;
  index?: Identifier | null;
  key?: Expression | null;
  empty?: BlockStatement | null;
  metadata: BaseNodeMetaData;
}

interface JSXSwitchExpression extends Node {
  type: "JSXSwitchExpression";
  statementType: "SwitchStatement";
  discriminant: Expression;
  cases: SwitchCase[];
  metadata: BaseNodeMetaData;
}

interface JSXTryExpression extends Node {
  type: "JSXTryExpression";
  statementType: "TryStatement";
  block: BlockStatement;
  handler: (CatchClause & { resetParam?: Pattern | null }) | null;
  finalizer: BlockStatement | null;
  pending?: BlockStatement | null;
  metadata: BaseNodeMetaData;
}

interface TSRXJSXOpeningElement extends JSXOpeningElement {
  name: MemberExpression | JSXIdentifier | JSXNamespacedName | JSXExpressionContainer;
}
interface TSRXJSXClosingElement extends JSXClosingElement {
  name: MemberExpression | JSXIdentifier | JSXNamespacedName | JSXExpressionContainer;
}
interface JSXStyleElement extends Node {
  type: "JSXStyleElement";
  openingElement: TSRXJSXOpeningElement;
  closingElement: TSRXJSXClosingElement | null;
  metadata: BaseNodeMetaData;
  children: StyleSheet[];
  css?: string;
  unclosed?: boolean;
}

interface CssBase { start: number; end: number; loc?: SourceLocation;
  innerComments?: Comment[]; leadingComments?: Comment[]; trailingComments?: Comment[] }
interface StyleSheet extends CssBase { type: "StyleSheet"; children: Array<Atrule | Rule>;
  source: string; hash: string }
interface Atrule extends CssBase { type: "Atrule"; name: string; prelude: string;
  block: Block | null }
interface Rule extends CssBase { type: "Rule"; prelude: SelectorList; block: Block;
  metadata: { parent_rule: Rule | null; has_local_selectors: boolean;
    is_global_block: boolean } }
interface SelectorList extends CssBase { type: "SelectorList"; children: ComplexSelector[] }
interface ComplexSelector extends CssBase { type: "ComplexSelector";
  children: RelativeSelector[];
  metadata: { rule: Rule | null; used: boolean; is_global?: boolean } }
interface RelativeSelector extends CssBase { type: "RelativeSelector";
  combinator: Combinator | null; selectors: SimpleSelector[];
  metadata: { is_global: boolean; is_global_like: boolean; scoped: boolean } }
interface TypeSelector extends CssBase { type: "TypeSelector"; name: string }
interface IdSelector extends CssBase { type: "IdSelector"; name: string }
interface ClassSelector extends CssBase { type: "ClassSelector"; name: string }
interface AttributeSelector extends CssBase { type: "AttributeSelector"; name: string;
  matcher: string | null; value: string | null; flags: string | null }
interface PseudoElementSelector extends CssBase { type: "PseudoElementSelector"; name: string }
interface PseudoClassSelector extends CssBase { type: "PseudoClassSelector"; name: string;
  args: SelectorList | null }
interface Percentage extends CssBase { type: "Percentage"; value: string }
interface NestingSelector extends CssBase { type: "NestingSelector"; name: "&" }
interface Nth extends CssBase { type: "Nth"; value: string }
interface Combinator extends CssBase { type: "Combinator"; name: string }
interface Block extends CssBase { type: "Block"; children: Array<Declaration | Rule | Atrule> }
interface Declaration extends CssBase { type: "Declaration"; property: string; value: string }
type SimpleSelector = TypeSelector | IdSelector | ClassSelector | AttributeSelector |
  PseudoElementSelector | PseudoClassSelector | Percentage | NestingSelector | Nth;
```

The declaration surface also preserves these accepted-only Ripple normalization shapes; the
facade parser does not normally emit them:

```ts
interface Attribute extends Node { type: "Attribute"; name: Identifier; value: unknown;
  shorthand?: boolean; metadata: BaseNodeMetaData }
interface SpreadAttribute extends Node { type: "SpreadAttribute"; argument: Expression;
  metadata: BaseNodeMetaData }
interface Element extends Node { type: "Element"; id: Expression;
  attributes: Array<Attribute | SpreadAttribute>; children: Node[];
  openingElement: JSXOpeningElement; closingElement: JSXClosingElement | null;
  selfClosing?: boolean; unclosed?: boolean; isDynamic?: boolean; css?: string;
  metadata: BaseNodeMetaData; start: number; end: number }
interface TsrxFragment extends Node { type: "TsrxFragment"; children: Node[];
  openingElement?: JSXOpeningFragment; closingElement?: JSXClosingFragment | null;
  selfClosing?: boolean; attributes?: Array<Attribute | SpreadAttribute>;
  metadata: BaseNodeMetaData & { tsrx_code_block_chain?: boolean };
  start: number; end: number }
interface Text extends Node { type: "Text"; expression: Expression;
  metadata: BaseNodeMetaData }
```

The CSS declarations are both the accepted and emitted compatibility shapes. Source construction
fixes their emitted topology and key order: `StyleSheet` has `source, hash, type, children, start,
end`; `Rule` has `type, prelude, block, start, end, metadata`; `SelectorList` has `type, start, end,
children`; `ComplexSelector` has `type, start, end, children, metadata`; `RelativeSelector` has
`type, combinator, selectors, start, end, metadata`; and leaf nodes use their source construction
order. `parse_style` does not synthesize CSS `loc` or comment properties. Rule metadata starts as
`{ parent_rule: null, has_local_selectors: false, is_global_block: false }`; complex metadata starts
as `{ rule: null, used: false }`; relative metadata starts with all three booleans false.

The canonical raw parser emits byte-exact `JSXStyleElement.css` and no parsed CSS tree. Its
canonical style child representation is empty. The compatibility materializer emits the exact
opening/closing element topology and `children: [StyleSheet]`; an unclosed recovered style has
`closingElement: null` and own enumerable `unclosed: true`. Empty or absent CSS is not substituted
for a failed parse.

`ParseResult.program`, `comments`, `module`, and `errors` derive from one parse and four separable
authored tables, each transferred once as specified in the lifecycle section. The module record
reflects authored imports/exports only: synthetic projection helpers and
dynamic-tag scaffolds cannot create entries. Comments reflect authored comments only, in source
order, with delimiters excluded from `value` and included in `start`/`end`. Comments inside raw CSS
belong to the CSS payload and are not JavaScript comments. Diagnostics label authored syntax only;
an unmappable synthetic-only OXC diagnostic is suppressed from public results and counted in
internal qualification telemetry, never mapped approximately.

Materialization stores every source string needed for `raw`, cooked literal/template values,
comments, CSS, and filename-derived fields from the original UTF-16 authority. Metadata objects,
including `path` arrays and any node back-references, are graph references rather than JSON
approximations. Generic traversal uses the frozen reference visitor keys and therefore sees every
node-valued child listed here while excluding metadata back-reference fields exactly as the
reference walker does. Appendix A is normative for every Markless-consumed discriminant and field.
`Element`, `TsrxFragment`, `Text`, `Attribute`, and `SpreadAttribute` are accepted type shapes made
by Ripple normalization, not `parseModule` output; they remain in declarations and traversal
fixtures but are not emitted unless a reference fixture emits them.

## Offset and coordinate domains

Covered criteria: UF-02, UF-06, UF-07, UF-08, UF-09, UF-11.

Internal scanner, overlay, projection map, OXC AST, and pre-serialization spans are zero-based,
half-open UTF-8 byte ranges. Stable public `start`, `end`, optional `range`, comments, module
entries, diagnostic labels, and compatibility `pos`/`raisedAt`/`end` are zero-based, half-open
UTF-16 code-unit offsets into the original `sourceText`. CSS `StyleSheet` descendants use
zero-based, half-open UTF-16 offsets into the exact `JSXStyleElement.css` string. The
`JSXStyleElement` itself remains in the original-source domain.

Ordinary-language requests dispatch first to the pinned OXC argument-conversion and span-conversion
path. They do not allocate an original UTF-16 source copy, TSRX boundary map, or surrogate fixup
table; their public offsets and strings are exactly those produced by the pinned binding.

For `.tsrx` requests and the compatibility facade only, the Node-API bridge copies the JavaScript
input losslessly into `Vec<u16>`; that original buffer is the authored slicing, location, and
public-offset authority for the TSRX request. Well-formed UTF-16 takes a direct UTF-8 fast path plus
one monotonic UTF-8-boundary-to-UTF-16 map.
A span endpoint that is not a mapped boundary is an internal invariant failure, never rounded.
CRLF contributes two UTF-16 units; line calculation treats the pair as one terminator and resets
the column after `\n`. Astral scalar values contribute a surrogate pair and two UTF-16 units.

Lone surrogates enter the explicit **WTF-8/UTF-16 compatibility lane**. A UTF-16 lexical prepass
recognizes only reference-equivalent opaque payload contexts: quoted strings, template raw parts,
regular-expression bodies, comments, JSX text/attribute text, and raw style payloads. It substitutes
one lexically inert BMP private-use scalar for each lone surrogate only in those contexts and
records original UTF-16 index, surrogate value, lexical context, and substituted UTF-8 byte
interval. This is collision-safe even when the original contains that scalar: fixups are keyed by
position, original occurrences have no fixup, and every affected raw/cooked value is reconstructed
from its node span and the original `Vec<u16>`, never by searching substituted output. The adapter
parses this lexically equivalent UTF-8 buffer and maps
all spans through the recorded boundary table. The authored materializer then overwrites raw,
cooked, literal, regex, template, JSX text, comment, CSS, module-value, diagnostic-source, and
filename-sensitive values from the original UTF-16 slices using reference-compatible decoders.

A lone surrogate in a syntactically active context is never disguised as an identifier or token.
The lane returns the same fail-closed acceptance class and UTF-16 error position frozen from the
reference invalid-context fixtures. If the prepass cannot prove a context, substitution is not
performed and the result is a structured fail-closed parse error, not replacement text or a
categorical input rejection. No production result is returned from a substituted parse until all
fixups are consumed exactly once and every public slice round-trips to the original `Vec<u16>`.
If the reference accepts a context that this lane cannot reproduce exactly, the differential gate
stops that implementation and withholds the affected product; it may not turn the case into a
public compatibility limitation.

CSS conversion uses a separate index whose zero is the first UTF-16 unit of `css`. It never adds
the module offset of the `<style>` payload to selectors. When the compatibility layer needs a
module edit, it explicitly computes `payloadStart + cssRelativeOffset` in UTF-16 after proving both
domains. Raw UTF-8 payload bytes remain the fidelity authority for core comparisons even though the
public `css` property is a JavaScript string.

CSS scope hashing is the deliberate exception to lossless string encoding. It operates on the
complete JavaScript preimage after U+000D removal and uses WHATWG `TextEncoder` bytes: valid scalar
sequences encode as UTF-8, while each unpaired UTF-16 surrogate encodes as U+FFFD (`ef bf bd`). It
must not reuse the WTF-8 placeholder/fixup bytes or hash an unpaired surrogate's WTF-8 encoding.
This difference changes only the compatibility hash; AST values, source slices, offsets, raw CSS,
and diagnostics continue to use the original `Vec<u16>` authority.

The conformance corpus includes, at minimum:

- an astral character before every node class, comment, module entry, and strict failure;
- astral characters inside a dynamic tag and style selector;
- `\n`, `\r\n`, and mixed endings before errors and style rules;
- empty and end-of-file spans; and
- selectors on both sides of an astral character and CRLF inside one CSS payload.
- lone high and low surrogates in every accepted opaque context and every invalid active context;
  valid pairs beside lone units; and parity for literals, templates, regexes, JSX, comments, CSS,
  module entries, diagnostics, and strict `pos`.

Assertions use UTF-16 `source.slice(start, end)` and `css.slice(start, end)` as their oracles and compare
explicit expected strings. Stable version 1 exposes no byte offset, byte range, byte position, or
conversion table. A later byte surface must be experimentally named with its domain, such as
`experimentalOriginalUtf8ByteRange`, and cannot overload `start` or `end`.

Qualification separately times ordinary direct binding, well-formed TSRX input, and the rare
WTF-8/UTF-16 lane. It counts the TSRX `Vec<u16>`, UTF-8 buffer, boundary/fixup table, and restored
copied bytes and enforces the frozen well-formed and surrogate-lane overhead budgets. No literal
zero-overhead claim is made without a measurement capable of establishing it.

## Diagnostics, strict mode, and loose/collect recovery

Covered criteria: UF-01, UF-05, UF-07, UF-09, UF-11, UF-13.

Canonical default parsing is fail-closed and result-oriented. `recovery: "none"` runs one scan and
one parse and never retries. Unsupported, malformed, or unterminated TSRX that prevents a complete
authored tree yields `program: null`, `module: null`, and returned `OxcError` records.
`showSemanticErrors` adds diagnostics only after syntax success.

`recovery: "editor"` is an isolated canonical exposure of the same native recovery machinery used
by the facade, translated into returned canonical diagnostics. It is not an insertion grammar.
Reference recovery continues through some broken markup, marks nodes `unclosed`, and, for a
mismatched closing tag that matches an ancestor, marks and pops intervening ancestors before
closing the match. In collect mode that mismatch reports the reference usage error. In loose mode
`#report_broken_markup_error` suppresses that diagnostic while preserving the same `unclosed`
topology. A closing tag matching no open ancestor, malformed active JavaScript, and direct CSS
failures remain unrecoverable throws in the facade. No design rule requires one diagnostic per
repair because the reference intentionally suppresses some loose diagnostics.

The native result carries `Complete`, `Recovered`, or `Failed` internally. Only `Complete` from
`recovery: "none"` may enter compiler, lint, format, fix, module analysis, or production paths.
Those paths invoke the fail-closed API themselves and never accept a caller-provided editor graph.
The canonical `editor` lane returns a recovered Program or null plus returned diagnostics; facade
throw behavior is isolated above it and cannot leak into canonical production parsing.

The 16-row facade matrix below is normative. `E`/`C` mean prefilled caller arrays with sentinel
entries. Fixture `V` is a complete file with one JS comment. `R` is a recoverable ancestor mismatch
with that comment (`<div><span>x</div>`): collect returns a graph with the `span` unclosed and one
`tsrx-mismatched-closing-tag` usage `Error`; loose returns the same topology and suppresses that
error. `M` has malformed CSS (`.a { color:`) and throws a plain `Error` from the style parser.
`U` has an unmatched closing tag (`<div></span>`) and throws an Acorn `SyntaxError`-like failure.
`return` means the Program is returned. `E+1` and `C+1` mean append after the sentinel without
changing array identity; `same` means byte-for-byte and identity unchanged. In `M` and `U`, the
chosen fixture has no earlier representable diagnostic, so `E` stays unchanged. A different file
may append earlier usage errors before a later unrecoverable throw, in observation order.

| `collect` | `loose` | E | C | `V` | `R` | `M` | `U` | Error array | Comment array |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| false | false | no | no | return | throw | throw | throw | absent | absent |
| false | false | yes | no | return | throw | throw | throw | same for every fixture | absent |
| false | false | no | yes | return | throw | throw | throw | absent | same for every fixture |
| false | false | yes | yes | return | throw | throw | throw | same for every fixture | same for every fixture |
| true | false | no | no | return | return, unclosed | throw | throw | private sink; `R` gets one | absent |
| true | false | yes | no | return | return, unclosed | throw | throw | `R`: E+1; others: same | absent |
| true | false | no | yes | return | return, unclosed | throw | throw | private sink; `R` gets one | `V`,`R`: C+1 after success; `M`,`U`: same |
| true | false | yes | yes | return | return, unclosed | throw | throw | `R`: E+1; others: same | `V`,`R`: C+1 after success; `M`,`U`: same |
| false | true | no | no | return | return, unclosed | throw | throw | private sink remains empty for `R` | absent |
| false | true | yes | no | return | return, unclosed | throw | throw | same for every fixture | absent |
| false | true | no | yes | return | return, unclosed | throw | throw | private sink remains empty for `R` | `V`,`R`: C+1 after success; `M`,`U`: same |
| false | true | yes | yes | return | return, unclosed | throw | throw | same for every fixture | `V`,`R`: C+1 after success; `M`,`U`: same |
| true | true | no | no | return | return, unclosed | throw | throw | private sink remains empty for `R` | absent |
| true | true | yes | no | return | return, unclosed | throw | throw | same for every fixture | absent |
| true | true | no | yes | return | return, unclosed | throw | throw | private sink remains empty for `R` | `V`,`R`: C+1 after success; `M`,`U`: same |
| true | true | yes | yes | return | return, unclosed | throw | throw | same for every fixture | `V`,`R`: C+1 after success; `M`,`U`: same |

The effective flag is exactly `!!(collect || loose)`; errors are passed only when it is true.
Comments are exported only when it is true and only after parse returns successfully. Strict
supplied arrays never enable collection and remain untouched. All appends preserve the supplied
array object, existing indices, and reference order. `CompileError` numeric fields use original
UTF-16 offsets when defined, and its `loc` uses one-based lines and zero-based UTF-16 columns.

## Embedded CSS policy and materialization

Covered criteria: UF-05, UF-06, UF-07, UF-08, UF-09, UF-11, UF-13.

The existing `KEEP_RAW` compliance record continues unchanged for canonical scanning, projection,
parsing, linting, formatting, diagnostics, fixes, and compiler acceptance. `tsrx_syntax` records the
payload span, projection uses a marker, formatting restores exact authored bytes, and `parser.node`
returns raw `css` without parsing it. The core graph contains no CSS parser, formatter, subprocess,
or CSS-derived decision.

The compatibility product makes an explicit **Deliberate divergence**: to remain viable if OXC
declines every generic proposal, it selects a dedicated data-oriented Rust implementation of the
pinned `@tsrx/core@0.1.32` CSS parser semantics in isolated `tsrx_compat_css`. This is not a silent
reinterpretation of `compliance/css-boundary.json`. Before any implementation, a separate approved
compliance decision must expressly supersede the canonical-OXC same-allocator condition only for
this compatibility product and freeze its dependency count, clean/incremental compile time,
stripped addon/package size, parse time, peak memory, fidelity, convergence, and malformed-input
gates. Without that approval, or if any gate fails, `tsrx_core_compat.node` and the facade are
withheld. Canonical `KEEP_RAW` remains in force.

There is no runtime pointer handoff between addons. `parser.node` invokes
`tsrx_parser_engine` and excludes `tsrx_compat_css`. `tsrx_core_compat.node` is a second Node-API
binding over the same engine crates: it invokes the core engine exactly once in-process, then runs
the isolated CSS/reference-compat materializer on the native authored tape, and directly creates
the compatibility graph. It does not load or call `parser.node`, reparse the outer module, walk a
full JavaScript object graph, mutate a canonical cached graph, or use a subprocess. Shared engine
source may be duplicated by static linking; stripped-symbol and section-size reports measure that
duplicate binary code per target and in aggregate.

For each style node, `css` and `StyleSheet.source` equal the exact original UTF-16 payload.
`StyleSheet.start` is 0, `end` is `source.length`, and every CSS descendant offset is half-open and
payload-relative. The exact hash preimage is `${filename}:${line}:${column}:${content}`, where
`line` is the one-based line and `column` the zero-based column of the opening `<style>` element,
and `content` is the unmodified payload. Remove every carriage return (`U+000D`) from the complete
preimage, then hash exactly the bytes returned by WHATWG `TextEncoder`, matching the reference's
`@noble/hashes` `utf8ToBytes` call. Valid scalar sequences use UTF-8; each unpaired high or low
UTF-16 surrogate is converted to U+FFFD and therefore contributes `ef bf bd`. Compute SHA-256 over
those bytes, take the first eight lowercase hexadecimal characters, and prefix `tsrx-`. This
hash-only replacement behavior is deliberately different from the lossless AST/source WTF-8 lane;
the Rust implementation must not hash WTF-8 surrogate bytes. Missing or empty filename with a style
throws the reference plain `Error` at style parsing; style-free source may omit it.

The following byte-exact vectors are normative. JavaScript escape notation denotes the stated
UTF-16 code units, not six literal backslash characters; the byte column is the complete encoded
preimage after removing all U+000D units.

| Complete JavaScript preimage | `TextEncoder` bytes (hex) | Required hash |
| --- | --- | --- |
| `"View.tsrx:1:0:.a{}"` | `566965772e747372783a313a303a2e617b7d` | `tsrx-92cba3a7` |
| `"View.tsrx:2:3:a\r\nb\rc"` | `566965772e747372783a323a333a610a6263` | `tsrx-f0e5f5fc` |
| `"View.tsrx:1:0:.a{content:" + "\uD800" + "}"` | `566965772e747372783a313a303a2e617b636f6e74656e743aefbfbd7d` | `tsrx-4ed992ab` |
| `"View.tsrx:1:0:.a{content:" + "\uDC00" + "}"` | `566965772e747372783a313a303a2e617b636f6e74656e743aefbfbd7d` | `tsrx-4ed992ab` |
| `"View.tsrx:1:0:.a{content:" + "\uD83D\uDE00" + "}"` | `566965772e747372783a313a303a2e617b636f6e74656e743af09f98807d` | `tsrx-a0925b69` |

The full vector set also varies filename (including unpaired surrogates), duplicate blocks,
line/column, CR placement, content astrals, and adjacent paired/unpaired surrogates.

The style node retains `openingElement`, `closingElement`, `metadata`, `css`, `children`, and, when
recovered, `unclosed`. A successful parse attaches exactly one `StyleSheet`. The CSS node fields,
metadata, construction order, and selector offsets are those fixed in the AST section. No empty or
partial tree stands in for failure.

CSS failure behavior follows the reference parser rather than a facade-generated diagnostic. In
strict and collect-without-loose modes, an empty ordinary declaration such as `color:` throws the
plain `Error` `CSS Declaration cannot be empty`; collect does not catch or append it. In loose mode,
that exact fixture returns a `Declaration` with `value: ""` and no diagnostic. Structural failures
still required in loose mode, such as an unterminated block or `read_value` reaching EOF, throw a
plain `Error` in all three modes. Because comments export only after the full parse succeeds, every
throw leaves the supplied comments array unchanged. The mode oracle covers all CSS grammar branches,
not just these discriminating fixtures.

Release gates include exact CSS graph snapshots, key/descriptors, source/hash vectors, selector
UTF-16 slices, strict/collect/loose malformed matrices, a one-outer-parse counter, zero
subprocesses, proof that parser-only calls load no CSS code, raw-style/formatter convergence, and
separate dependency/compile/memory/size budgets. Failure blocks only the compatibility product.

## Arena lifetime, serialization, and transport

Covered criteria: UF-02, UF-03, UF-04, UF-06, UF-07, UF-11.

Ordinary-language lifecycle is separate and ends before this section's TSRX tape states. The ESM
wrapper dispatches to the ordinary native export; Node-API performs the pinned argument conversion;
`oxc_adapter::parse_ordinary` runs the pinned parse, UTF-16 conversion, ESTree serialization,
module/comment/error construction, and allocator drop; and the pinned lazy wrapper consumes that
ordinary result. No original `Vec<u16>`, overlay, projection, boundary/fixup map, projected or
authored TSRX tape, recovery state, or CSS materializer exists in that lifecycle.

For `.tsrx` and compatibility calls, the exact serialization point is
`oxc_adapter::parse_to_projected_tape`. It creates the local
`Allocator`, borrows projected UTF-8, calls pinned OXC, and while `ParserReturn`, source, and arena
are alive walks the OXC AST once into `tsrx_tape_schema` records, comments, complete module records,
diagnostics, and special-value fixups. After serialization, the OXC return and allocator drop
inside the adapter before the owned projected tape returns. No callback, iterator, pointer, arena
string, OXC `Span`, `Program`, `SourceType`, module record, or diagnostic crosses the boundary.

Projected records are destructively consumed. Reconstruction mem-takes record columns and string
buffers, reuses their capacities only where element layout and lifetime proof permit, emits authored
records incrementally, and immediately releases scaffold nodes, projected-only strings, affine
segments, fixup paths, and tables after their last consumer. It does not retain a second complete
projected tape. Original `Vec<u16>`, UTF-8 source, projected source, UTF-16 boundary map, and
surrogate fixups remain live only through authored reconstruction and string restoration; then all
drop. Stable raw/cooked/comment/CSS/module strings needed later are owned once by authored tables.

The fixed TSRX lifecycle is:

| State | Live native state | Transition and mandatory release |
| --- | --- | --- |
| Input bridge | Original `Vec<u16>`; direct UTF-8 or substituted UTF-8; boundary/fixup tables | Build overlay/projection; account every source byte and code unit |
| OXC parse/serialize | Input state, projected source, arena AST, growing projected records | Finish projected serialization; drop OXC return and arena inside adapter |
| Authored reconstruction | Input state, shrinking projected records, growing authored records | Destructively consume records; drop each projected-only column/table after last use |
| Native result | Authored program/module/comment/error tables only | Drop both source buffers, overlay, projection, maps, fixups, and projected tables before returning to JS |
| First getter | One table is mem-taken from the result and a JS graph grows | Native and JS representations coexist transiently during direct construction; release the taken table before getter returns |
| Cached getter | Cached JS value plus untouched tables for unread getters | Wrapper cache owns the JS value; native storage for that property is empty and cannot be materialized twice |
| Result release | Cached JS values are JS-owned; unread native tables remain | Finalizer drops only unread native tables; no arena/source is retained |

Consequently a complete authored program tape and a complete cached JS Program are never retained
together after a getter returns. During that getter, the full taken native table and a partially
built JS graph necessarily coexist; during reconstruction, remaining projected records and partial
authored records coexist. These transient peaks are measured, not described as zero-copy.
`module`, `comments`, and `errors` each have independent mem-taken storage and initialized cache
sentinels. Compatibility shares the input, adapter-drop, destructive-reconstruction, and direct
materialization states, but `parseModule` has no lazy `ParseResult`: it consumes the completed
compatibility tape into the returned JS Program in the same native call and drops that tape before
return. It never creates or retains a canonical result tape, handle, or graph.

TSRX copied-byte accounting records original UTF-16 bytes (`2 * code_unit_count`), UTF-8 bridge bytes,
projected bytes, each record/table capacity, restored string bytes/code units, raw/CSS copies,
Node-API external/allocation bytes where observable, and duplicated addon text/data sections. Peak
gates separately bound the input/OXC, reconstruction, first-getter, and retained-result states.
Ordinary accounting instead records the pinned binding allocations plus only the separately
measured wrapper dispatch, native route branch, and loader costs; it cannot charge TSRX structures
to the ordinary baseline or hide them in aggregate process RSS.

Raw transfer is experimental and capability-gated only. It requires a 64-bit little-endian host,
the advertised Node-API/ABI, safe JS-owned backing memory, generated record layouts, bounds checks,
and parity for Program, module, comments, errors, special values, enumerability, and UTF-16 offsets.
The option throws `ERR_TSRX_CAPABILITY_RAW_TRANSFER` before parsing when unavailable. A raw layout
change increments the transport ABI even when the stable AST does not change. Raw transfer cannot
become the sole transport or a stable default until every target and semantic/performance oracle
passes.

Transport parity is defined by canonical graph-digest equality after materialization, identical property
descriptors, identical error ordering, and identical source slices—not by shared bytes or object
identity. Upgrading OXC changes only adapter code and its serializer tests; the stable tape and JS
contract change only through an explicit public-version decision.

## Packaging, targets, capabilities, and version binding

Covered criteria: UF-01, UF-04, UF-05, UF-10, UF-11, UF-13.

The package topology is:

```text
@oxc-tsrx/parser                 ESM API, declarations, lazy wrapper, loader
@oxc-tsrx/tsrx-core-compat       ESM facade, helpers, compatibility declarations
@oxc-tsrx/native-<target>        existing optional target package family
  parser.node                    canonical addon
  tsrx_core_compat.node          facade-only addon after CSS/recovery qualification
  existing CLI/LSP executables   unchanged package responsibilities
```

`@oxc-tsrx/parser` declares exact-version optional dependencies on all eight native packages.
`@oxc-tsrx/tsrx-core-compat` declares an exact dependency on the same-version parser and
exact-version optional native targets for its addon. Both require Node
`^20.19.0 || >=22.12.0`, matching the existing [packager](../../scripts/package-native.mjs) and the
pinned OXC parser. Both are `type: "module"`. No CommonJS shim, executable protocol, child process,
or JavaScript AST reconstruction is the primary path.

Version 1 requires these targets as one release set: Darwin arm64 and x64; Linux GNU arm64 and x64;
Linux musl arm64 and x64; and Windows MSVC arm64 and x64. The loader resolves OS, CPU, and Linux
libc, tries the adjacent verified addon and then the exact optional package, and never downloads,
builds, or selects a near version. An unsupported tuple throws `ERR_TSRX_UNSUPPORTED_TARGET`; a
missing package throws `ERR_TSRX_NATIVE_NOT_INSTALLED`; a bad checksum throws
`ERR_TSRX_NATIVE_INTEGRITY`; and an identity mismatch throws `ERR_TSRX_NATIVE_VERSION` with expected
and actual non-secret identities.

Each of the eight generated native `package.json` manifests retains the three existing executable
names in `oxcTsrx.binaries` and changes `files` to the exact set `bin`, `parser.node`,
`tsrx_core_compat.node`, `checksums.json`, `licenses`, `LICENSE`, `README.md`, and
`THIRD_PARTY_NOTICES.md`. Neither addon replaces `oxc-tsrx`, `oxc-tsrx-fmt`, or `oxc-tsrx-lsp`.
The manifest schema advances to 2 and adds `oxcTsrx.addons` with exactly two entries conforming to
this generated-file contract:

```ts
interface NativeAddonManifest {
  role: "canonical-parser" | "tsrx-core-compat";
  file: "parser.node" | "tsrx_core_compat.node";
  bytes: number;
  sha256: string; // exactly 64 lowercase hexadecimal characters
  object: {
    format: "mach-o" | "elf" | "pe";
    imageKind: "dynamic-library";
    bits: 64;
    architectures: ["arm64"] | ["x64"];
    os: "darwin" | "linux" | "win32";
    libc: "glibc" | "musl" | null;
  };
  nodeApi: number;
  apiVersion: 1;
  transportAbi: 1;
  oxcRevision: "8e0ed2ebb96137fb1611cdbd5742d5cb46037d40";
  capabilities: {
    lazy: boolean;
    async: boolean;
    editorRecovery: boolean;
    cssMaterialization: boolean;
    rawTransfer: boolean;
  };
}
```

The canonical entry has file/role `parser.node`/`canonical-parser`, `lazy` and `async` true, and
`cssMaterialization` false. The compatibility entry has
`tsrx_core_compat.node`/`tsrx-core-compat`, `editorRecovery` and `cssMaterialization` true, `async`
false, and `rawTransfer` false. Other bits equal the qualified build and are never inferred by the
loader. The surrounding `oxcTsrx` retains schema/protocol versions, exact matrix target and VS Code
target, OXC revision, and all three executable names (with `.exe` on Windows). The pack/readback
gate rejects a generated manifest lacking any field or containing an out-of-matrix combination.
The generated `checksums.json` schema 2 retains the existing `binaries` entries and adds an
`addons` object keyed by both exact filenames. Each addon record contains its role, byte length,
SHA-256, inspected object format/header, 64-bit architecture, OS/CPU/libc target identity,
dynamic-library image kind, Node-API version, public API version, transport/facade ABI, exact OXC
revision, and capability-bit set. The top-level record continues to bind package name/version,
Rust target/rustc identity, verification mode, and exact OXC revision. A role is semantic: a file
whose bytes/checksum are valid but whose exported identity reports the other role is rejected.

The wrapper reads and validates native identity before accepting a parse. Exact means string-equal
versions and revisions; semver compatibility ranges do not authorize a mismatch. The compatibility
loader additionally requires parser API version and facade ABI equality. A failed validation never
falls back to a process or a different transport. `capabilities` is created from the validated
native identity, so it cannot advertise an unshipped feature.

Packaging first validates each source addon as a Node-API dynamic library for the selected target,
then packs, reads the tarball back, hashes the packed bytes, inspects their headers, and compares the
exact tar entry set to `files`, manifest roles, and checksum records. Qualification rejects a
missing addon, an undeclared extra addon, parser/compat files or roles swapped, checksum/length
drift, executable-image/addon-image confusion, wrong architecture/OS/libc, wrong Node-API/API/ABI,
wrong OXC revision, or capability disagreement. All eight packages run the same readback checks.

Size reports cannot hide the shared-package cost. They record compressed and unpacked bytes for
the pre-parser three-executable package, `parser.node`, `tsrx_core_compat.node`, manifest/checksum
growth, legal/shared files, and the final package. The parser-only install-size gate is charged the
actual final optional package installed by a parser consumer—including the compatibility addon
that shares it—while also reporting the marginal canonical-addon bytes. The compatibility gate
reports its marginal addon bytes and the same actual package total. Excess shared-package cost can
therefore withhold one or both products even when individual addon sizes pass.

Browser and WASI are deferred because consumer one runs in Node build/dev/editor tooling, the
current repository qualifies eight native packages, recovery/CSS dependencies need a separate
portable audit, and raw transfer is host-specific. This is a declared version-1 divergence from
OXC's broader package matrix, not a browser fallback claim.

## Performance, dependency, compile-time, and binary-size posture

Covered criteria: UF-03, UF-08, UF-10, UF-11, UF-13.

The implementation must retain compact overlay records, indexed child lists, string/list tables,
bounded source copies, and lazy JS materialization. It must not create an eager permanent Rust
object per public AST node, clone the full source more than the measured transport requires, parse
CSS by default, enter recovery by default, or walk the full JS graph merely to repair special
values. Ordinary JS/JSX/TS/TSX/DTS paths dispatch before all TSRX work to the direct binding
described above. Their only permitted additions over pinned `oxc-parser` are the source-family
branch and package-loader/wrapper machinery, and even those costs must satisfy frozen budgets; the
design does not call them literally zero overhead.

The benchmark harness reports distributions, raw samples, toolchain, CPU/OS, Node version, package
versions, OXC revision, corpus hashes, warmups, sample policy, and allocator/RSS method. It measures
these boundaries separately:

1. pinned `oxc-parser` ordinary binding versus the candidate direct ordinary native export, with
   identical JS/JSX/TS/TSX/DTS inputs and properties accessed;
2. the incremental JS source-family branch alone, using an already loaded addon, and the package
   loader/capability validation alone, using matched warm/cold processes;
3. native TSRX scan plus OXC parse;
4. TSRX projection and authored reconstruction;
5. allocator-contained serialization;
6. first and cached JS materialization for each lazy property;
7. end-to-end `parseSync` and `parse` resolution, with and without property access;
8. peak/RSS memory, retained memory, allocation count, and copied source bytes;
9. cold loader and first-parse start time;
10. clean and incremental Rust compile time;
11. direct and transitive dependency count; and
12. stripped `parser.node`, facade addon, per-target package, and aggregate install binary size.

Matrices include ordinary JS, JSX, TS, TSX, `.d.ts`, and explicit `dts` controls; TSRX without
controls; dense controls; dynamic tags; styles; Unicode; large modules; valid corpus files; invalid
strict files; and recovery files. Ordinary instrumentation asserts zero entries into TSRX scan,
projection, UTF-16 source-copy, boundary-map, surrogate-prepass, tape-reconstruction, CSS, and
recovery counters. Canonical TSRX runs prove zero CSS parse calls and zero recovery repairs by
default. Compatibility runs report CSS and recovery separately. Raw transfer receives a separate
experimental row and can never hide ordinary-transport regressions.

Numeric budgets must be frozen from reproducible baselines before implementation approval. Each
budget records an absolute bound and, where a matched control exists, a ratio bound against pinned
OXC or the pre-parser product. Existing ordinary lint/format budgets remain independent and must
stay green. Once frozen, no budget may be weakened, widened, reclassified, or have its corpus
changed merely to admit a regression; changing a flawed measurement requires a written methodology
correction and rerunning both old and new candidates. A regression blocks the responsible stage or
experimental capability.

The ordinary release budget is independently blocking: candidate end-to-end time, allocation and
copied-byte counts, peak/retained memory, result materialization, and result-shape parity are paired
against the exact pinned `oxc-parser` package. The incremental JS/native route branch and package
loader are reported as separate rows rather than absorbed into TSRX work. Any ordinary-path metric
beyond its Stage-1 absolute or matched ratio budget blocks the canonical package and every facade
that depends on it, even when TSRX benchmarks pass.

Stage 1 fixes the measurement protocol before Rust/API implementation: exact corpus commit and
per-file SHA-256 list; exact Node, Rust, linker, OS, CPU, governor/power, allocator, package, and OXC
identities; ten warmups followed by thirty recorded samples in each of five clean processes; no
outlier deletion; median, p95, median absolute deviation, and raw samples; randomized corpus order
from a recorded seed; and cold-start runs in fresh processes. The numeric budget file records an
absolute ceiling and matched-control ratio for every time/byte/count family. A candidate passes
only when both bounds pass in at least four of five processes and the aggregate p95 passes; a noisy
baseline whose coefficient of variation exceeds 5% is rerun, never used
to widen a candidate budget. All raw logs, commands, environment identity, corpus hashes, and
budget derivations are retained before Stage 2 starts.

## Conformance and benchmark oracle

Covered criteria: UF-05, UF-06, UF-07, UF-08, UF-09, UF-10, UF-11, UF-13.

The semantic oracle is differential against exact `@tsrx/core@0.1.32`. It covers all 179 parser-
valid and 12 parser-invalid pinned Markless cases, but the existing formatter/reparse/convergence
gate is only one input. Formatter reparse and convergence are explicitly insufficient proof of AST
equivalence.

Before implementation approval, the oracle freezes:

- full enumerable object-graph captures for representative instances of every Appendix A
  discriminant and field, plus structural digests for all 179 valid files;
- `Object.keys`, property descriptors, array order, null/absent distinctions, literal values, and
  source slices for every enumerable node;
- exact invalid acceptance, thrown `name`/`message`/numeric UTF-16 `pos`, collected error fields and
  order, and comments for all strict/collect/loose combinations;
- dedicated astral, lone-surrogate, CRLF, mixed-ending, empty-span, EOF, dynamic-tag, and
  module-record fixtures;
- raw CSS equality, complete CSS AST snapshots, selector offsets, astral/CRLF CSS, malformed CSS,
  and filename-sensitive scope vectors, including the normative CR/high-surrogate/low-surrogate/
  valid-pair `TextEncoder` byte and hash values in the CSS section;
- helper probes including ASCII, digits, uncased Unicode, exact and wrong-case `Capture`,
  got/lost-pointer-capture, empty/short strings, and direct normalization of non-event names;
- successful resolution and declaration compilation for root, `./types`, and `./types/estree`;
- unsupported-target, missing optional package, wrong exact version, wrong ABI, wrong OXC revision,
  corrupt checksum, and Node-engine failures; and
- deep semantic parity across ordinary lazy, sync, async, and experimental raw transport when the
  capability exists; and
- ordinary `.js`, `.jsx`, `.ts`, `.tsx`, `.d.ts`, explicit `dts`, override, null/undefined-option,
  and extensionless fixtures against pinned `oxc-parser`, asserting identical argument failures,
  Program/module/comments/errors values and descriptors, special values, and async behavior while
  native counters prove no TSRX scan, projection, UTF-16 source copy, boundary map, surrogate
  prepass, tape reconstruction, CSS, or recovery work.

The structural digest is SHA-256 over a deterministic tagged binary object-graph encoding, never
`JSON.stringify`. Encoding assigns monotonically increasing object IDs on first encounter and emits
a back-reference tag plus ID on later encounters, preserving cycles such as metadata/path links.
Lengths and IDs are unsigned LEB128; tag bytes and all following rules are versioned in the capture
header.

- Primitives have distinct tags for undefined, null, false, true, finite number, negative zero,
  positive/negative infinity, NaN, bigint, and UTF-16 string. Finite numbers use their big-endian
  IEEE-754 binary64 bytes; NaN has one canonical tag; bigint uses sign plus big-endian magnitude;
  strings encode code-unit length followed by big-endian 16-bit units, preserving lone surrogates.
  This graph-digest string encoding is not the CSS hash encoding: the CSS oracle separately runs
  CR removal and WHATWG `TextEncoder`, and verifies U+FFFD bytes for each unpaired surrogate.
- Ordinary objects emit object ID, a frozen prototype-class tag, `Object.keys` count and exact key
  order. Each key is a UTF-16 string followed by its own descriptor: data/accessor, enumerable,
  configurable, writable for data, getter presence, and setter presence. Data values are encoded
  recursively. Accessors are invoked exactly once after descriptor capture and their observed value
  is encoded; function identity/source is a frozen exclusion, while getter/setter presence and all
  flags are not.
- Arrays emit ID, `length`, then for every integer from zero to length minus one a present/hole tag
  and value if present, followed by non-index `Object.keys` entries and descriptors. This preserves
  holes, undefined elements, extra enumerable keys, and key order.
- `RegExp` emits ID, source, flags, lastIndex, then enumerable own keys/descriptors. `Error` emits
  ID, prototype name, own `name`, `message`, `cause` presence/value, and every enumerable field and
  descriptor; nondeterministic `stack` is excluded before capture. `Map` and `Set`, if encountered,
  emit ID, size, insertion-ordered entries/values, then enumerable own keys/descriptors. Any other
  branded object is rejected until its encoding and exclusion decision are frozen.

Exclusions are frozen before the first capture and are limited to function identity/source,
`Error.stack`, host-specific prototype identity beyond the declared class tag, and test-harness
timing. Node types, keys, descriptor flags, undefined/null distinctions, metadata, maps, source
locations, raw/cooked values, offsets, errors, comments, CSS topology, filename behavior, array
holes, and order are never excluded. Representative full graph fixtures are retained beside all
digests for diagnosis.

Two clean reference captures from independent processes must have identical full fixtures and
digest bytes before they become the oracle. Mutation tests independently change one field, one
descriptor flag, one key order, one null/undefined choice, one array hole, and one offset; every
mutation must fail. For accepted-only normalization shapes, fixture-driven unchanged Markless
traversal proves declaration/runtime acceptance without requiring `parseModule` to emit them.

Every matrix cell has a binary pass condition. A transport, mode, target, or facade is absent from
release if its cell fails. The performance oracle described in the prior section is frozen in the
same first stage so semantic repairs cannot move the baseline. The retained receipt records corpus
commit, package integrity, fixture hashes, commands, environment, and raw results.

Stage 1 captures design-fixed behavior; it does not select recovery, CSS, string, AST, transport,
or error semantics. Any reference result that contradicts this source-fixed contract returns the
design to review before implementation rather than allowing the implementation to choose.

## Upstream boundary and engagement plan

Covered criteria: UF-04, UF-12, UF-14.

The ownership boundary is explicit:

| Mechanism or policy | Generic-upstream candidate | TSRX-owned responsibility |
| --- | --- | --- |
| Stable ESTree tape generation | Reusable record generation or serializer hooks that preserve ordinary OXC grammar | TSRX node tags, reconstruction, and public contract |
| Span conversion | General UTF-8-to-UTF-16 helpers and diagnostic conversion | Original/CSS domain policy and TSRX repair positions |
| Lazy/raw transport | Generated lazy records, capability probes, safe raw machinery | TSRX tape schema, feature qualification, and fallback policy |
| Loader integrity | Exact-version loader, checksum, target, and capability mechanisms | Eight-target release decision and facade pairing |
| Parser extension seam | A narrow mechanism only if OXC maintainers request it | Scanning, projection, overlays, dynamic tags, and authored reconstruction |
| Diagnostics | Generic structured diagnostic DTO helpers | TSRX codes, unsupported grammar, and recovery diagnostics |
| Language policy | None presumed | Fail-closed grammar, source-verified compatibility recovery, CSS boundary, event helpers, and Markless semantics |
| Compatibility package | None | `parseModule`, arrays, strict errors, types, aliases, and CSS attachment |

OXC acceptance is unknown. No public OXC custom-grammar/plugin interface is assumed or described as
existing. oxc-tsrx remains viable if every generic proposal is declined because the exact revision
stays behind `oxc_adapter` and all TSRX semantics stay out of OXC.

Any later architectural contribution begins with an OXC GitHub issue or Discussion, not a pull
request. Contributions are small and separable: for example, a span helper is independent of a
serializer mechanism, and neither includes TSRX grammar. Each proposal includes API documentation,
focused tests, runtime benchmarks, compile-time/dependency/binary-size evidence, and the non-TSRX
use case. It follows current [OXC contribution rules](https://oxc.rs/docs/contribute/rules).

A human contributor must understand, rerun, and validate every change, incorporate maintainer-
authored validation criteria, and disclose AI assistance as required by OXC policy. Maintainer
feedback can narrow or reject upstream candidates but cannot silently move TSRX policy into OXC or
weaken local gates. This design tranche performs no contact and makes no readiness or endorsement
claim.

## Implementation stages and exit gates

Covered criteria: UF-02, UF-03, UF-04, UF-05, UF-08, UF-09, UF-10, UF-11, UF-12, UF-13, UF-14.

This tranche contains no implementation. Later work is ordered as follows; a stage starts only
after its prerequisite gate is retained in a receipt.

| Stage | Prerequisites | Owned outputs | Binary exit gate | Rollback, fallback, and explicit stop |
| --- | --- | --- | --- | --- |
| 1. Freeze conformance and performance oracles | This revised design and exact pinned sources | Two deterministic reference graph captures; 179/12 corpus; fixed AST/error/offset/CSS/string/package fixtures including `TextEncoder` bytes; pinned `oxc-parser` ordinary baseline with separate JS/native branch and loader rows; exact identities, raw evidence, variance policy, numeric budgets; compatibility-only CSS compliance decision | Graph bytes/digests match twice; mutation suite fails; ordinary result/descriptor parity holds; hash vectors match; every expected result is behavior already fixed here; numeric baseline reruns satisfy declared variance | Stop before Rust/API work if behavior cannot be captured deterministically, an ordinary budget cannot be frozen, or CSS approval is absent; Stage 1 never chooses semantics |
| 2. Leaf schema and overlay seam | Stage 1 green | OXC-independent `tsrx_tape_schema`; borrowed `tsrx_syntax` overlay/projection views using `first_root`/`next_sibling` | Dependency audit is acyclic; nested-root/control snapshots reconstruct every custom node; syntax/schema crates add no Node-API/OXC/CSS/serde dependency | Revert accessors/schema without touching scan/projection; stop on a cycle or permanent node graph |
| 3. Allocator-contained serialization | Stage 2 green | Direct `oxc_adapter::parse_ordinary` preserving the pinned binding pipeline; TSRX-only `oxc_adapter::parse_to_projected_tape`, revision-local serializer, destructive authored reconstruction | Ordinary binding snapshots match pinned OXC and instrumentation enters no TSRX lane; lifetime/dependency audit proves no OXC type/borrow escapes; exact module/error shapes, root chains, release points, copied-byte and transient-peak gates pass | Keep current adapter APIs; stop on ordinary-pipeline drift, fork, patch, mixed revision, cycle, or unbounded copies |
| 4. Canonical binding | Stage 3 green | `parser.node`, source-family dispatching ESM wrapper, exact declarations, configurable lazy getters with null sentinels, capabilities, sync/async | Canonical OXC-fidelity, module, diagnostics, descriptors, UTF-16/WTF-8, transport, memory, and size matrices pass on a host target; every ordinary extension/override takes the direct binding; separate branch/loader measurements and all ordinary metrics stay within frozen pinned-`oxc-parser` budgets | Ship nothing or later ship raw-only canonical parser after full target qualification; any ordinary regression blocks release; no facade dependency |
| 5. Recovery and CSS compatibility | Stages 1 and 4 green, plus approved compatibility CSS decision | Source-fixed collect/loose recovery, completeness marker, isolated Rust CSS materializer and compatibility tape | Reference unclosed/ancestor-pop and diagnostic suppression match; compiler fail-closed invariant; CSS topology/hash/modes and all compatibility-product gates pass | Disable recovery capability or withhold compatibility addon; canonical parser remains raw and fail-closed |
| 6. Facade and types | Stage 5 green | Direct `tsrx_core_compat.node` materialization, three exports, exact 16-mode mapping, helpers, root/`./types`/`./types/estree`, alias trial | One outer parse, no addon call/JS graph walk; graph digest, errors/comments/arrays, helpers, declarations, and unchanged-source Markless differential pass | Withhold compatibility package; no Markless compiler rewrite or AST adapter inside Markless |
| 7. Eight-target qualification | Stages 4 and, for facade release, 6 green | Both addons beside three executables in every optional package; role manifests, checksums, pack/readback, provenance | All eight missing/swapped/wrong-target, Node-engine, exact-version, API/ABI, OXC, checksum, duplicate-code, cold-start, dependency, and actual parser-install-size cells pass | Canonical release requires eight parser addons; facade independently waits for eight compatibility addons; no partial advertised target set |
| 8. Optional upstream proposals | Stable local evidence from earlier stages and explicit authority to engage | Issue/Discussion, then invited small generic contributions | Maintainer-authored validation, benchmarks, compile-size evidence, AI disclosure, and normal OXC review pass | Decline or withdraw without changing local architecture; never bundle TSRX policy into an unsolicited patch |

At every stage, a failed binary gate stops dependent stages. Rollback removes only the new layer;
existing lint, formatter, LSP, raw CSS, and fail-closed compiler lanes remain intact.

## Alternatives rejected and residual risks

Covered criteria: UF-02, UF-03, UF-04, UF-05, UF-08, UF-09, UF-10, UF-11, UF-12, UF-14.

Rejected alternatives are:

- Exposing projected OXC TSX: it leaks scaffolding, offsets, module entries, and a second AST.
- Returning OXC arena values or pinning an allocator in a JS object: it leaks revision/lifetime
  ownership and complicates upgrades.
- Building a permanent owned Rust AST mirroring every JS node: it violates the compact,
  data-oriented posture and forces eager allocations.
- Making `parseModule` the canonical API: it misstates OXC's result/error convention and couples
  core parsing to one consumer.
- Throwing syntax errors from canonical parse: it erases structured result behavior; throwing stays
  in the facade.
- Retrying strict input in loose mode: it makes production acceptance depend on editor policy.
- Globally replacing `KEEP_RAW` with parsed CSS: it silently reverses a recorded compliance
  decision and contaminates ordinary paths.
- Materializing CSS in JavaScript or a subprocess: it adds a hot-path/process boundary and weakens
  native dependency and integrity control.
- Publishing fewer than eight advertised native targets: it makes compiler installation
  nondeterministic across the existing support matrix.
- Browser/WASI in version 1: it expands ABI, dependency, and recovery/CSS qualification before the
  Node consumer contract is proven.
- Treating formatter convergence as AST proof: it does not cover enumerable topology, offsets,
  errors, comments, module data, or helpers.
- Sending a large TSRX grammar patch upstream: no such public extension point or acceptance exists.

### T005 disposition

| Finding | Severity and disposition | Revised sections | Source evidence | Binary acceptance test |
| --- | --- | --- | --- | --- |
| T005-R01 | Blocker resolved: exact compatibility AST and types | `@tsrx/core` compatibility facade; AST and ParseResult contract; Appendix A | Installed `@tsrx/core@0.1.32` `types/index.d.ts`, `src/plugin.js`, and [T005 critique](../goals/parser-upstream-design/notes/T005-adversarial-critique.md) | Every custom/CSS discriminant matches reference graph keys, descriptors, null/absence, topology, values, locations, metadata, raw slices, and declarations; one mutation fails |
| T005-R02 | Blocker resolved: exact facade modes | `@tsrx/core` compatibility facade; diagnostics/recovery; Appendix B | Installed 0.1.32 `src/parse/index.js`, `src/plugin.js`, `src/errors.js` | All 16 option combinations on complete, ancestor-mismatch, loose-suppression, malformed-CSS, and unmatched-close fixtures match return/throw, constructor/fields, array identity, append order/timing, comments, and unclosed topology |
| T005-R03 | Blocker resolved: pinned OXC API fidelity | Evidence ledger; architecture; canonical JavaScript API | Pinned [declarations](https://github.com/oxc-project/oxc/blob/8e0ed2ebb96137fb1611cdbd5742d5cb46037d40/napi/parser/src-js/index.d.ts), [wrapper](https://github.com/oxc-project/oxc/blob/8e0ed2ebb96137fb1611cdbd5742d5cb46037d40/napi/parser/src-js/wrap.js), `lib.rs`, and `types.rs` | Declaration/module/diagnostic/descriptor snapshots match; every ordinary language dispatches before the TSRX bridge and preserves pinned conversion, parse, serialization, lazy shapes, and frozen performance budgets; nullable TSRX getters transfer once |
| T005-R04 | Blocker resolved: CSS contract and native handoff | AST contract; offset/string domains; embedded CSS; packaging | Installed 0.1.32 `src/parse/style.js`, `src/utils/hashing.js`, `src/plugin.js`; `@noble/hashes@2.2.0` `utf8ToBytes`; [CSS record](../../compliance/css-boundary.json) | CSS graphs/modes/selector slices match; CR, astral, paired and unpaired-surrogate `TextEncoder` bytes and hashes match; one engine parse, zero subprocess/addon calls, no full JS walk/mutation, and parser-only binary contains/loads no CSS crate |
| T005-R05 | Blocker resolved: acyclic ownership and bounded live state | Architecture; arena lifetime; performance | [Overlay model](../../crates/tsrx_syntax/src/model.rs), [scanner](../../crates/tsrx_syntax/src/scanner.rs), pinned OXC allocator-contained serializer | Dependency audit is acyclic; ordinary calls allocate no TSRX bridge/tape state; root/child sibling chains reconstruct exactly; release assertions and source/copy/transient-peak measurements satisfy frozen budgets; getter leaves no taken native table |
| T005-R06 | High resolved: arbitrary JavaScript strings | Offset and coordinate domains; embedded CSS; arena lifetime; conformance oracle | Node UTF-16 semantics, installed Acorn-based 0.1.32 reference, pinned OXC UTF-16 conversion, and noble `TextEncoder` delegation | TSRX lone high/low surrogates, pairs, astral/CRLF, literals, regex/templates, JSX, comments, module values, errors, CSS, and strict positions match losslessly; CSS hash alone matches U+FFFD encoding; an unconsumed fixup or wrong hash byte fails closed and withholds release |
| T005-R07 | High resolved: canonical oracle | Performance posture; conformance oracle; Stage 1 | Exact 0.1.32 capture, pinned `oxc-parser` baseline, plus [T005 critique](../goals/parser-upstream-design/notes/T005-adversarial-critique.md) | Two clean tagged graph captures and digests are identical; field/descriptor/order/hole/null/offset/hash-byte mutations fail; ordinary route/result parity and separately measured branch/loader budgets pass; identities, variance, corpora, and raw evidence exist before Stage 2 |
| T005-R08 | Medium resolved: concrete native packaging | Packaging; Appendix B | Existing [target table](../../packages/runtime/dist/targets.js) and [native packager](../../scripts/package-native.mjs) | All eight pack/readbacks contain three executables and both role-bound addons; missing/swapped/wrong-target/API/ABI/OXC/checksum cases fail; actual shared parser-install bytes meet budget |

Concrete residual risks remain. The dedicated compatibility CSS parser may not satisfy fidelity,
filename, dependency, compile, performance, or size gates; then the facade stays unreleased. Exact
reference recovery may be too costly for the Markless editor corpus; then recovery capability and
the facade stay blocked.

OXC ESTree changes at a later pinned revision may require adapter-local normalization and could
exceed budgets. Raw transfer may be infeasible on some targets and therefore stays experimental or
false in capabilities. npm alias rights, package names, and provenance policies require release-
stage validation. Native addon coexistence with the current executable packages may increase
install size beyond the frozen budget. The TSRX-only UTF-16/WTF-8 bridge may fail differential or
performance gates, and the direct ordinary route may exceed its frozen pinned-`oxc-parser` budget;
either affected product remains withheld. Silent AST/source replacement and reference-incompatible
rejection remain forbidden. OXC may reject all generic proposals;
the non-fork local adapter remains the fallback.

## Upstream-fit rubric traceability

Covered criteria: UF-01, UF-02, UF-03, UF-04, UF-05, UF-06, UF-07, UF-08, UF-09, UF-10, UF-11, UF-12, UF-13, UF-14.

Each row is binary and maps the criterion to concrete design and evidence.

| Criterion | Design proof | Evidence | Rejection prevented |
| --- | --- | --- | --- |
| UF-01 | Exact `dts`/`commonjs`/nullable options, complete module and diagnostic shapes, configurable lazy getters, and pre-bridge ordinary dispatch preserving the pinned binding | Pinned OXC declarations/wrapper/lib via [T002](../goals/parser-upstream-design/notes/T002-oxc-upstream-conventions.md) | Core neither invents lossy OXC shapes, routes ordinary input through TSRX conversion, nor presents facade throws as OXC behavior |
| UF-02 | Leaf schema, acyclic dependency graph, first-root chain, exact adapter serialization/drop boundary, transport parity | [T001](../goals/parser-upstream-design/notes/T001-parser-seam-inventory.md) | No cycle, allocator borrow, or OXC type escapes |
| UF-03 | No TSRX state on ordinary calls; destructive TSRX tapes, mem-taken getters, explicit transient peaks/copies, and separated ordinary/TSRX numeric gates | [Rust/OXC architecture](rust-oxc-core.md) and pinned `oxc-parser` baseline | No retained full tape plus cached JS graph, ordinary UTF-16 source copy, or unmeasured transport |
| UF-04 | Sole exact-revision adapter and local upgrade boundary | [Adapter source](../../crates/oxc_adapter/src/lib.rs) | No fork, patch, snapshot, mixed revision, or direct import |
| UF-05 | Exact three-export facade, `CompileError extends Error`, successful-only collection comments, helpers, ESM, and type aliases | Installed 0.1.32 source summarized in [T005](../goals/parser-upstream-design/notes/T005-adversarial-critique.md) | Markless absorbs no parser/error adapter or compiler rewrite |
| UF-06 | Exact standard/custom/CSS enumerable graph, locations/raw/comments/metadata, emitted-versus-accepted distinction | Installed 0.1.32 declarations/plugin plus [syntax model](../../crates/tsrx_syntax/src/model.rs) | No vague ESTree claim, missing discriminant, or false topology |
| UF-07 | TSRX-only original `Vec<u16>` authority, fast UTF-8 plus WTF-8/UTF-16 fixups, original/CSS domains, and explicit hash-only `TextEncoder` replacement semantics | Pinned conversion evidence in [T002](../goals/parser-upstream-design/notes/T002-oxc-upstream-conventions.md) plus installed noble source | No mixed domains, AST/source surrogate replacement, WTF-8 CSS hashing, or categorical rejection of reference-valid strings |
| UF-08 | Unchanged canonical `KEEP_RAW`; separately approved isolated CSS product; exact source/`TextEncoder` hash/modes and one-engine-parse binding | [CSS boundary](../../compliance/css-boundary.json) and installed style/hash sources via [T005](../goals/parser-upstream-design/notes/T005-adversarial-critique.md) | No raw-only facade, global policy reversal, wrong surrogate hash bytes, cross-addon pointer, or second parse |
| UF-09 | Source-verified collection flag, ancestor mark/pop, loose suppression, successful comment export, and 16 exact option rows | Installed 0.1.32 parse/plugin behavior via [T005](../goals/parser-upstream-design/notes/T005-adversarial-critique.md) | Strict, collect, loose, plain Error, and unrecoverable behavior cannot substitute for one another |
| UF-10 | Both role-bound addons beside three executables, eight targets, pack/readback and exact identity/size binding | [Target table](../../packages/runtime/dist/targets.js) and [packager](../../scripts/package-native.mjs) | No missing/swapped/wrong-target addon, partial release, or hidden shared-package cost |
| UF-11 | Tagged cyclic graph encoding, exact `TextEncoder` hash vectors, ordinary-route/result/counter parity, two clean captures, mutation tests, full differential matrices, and preimplementation numeric budgets | [Seam inventory corpus analysis](../goals/parser-upstream-design/notes/T001-parser-seam-inventory.md) and pinned binding/hash sources | Formatter convergence, a vague JSON digest, or aggregate benchmarks cannot hide AST/hash/ordinary-path drift |
| UF-12 | Acceptance disclaimer, prior issue/Discussion, separable evidence, human validation and AI disclosure | [OXC rules](https://oxc.rs/docs/contribute/rules) | No endorsement claim, contact in this tranche, or unsolicited architecture patch |
| UF-13 | Eight prerequisite/output/gate/fallback stages; Stage 1 freezes ordinary pinned-package branch/loader budgets and design-fixed CSS hash bytes before Rust/API work | [T003 stage ruling](../goals/parser-upstream-design/notes/T003-design-tenets-and-rubric.md), corrected by [T005](../goals/parser-upstream-design/notes/T005-adversarial-critique.md) | No implementation-time semantic choice, unfrozen ordinary regression, ungated phase, or facade-coupled rollback |
| UF-14 | Generic-versus-TSRX responsibility table and viable all-local fallback | [T002 boundary](../goals/parser-upstream-design/notes/T002-oxc-upstream-conventions.md) | No presumed plugin interface or uninvited TSRX semantics in OXC |

## Appendix A: Markless node, field, helper, and type inventory

Covered criteria: UF-05, UF-06, UF-07, UF-08, UF-09, UF-11.

This appendix is normative and complete for the recorded Markless receipt.

| Category | Emitted and consumed discriminants |
| --- | --- |
| Program and declarations | `Program`, `ImportDeclaration`, `ImportSpecifier`, `ImportDefaultSpecifier`, `ImportNamespaceSpecifier`, `ExportNamedDeclaration`, `ExportDefaultDeclaration`, `VariableDeclaration`, `VariableDeclarator`, `FunctionDeclaration`, `FunctionExpression`, `ArrowFunctionExpression`, `ClassDeclaration` |
| Statements and standard control | `BlockStatement`, `ExpressionStatement`, `ReturnStatement`, `IfStatement`, `ForStatement`, `ForInStatement`, `ForOfStatement`, `SwitchStatement`, `SwitchCase`, `TryStatement`, `CatchClause` |
| Expressions and patterns | `Identifier`, `Literal`, `ArrayExpression`, `ObjectExpression`, `Property`, `MemberExpression`, `ChainExpression`, `CallExpression`, `NewExpression`, `AssignmentExpression`, `AssignmentPattern`, `UpdateExpression`, `UnaryExpression`, `BinaryExpression`, `LogicalExpression`, `ConditionalExpression`, `AwaitExpression`, `TemplateLiteral`, `TaggedTemplateExpression`, `ThisExpression`, `Super`, `MetaProperty`, `ArrayPattern`, `ObjectPattern`, `RestElement`, `SpreadElement` |
| TypeScript | `TSAsExpression`, `TSSatisfiesExpression`, `TSNonNullExpression`, `TSInstantiationExpression`, `TSModuleDeclaration` |
| JSX and TSRX | `JSXElement`, `JSXOpeningElement`, `JSXFragment`, `JSXIdentifier`, `JSXMemberExpression`, `JSXAttribute`, `JSXSpreadAttribute`, `JSXExpressionContainer`, `JSXEmptyExpression`, `JSXText`, `JSXCodeBlock`, `JSXStyleElement`, `JSXIfExpression`, `JSXForExpression`, `JSXSwitchExpression`, `JSXTryExpression`, `TSRXExpression` |
| CSS | `StyleSheet`, `Atrule`, `Rule`, `SelectorList`, `ComplexSelector`, `RelativeSelector`, `TypeSelector`, `IdSelector`, `ClassSelector`, `AttributeSelector`, `PseudoElementSelector`, `PseudoClassSelector`, `Percentage`, `NestingSelector`, `Nth`, `Combinator`, `Block`, `Declaration` |

Markless also accepts `Element`, `Fragment`, `BooleanLiteral`, `StringLiteral`, `NumericLiteral`, and
`NullLiteral`. These are accepted compatibility shapes, not normal emitted shapes: the parser emits
JSX elements/fragments and ESTree `Literal` unless the differential reference emits otherwise.
The installed declarations also describe normalized `Element` and `TsrxFragment`; core
`parseModule` emits JSX-shaped TSRX nodes, so `TsrxFragment` is not added to the emitted list.

Every child-valued enumerable property participates in generic traversal. The explicitly consumed
fields, in addition to enumerable topology, are:

| Shape | Explicitly consumed fields |
| --- | --- |
| Base and router | `type`, `start`, `end`, `loc`; optional `range`, `metadata`, `comments`, `leadingComments`, `trailingComments`, `innerComments`; all own enumerable reference keys/descriptors |
| Program, blocks, containers | `body`; code blocks also use `render` |
| Imports and exports | `source`, `specifiers`, `imported`, `local`, `importKind`, `declaration` |
| Functions, classes, variables | `id`, `params`, `body`, `async`, `declarations`, `kind`, `init` |
| Identifiers and literals | `name`, `value`, source-exact `raw`, regex/bigint fields, optional identifier/type metadata |
| Calls and construction | `callee`, `arguments`, `optional` |
| Members and chains | `object`, `property`, `computed`, `optional`, `expression` |
| Operators and branches | `argument`, `left`, `right`, `operator`, `test`, `consequent`, `alternate` |
| Arrays, objects, patterns | `elements`, `properties`, `key`, `value`, `method`, `shorthand`, `argument` |
| Return and expression statements | `argument`, `expression` |
| Switch | `discriminant` with `test` compatibility fallback, `cases`; case `test`, `consequent` |
| Try and catch | `block`, `handler`, `finalizer`, `pending`; handler `param`, `resetParam`, `body` |
| JSX elements | `openingElement`, `closingElement`, `children`; opening `name`, `attributes`, `selfClosing`; closing `name` |
| JSX attributes | `name`, `value`; spread `argument` |
| Expression containers | `expression` |
| TSRX code/if | code `body`, `render: Node | null`, optional inner comments; if `statementType`, `test`, statement-shaped `consequent`/`alternate`, `metadata` |
| TSRX for | `statementType`; classic `init`/`test`/`update`; in/of `left`/`right`; of `await`; optional `index`/`key`; statement `body`; `empty`; `metadata` |
| TSRX switch | `statementType`, `discriminant`, `cases`, `metadata`; case `test`, `consequent` |
| TSRX try | `statementType`, `block`, `handler`, `finalizer`, `pending`, `metadata`; catch `param`, `resetParam`, `body` |
| Style topology | `openingElement`, `closingElement`, `metadata`, `css`, `children`, optional `unclosed`; opening `name`/`attributes`/`selfClosing`; closing `name` |
| CSS root/rules | StyleSheet `source`, `hash`, `children`, `start`, `end`; Atrule `name`, string `prelude`, nullable `block`; Rule selector `prelude`, `block`, and exact parent/local/global metadata |
| CSS selectors | SelectorList/Complex `children`; Relative `combinator`, `selectors`, exact global/scoped metadata; selector names/values/args/matcher/flags; all payload-relative `start`/`end` |
| CSS block/declaration | Block `children`; Declaration `property`, `value`; exact null versus absence and construction order |

The runtime helper inventory is exactly `parseModule`, `isEventAttribute`, and
`normalizeEventName`, with semantics specified in the facade section. The type inventory required
for unchanged compilation is:

- root declarations for the three runtime exports;
- `@tsrx/core/types`: `ParseOptions`, `CompileError`, `CommentWithLocation`, `CodeMapping`,
  `MappingData`, and `VolarMappingsResult`; and
- `@tsrx/core/types/estree`: the augmented `Program`, node namespace, TSRX nodes, CSS namespace,
  and `CommentWithLocation` reachability expected by current imports.

The declaration-only aliases are exact, not nominal substitutes:

```ts
interface CustomMappingData extends PluginActionOverrides {
  embeddedId?: string;
  content?: string;
}
interface MappingData extends VolarCodeInformation {
  customData: CustomMappingData;
}
interface CodeMapping extends Omit<VolarMapping<MappingData>, "generatedLengths"> {
  generatedLengths: number[];
  data: MappingData;
}
interface VolarMappingsResult {
  code: string;
  mappings: CodeMapping[];
  cssMappings: CodeMapping[];
  errors: CompileError[];
  sourceAst: Program;
}
```

`PluginActionOverrides`, `DefinitionLocation`, and the imported Volar/VS Code types retain their
exact 0.1.32 property shapes so the aliases remain assignable. The runtime parser never constructs
them. The `./types/estree` declaration performs the reference augmentation import and re-exports
`estree`; it is not a locally invented Program namespace. Root declarations expose the three
runtime functions with `parseModule(source, filename?, options?)`, while `./types` carries the exact
`ParseOptions`, `CompileError`, comment, metadata, custom/CSS, and alias declarations above.
Qualification resolves all three subpaths in an unchanged Markless typecheck and compares both
directions of assignability plus emitted module resolution, not only export names.

## Appendix B: API, mode, target, and version matrices

Covered criteria: UF-01, UF-05, UF-07, UF-09, UF-10, UF-11, UF-13.

The API matrix is:

| Package/API | Sync | Async | Syntax failure | CSS tree | Stability |
| --- | --- | --- | --- | --- | --- |
| `@oxc-tsrx/parser.parseSync` | Yes | No | Returned diagnostic; Program may be null | Raw only | Stable |
| `@oxc-tsrx/parser.parse` | No | Promise | Same semantic result | Raw only | Stable |
| Lazy result properties | Access-thread materialization | Same after promise | Available independently | Raw style child empty | Stable |
| `experimentalRawTransfer` | Yes where capable | Yes where capable | Same semantic result | Raw only | Experimental and gated |
| `@oxc-tsrx/tsrx-core-compat.parseModule` | Required | No | Strict throw or collected/recovered per mode | Required populated child | Stable only after full qualification |

The compact mode matrix is:

| Surface | Default | Collection | Recovery | Production eligibility |
| --- | --- | --- | --- | --- |
| Canonical default | `recovery: none`, returned OXC-shaped errors | Result always collects | None and no retry | Complete default parses only |
| Canonical editor | Explicit `recovery: editor` | Returned canonical diagnostics | Reference recovery engine, isolated | Editor artifact only |
| Facade strict | `!!(collect || loose)` is false; first failure throws | Supplied arrays ignored and untouched | None and no loose retry | Compatibility compile path after qualification |
| Facade collect | `collect: true`; errors passed, comments exported after success | Append usage errors in observation order | Broken markup may continue with `unclosed` | Never changes core eligibility |
| Facade loose | `loose: true` implies collection | Some broken-markup diagnostics suppressed | Same ancestor mark/pop and reference loose CSS behavior | Editor artifact only |

The target matrix is:

| Rust target | Optional package | OS | CPU | libc | Version-1 gate |
| --- | --- | --- | --- | --- | --- |
| `aarch64-apple-darwin` | `@oxc-tsrx/native-darwin-arm64` | Darwin | arm64 | system | Required |
| `x86_64-apple-darwin` | `@oxc-tsrx/native-darwin-x64` | Darwin | x64 | system | Required |
| `aarch64-unknown-linux-gnu` | `@oxc-tsrx/native-linux-arm64-gnu` | Linux | arm64 | glibc | Required |
| `x86_64-unknown-linux-gnu` | `@oxc-tsrx/native-linux-x64-gnu` | Linux | x64 | glibc | Required |
| `aarch64-unknown-linux-musl` | `@oxc-tsrx/native-linux-arm64-musl` | Linux | arm64 | musl | Required |
| `x86_64-unknown-linux-musl` | `@oxc-tsrx/native-linux-x64-musl` | Linux | x64 | musl | Required |
| `aarch64-pc-windows-msvc` | `@oxc-tsrx/native-win32-arm64-msvc` | Windows | arm64 | MSVC | Required |
| `x86_64-pc-windows-msvc` | `@oxc-tsrx/native-win32-x64-msvc` | Windows | x64 | MSVC | Required |

The capability matrix is:

| Capability | Canonical advertisement | Facade requirement | Failure behavior |
| --- | --- | --- | --- |
| Ordinary lazy transport | Always true after valid load | Required | Loader failure before parse |
| Async parse | Always true after valid load | Not exposed by facade | Promise rejects only operationally |
| Editor recovery | Native bit and oracle-qualified | Required for loose | Capability error; never strict fallback |
| CSS materialization | Always false in canonical capability | Separate compat-addon bit required | Facade blocked or capability error |
| Raw transfer | Host/build-dependent bit | Not required | Explicit option throws before parse when false |
| Browser/WASI | False in version 1 | Not supported | Package export/target unavailable |

The version matrix is:

| Identity | Match rule | Checked by | Mismatch result |
| --- | --- | --- | --- |
| Parser wrapper ↔ `parser.node` | Exact package version and API version | Parser loader | `ERR_TSRX_NATIVE_VERSION` |
| Facade ↔ parser wrapper | Exact package version; shared engine schema/API identity, no runtime addon call | Package manager and facade loader | `ERR_TSRX_NATIVE_VERSION` |
| Facade ↔ compat addon | Exact package version and facade ABI | Facade loader | `ERR_TSRX_NATIVE_VERSION` |
| Addon ↔ target package | Exact target/version/filename/SHA-256/object header | Target loader | Integrity, target, or version error |
| Addons ↔ OXC | Exact revision string | Both loaders | `ERR_TSRX_NATIVE_VERSION` |
| Wrapper ↔ Node | `^20.19.0 || >=22.12.0` and compatible Node-API | npm and loader | Engine or ABI error before parse |
| Tape ↔ transport | Exact internal schema/ABI; stable API independently versioned | Native and JS materializer | Capability/version error, no approximate decode |

## References

Covered criteria: UF-01, UF-02, UF-03, UF-04, UF-05, UF-06, UF-07, UF-08, UF-09, UF-10, UF-11, UF-12, UF-13, UF-14.

- [Goal charter and Markless intake](../goals/parser-upstream-design/goal.md)
- [Canonical task state](../goals/parser-upstream-design/state.yaml)
- [T001 parser seam inventory](../goals/parser-upstream-design/notes/T001-parser-seam-inventory.md)
- [T002 OXC parser-binding conventions](../goals/parser-upstream-design/notes/T002-oxc-upstream-conventions.md)
- [T003 design tenets and binary rubric](../goals/parser-upstream-design/notes/T003-design-tenets-and-rubric.md)
- [T005 adversarial critique](../goals/parser-upstream-design/notes/T005-adversarial-critique.md)
- [Rust/OXC core architecture](rust-oxc-core.md)
- [Embedded CSS boundary](embedded-css-boundary.md)
- [CSS compliance record](../../compliance/css-boundary.json)
- [`tsrx_syntax` public surface](../../crates/tsrx_syntax/src/lib.rs)
- [Compact overlay model](../../crates/tsrx_syntax/src/model.rs)
- [Projection and mapping implementation](../../crates/tsrx_syntax/src/projection.rs)
- [Pinned OXC adapter](../../crates/oxc_adapter/src/lib.rs)
- [Eight native targets](../../packages/runtime/dist/targets.js)
- [Native packaging and identity checks](../../scripts/package-native.mjs)
- [Pinned OXC parser binding](https://github.com/oxc-project/oxc/tree/8e0ed2ebb96137fb1611cdbd5742d5cb46037d40/napi/parser)
- [Official OXC contribution rules](https://oxc.rs/docs/contribute/rules)
- [Official OXC contribution introduction](https://oxc.rs/docs/contribute/introduction)
