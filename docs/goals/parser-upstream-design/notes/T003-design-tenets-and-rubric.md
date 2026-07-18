# T003 parser design tenets and upstream-fit rubric

## Decision

Approve T004 as one whole-document design Worker package.

The evidence supports a layered design with no owner-blocking question:

- preserve the raw, fail-closed Rust compiler core;
- add recovery and CSS materialization only as explicit compatibility lanes;
- expose an OXC-shaped parser result as the canonical JavaScript API;
- place the strict `@tsrx/core` contract in a thin compatibility facade; and
- keep OXC-specific lifetimes and revision-sensitive types inside `oxc_adapter`.

This is approval to draft a design. It is not approval to implement, contact OXC
maintainers, claim upstream acceptance, or claim Markless replacement readiness.

## Ranked tenets

1. **Preserve the non-fork boundary.** One canonical pinned OXC revision remains
   isolated behind `crates/oxc_adapter`. No OXC arena type, fork, patch, snapshot,
   vendor tree, or mixed revision may escape. Evidence: the goal charter and T001
   sections 4 and 7.
2. **Separate core truth from compatibility policy.** The canonical API follows OXC
   result and error conventions. Strict throwing, CSS materialization, event helpers,
   and mutation of caller arrays belong to the compatibility facade. Evidence: T002
   sections 2 through 5 and the Markless contract receipt sections 2, 4, 5, and 6.
3. **Treat Markless compatibility as exact.** Preserve every consumed discriminant,
   enumerable field, offset domain, CSS child topology, error behavior, helper edge
   case, and type subpath. "ESTree-compatible" is not sufficient. Evidence: the
   Markless verdict and contract receipt sections 1 through 6.
4. **Serialize while lifetimes are valid.** Allocator-backed OXC values become an
   owned stable transfer representation before the allocator and borrowed source are
   released. Evidence: T001 section 4 and T002 sections 4 and 5.
5. **Name every coordinate domain.** Rust retains UTF-8 byte spans internally. Stable
   JavaScript offsets are zero-based, half-open UTF-16 code-unit offsets into authored
   source. CSS descendant offsets are zero-based, half-open UTF-16 offsets relative to
   the raw style payload. Evidence: T001 sections 2 and 3, T002 section 6, and the
   Markless contract receipt section 2.
6. **Retain OXC's data-oriented posture.** Keep the compact indexed overlay, do not
   introduce a permanent owned per-node Rust graph, materialize JavaScript objects
   lazily, and measure native parse, serialization, JavaScript materialization,
   end-to-end time, compile time, dependencies, memory, and binary size separately.
   Evidence: T001 section 7 and T002 sections 4 and 8 through 10.
7. **Treat packaging as correctness.** A compiler dependency needs Node-API target
   packages, deterministic capability failures, exact wrapper/native/OXC version
   binding, and all existing eight targets before the compatibility facade releases.
   Evidence: T001 section 6 and T002 section 7.
8. **Describe upstreamability honestly.** Generic mechanisms may be proposed only
   after an OXC issue or discussion. TSRX grammar and compatibility policy stay in
   oxc-tsrx unless OXC maintainers invite them upstream. Evidence: T002 sections 9
   and 11 and the charter's authority constraints.

## Explicit rulings

### CSS and `KEEP_RAW`

`KEEP_RAW` remains the invariant of `tsrx_syntax`, projection, formatting,
diagnostics, and fixes. A separate opt-in compatibility materializer may parse a
borrowed copy of a raw style payload after successful core parsing, attach the
required `StyleSheet` child topology, and expose CSS-relative UTF-16 offsets. It may
not rewrite the payload, affect core acceptance, or enter the ordinary format, lint,
or compiler hot path.

This is a deliberate, scoped divergence from the repository-wide no-CSS-parser
decision. The compatibility materializer must pass the recorded fidelity, failure,
convergence, dependency, and performance requalification gates before the facade may
claim Markless compatibility. Failure blocks that facade, not the canonical raw
parser.

### Strict, collect, and loose modes

Fail-closed parsing remains the default and the only production compiler lane.
Recovery is an explicitly requested editor lane with an enumerated repair grammar,
one structured diagnostic per repair, and an internal completeness marker. Strict
parsing never silently retries in loose mode.

The `@tsrx/core` facade maps `{ collect, loose, errors, comments }` exactly and appends
to caller-owned arrays. Its behavior must be proven against a Markless editor oracle.

### AST and result shape

The public AST is one authored-source ESTree, TypeScript, JSX, and TSRX `Program`.
Projection scaffolding is never public. Nodes are ordinary enumerable objects.

The canonical surface provides typed `parseSync(filename, sourceText, options?)` and
the equivalent `parse(...)`. It returns a lazy `ParseResult` exposing `program`, an
authored-source `module` record, `comments`, and structured `errors`. The compatibility
facade returns the same `Program` directly.

### Offset domains

Byte spans remain internal. Stable JavaScript `start` and `end`, comments, module
records, diagnostic positions, and strict `pos` use zero-based UTF-16 offsets into the
original source. CSS descendants use zero-based UTF-16 offsets into
`JSXStyleElement.css`. Stable v1 does not expose byte coordinates. Any future byte
coordinates require experimental, domain-specific names.

### Ownership and serialization lifetime

`oxc_adapter` owns every access to allocator-backed OXC values and serializes them
before returning. The TSRX parser and binding layer own stable DTO and transport
definitions, overlay reconstruction, UTF-16 conversion, lazy JavaScript
materialization, CSS attachment, and facade policy. `tsrx_syntax` exposes compact
enumerable snapshots or accessors without acquiring Node-API or general serialization
dependencies.

Ordinary and future raw-transfer transports implement the same semantic contract.
Raw transfer remains experimental and capability-gated until proven.

### Error surfaces

Canonical `ParseResult.errors` follows OXC's collected structured-diagnostic
convention. Only capability or transport failures throw from the canonical API. The
compatibility facade throws a synchronous `SyntaxError`-like object with a numeric
UTF-16 `pos` in strict mode.

### Package form and targets

The primary boundary is a Node-API addon, not JSON over a subprocess. An ESM
`@oxc-tsrx/parser` wrapper loads target-specific optional native packages. A thin
`@oxc-tsrx/tsrx-core-compat` package provides `parseModule`, `isEventAttribute`,
`normalizeEventName`, `./types`, and `./types/estree`; it can be installed under the
`@tsrx/core` dependency name for a source-unchanged Markless trial.

Version 1 covers the existing eight targets: Darwin arm64 and x64, Linux GNU arm64 and
x64, Linux musl arm64 and x64, and Windows MSVC arm64 and x64. Browser and WASI are
explicitly deferred divergences because the first consumer uses Node tooling.

The wrapper, facade, addon, target package, ABI or transport protocol, checksum, Node
engine, and pinned OXC revision must be mutually validated.

### Compatibility facade

The facade preserves the exact synchronous signature, three runtime exports, event
helper edge cases, ESM behavior, caller-array behavior, type subpaths, filename-sensitive
style behavior, and the consumed node and field matrix. Compatibility logic does not
enter Markless compiler passes.

### Generic upstream boundary

Potentially generic contributions are serializer generation, UTF-8 to UTF-16 conversion
helpers, structured diagnostic conversion, lazy or raw-transfer machinery, package
loaders, and capability probes. TSRX scanning, overlays, reconstruction, node
discriminants, CSS policy, recovery, diagnostics, event helpers, and the facade remain
owned by oxc-tsrx. No public OXC custom-grammar interface or upstream acceptance is
assumed.

## Upstream-fit rubric

Every criterion is binary. Any rejection rejects the draft; criteria are not averaged.

| ID | Criterion and evidence | Pass requirement | Rejection test |
| --- | --- | --- | --- |
| UF-01 | OXC JavaScript API convention. T002 sections 2 through 5 and 10. | Exact core signatures, lazy `ParseResult`, returned structured diagnostics, sync/async parity, and labeled conventions or divergences. | Reject if the core is only `parseModule`, eagerly materializes everything, or presents thrown syntax errors as OXC convention. |
| UF-02 | Arena and serialization safety. T001 section 4; T002 sections 4 and 5. | Name allocator ownership, the serialization point, the owned return boundary, and semantic parity across transports. | Reject if an arena borrow or revision-specific OXC type escapes, or lifetime ownership is implicit. |
| UF-03 | Data-oriented performance, compile-time, and size posture. T001 section 7; T002 sections 4 and 8 through 10. | Preserve the compact overlay, lazy JS materialization, and separate parse, serialize, materialize, end-to-end, dependency, compile-time, memory, and binary-size gates. | Reject an eager Rust object graph, permanent unmeasured transport, or missing regression gates. |
| UF-04 | Non-fork adapter isolation. Charter; T001 sections 4 and 7. | One exact OXC revision is isolated in `oxc_adapter`, no OXC type crosses it, and upgrades remain adapter-local. | Reject a fork, snapshot, vendor tree, Cargo patch, mixed revisions, or direct OXC imports elsewhere. |
| UF-05 | Exact Markless runtime and type compatibility facade. Markless verdict and receipt sections 1, 4, 5, and 6. | Specify three runtime symbols, exact helpers, synchronous `parseModule`, ESM form, caller arrays, and both type subpaths. | Reject if Markless must absorb AST or error policy or broadly rewrite compiler passes. |
| UF-06 | AST and result shape. T001 sections 2, 3, and 9; Markless receipt section 2. | Specify the authored `Program`, every consumed standard, TS, JSX, TSRX, and CSS discriminant and field, enumerable topology, and no projected scaffolding. | Reject vague ESTree compatibility or a second incompatible AST. |
| UF-07 | Offset domains. T002 section 6; Markless receipt sections 2, 5, and 6. | Name internal bytes, module-relative UTF-16, CSS-relative UTF-16, half-open semantics, comments, errors, module records, and astral cases. | Reject mixed unnamed domains or stable byte-valued JavaScript `start` and `end`. |
| UF-08 | CSS raw and tree layering. T001 sections 3, 7, and 10; Markless verdict mismatch 3. | Raw bytes stay authoritative; CSS parsing is compatibility-only and non-mutating; selector offsets are CSS-relative UTF-16; compliance gates block release. | Reject raw-only Markless output or a silent global reversal of `KEEP_RAW`. |
| UF-09 | Strict, collect, loose, and error semantics. T001 sections 7 and 10; T002 section 3; Markless receipt sections 2, 5, and 6. | Include a mode truth table covering fail-closed default, returned core errors, strict facade throws, caller-array appends, explicit recovery, and no strict-to-loose fallback. | Reject if one policy silently replaces the other. |
| UF-10 | Packaging, targets, capabilities, and version binding. T001 section 6; T002 section 7. | Specify Node-API, ESM wrapper, facade, all eight target packages, exact version, OXC, checksum, and Node-engine binding, plus deterministic unsupported-target behavior. | Reject a process protocol as primary, a partial target release, or loosely matched native versions. |
| UF-11 | Conformance and benchmark oracle. T001 section 8; T002 section 8; Markless receipt sections 2 through 6. | Differentially compare against `@tsrx/core@0.1.32` across 179 valid and 12 invalid cases plus Unicode, CSS, recovery, errors, comments, type exports, helpers, and transport parity; freeze separated performance budgets before implementation. | Reject formatter convergence as AST proof or any oracle without binary gates. |
| UF-12 | Honest upstream engagement. T002 sections 9 and 11; charter authority. | Disclaim acceptance, require an issue or discussion before architectural PRs, propose small separable pieces, and record AI disclosure and maintainer-validation duties. | Reject endorsement claims, upstream readiness stated as fact, contact in this tranche, or a large unsolicited patch plan. |
| UF-13 | Implementable staging. Charter slice rules; T001 section 9; T002 sections 8 through 11. | Every stage has prerequisites, owned outputs, verification, exit criteria, and rollback boundaries, beginning with oracles and ending with package and Markless qualification. | Reject ungated phase names, implementation in this tranche, or a plan unable to stop the facade while retaining the core. |
| UF-14 | Generic upstream and TSRX-owned boundary. T002 section 11; T001 sections 1 through 4. | Include a responsibility table separating generic mechanisms from TSRX grammar and policy, with upstream acceptance explicitly unknown. | Reject moving TSRX semantics into OXC without invitation or describing a nonexistent plugin interface as public. |

## Required design path and outline

The design path is `docs/architecture/tsrx-parser-api.md`.

Its H2 sections must appear exactly once and in this order:

1. Status, scope, and non-goals
2. Evidence base and convention/divergence ledger
3. Requirements and ranked invariants
4. Architecture and ownership boundaries
5. Canonical JavaScript API
6. `@tsrx/core` compatibility facade
7. AST and ParseResult contract
8. Offset and coordinate domains
9. Diagnostics, strict mode, and loose/collect recovery
10. Embedded CSS policy and materialization
11. Arena lifetime, serialization, and transport
12. Packaging, targets, capabilities, and version binding
13. Performance, dependency, compile-time, and binary-size posture
14. Conformance and benchmark oracle
15. Upstream boundary and engagement plan
16. Implementation stages and exit gates
17. Alternatives rejected and residual risks
18. Upstream-fit rubric traceability
19. Appendix A: Markless node, field, helper, and type inventory
20. Appendix B: API, mode, target, and version matrices
21. References

The decision ledger labels every major choice as either `OXC convention` with a citation
or `Deliberate divergence` with a rationale. Each substantive section lists its covered
UF IDs. The traceability section contains exactly one row per UF-01 through UF-14 and
maps each criterion to concrete sections and evidence.

## Exact T004 Worker package

- **Objective:** Draft the complete design at the approved path, implementing every
  ruling and outline item and mapping every UF criterion. Do not write implementation
  code, tests, board state, generated docs, or Markless changes.
- **Allowed files:** `docs/architecture/tsrx-parser-api.md` only.
- **Design-specific test:** none. Inline read-only verification is sufficient.
- **Verification:**
  1. Require the only architecture-tree change to be
     `docs/architecture/tsrx-parser-api.md`.
  2. Run `git diff --check -- docs/architecture/tsrx-parser-api.md`.
  3. Run a read-only Node and Marked structural check requiring one H1; all 21 H2
     headings exactly once and in order; UF-01 through UF-14 at least twice and exactly
     once each as traceability-table rows; both decision labels; no placeholder language;
     balanced fences; no heading-level jumps; blank lines after headings; no tabs or
     trailing whitespace; a final newline; and successful `marked.lexer` parsing.
  4. Run a read-only Markdown link check requiring resolved local paths, valid local
     heading fragments, HTTPS external links, and no absolute local filesystem links.
- **Stop if:**
  - any file outside the allowed path is needed or changed;
  - an authoritative receipt contradicts a ruling or a UF criterion cannot be mapped;
  - drafting requires implementation, generated documentation, a design-specific test,
    Markless edits, maintainer contact, or an acceptance claim;
  - CSS or recovery compatibility can only be asserted by silently reversing
    `KEEP_RAW` or fail-closed policy; or
  - either verification check fails twice after bounded correction.

## Residual risks and parked questions

- CSS parser selection is deferred to implementation qualification and does not change
  the ownership ruling.
- Recovery coverage may be narrower than Markless's editor corpus. That blocks the
  compatibility facade, not the canonical parser.
- Raw-transfer feasibility varies by target. Stable v1 may retain ordinary lazy
  transport while raw transfer remains experimental.
- OXC may reject every generic contribution. The non-fork adapter remains viable.
- Exact npm naming and alias publication rights require later release validation.
- The formatter corpus is insufficient. New AST, error, offset, and benchmark oracles
  are mandatory before any implementation claim.
- Project-versioned Markless skill guidance is unavailable because this worktree does
  not resolve `@markless/core`. The user-designated receipts are authoritative for this
  design tranche; no other checkout's guidance was substituted.
