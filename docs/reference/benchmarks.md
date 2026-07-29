---
title: Benchmarks
description: The frozen release performance gates, generated from aggregate-selected reports.
---

# Benchmarks

Performance is a release gate here, not a marketing page. Every number below is
generated at build time from the reports named in
`docs/acceptance/performance-report.json`, so this page cannot quietly pick up a
newer failed or incomplete run.

**MiB/s** is throughput, and higher is better. **p95** is the nearest-rank 95th
percentile of the retained samples, so it shows tail behavior rather than the
average. Hover or focus any table row for its budget, sample count, median, and
p95.

<!-- benchmarks:auto -->

## How a number gets on this page

Noise is handled by a fail-closed rerun policy rather than by picking a
favorable sample. A first report inside the 3% near-threshold band requires
exactly two additional fresh reports, and only the assertions that triggered get
two-of-three tolerance; everything else has to pass in all three. The
representative is chosen by median normalized budget pressure with a stable
report-path tie-break, never by taking the fastest run, and the aggregate fails
if the selected representative is red or if any failure lands more than 3%
beyond its threshold.

The harnesses are release-only, and `memory-stats` is linked into benchmark
executables only, never into the CLI you install. Make the authoritative release
decision with:

```sh
node tests/acceptance/run-performance.mjs
```
