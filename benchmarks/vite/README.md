# Vite and Vite+ boundary benchmark

Run from the repository root after the release native binaries exist:

```sh
node benchmarks/vite/run.mjs
```

The harness compares the project-owned mixed `.tsx`/`.tsrx` command packages
with canonical Oxlint/Oxfmt over two equivalent ordinary TSX files. It also
measures a complete current-when-frozen Vite+ mixed lint command, retains every
raw sample, and asserts that the TSRX lane records one native parse while
ordinary files remain in the canonical upstream process. Each lane uses five
warmups and 15 measured fresh processes; the report retains host/build/OXC and
corpus identity.

These process-boundary budgets supplement, rather than replace, the much tighter
native hot-path budgets in `benchmarks/native-lint` and
`benchmarks/native-format`. Vite runtime build/HMR compilation remains entirely
framework-owned and is exercised by `tests/vite/framework-chain.test.mjs`; OXC
for TSRX does not add a transform or parser to that path.

Latest passing Apple M5 Pro report:
`results-1784242073158.json`. The mixed companion p95 is 61.18 ms for lint
(1.868× canonical two-file TSX) and 137.89 ms for format-check (1.331×
canonical). A complete Vite+ 0.2.4 mixed lint is 322.65 ms p95. The
native metadata records exactly one TSRX parse and zero ordinary files in the
project-owned lane.
