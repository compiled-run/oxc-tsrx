---
title: Benchmarks
description: The frozen release performance gates, generated from aggregate-selected reports.
---

# Benchmarks

Performance is a release gate, not a marketing page. Every selected report
keeps the host, toolchain, and OXC identity, corpus hash, raw timing or RSS
samples, sampling policy, distributions, and every assertion. Everything
below is generated at build time from the reports listed in
`docs/acceptance/performance-report.json`, so this page cannot silently pick
up a newer failed or incomplete run.

How to read the results:

- **MiB/s** is throughput: mebibytes of source processed per second. Higher
  is better.
- **p95** is the nearest-rank 95th percentile of the retained samples. It
  shows tail behavior, not just the average.
- **Ratios like 1.004×** divide by the denominator named in that row.
- **One chart per gate.** Every numeric release gate gets its own small
  chart with the gate's name on top and a one-line summary under it,
  including a plain "pass" or "FAIL" word.
- **The top strip** shows the checked value as a diamond on an axis that
  starts at zero. The dashed vertical line is the frozen budget, named in
  the strip's left label, and the lightly shaded area is the failing side
  of that budget. The release fails if the diamond ever lands in the
  shaded area.
- **The bottom strip** zooms in on the retained samples from the selected
  report, one dot per sample. Its axis is zoomed to the spread, so read
  its tick labels; it usually covers a much smaller range than the top
  strip. The solid vertical line is the median sample and the dotted
  vertical line is the p95 sample.
- **Ratio charts.** The diamond in the top strip is the asserted ratio,
  which is the value the release gate checks. The bottom strip shows the
  raw samples of the two runs that ratio divides, on one shared
  milliseconds (or MiB) axis, each run with its own median tick. The two
  runs are sampled independently, so dividing sample pairs would invent
  data; only the asserted ratio is plotted against the budget.
- **Single-value charts.** Some gates record exactly one number per
  report, like cold starts and editor memory. Those charts show only the
  top strip and say "single measurement per report".
- Hover or focus any table row for the exact result, the budget, the
  sample count, the median, and the p95.

Three results need extra care:

- The matched CLI lane gives ESLint, official Oxlint, and OXC for TSRX the
  same byte-identical 1,000-file TSX corpus, one rule, and identical run
  conditions. The separate mixed-file-types result only compares the product
  against itself on a different workload.
- The native cold-start ratios compare a direct Rust executable with the
  official npm launcher, so they are diagnostic, not tool-speed claims.
- The formatter's 16.6 MiB/s floor is a regression threshold derived from an
  older 1.66 MiB/s measurement on a different corpus, not a like-for-like
  Prettier comparison. The same-build canonical OXC controls are the
  comparable overhead measurements.

Noise is handled by a fail-closed rerun policy instead of picking a
favorable sample:

- Each invocation creates exactly one fresh report.
- A first report inside the 3% near-threshold band requires exactly two
  additional fresh reports. Only the assertions that triggered get
  two-of-three tolerance; everything else must pass in all three.
- Any failure more than 3% beyond its threshold is definitive.
- The representative report is selected by median normalized budget pressure
  with a stable report-path tie-break, never by picking the fastest run. The
  aggregate fails if the selected representative is red.

Make the authoritative release decision with:

```sh
node tests/acceptance/run-performance.mjs
```

The individual commands below produce diagnostic raw reports without the
aggregate admission, rerun, and selection policy:

```sh
cargo run --release --locked -p oxc_tsrx_benchmark -- \
  --assert benchmarks/native-lint/budgets.json
cargo run --release --locked -p oxc_tsrx_format_benchmark -- \
  --assert benchmarks/native-format/budgets.json
node benchmarks/vite/run.mjs
pnpm run benchmark:type-aware
pnpm run benchmark:editor
pnpm run benchmark:comparative
```

<!-- benchmarks:auto -->

## Measurement hygiene

- Harnesses are release-only.
- `memory-stats` is linked only into benchmark executables, never the
  distributed CLI.
- Normal source metadata excludes configuration time; aggregate metadata and
  dedicated configuration lanes report it separately.
- Formatter reports keep scan, projection, canonical parse, canonical format,
  and checked-lift timing arrays separately.
- Thresholds never change during adjudication.
