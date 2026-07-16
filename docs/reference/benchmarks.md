---
title: Benchmarks
description: The frozen release performance gates, generated at build time from the latest committed reports.
---

# Benchmarks

Performance is a release gate, not a marketing page. Dedicated native and
ecosystem harnesses assert frozen budgets. Each latest release-gate report
keeps the applicable host/toolchain/OXC identity, corpus hash, raw timing or
RSS samples, sampling policy, distributions, and every assertion.

Everything below is generated at build time from the reports selected by
`docs/acceptance/performance-report.json`, so this page cannot silently select
a newer failed or incomplete raw run: rerun the aggregate performance oracle,
rebuild the docs, done.

How to read the results:

- **MiB/s** is throughput: how many mebibytes of source the tool processes
  per second. Higher is better.
- **p95** is the nearest-rank 95th percentile of the retained sample set. Each
  raw report records its sample count; p95 shows tail behavior rather than only
  the average.
- **Ratios like 1.004×** use the denominator named by that row. Same-build
  ordinary-path ratios measure comparable OXC overhead; cold-launcher,
  type/default, and scaling ratios are different contracts described below.
- **Budget** is the frozen threshold the result must beat. If any budget
  fails, the release fails. Each chart bar fills toward the dashed budget
  line; staying short of the line passes. The number next to the bar is the
  measured result, and hovering or focusing a row shows the full details.

Three comparisons need extra care. The matched CLI lane gives ESLint, official
Oxlint, and OXC for TSRX the same byte-identical 1,000-file TSX corpus, one
`no-debugger` rule, one explicit file list, zero-diagnostic default output,
five warmups, and twenty measured processes. Its separate 20% TSRX result is
paired from the same generated component specifications and is only an
internal all-TSX-versus-mixed workload ratio. The native cold-start ratios compare a direct
Rust executable with the official npm launcher, so the absolute cold limits are
the product gates and the ratios are diagnostic rather than tool-speed claims.
The formatter's 16.6 MiB/s historical-incumbent floor is an absolute regression
threshold derived from an older 1.66 MiB/s measurement on a different corpus;
it is not a like-for-like Prettier speedup claim. Same-build canonical OXC
controls and the ordinary-source parity lanes are the comparable overhead
measurements.

Regenerate the reports with:

```sh
cargo run --release --locked -p oxc_tsrx_benchmark -- \
  --assert benchmarks/native-lint/budgets.json
cargo run --release --locked -p oxc_tsrx_format_benchmark -- \
  --assert benchmarks/native-format/budgets.json
node benchmarks/vite/run.mjs
npm run benchmark:type-aware
npm run benchmark:editor
npm run benchmark:comparative
```

<!-- benchmarks:auto -->

## Measurement hygiene

- Harnesses are release-only.
- `memory-stats` is linked only into benchmark executables, never the
  distributed CLI.
- Normal source metadata excludes configuration time; aggregate metadata and
  dedicated configuration lanes report it separately.
- Formatter reports retain scan, projection, canonical parse, canonical
  format, and checked-lift timing arrays separately. Near-threshold RSS follows
  the retained three-run adjudication policy.
- Every frozen assertion must pass without changing a threshold.
