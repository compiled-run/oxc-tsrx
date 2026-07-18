# Vite and Vite+ boundary benchmark

Run from the repository root after the release native binaries exist:

```sh
node benchmarks/vite/run.mjs
```

The harness compares the project-owned mixed `.tsx`/`.tsrx` command packages
with canonical Oxlint/Oxfmt over two equivalent ordinary TSX files. A separate
matched Oxfmt lane runs `oxfmt-tsrx` over those same two ordinary files, proving
the exact manifest-declared Oxfmt launcher is imported in the same Node process,
with zero TSRX dispatch, without conflating it with mixed formatting. It also
measures a complete current-when-frozen Vite+ mixed lint command, retains every
raw sample, and asserts that the TSRX lane records one native parse while
ordinary files remain in the canonical upstream process. Each lane uses five
warmups and 20 measured fresh processes; the report retains host/build/OXC and
corpus identity.

These process-boundary budgets supplement, rather than replace, the much tighter
native hot-path budgets in `benchmarks/native-lint` and
`benchmarks/native-format`. Vite runtime build/HMR compilation remains entirely
framework-owned and is exercised by `tests/vite/framework-chain.test.mjs`; OXC
for TSRX does not add a transform or parser to that path.

Aggregate-selected representative report:
`results-1784321678410.json`. The ordinary `oxfmt-tsrx` median is 103.26 ms
versus canonical Oxfmt's 100.99 ms on the identical files; p95 is 113.44 ms
versus 103.01 ms (1.101×). Exact normalized stdout/stderr and exit status
match, and trace evidence records zero TSRX dispatch events. The mixed
companion p95 is 57.91 ms for lint (1.813× canonical two-file TSX) and 127.11
ms for format-check (1.234× canonical). A complete Vite+ 0.2.4 mixed lint is
237.08 ms p95. The
native metadata records exactly one TSRX parse and zero ordinary files in the
project-owned lane.
