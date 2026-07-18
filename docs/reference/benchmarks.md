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
- **Budget** is the frozen threshold, drawn as the dashed vertical line in
  every chart.
- **Dot strips.** Most chart rows draw one dot per retained sample from the
  selected report, placed by how much of the row's budget that sample uses.
  The tall solid tick marks the median sample. The hollow diamond marks the
  p95 sample. A row passes while its gated value stays on the near side of
  the dashed budget line. Where dots pile up, the strip gets darker, so you
  see the whole spread instead of one summary number.
- **Ratio rows.** The solid marker on the budget scale is the asserted
  ratio, which is the value the release gate checks. Below it, two thin
  labeled strips show the raw samples for the numerator and the denominator
  on one shared scale. The two runs are sampled independently, so dividing
  sample pairs would invent data; only the asserted ratio is plotted on the
  budget scale.
- **Single-value rows.** Some gates record exactly one number per report,
  like cold starts and editor memory. Those rows keep a plain filled bar,
  and their tooltip says "single measurement per report".
- Hover or focus any row for the exact result, the budget, the sample
  count, the median, and the p95.

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
- Formatter reports keep scan, projection, canonical parse, canonical format,
  and checked-lift timing arrays separately.
- Thresholds never change during adjudication.
