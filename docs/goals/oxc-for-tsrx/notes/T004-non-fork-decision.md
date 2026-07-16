# T004 non-fork architecture decision

Date: 2026-07-15

Decision: proceed with an independent **language adapter**, never an OXC source
fork. Official OXC/Vite+ packages are black-box, independently upgradable
dependencies. Native stock-binary TSRX support remains an upstream objective,
not a claim about today's binaries.

## Invariants

1. No OXC, Oxlint, Oxfmt, or Vite+ source is vendored, patched, copied, or
   compiled into this repository.
2. No private OXC Rust crate or generated AST API is imported.
3. Ordinary JS/TS/JSX/TSX paths delegate directly to the official tool with no
   TSRX parse, AST conversion, or output rewrite.
4. TSRX diagnostics/fixes fail closed when mapping is ambiguous.
5. Each official dependency is exercised at the Vite+-bundled baseline, current
   release, and a newer/canary capability lane when available.
6. Compatibility is capability-driven. Unknown upstream CLI options pass
   through; unsupported output schemas produce a clear error rather than a
   best-effort rewrite.

## Verified current releases and seams

Registry metadata fetched on 2026-07-15:

| Package | Current | Relevant public seam |
| --- | ---: | --- |
| `oxlint` | 1.74.0 | CLI, JSON diagnostics, config/JS plugins, LSP |
| `oxfmt` | 0.59.0 | CLI/stdin/LSP and public async `format()` API |
| `vite-plus` | 0.2.4 | Vite plugin pipeline, config, installed tool resolution, tasks |
| Vite+ 0.2.4 bundled `oxlint` | 1.72.0 | minimum current Vite+ compatibility lane |
| Vite+ 0.2.4 bundled `oxfmt` | 0.57.0 | minimum current Vite+ compatibility lane |
| `@tsrx/core` | 0.1.40 | authoritative parser and mapping types |
| `@markless/compiler` | 0.1.1 | public `compile_to_volar_mappings` adapter |
| `@tsrx/react` | 0.2.40 | public `compile_to_volar_mappings` adapter |

Oxlint 1.74.0 was black-box verified to emit JSON diagnostics with filename,
rule code, severity, and byte-offset labels. `@markless/compiler` 0.1.1 was
black-box verified to emit legal virtual TSX plus Volar source/generated
mappings from TSRX.

Yuku needs a precise qualification. The official published `yuku-parser` and
`yuku-codegen` 0.6.3 packages do **not** accept `lang: "tsrx"`. The read-only
local `feat/tsrx` branch at `bf03e146d97ae2f0c2d4c4ec90456e1e544d2760`
does parse, print, and source-map TSRX successfully and produced the retained
performance baseline. It is a high-value native backend candidate, but it is
not yet a published clean-checkout dependency and must not be represented as
one. Product use requires an upstream release or a separately reviewed
build/dependency package; it does not justify an OXC fork.

## Architecture

```text
.tsrx source
   |
   +-- framework compiler adapter (public compile_to_volar_mappings)
   |       -> virtual .tsx + mapping table
   |       -> official oxlint / tsgolint
   |       -> diagnostics and proven-safe edits mapped back to .tsrx
   |
   +-- TSRX formatter backend
   |       -> authoritative TSRX AST/printer
   |       -> Oxfmt/Vite+ option subset normalized by this project
   |       -> parse/idempotence/semantic checks
   |
   +-- existing framework Vite plugin
           -> Vite/Vite+ dev, build, HMR, Rolldown, downstream plugins
```

The compiler is an adapter, not hard-coded to Markless. A module satisfies the
protocol when it exports:

```ts
compile_to_volar_mappings(
  source: string,
  filename: string,
  options?: object,
): {
  code: string;
  mappings: Array<{
    sourceOffsets: number[];
    generatedOffsets: number[];
    lengths: number[];
    generatedLengths: number[];
    data?: { verification?: boolean };
  }>;
  errors: unknown[];
}
```

For diagnostics, Oxlint byte offsets are converted to generated UTF-16 offsets,
then mapped through verified Volar entries. Exact unchanged runs between source
and generated code supplement semantic mappings for copied syntax such as
keywords. A diagnostic may be shown only when at least its primary start maps;
the UI marks synthetic-only diagnostics separately or suppresses them by
policy.

For fixes, both endpoints must lie in one contiguous equal-length unchanged
run, the generated preimage must byte-for-byte equal the original TSRX slice,
and all edits must be non-overlapping. Anything else is reported without an
autofix. This intentionally supports fewer fixes before it risks corrupting
source.

## Present capability versus upstream completion

| Surface | Non-fork product now | Required for true stock first class |
| --- | --- | --- |
| Vite/Vite+ dev/build/HMR | Existing framework Vite plugin; test composition | Already public |
| Oxlint diagnostics | Batched virtual TSX, official CLI JSON, mapping | Custom file/parser hook (open OXC work) |
| Oxlint fixes | Only identity-proven mapped edits | Parser/processor hook with original spans |
| JS lint plugins | Run by official Oxlint on virtual TSX; synthetic-node caveats | Custom parser/visitor-key support |
| Type-aware lint/check | Shadow TSX project and official tsgolint, mapped output | TSRX-aware tsgo/virtual-file provider |
| Oxfmt workflow | Independent TSRX formatter backend plus direct official delegation for stock files | Oxfmt language adapter/parser-printer hook |
| `vp lint/fmt/check` | Tested proxy/alias mode or explicit Vite+ task composition | Vite+ custom check-provider hook |
| VS Code/Zed | Independent TSRX LSP/client composed with official OXC tooling | Official document selector/language hook |

The project must never say that its TSRX formatter *is* Oxfmt's native engine
before the upstream hook exists. It may say that users get one OXC-compatible
workflow, shared supported configuration, direct Oxfmt delegation for standard
files, and a native-speed TSRX backend.

## Revised performance gates

Historical same-machine baselines remain valid evidence: Yuku packed TSRX parse
136.74 MiB/s, decoded TSRX 38.35 MiB/s, OXC raw-transfer TypeScript AST 127.20
MiB/s, incumbent TSRX Prettier 1.66 MiB/s, and native Oxfmt TypeScript 37.39
MiB/s sequential / 315.91 MiB/s batched.

The adapter adds these gates:

| Gate | Budget |
| --- | --- |
| Standard-file fast path | Same official binary/API; median throughput >=95% and warm p95 <=1.10x direct upstream |
| TSRX frontend parse | Final native backend >=30 MiB/s decoded median and no worse than 1.25x its pinned direct control |
| Virtual TSX + mappings | >=15 MiB/s median on valid Markless intersection; retained time split from lint |
| Batched TSRX lint | >=10 MiB/s adapter preparation; one Oxlint process per batch; total median >=5 MiB/s on corpus |
| CLI cold overhead | Adapter p95 <= direct official control +35 ms before framework compilation |
| Incremental editor | cached unchanged files; edit-to-diagnostics p50 <=75 ms and p95 <=200 ms on representative files |
| TSRX format | >=15 MiB/s sequential median, >=9x incumbent Prettier, idempotent output included |
| Memory | peak RSS <=2.0x direct official batch and <=256 MiB above it on retained corpus |

The published compiler-adapter fallback may initially miss the final native
frontend gate. That is an explicit performance debt, not grounds to weaken the
final contract. Measurements determine whether to optimize the generic lowering
path, use a released Yuku backend, or pursue the upstream parser hook.

## First non-fork Worker slice (T005)

Build a package/CLI lint vertical slice around the compiler protocol and
official Oxlint CLI. Start with failing tests. A copied Markless-style fixture
must produce `no-debugger` at the original TSRX location under Oxlint 1.72.0 and
1.74.0; a copied `var` token must accept a proven-safe `no-var` fix; a synthetic
unmapped edit must be rejected; Unicode offsets must map correctly; ordinary
TSX must be handed to official Oxlint unchanged. Retain phase timings and a
small corpus benchmark.

This slice does not claim grammar-complete lint, type-aware lint, formatter,
Vite+ proxy mode, or editor support. Those remain required subsequent vertical
slices.

No external repository was modified. The rejected OXC tree is preserved only
as the reversible `/tmp/oxc-tsrx-rejected-native-20260715/root` archive and is
absent from the product repository.
