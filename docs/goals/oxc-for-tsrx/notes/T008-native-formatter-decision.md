# T008 native formatter decision

Date: 2026-07-15

Decision: **approved with conditions**. Use the public, MIT-licensed TSRX Yuku
frontend at exact revision `bf03e146d97ae2f0c2d4c4ec90456e1e544d2760`
as a pinned build dependency behind a project-owned native source-to-output
package. Do not vendor it, copy it, patch it, or expose its tree to JavaScript.
Keep official Oxfmt as the direct owner of ordinary JavaScript and TypeScript.

The Yuku parser/tree is approved; its current unmodified code-generator layout
is not approved as the product formatter. T009 must add a format-oriented
native layout layer and must pass configuration, semantic, idempotence, package,
and end-to-end performance gates before the native backend becomes the default.
The T007 Prettier implementation remains the capability/platform fallback.

## Why this remains a non-fork OXC integration

No OXC, Oxlint, Oxfmt, or Vite+ source is imported or changed. Their official
packages remain independently selected and upgradable. Standard files still go
straight to the selected official Oxfmt executable, and lint retains the public
Oxlint process/config boundary proved in T005.

The TSRX frontend is a separate language dependency, not an OXC fork. Zig must
fetch the exact public commit and content hash during maintainer builds; release
artifacts must contain only project-owned bindings plus the license notices.
Consumers install a prebuilt platform package and do not need Zig or a Yuku
checkout. Upgrading Oxfmt or Oxlint therefore cannot cause this frontend to stop
compiling, while changing the frontend revision remains an explicit, tested
maintenance action.

This distinction is important: the selected commit is on a public fork branch
because upstream Yuku 0.6.3 does not yet contain `Lang.tsrx`. OXC for TSRX does
not create or maintain another source fork or patch queue. The unreleased branch
is an acknowledged supply/maintenance risk, not a clean-install claim hidden
behind a local checkout.

## Dependency and public-API qualification

| Item | Evidence |
| --- | --- |
| Upstream Yuku release | 0.6.3, revision `9153d418...`, MIT; JS/TS/JSX/TSX only |
| TSRX revision | `thejackshelton/yuku` `feat/tsrx` at `bf03e146...`, MIT |
| Reproducible fetch | `git+https://github.com/thejackshelton/yuku.git#bf03e146...` |
| Zig content hash | `yuku-0.3.0-m6ransbeEgBItlR3ktj6CNCkRREbMIaaXIzLLrvI6T5u` |
| Public Zig surface | Build module `parser`; exports `parse`, `ast`, `codegen`, traversal and semantic APIs |
| Toolchain | Minimum Zig 0.16.0; local qualification used Zig 0.16.0 ReleaseFast |
| Upstream divergence | Branch is 1 commit ahead and 38 behind current upstream main; its patch does not apply cleanly to v0.6.3 |

T009 may consume only the exported build module. If implementing the layout
requires importing file-relative internals, copying the existing printer, or
carrying changes against the dependency, the task must stop. A public dependency
update may require an intentional adapter change, but a new OXC release may not.

## Direct source-to-output evidence

The retained control in `T008-yuku-control/` compiles against the public parser
module. It accepts UTF-8 source, parses once into Yuku's indexed arena, prints
directly in Zig, and returns UTF-8 output. It performs no AST decode/encode,
JSON serialization, or per-node JavaScript allocation.

On the read-only Markless corpus at
`fdcb833616c609385419c6b810069ac7df6ba4dd`, Apple M5 Pro, macOS 25.5.0,
Node 24.15.0, and Zig 0.16.0:

| Boundary | Valid input | Median | Throughput | Result |
| --- | ---: | ---: | ---: | --- |
| T007 published fallback | 177,376 bytes | 74.08 ms | 2.28 MiB/s | 178 valid; BigInt fails |
| T006 Yuku through full JS AST decode/re-encode | 181,454 bytes | 13.95 ms | 12.41 MiB/s | Below final gate |
| T008 direct native parse + full output | 181,454 bytes | 1.334 ms | 129.73 MiB/s | 178 valid; BigInt succeeds |

Five independent direct runs ranged from 81.4 to 129.73 MiB/s. The latest
five-sample run produced 177,741 output bytes with zero second-pass differences.
Twenty fresh-process formats of a representative document measured 1.68 ms
median and 2.10 ms p95. The single-corpus control process peaked at 5,408 KiB
RSS; an eight-corpus argument/memory stress run peaked at 23,328 KiB. These RSS
numbers measure the standalone control process rather than a Node process delta,
so T009 must retain a true binding/host measurement.

The direct boundary is about 10.45x faster than the JS AST boundary. This is the
decisive evidence for one native call and no JavaScript tree materialization.

## Correctness and layout qualification

The authoritative `@tsrx/core` parser accepts 179 of 191 Markless `.tsrx`
files. The native frontend accepts 178 of those. On that 178-file intersection:

- all native outputs reparse with the authoritative parser;
- normalized semantic ASTs match;
- normalized comments match;
- all second calls return identical output;
- the BigInt source that defeats the T007 printer succeeds natively.

The sole authoritative-valid native failure is
`completion-matrix/construct-typing.tsrx`, an editor-completion fixture. It is a
capability-fallback control, not permission to treat arbitrary corrupt native
output as fallback input.

Unmodified Yuku output is nevertheless not the final formatter:

- it exactly matches the configured fallback on 0 of 178 files;
- 119 files contain leading tabs under the default `useTabs: false`;
- 57 files contain lines beyond `printWidth: 100`;
- a breakable form/JSX line reaches 288 columns;
- the code-generator options do not implement the complete shared format
  contract.

Consequently, T009 must build a repository-owned native layout implementation
over the public tree or over a token-correct native intermediate. Merely wrapping
the current code generator and declaring success is rejected.

## Approved hot-path boundary

```text
JS/editor/CLI source string
  -> lazy @oxc-tsrx/formatter-native loader
  -> one N-API call
       -> one UTF-8 input conversion/copy
       -> one Yuku parse into indexed arena storage
       -> native format/layout traversal
       -> one owned UTF-8 output buffer
  -> one JS output string
```

No complete AST, node list, or document tree crosses N-API. No second production
parse validates ordinary successful output. Semantic equivalence and repeated
formatting are exhaustive test/oracle checks; the native algorithm itself must
preserve the token/AST meaning by construction. The implementation should fuse
printing and layout where practical so a temporary complete codegen string does
not become permanent architecture. Phase timings must expose parse, print/layout,
and output conversion separately.

The unavoidable JavaScript-string to native-UTF-8 conversion and native output
to JavaScript-string conversion are included in end-to-end budgets. A Buffer
entry point may be added for batch callers, but it cannot replace the existing
string backend contract.

## Native capability and fallback policy

The default loader is native-first and lazy:

1. Standard `.js/.jsx/.ts/.tsx` commands never resolve or load the native TSRX
   package; official Oxfmt owns them directly.
2. A supported native binding formats `.tsrx` in one native call.
3. Missing platform binding, explicit unsupported option, or native parse/
   grammar capability failure may invoke the T007 fallback.
4. If both parsers reject input, return the original bytes and diagnostics.
5. Native internal errors, invalid result shapes, or detected corruption fail
   closed and do not silently route potentially corrupted output into fallback.
6. Every fallback result remains subject to T007's no-write and bounded
   convergence contract.

Fallback parsing on a native failure is allowed because it is off the successful
hot path and is necessary during grammar-version overlap. It must be counted and
reported in corpus benchmarks. BigInt is a retained native-success control and
`construct-typing.tsrx` is a retained fallback-success control.

## Frozen T009 performance gates

All throughput measurements include complete output production and use the same
read-only Markless revision, byte denominator, warmup, and sample policy.

| Gate | Budget |
| --- | ---: |
| Raw native parse + layout + output, native-supported intersection | >=50 MiB/s median |
| End-to-end in-process backend, native-supported intersection | >=15 MiB/s median |
| Warm representative small-document backend latency | <=2.0 ms p95 |
| Cold native module import + one format | <=50 ms p95 |
| Node host RSS delta for representative corpus | <=32 MiB peak |
| Successful native output parse/semantic/comment/idempotence failures | 0 |
| Ordinary standard-file CLI p95 ratio versus direct official Oxfmt | <=1.10x |
| Standard-only native/Prettier modules loaded | 0 |

Use at least two warmups and ten measured in-process corpus samples, report
median/p95/raw samples, and retain a separate cold-process sample set. Classify
unbreakable strings/comments/tokens separately when checking width; no breakable
line may silently exceed the configured width. Any budget change requires a new
Judge receipt with stronger comparable evidence.

## Packaging contract

`@oxc-tsrx/formatter-native` is a small JavaScript loader/backend package. It
uses optional platform packages, following ordinary native npm practice:

- `@oxc-tsrx/formatter-native-darwin-arm64`
- `@oxc-tsrx/formatter-native-darwin-x64`
- `@oxc-tsrx/formatter-native-linux-x64-gnu`
- `@oxc-tsrx/formatter-native-linux-arm64-gnu`
- `@oxc-tsrx/formatter-native-linux-x64-musl`
- `@oxc-tsrx/formatter-native-linux-arm64-musl`
- `@oxc-tsrx/formatter-native-win32-x64-msvc`

T009 must prove a clean current-platform tarball install with no Zig or source
checkout in the consumer. It must also retain loader tests and build metadata
for the complete matrix above. It may not claim the unbuilt platforms are
released; later packaging/CI work remains required before publication.

## T009 Worker package

Implement the native-first formatter as one user-visible vertical slice:

- begin with red black-box, style, capability, package, and performance tests;
- add a pinned public-source Zig build and one-call N-API binding;
- implement format-oriented native layout for the shared configuration subset;
- preserve the T007 backend result/CLI/editor contract;
- cover control flow, TypeScript, JSX, inline style, comments, Unicode, quotes,
  semicolons, indentation, width-driven breaks, and final-newline behavior;
- format all 179 authoritative-valid Markless files through native-first plus
  explicit fallback, with exact accounting;
- retain direct official Oxfmt behavior and its no-native-load regression;
- pack and install the native loader, current platform artifact, formatter, and
  CLI into a fresh offline consumer.

Stop if this requires OXC source, a Yuku source copy/patch queue, private
file-relative Yuku APIs, complete JS AST materialization, or a second parse on
the ordinary successful path. Stop after two evidence-based layout approaches
if semantic/idempotence/style gates cannot pass, or after two measured boundary
optimizations if end-to-end throughput remains below 15 MiB/s.

No external repository was modified.
