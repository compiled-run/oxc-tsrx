# T006 formatter architecture decision

Date: 2026-07-15

Decision: approve a two-tier formatter architecture and make the first tier a
complete user-facing vertical slice. Standard files delegate directly to an
official Oxfmt release. TSRX uses a public formatter-backend protocol whose
default implementation is the currently published `@tsrx/prettier-plugin`.
That default is a correctness/installability fallback, not the final native
performance backend and not "Oxfmt parsing TSRX." A later native backend can
replace it without changing CLI, config, editor, or Vite+ integration.

## Current public capability evidence

Registry metadata was queried on 2026-07-15:

| Package | Current tested version | Result |
| --- | ---: | --- |
| `@tsrx/prettier-plugin` | 0.3.97 | Published, MIT, depends on `@tsrx/core` 0.1.40, peer `prettier >=2` |
| `prettier` | 3.9.5 | Published control runtime |
| `oxfmt` | 0.59.0 | Published CLI and async `format(fileName, source, options)` API |
| Vite+ bundled `oxfmt` | 0.57.0 | Same CLI surface and materially identical public format types |
| `yuku-parser` / `yuku-codegen` | 0.6.3 | Published, but parser rejects `lang: "tsrx"` |
| local Yuku `feat/tsrx` | `bf03e146...` | Parses/prints TSRX, but is an unreleased user-fork branch |

Black-box Oxfmt calls under both 0.57.0 and 0.59.0 formatted `.tsx` through the
public API and returned `Unsupported file type: input.tsrx` for the same TSRX
source. Therefore stock Oxfmt support cannot be supplied by configuration or a
filename shim.

`@tsrx/core` 0.1.40 exposes the authoritative complete TSRX parser and comment/
style AST, but no general-purpose source printer. The published TSRX Prettier
plugin is currently the only complete, clean-install parser/printer pair.

## Same-machine formatter measurements

Machine: Apple M5 Pro, 18 logical CPUs, Node 24.15.0, macOS 25.5.0. Corpus:
read-only Markless commit `fdcb833616c609385419c6b810069ac7df6ba4dd`,
hidden directories and `node_modules` excluded.

### Published TSRX Prettier control

- 178/191 files formatted; 13 failures were incomplete/error-oriented editor
  fixtures plus one valid BigInt source that the printer could not serialize.
- Median complete output time: 70.65 ms for 177,376 source bytes, 2.39 MiB/s.
- Two real files required a second formatting pass to converge:
  `packages/router/fixtures/router/pages/index.tsrx` and
  `packages/vitest-browser/browser/fixtures/progressive-event-only.tsrx`.
- A bounded convergence wrapper can make returned output idempotent, but this
  fallback does not satisfy the final >=15 MiB/s formatter gate.

### Unreleased Yuku TSRX branch control

- 178/191 files parsed and printed; the failure intersection differs slightly
  from the Prettier control.
- Median parse + full JS AST decode + re-encode + native print: 13.95 ms for
  181,454 bytes, 12.41 MiB/s.
- Zero second-pass differences on its valid intersection.
- The JS materialization boundary is exactly the avoidable overhead warned
  about by the performance contract. A direct native source-to-formatted-output
  entry point should be measured when the upstream TSRX grammar is releasable.
- Published 0.6.3 was independently installed in `/tmp` and failed with
  `invalid enum value for ast.Lang: 'tsrx'`; the local branch cannot be a clean
  product dependency.

## Product contract

```text
oxc-tsrx fmt
  standard files -> selected official Oxfmt binary, original args/config
  .tsrx files     -> @oxc-tsrx/formatter backend contract
                       default: published TSRX Prettier adapter
                       future: native source-to-output adapter
```

The formatter package must remain separate from lint core so lint startup does
not load Prettier. The CLI dynamically loads only the command-specific package.
The editor-facing API accepts source, filename, normalized options, and an
optional backend specifier; it returns formatted code, diagnostics, backend
identity, convergence count, and separated timings.

For `.tsrx`, read a documented shared subset of `.oxfmtrc.json` with Oxfmt
defaults: `printWidth`, `tabWidth`, `useTabs`, `semi`, `singleQuote`,
`jsxSingleQuote`, `trailingComma`, `bracketSpacing`, `bracketSameLine`,
`arrowParens`, `singleAttributePerLine`, `endOfLine`, and
`insertFinalNewline`. Unsupported TSRX options must be reported, not silently
claimed. Standard files remain wholly owned by official Oxfmt config loading.

Default TSRX output must parse, converge within three bounded passes, preserve
semantic AST content and comments, and leave the source unchanged on failure.
The known BigInt control becomes a retained unsupported/failure fixture until a
backend handles it; no formatter success may be reported for that file.

## Claim boundary

Allowed now after T007 proof:

> OXC for TSRX provides one formatter workflow: official Oxfmt for supported
> files and a TSRX-aware fallback for `.tsrx`, with a shared documented config
> subset and an editor-ready API.

Not allowed:

- "Oxfmt supports or formats TSRX natively."
- "The TSRX fallback has Oxfmt performance."
- "All Oxfmt options/plugins apply to TSRX."
- "The final native formatter gate is met."

## T007 Worker package

Deliver the full fallback vertical slice rather than only a protocol. Tests
must start red and then prove CLI write/check/stdin behavior, real comments and
style syntax, Unicode, semantic AST equivalence, bounded idempotence, clear
failure/no-write behavior, shared config options/defaults, direct standard-file
delegation under Oxfmt 0.57.0 and 0.59.0, dynamic command loading, tarball
installation, and phase/memory benchmarks. Retain the Markless control and the
native-backend debt so the following package can replace the backend without
changing user-facing integration.

No external repository was modified.
