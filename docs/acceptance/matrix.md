# Release acceptance matrix

This is the human-readable index for the fresh `0.1.0` owner oracle run on
2026-07-16. Machine truth is retained in
[`clean-room-report.json`](clean-room-report.json) and
[`performance-report.json`](performance-report.json). Both reports have
`status: "passed"` and `failure: null`.

The correctness run used a byte-exact disposable copy, a lifecycle-script-free
`npm ci`, empty consumer projects, fresh Cargo targets, and locally produced
package artifacts. The performance run used the frozen same-machine budgets.
Neither run publishes or deploys anything.

## Correctness and ecosystem oracle

| Oracle clause | Direct observation | Retained evidence |
| --- | --- | --- |
| Clean install and native build | Fresh npm install, Rust formatting, Clippy, 44 Rust tests, release binaries, and the editor bundle all exited successfully in a disposable source copy. No source-tree `node_modules` or binary override was used. | `clean-room-report.json`: `isolation`, `matrix.cleanSource`, and command records 6–13 |
| Installable packages | Untouched `@oxc-tsrx/runtime`, native, `oxlint-tsrx`, and `oxfmt-tsrx` tarballs were installed into an empty consumer with install scripts disabled. All native binaries resolved from that consumer's `node_modules`; the npm audit was empty. | `matrix.packagedConsumer.install`, `.resolutions`, `.packages`, `.audit`, and all nine `.assertions` |
| Native TSRX diagnostics and fixes | One OXC parse reported `debugger;` at authored byte 37 and `var` at byte 49. The identity-safe fix changed only `var` to `const`, then reparsed and relinted successfully. | `matrix.authoredSpanAndFix`; the diagnostic command's exit status `1` is intentional because `no-var` is denied |
| Formatter correctness | Check/write/check converged from installed packages. Every one of 179 parser-valid Markless files formatted, reparsed, and converged; all 12 parser-invalid completion fixtures were rejected; raw style payloads stayed byte-exact. | `matrix.claims.formatCheckWriteConverges`, `matrix.marklessCorpus`, and the successful `read-only 179-file Markless format/reparse/convergence corpus` command |
| Config, rules, plugins, and type awareness | JSON, JSONC, and serializable Vite config behavior passed; real OXC rules and built-in plugin namespaces passed; tsgolint ran once per opted-in batch. Unsupported JavaScript plugins fail loudly rather than silently downgrading. | `matrix.claims.jsonJsoncAndViteConfig`, `.builtinPluginAndTypeAware`, `.javascriptPluginUnsupportedLoudly`; `matrix.packagedConsumer.assertions.typeAwareTsgolint`; successful product matrix |
| Ordinary JS/JSX/TS/TSX path | Every ordinary source family remained delegated to the canonical direct path, with output/diagnostic parity and zero TSRX scan or projection allocation. | `matrix.claims.ordinaryJsJsxTsTsxDelegated`; successful product matrix; native lint/format direct-path parity tests and P01/P04 assertions |
| Vite build, plugin chain, dev, and HMR | A real framework compiler build and a real Vite dev-server edit passed through the official TSRX plugin chain, including watcher invalidation and an HMR payload, without an OXC transform. | `matrix.claims.viteFrameworkBuildDevHmr`; successful framework-chain test |
| Vite+ minimum and current | Clean physical consumers passed literal `vp build`, literal `vp dev` served compilation plus changed-source retransform, mixed lint, format-check, and convergent `check --fix` on Vite+ `0.1.24` and `0.2.4`. Both lanes had zero audit findings and no environment overrides. | `matrix.claims.vitePlusBuildDevRetransform`, `matrix.vitePlus.policy`, and both `matrix.vitePlus.lanes[].proof` objects |
| Installed editor behavior | A real target VSIX activated automatically beside Markless, used its embedded native server, published exact authored diagnostics, performed format-on-save, and applied a safe code action. | All six `matrix.editor.assertions`; embedded-server SHA-256 and installed directory in `matrix.editor` |
| External repository safety | Markless HEAD, tracked/staged diffs, status, untracked paths, and untracked bytes had the same combined fingerprint before and after. The editor operated on a disposable copy. | `external.unchanged: true`, identical `external.before/after`, `matrix.editor.markless.externalWrites: false` |
| Non-fork OXC architecture | The package boundary suite proved one exact canonical OXC adapter revision and rejected Cargo patches, vendor trees, copied OXC crates, and adapter bypasses. | Successful `package/non-fork/legal artifact matrix` command; `matrix.versions.oxcRevision` = `8e0ed2ebb96137fb1611cdbd5742d5cb46037d40` |
| Legal and release artifacts | Locked inventories verified 205 Rust and 12 bundled VS Code dependencies; host packages and the VSIX passed artifact checks. | Successful `locked legal inventories` and 23-test package/non-fork/legal matrix commands |

Every boolean under `matrix.claims`, `matrix.packagedConsumer.assertions`, and
`matrix.editor.assertions` is `true`. Both supported Vite+ lanes have
`proof.supported: true` and every performance lane below has
`allPassed: true`.

## Frozen performance oracle

| Lane | Fresh observation | Frozen gate result | Raw report |
| --- | --- | --- | --- |
| Native lint | 262.09 MiB/s median scan/project/parse; 78.67 MiB/s complete CLI lint; 1.160× equivalent-TSX CLI latency; 3.22 ms fresh-process p95 | 19/19 pass | [`native-lint/results-1784242044684.json`](../../benchmarks/native-lint/results-1784242044684.json) |
| Native format | 129.45 MiB/s sequential; 742.54 MiB/s default-thread p95; 20.14 MiB/s generalized control; 3.26 ms fresh-stdin p95; 1.143× complete-output RSS in three adjudication runs | 25/25 pass | [`native-format/results-1784242059253.json`](../../benchmarks/native-format/results-1784242059253.json) |
| Type-aware lint | Default syntax 2.62 ms p95 with zero type processes; one-file type-aware 25.04 ms p95; two-file project 24.46 ms p95; one type process per batch | 8/8 pass | [`type-aware/results-1784242060765.json`](../../benchmarks/type-aware/results-1784242060765.json) |
| Vite/Vite+ process boundary | Mixed lint 61.18 ms p95 / 1.868× canonical; mixed format 137.89 ms p95 / 1.331× canonical; Vite+ 0.2.4 mixed lint 322.65 ms p95; one native TSRX parse | 6/6 pass | [`vite/results-1784242073158.json`](../../benchmarks/vite/results-1784242073158.json) |
| Incremental editor | Fresh open 2.49 ms median / 2.84 ms p95 across 100 processes; diagnostics 0.124 ms p95; format 0.378 ms p95; code action 0.195 ms p95; 11.14 MiB RSS and 0 MiB growth after 1,000 edits | 8/8 pass | [`editor/results-1784242073843.json`](../../benchmarks/editor/results-1784242073843.json) |
| Matched CLI comparison | Same 1,000 explicit TSX files and one rule: ESLint 660.17 ms, official Oxlint 40.87 ms, OXC for TSRX 24.99 ms median; paired 20% TSRX workload 26.28 ms / 1.052× all-TSX | 3/3 pass after 5 warmups + 20 measurements | [`comparative/results-1784242094588.json`](../../benchmarks/comparative/results-1784242094588.json) |

Same-build ordinary-path and mixed-companion ratios are like-for-like as named.
Cold direct-Rust versus official npm-launcher ratios are diagnostic guardrails,
and 16.6 MiB/s is an absolute cross-corpus-derived formatter threshold rather
than a Prettier speedup claim. No lane equates complete TSRX formatting or
linting with a parser-only benchmark.

## Reproduce

```sh
MARKLESS_ROOT=/Users/jacksm5pro/dev/open-source/markless \
  node tests/acceptance/run.mjs

node tests/acceptance/run-performance.mjs
```

These commands may open an isolated VS Code test window. They must leave the
external Markless fingerprint unchanged. Publishing packages, deploying the
site, pushing a repository, publishing the VSIX, and posting launch material
remain separate approval-gated actions.
