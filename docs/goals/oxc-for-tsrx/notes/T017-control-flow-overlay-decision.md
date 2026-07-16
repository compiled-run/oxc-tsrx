# T017 — Control-flow overlay decision

## Decision

Approve one end-to-end Rust grammar vertical for `@if`/`@else` and
`@for`/`@empty` in direct JSX-child and general expression positions. The
Worker must replace the current token-only model with a compact recursive
structural overlay, use one expanded legal-TSX formatter projection with a
checked lift, and use a separate mapped lint projection for descendant-rule
diagnostics and identity-only safe fixes.

This is the largest safe next slice. It adds 39 valid tracked Markless files
(104/179 to 143/179 at committed HEAD) and proves both difficult reusable boundaries: expression
wrappers and annotated `@for` headers. `@try` is deliberately next rather than
bundled now because its pending-before-catch order and two-binding catch form
require a different scaffold, while expanded diagnostic/fix mapping and
wrapper indentation have not yet been proven once. `@switch` then reuses the
same overlay and marker protocol. This sequencing does not narrow the goal;
GoalBuddy must continue immediately through the remaining grammar and product
surfaces after this Worker.

## Read-only evidence

- Markless: `76d0e6a07fa728b9343cc0d342fbe03813c43703`.
- Authoritative Ripple: `03a98fd2a230ab5853808a44ff024568d68142fb`.
- Markless-pinned `@tsrx/core@0.1.32` and current Ripple agree exactly: 179 of
  191 tracked `.tsrx` files parse; the other 12 are incomplete/cursor/error
  fixtures under the TypeScript-plugin completion matrix.
- The current formatter accepts 104/179 valid committed-HEAD files and rejects all 75 valid
  feature-bearing files. It also rejects all 12 parser-invalid fixtures.
- Of 125 valid control-flow nodes, 124 are direct JSX children and one is an
  assigned-expression `@if`; none is a statement-position control node.
  Therefore adding more one-byte `@` masks cannot unlock real Markless.
- One valid file exposes a scanner bug: the `/` in `</button>` is mistaken for
  a regular-expression start in
  `poc/fixtures/proofs/scheduler-journal/src/App.tsrx`.

No Markless, Ripple, OXC, Yuku, or other external file was modified.

## Current-slice audit

### Proven and retained

- The production workspace is Rust-only and does not fork, vendor, patch, or
  copy OXC.
- Canonical OXC revision
  `8e0ed2ebb96137fb1611cdbd5742d5cb46037d40` is isolated behind
  `crates/oxc_adapter`.
- Ordinary JS/TS/JSX/TSX bypasses TSRX scanning and projection.
- Normal lint and format operations perform one canonical OXC parse.
- The existing equal-width lint subset reports original UTF-8 byte spans and
  applies only identity-proven safe fixes followed by validation reparse.
- The formatter marker lift rejects missing, duplicated, reordered, moved, or
  residual markers and rescans the lifted structure.
- All existing correctness gates and P01-P07 budgets pass. The retained latest
  runs report 336.13 MiB/s scan+copy+parse lint throughput, 104.25 MiB/s CLI
  lint throughput, 94.46 MiB/s sequential complete formatting, 623.27 MiB/s
  batch formatting, 3.13 ms formatter stdin p95, and 1.095x formatter RSS.

### Not yet proven

- Current formatter coverage is 58.7% of parser-valid tracked Markless, not
  grammar-complete support.
- `program.source_text = original_source` is valid only for the current
  equal-width lint projection. It is invalid once projected offsets shift.
- Marker survival alone does not prove indentation fidelity after removing an
  expression scaffold.
- Config/options, plugins, type-aware lint, Vite/Vite+, editor behavior,
  platform packaging, upgrade lanes, dynamic tags, raw style, and clean-room
  Markless acceptance remain absent.
- Expanded scaffolding changes AST parents/scopes. This slice may prove named
  descendant rules; it may not claim arbitrary rule-semantic equivalence.

## Authoritative grammar and feasibility matrix

| Construct | Valid occurrences/files | Required model | Slice |
| --- | ---: | --- | --- |
| `@if`/`@else if`/`@else` | 24 / 23 | Real standard `if` inside a marked expression scaffold; recursive braced bodies | T019 |
| `@for`/`@empty` | 47 / 34 | Real loop plus marked `index`, `key`, and empty-arm slots; declaration and assignment bindings; `for await` oracle fixture | T019 |
| `@switch`/`@case`/`@default` | 3 / 3 | Real marked `switch` with braced cases; duplicate/default/order validation | immediate next control Worker |
| `@try`/`@pending`/`@catch` | 51 / 28 | Source-order marked clause methods/blocks; optional pending/catch and error/reset bindings | immediate next control Worker |
| Expression-position controls | 1 in Markless; all families in Ripple tests | Expression scaffold without JSX container; assignment/return/argument coverage | T019 for if/for; next Worker for switch/try |
| Dynamic `<{expr}>...</{expr}>` | 1 / 1 | Matched opening/closing expression overlay and synthetic intrinsic JSX element | later grammar Worker |
| Raw `<style>` | 4 / 4 | Opaque embedded-CSS region with byte-safe lift; separate CSS formatting decision | later grammar Worker |

Valid-file combinations are 25 for-only, 17 try-only, 12 if-only, four
if+for+try, four if+try, two if+for, two for+try, two switch-only, one all-four,
five style-only, and one dynamic-only. T019 therefore raises valid acceptance
to exactly 143/179. Completing all control families raises it to 173/179;
style and dynamic tag support then reach 179/179.

The first filesystem count observed 105/179 and four style files because the
external Markless worktree has user-owned modifications to `docs/pages/index.tsrx`
and additional untracked files. The reproducible oracle reads committed bytes
with `git ls-tree`/`git show` at the pinned HEAD: 191 total, 179 valid, 104
accepted by the current formatter, and five style files. T019 uses that immutable
view and separately reports (but never modifies) the live worktree.

## Structural representation

The scanner becomes a byte-linear context-aware parser that owns no source
substrings and emits flat indexed vectors, conceptually:

```text
SyntaxNode { kind, flags, parent, span, first_clause, clause_count }
Clause     { role, header_span, body_span }
Boundary   { role, span, owner }
```

All offsets and indices are fixed-width integers. No boxed syntax tree,
per-node heap string, JS AST, JSON transfer, full OXC AST conversion, or second
source parse is allowed. Context records statement/expression/direct-JSX-child,
JSX expression containers, template interpolation, and protected lexical/raw
regions. A full structural fingerprint includes kinds, nesting, clauses,
headers, and bodies.

## Formatter source-fidelity model

1. Scan once into the compact overlay and reject malformed or unsupported
   grammar before invoking OXC.
2. Allocate one legal-TSX projection and one compact ordered manifest.
3. Project `@if` as a real standard `if` and `@for` as a real standard loop
   inside collision-namespaced synthetic expression scaffolds. A direct JSX
   child additionally receives a JSX expression container. Nested directives
   reuse the enclosing statement-capable scaffold.
4. Preserve source order. Marked synthetic slots carry `index`, `key`, and
   `@empty` payloads without hiding their standard expressions from Oxfmt.
5. Fence every synthetic token run and structural replacement with typed,
   collision-free comments. Namespace selection itself must remain linear.
6. Call canonical `parse_for_format` once and `format_program` once.
7. Lift with one forward marker scan. Require every marker exactly once, in
   order, properly nested, and adjacent to its expected scaffold fingerprint;
   discard only proven synthetic bytes; restore canonical TSRX punctuation;
   remove only marker-proven wrapper indentation outside raw regions.
8. Rescan the lifted output and require the complete structural fingerprint to
   match. Production does not perform a second OXC parse. Idempotence is a
   black-box second formatter invocation in tests.

## Lint mapping model

Formatter scaffolding is not reused blindly. The same overlay builds a
separate one-buffer lint projection and a sorted piecewise map:

```text
Identity  { projected_start, original_start, length }
Synthetic { projected_span, original_anchor, role }
```

OXC receives projected text as `program.source_text`. A diagnostic label maps
only when its complete range is affine in identity segments; synthetic or
cross-segment diagnostics are suppressed and counted. A fix applies only when
safe, wholly inside one identity segment, not inserted at a synthetic boundary,
and projected/original bytes are equal. The translated edit is applied to the
original and the existing validation reparse remains mandatory. The Worker
must directly prove `no-debugger`, `no-unused-vars`, and `no-var` on standard
descendants at original offsets; it must document rather than overclaim rules
whose parent/CFG/source-text semantics see scaffolding.

## T019 red-first Worker package

### Objective

Implement the generalized compact Rust overlay and complete end-to-end
`@if`/`@else` plus `@for`/`@empty` vertical across direct JSX-child, nested, and
general expression positions. Deliver one-parse Oxfmt formatting with checked
lift, mapped native OXC descendant lint diagnostics and identity-only fixes,
the JSX-closing-tag scanner regression, provenance-recorded Markless/Ripple
fixtures, read-only committed-HEAD corpus acceptance at exactly 143/179 valid files, and all
existing correctness/performance gates without weakening thresholds.

### Red tests

- Stock Oxfmt must fail the new control-format black-box contract.
- Stock Oxlint must fail the new control-lint black-box contract.
- Current product binaries must fail Markless-derived direct-JSX `@if`, bare
  keyed `@for`, indexed/keyed/empty `@for`, nested depth-three controls, the
  assigned-expression form, `for await`, and the scanner regression before
  implementation.
- Corrupted/missing/reordered/nested markers, source collisions, unsafe map
  boundaries, invalid clauses, and partial multi-file writes must fail closed.

### Required acceptance

- Representative output matches the authoritative TSRX formatter convention,
  preserves comments/Unicode/raw text, reparses with authoritative TSRX in
  tests, and converges after one pass.
- Lint labels land on original TSRX bytes; descendant safe fixes apply and
  structural/cross-segment fixes are rejected and counted.
- All 12 parser-invalid tracked files remain non-mutating rejects. Exactly
  143/179 valid committed-HEAD files are accepted; remaining valid failures classify
  only as later `@switch`, `@try`, dynamic tag, or style work.
- Ordinary standard-file output remains byte-identical to canonical Oxfmt for
  the same options, and ordinary lint remains within its upstream budgets.
- The control-flow benchmark corpus exercises nested and large repeated
  structures and keeps scan/build/lift linear, one projection buffer, and the
  existing P01-P07 thresholds.

### Stop conditions

- A legal projection requires source payload reordering, a second normal OXC
  parse, full AST conversion, per-node owned strings/boxes, JS hot-path ASTs,
  approximate diagnostic/fix mapping, or heuristic raw-text edits.
- Lift cannot prove marker identity/order/nesting/scaffold adjacency and exact
  indentation removal after two evidence-based corrections.
- Descendant labels or safe fixes cannot be mapped exactly.
- An OXC private API, source patch, fork, vendor snapshot, or adapter leak is
  required.
- Existing performance gates remain red after two evidence-based
  optimizations, or a threshold would need weakening.
- Work requires an external-repository write or a path outside the Worker card.

## Required continuation after T019

1. Add `@switch` and the source-order `@try` family using the now-proven
   overlay/marker/map protocol; cover all remaining expression positions and
   reach 173/179 valid Markless.
2. Add matched dynamic tags and raw style handling to reach 179/179 valid
   strict-format/lint parsing acceptance.
3. Add OXC config/options/plugin and type-aware capability matrices.
4. Ship Vite/Vite+ command and plugin-chain integration.
5. Ship platform npm binaries and a thin editor selector/client with
   format-on-save and live diagnostics.
6. Run upgrade lanes and the final temporary-copy Markless clean-room audit.

Zig/Yuku remains historical performance evidence only and is forbidden from
the product, build graph, Worker files, and distributed artifacts.
