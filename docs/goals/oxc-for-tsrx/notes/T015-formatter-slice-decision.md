# T015 — Native formatter slice decision

## Decision

Approve a combined Markless-shaped native formatter vertical for T016. The slice
formats ordinary JS/TS/JSX/TSX directly with canonical Oxfmt and formats the
proven TSRX subset (`@{`, statement-position `@if`, and `@else`) through a
collision-checked structural marker projection and a verified native lift. It
must include an in-memory Rust API, an editor-ready stdin boundary, check/write
CLI behavior, a real Markless-derived fixture, fail-closed source-fidelity
tests, and retained P04/P05/P07 performance evidence.

This is larger and more useful than another scanner helper while remaining
honest: T016 does not claim the complete TSRX grammar. Files containing
unsupported custom constructs must produce a precise error before any write.

## Foundation audit

### Proven

- The workspace is Rust-native and does not fork, vendor, copy, or patch OXC.
- Revision-specific OXC use is isolated to `crates/oxc_adapter` at canonical
  commit `8e0ed2ebb96137fb1611cdbd5742d5cb46037d40`.
- Ordinary JS/TS/JSX/TSX linting bypasses TSRX scanning and projection.
- The current TSRX lint path performs one native scan, one equal-width
  projection allocation, one OXC parse, semantic analysis, and real OXC rules.
- Diagnostics use original UTF-8 byte spans and safe fixes are restricted to
  identity ranges and validation reparse.
- The retained lint benchmark passes its P01/P02/P03/P05/P07 gates, including
  `no-debugger` plus `no-unused-vars` in fresh processes.
- Canonical OXC exposes the required public formatter seam:
  `oxc_formatter::parse_for_format` followed by `format_program`. The latter
  performs no hidden parse.
- The T010 disposable Rust probe proved canonical Oxfmt preserves unique block
  comment markers around `@{` and `@if` well enough for checked lifting.

### Fragile

- `tsrx_syntax::scan` currently treats a complete template literal as opaque,
  so code in `${...}` is not scanned.
- It has no regular-expression-literal or JSX-text state. A structural-looking
  `@if` in either context can be misclassified.
- Recognition is lexical and limited to three sigils; it does not yet validate
  statement position or structural nesting independently of the projected OXC
  parse.
- The first lint projection is equal-width and works only where removing the
  sigil produces legal, semantically equivalent TSX.

### Missing

- `@for` (including `await`, `index`, and `key` clauses), `@empty`,
  `@switch/@case/@default`, `@try/@pending/@catch`, dynamic JSX tags, inline raw
  `<style>`, and custom forms used in expression position.
- Native formatting, formatter options/config discovery, Vite+, editor,
  platform packaging, JS-plugin/type-aware lint compatibility, and upgrade
  matrices.
- Complete temporary-copy Markless and editor acceptance.

## Source-fidelity model for T016

1. A single native structural scan records only supported TSRX sigils and
   explicitly reports unsupported custom forms or ambiguous lexical contexts.
2. The custom path allocates one projected source string. Each supported sigil
   becomes a source-collision-free block-comment marker immediately before the
   equivalent standard token (`{`, `if`, or `else`).
3. `oxc_adapter` invokes `parse_for_format` once and `format_program` once with
   `JsFormatOptions`; no second compiled-TSX parse or AST conversion is allowed.
4. A single native lift requires every marker exactly once, in source order,
   adjacent (modulo formatter whitespace) to its expected token. Missing,
   duplicate, reordered, or residual markers are errors.
5. The lifted result is rescanned structurally and must reproduce the original
   supported token sequence. This is a linear scan, not another OXC parse.
6. The library returns a complete result or an error. The CLI formats every
   requested file successfully before performing any write, so one bad file
   cannot partially mutate a batch.
7. Ordinary files go directly to the same canonical Oxfmt adapter without a
   TSRX scan, marker allocation, or lift.

T016 supports `@{`, statement-position `@if`, and `@else`, including nesting,
TypeScript, ordinary JSX, comments, Unicode, strings, regex literals, template
interpolation, JSX text/attributes, and standard decorators around that subset.
Custom control flow in expression position and every other TSRX construct are
rejected explicitly until their projection and lift are proven.

## Rejected alternatives

- **Complete grammar before any formatter:** too long without user-observable
  progress and unnecessary for the current real Markless Counter proof.
- **Formatter without scanner hardening:** risks silently treating regex, JSX
  text, or template content as structure.
- **Custom formatter on `oxc_formatter_core`:** duplicates Oxfmt layout policy
  and would drift when Oxfmt updates.
- **Prettier/JavaScript/Zig/process remapping:** violates the selected native
  architecture or loses the required performance/update boundary.
- **Full AST conversion or a second emitted-TSX parse:** erases the intended OXC
  hot-path advantage and complicates source fidelity.

## Canonical OXC boundary

Only `crates/oxc_adapter` may add the exact-revision `oxc_formatter` dependency
and import `JsFormatOptions`, `parse_for_format`, and `format_program`. The
adapter returns owned formatted code, parse/format timings, and `parse_count: 1`.
No formatter internals or private/generated OXC modules may cross that crate.

## Required evidence after T016

- Stock Oxfmt fails the retained `.tsrx` contract before implementation.
- Product library and CLI format the Markless-derived Counter and nested
  `@if/@else` fixture, preserve sigils/comments/Unicode, and converge after one
  pass.
- Regex, template interpolation, JSX text/attributes, marker-collision, marker
  corruption, unsupported syntax, invalid projection, and multi-file no-write
  behavior are directly tested.
- Ordinary TSX output matches canonical Oxfmt for the same options.
- Release benchmarks retain raw samples and pass: ordinary direct-path median
  and p95 ratios at most 1.05/1.08; sequential complete TSRX formatting at least
  15 MiB/s and at least 10x the retained 1.66 MiB/s Prettier baseline;
  default-thread batch throughput at least 100 MiB/s; fresh stdin p95 at most
  110 ms and at most 1.25x official Oxfmt; peak RSS at most 1.15x equivalent
  canonical Oxfmt.

## Remaining work after T016

T016 cannot complete the goal. The next phase must generalize the Rust syntax
overlay/projection to the complete authoritative TSRX grammar and extend lint
and format acceptance across the representative Markless corpus. Later phases
must add config/options compatibility, Vite and literal Vite+ commands, editor
activation/format-on-save/live diagnostics, platform npm packages, supported
OXC upgrade lanes, and the final temporary-copy Markless walkthrough.
