# Rust/OXC core architecture

## Decision and ownership

The language core is Rust and uses the canonical OXC crates as-is, with no
fork, source snapshot, Cargo patch, or local checkout. The JavaScript side is
only thin npm, Vite+, configuration, and editor launchers. Zig/Yuku is a
read-only design reference, not a dependency.

`crates/oxc_adapter` is the only crate that imports OXC, pinned to commit
`8e0ed2ebb96137fb1611cdbd5742d5cb46037d40`. Every adapter dependency comes
from that one Git source so Cargo never mixes two incompatible copies of the
same crate. Upgrading OXC means bumping this adapter and the lockfile, then
passing the full behavior and performance suites.

## Compact TSRX overlay

`tsrx_syntax::scan` reads the source bytes once and records the TSRX
structure in a few compact arrays instead of building a full syntax tree,
which keeps scanning fast and memory-light. Its only direct dependency is
`unicode-id-start = "1"` (no direct OXC dependency). See
[Upstreaming TSRX to OXC](./upstreaming-to-oxc.md).

The overlay currently recognizes:

- `@{ }` statement containers;
- `@if`, `@else if`, and `@else`;
- `@for`, `@for await`, and `@empty`;
- `@switch`, `@case`, and `@default`;
- `@try`, `@pending`, and headerless or bound `@catch` clauses;
- declaration and assignment loop bindings;
- `index` and `key` annotations;
- dynamic JSX tags, where the opening and closing names must refer to the
  same expression;
- lowercase `<style>` elements, whose raw contents are carried through
  untouched; and
- statement, expression, direct JSX-child, and nested contexts.

Strings, comments, regex literals, template text, JSX text and attributes,
and Unicode identifiers and escapes are never misread as TSRX syntax. When
the scanner is not sure, it stops with a clear error instead of guessing.
That covers stale scan results, orphan or reordered clauses, mismatched
closing tags, and unsupported grammar.

## Native lint path

<!-- diagram:native-lint -->

- Ordinary `.js`, `.jsx`, `.ts`, and `.tsx` files skip the TSRX scanner
  entirely, and each run's metadata proves zero scan and projection cost for
  them.
- TSRX control syntax is projected (translated) into legal TSX that OXC can
  parse. Your code is copied in byte-for-byte, with a map of exactly which
  output ranges are your original bytes, so OXC lints the exact bytes you
  wrote.
- A diagnostic must land entirely on your bytes or it is suppressed and
  counted. A fix that would touch the generated glue code, or cross between
  authored and generated ranges, is rejected.
- Dynamic tag names are validated as real expressions during the single OXC
  parse of the file.
- Raw `<style>` payloads stay outside the JavaScript AST, so this path does
  not claim CSS linting.

End-to-end tests cover `no-debugger` and `no-unused-vars` labels plus
`no-var` fixes across the branch, loop, switch, and try families. An
accepted fix is applied to the original TSRX and rescanned, projected, and
reparsed before write. The project does not claim every rule behaves
identically over the generated glue code.

## Opt-in type-aware lint path

When type-aware linting is off (the default), each TSRX file gets one
canonical OXC parse and no TypeScript-Go process starts. `--type-aware` and
`--type-check` add a separate project lane:

<!-- diagram:type-aware-lint -->

- A second, type-focused projection emits legal TypeScript/TSX that keeps
  loop element types, variable scopes, `.tsrx` imports, catch-parameter
  context, and component callability. Declarations are appended after your
  code so byte offsets never move.
- Rules resolve against the authored file name, and the whole mixed project
  goes to the type checker over stdin in one request. Nothing is written
  into your project.
- The adapter verifies the official binary from `oxlint-tsgolint` 0.24.0. A
  missing or mismatched binary fails with an actionable error instead of
  silently downgrading.
- A fix must be marked safe, land in one exact authored range, and survive a
  rescan and reparse before write. Everything else stays visible as a
  non-applicable diagnostic.

## Native formatter path

<!-- diagram:native-formatter -->

`tsrx_format::format_text` is the default-options library entry point,
`tsrx_format::FormatSession::format_text` is the configured editor and batch
entry point, and ordinary files call `oxc_adapter::format` directly.

Formatting a `.tsrx` file is a three-step round trip:

- Projection turns the TSRX-specific pieces into tagged comment markers
  inside one legal TSX buffer.
- Canonical Oxfmt formats that buffer exactly once.
- A lift pass (the reverse of projection) removes the scaffolding and
  restores the authored `@` syntax, then checks its own work: every marker
  present, unique, and in order, no scaffolding left behind, and a rescan
  must find the same TSRX structure the original had.

Dynamic closing tag names are rebuilt from the formatted opening expression,
and each raw `<style>` payload is copied from the original source. Raw CSS
is preserved, never reformatted, until OXC exposes its CSS formatter through
a clean public boundary. The indexed lift renderer holds 21.79 MiB/s with no
per-byte slowdown on the stress corpus, and 134.78 MiB/s on the fast path
for pure statement controls.

`oxc-tsrx-fmt` supports stdin, check mode, transactional writes, explicit
multi-file input, and an optional thread count. Every file must format
successfully before any write is staged, recoverable staging failures restore
the originals, and symbolic links are rejected.

## Configured session layer

Configuration lives above the per-file hot path. A command or editor host
builds configured `LintSession` and `FormatSession` values, loads JSON/JSONC
configuration once, and reuses it for every file. The editor keeps its
diagnostic session separate from its fix-enabled session, so an editor
request can never reach a disk-writing CLI path:

<!-- diagram:configured-session -->

The npm and Vite+ hosts may resolve your `vite.config.*` once through Vite+'s
public `resolveConfig` API, extract the `lint` or `fmt` field, and hand the
native process disposable JSON. None of this creates files in your project.

Unsupported capabilities are rejected before any source work: direct-native
JS/TS config modules, JavaScript lint plugins, `.editorconfig`, and
callback-backed or unknown TSRX-affecting formatter options. Type-aware lint
runs only with the explicit CLI opt-in; the Vite+ companion turns a resolved
`typeAware` or `typeCheck` option into that opt-in. The full option matrix is
in `docs/integrations/configuration.md` and `docs/integrations/vite-plus.md`.

## Performance evidence

Performance is a release gate: every release build must pass a frozen set
of budgets, and each report keeps enough detail to be re-checked later. The
headline results:

- Linting and formatting ordinary JS/TS/TSX files adds no measurable
  overhead over plain OXC. TSRX scanning and projection run in the hundreds
  of MiB/s.
- On a matched 1,000-file corpus the CLI finishes in about 46 ms, where
  official Oxlint takes about 41 ms and ESLint takes about 609 ms.
- Type-aware lint costs roughly 23 ms per file, and editor responses stay
  well under a millisecond after a fresh open of about 2.4 ms.

All the tables, budgets, methodology, and report files live on the
[Benchmarks](/reference/benchmarks) page; this page does not duplicate them.

## Current boundary

A pinned, read-only checkout of the real Markless codebase serves as the
reference corpus. What is proven today:

- All 179 parser-valid tracked files format, reparse, and converge
  (formatting the output again changes nothing), all 12 known parser-invalid
  fixtures are rejected, every raw `<style>` payload survives byte-for-byte,
  and the external worktree is never modified. This covers the grammar and
  formatter corpus, not Markless's compiler or runtime.
- Real Vite build, dev watcher/HMR, and literal Vite+ `vp build` and
  `vp dev` matrices pass, alongside mixed lint, format, and check commands,
  with no OXC for TSRX runtime transform.
- A real VS Code Extension Host walkthrough proves automatic activation,
  authored diagnostics, live config refresh, configured format-on-save, one
  validated quick fix, and no external Markless writes. A protocol-level
  test matrix proves diagnostics and recovery for malformed buffers.

Known gaps:

- Nested dynamic JSX inside a dynamic-name expression is unsupported.
- Raw CSS stays unformatted and unvalidated until the canonical CSS
  formatter is usable without patches.
- JSON/JSONC configuration is implemented; JavaScript plugins, direct-native
  JS/TS config modules, nested config, and alternate reporters are not.
- Hosted production of all eight platform candidates, external publication,
  and deployment remain approval-gated release operations.
