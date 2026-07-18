# T005 adversarial critique

## Verdict

Rejected. T006 must revise the complete document before final audit.

The first draft satisfies the non-fork, package-target, upstream-engagement, and
generic-versus-TSRX ownership criteria. It fails the remaining binary criteria because
several supposedly exact contracts were inferred rather than copied from the pinned
sources, and because its transport lifecycle can hold two complete native tapes plus a
materialized JavaScript graph.

The Markless checkout did not have `node_modules/@tsrx/core` at its root. The judge and
PM therefore validated against an installed exact `@tsrx/core@0.1.32` copy retained in
the Markless `.fable-codex` worktrees. The inspected package reports version `0.1.32`.
No external file was changed.

## Criterion verdicts

| ID | Verdict | Finding |
| --- | --- | --- |
| UF-01 | Reject | The canonical API omits or changes pinned OXC `dts`, `commonjs`, nullable options, complete `EcmaScriptModule`, diagnostic severity/help/label shapes, and configurable enumerable getter descriptors without labeling each divergence. |
| UF-02 | Reject | Serialization occurs before allocator drop, but crate ownership is cyclic as written: the parser owns the tape schema, `oxc_adapter` depends on it, and the parser consumes `oxc_adapter`. A leaf schema crate or another acyclic direction is required. |
| UF-03 | Reject | The fixed lifecycle can retain a full projected tape, a full authored tape, and a cached full JavaScript graph. It does not define destructive consumption, table release, peak-live state, or copied-byte bounds. |
| UF-04 | Pass | The exact OXC revision stays inside `oxc_adapter`; no fork, patch, vendor tree, snapshot, mixed identity, or escaping OXC type is allowed. |
| UF-05 | Reject | Compatibility `CompileError` and caller-comment behavior do not match the exact package. `CompileError` extends `Error`; numeric fields and `loc` may be undefined; `fileName` is string or null. Comments append only for collect/loose and only after successful parse. |
| UF-06 | Reject | Custom node interfaces and CSS inventory are incomplete or wrong, including code-block render, control statement fields and metadata, for variants, try finalizer, catch reset parameter, style topology, and the complete CSS tree. |
| UF-07 | Reject | Public domains are otherwise strong, but the draft omits reference enumerable `loc`, comment context, raw/literal, and metadata fields while requiring full equality, and it rejects lone surrogates that the JavaScript reference accepts. |
| UF-08 | Reject | Raw/CSS layering is conceptually sound, but the design omits the exact CSS tree, hash preimage, CR handling, and mode behavior, and never defines a one-outer-parse native handoff between the canonical and compatibility addons. It also fails to reconcile the current same-allocator compliance condition. |
| UF-09 | Reject | The insertion-only recovery grammar and 16-row table do not match the reference. Collect recovers some broken markup, loose suppresses some diagnostics, mismatched tags mark/pop ancestors, and comments do not append in strict or throwing cases. |
| UF-10 | Pass | Node-API, ESM wrappers, all eight targets, deterministic failure, Node engine, ABI, checksums, and exact OXC binding are sufficient at the package-topology level. |
| UF-11 | Reject | Oracle scope is broad, but cannot pass while the normative AST deliberately omits or mistypes enumerable fields. The structural digest is undefined. |
| UF-12 | Pass | Acceptance is disclaimed; later engagement begins with an issue or Discussion and uses small measured generic proposals with human validation and AI disclosure. |
| UF-13 | Reject | Stage 1 is asked to discover facade contracts that prior sections claim are fixed. Qualification may select an implementation, but cannot defer design-defining shapes, recovery, CSS, handoff, or string semantics. |
| UF-14 | Pass | Generic candidates remain narrow and optional; TSRX grammar and policy stay local; the adapter remains viable if OXC declines every proposal. |

Any rejection rejects the draft.

## Required revisions

### Blocker 1: source-verified compatibility AST and types

Affected sections: compatibility facade, AST contract, Appendix A.

Replace invented interfaces with the exact emitted and accepted contract from the
installed declarations and parser source. At minimum:

- `CompileError extends Error` with `code`, `pos`, `raisedAt`, `end`, and `loc`
  potentially undefined, and `fileName: string | null`;
- `JSXCodeBlock.render: Node | null`, metadata, and optional inner comments;
- control `statementType`, metadata, statement-shaped children, complete for-loop
  variants, switch shape, try `finalizer`, pending, and catch `resetParam`;
- style opening and closing element topology, CSS, children, unclosed state, metadata,
  and source locations;
- `StyleSheet.source` and `hash`, rule metadata, and all CSS discriminants and fields;
- the exact root, `./types`, and `./types/estree` declaration aliases Markless imports.

Acceptance requires reference/candidate fixtures for each custom and CSS discriminant
to match `Object.keys`, descriptors, null-versus-absent distinctions, topology, values,
and source slices, plus declaration assignability for all Markless imports.

### Blocker 2: exact collect, loose, error, and comment behavior

Affected sections: compatibility facade and diagnostics/recovery.

Replace the insertion-only model and incorrect truth table with source-verified
`0.1.32` semantics. Cover complete, recoverable, malformed-CSS, and unrecoverable input
with every collect/loose/errors/comments combination and prefilled arrays. The design
must distinguish:

- collect-mode broken-markup recovery and `unclosed` nodes;
- loose-mode diagnostic suppression;
- mismatched closing tags that mark and pop ancestors;
- successful-only comment export when collect or loose is enabled;
- strict and plain-`Error` cases, not a universal `SyntaxError` claim; and
- return versus throw, append timing, array identity, error field shapes, and order.

### Blocker 3: exact OXC convention or labeled divergence

Affected sections: ledger and canonical API.

Match or explicitly diverge from the pinned API for:

- `lang: "dts"`, `sourceType: "commonjs"`, and null options;
- the complete `EcmaScriptModule` structure;
- diagnostic `Advice`, nullable label message, and `helpMessage` shape;
- result getter enumerability, configurability, caching, and descriptors; and
- experimental option placement.

Retain the already honest `Program | null` fail-closed TSRX divergence.

### Blocker 4: complete CSS contract and native handoff

Affected sections: AST, CSS, packaging.

Specify the exact `StyleSheet.source/hash`, CSS tree, style topology, hash preimage
`${filename}:${line}:${column}:${content}`, `tsrx-` plus the first eight lowercase hex
characters of SHA-256, removal of every carriage return before hashing, selector
offsets, and strict/collect/loose failure behavior.

Define one implementable native handoff with one outer TSRX/OXC parse, no full
JavaScript graph traversal, and no mutation of the canonical cached AST. The design
must explicitly choose whether the facade waits for the current compliance record's
canonical-OXC same-allocator condition or creates a separately argued compatibility-
product decision that supersedes that condition only for the isolated facade. It may
not silently reinterpret the existing record.

Acceptance requires exact CSS snapshots and hash vectors, the malformed-mode matrix,
selector offsets, a one-parse counter, zero subprocesses, and proof parser-only calls
load no CSS code.

### Blocker 5: acyclic ownership and bounded live representations

Affected sections: architecture, serialization, performance.

Provide an acyclic crate graph, preferably a small OXC-independent leaf tape-schema
crate consumed by both `oxc_adapter` and the parser engine. Reconcile the proposed root
representation with the actual `first_root` and `next_sibling` model. Define destructive
or streaming conversion and exact release points so projected tape, authored tape,
source copies, result handle, and JavaScript graph do not remain live without need.

Acceptance requires an acyclic dependency audit, exact nested-root reconstruction, and
peak-live/copy assertions inside the frozen budgets.

### High: lossless JavaScript strings

The canonical input bridge cannot accidentally reject JavaScript strings accepted by
Acorn. Specify lossless UTF-16/WTF-8 handling, or state and justify an explicit
compatibility limitation. The Markless facade must preserve source slicing for lone
high and low surrogates, valid pairs, astral characters, CRLF, comments, module entries,
errors, CSS, and strict positions.

### High: self-consistent oracle

Define a canonical structural-digest serialization or require full enumerable equality
for all 179 valid artifacts. Freeze all exclusions before capture. Two clean reference
captures must match; mutating one field, descriptor, or offset must fail. Numeric
performance budgets remain a pre-implementation Stage 1 prerequisite.

### Medium: native package manifest details

List exact manifest, `files`, and checksum-schema additions for both addon files in all
eight target packages, coexistence with existing executables, and parser-only install-
size accounting.

## T003 corrections

Authoritative evidence narrows several T003 rulings:

- Recovery cannot require one diagnostic per repair because exact loose behavior
  suppresses some broken-markup diagnostics.
- Comments append only in collect/loose mode and after a successful parse.
- The compatibility Program is a materialized view with CSS, locations, metadata, and
  fields not promised by the canonical raw view; the handoff must name that distinction.
- A separate CSS materializer must explicitly supersede or wait for the current
  canonical-OXC same-allocator requalification condition.
- Not every strict failure is `SyntaxError`-like. Missing style filename and direct CSS
  failures are plain `Error` cases in the pinned reference.

These are evidence-driven corrections, not owner questions.

## Optional improvements

- Add line-level source evidence beside each custom and CSS field.
- Separate stable and experimental options in declarations.
- Add a memory-state diagram showing live source, tape, and JS representations at each
  result/getter transition.

## Exact T006 package

- **Objective:** Revise `docs/architecture/tsrx-parser-api.md` as one whole-document
  package to resolve and record every required finding. Replace incorrect fixed
  contracts instead of deferring them to implementation qualification.
- **Allowed file:** `docs/architecture/tsrx-parser-api.md` only.
- **Verification:** the T004 scope, diff, structural, and link checks, plus explicit
  source-backed disposition and contradiction checks supplied in the T006 packet.
- **Stop if:** another file is required; exact behavior cannot be resolved from the
  designated sources; resolution requires implementation, generated docs, Markless
  edits, upstream contact, or publication; CSS/recovery can only be stated by silently
  reversing core policy; or a verification fails twice.

## Residual risks after revision

- The overlay may need more compact spans or indices to reconstruct reference metadata
  and recovery shapes without reparsing.
- Exact Acorn recovery may be too expensive or incompatible with the fail-closed
  scanner, permanently blocking the facade but not the canonical parser.
- A separately compliant CSS parser may remain unavailable or exceed dependency,
  compile-time, or binary-size budgets.
- Two addons per target may make parser-only installs too large.
- Lossless arbitrary Node strings may require a custom UTF-16/WTF-8 bridge.
- Raw transfer may remain unavailable on some targets.
- npm naming and alias rights need later release qualification.
- OXC may decline every generic proposal; the non-fork adapter remains viable.
