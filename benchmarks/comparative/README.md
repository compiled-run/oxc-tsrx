# Matched cross-tool CLI benchmark

Run the release-only comparison after building the native lint binary:

```sh
cargo build --release --locked -p oxc_tsrx_cli --bin oxc-tsrx
npm run benchmark:comparative
```

The cross-tool lanes give ESLint plus typescript-eslint, official Oxlint, and
OXC for TSRX the same byte-identical 1,000-file TSX corpus, the same explicit
file list, and the same single `no-debugger` rule. Every tool must validate
1,000 files with zero diagnostics and default output before timings are
accepted. The harness records five warmups and twenty measured fresh processes
per lane, using median and nearest-rank p95 latency.

A fourth lane replaces 20% of those files with paired TSRX generated from the
same component specifications. That lane is an internal OXC for TSRX workload
ratio only. It is not compared with ESLint or official Oxlint because those
tools do not parse TSRX.

Latest passing Apple M5 Pro report: `results-1784242094588.json`.

| Lane | Median | p95 |
| --- | ---: | ---: |
| ESLint + typescript-eslint, matched TSX | 660.17 ms | 737.67 ms |
| Official Oxlint, matched TSX | 40.87 ms | 43.46 ms |
| OXC for TSRX, matched TSX | 24.99 ms | 25.58 ms |
| OXC for TSRX, paired workload with 20% TSRX | 26.28 ms | 27.31 ms |

On this retained host and corpus, OXC for TSRX's matched-TSX median was 0.611×
official Oxlint's median, ESLint's median was 26.42× OXC for TSRX's, and the
mixed OXC for TSRX workload was 1.052× its all-TSX lane. These are bounded
fresh-process results for the recorded versions and fixture, not a universal
tool-speed ranking or a claim about unrelated rules and projects.

The report records tool versions, native build and OXC revision, host identity,
corpus/config hashes, validation counts, raw warmup and measured arrays, and
every frozen assertion. The performance acceptance runner verifies the
sampling policy, comparison boundary, selected report identity, and unchanged
budget file before release.
