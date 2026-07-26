# Clean-room owner oracle

This directory contains the final local release evidence. It never publishes an
artifact and never writes to Markless.

Run the complete correctness and ecosystem matrix:

```sh
MARKLESS_ROOT=/Users/jacksm5pro/dev/open-source/markless \
  node tests/acceptance/run.mjs
```

The runner copies the current source into a disposable directory while
excluding `node_modules`, `target`, and generated site output. It verifies the
copy hash, runs a fresh lifecycle-script-free `pnpm install --frozen-lockfile`,
builds and tests Rust
from fresh target directories, produces release binaries, installs untouched
npm tarballs in empty consumers, exercises Vite/Vite+, and installs a real
target VSIX into an isolated VS Code profile. Markless's committed corpus and a
temporary editor copy are used read-only; a fingerprint covering HEAD, tracked
diffs, staged diffs, status, and untracked bytes must be identical before and
after. The result is `clean-room-report.json`.

Run every frozen same-machine performance lane separately:

```sh
node tests/acceptance/run-performance.mjs
```

This writes new retained reports under `benchmarks/*/results-*.json` and a
single index at `performance-report.json`. Each invocation must create exactly
one fresh report. If the first report places a numeric assertion within 3% of
its unchanged threshold, the aggregate runs exactly two more identity-coherent
reports. Only assertions that triggered on the first report receive two-of-
three tolerance. Any invariant failure, non-triggering assertion failure, or
failure more than 3% beyond its threshold fails the aggregate. Reports are
ordered by normalized budget pressure and the median is selected with a stable
report-path tie-break; the aggregate also fails if that selected raw report is
red. GitHub-hosted runner timing is not a replacement for this stable-machine
proof.

The two reports are evidence, not publication authority. Repository push,
hosted multi-platform candidate production, npm/Marketplace publication, Pages
deployment, and social posting each remain separately approval-gated.

See [`matrix.md`](matrix.md) for the clause-by-clause human-readable index of
the latest successful correctness, ecosystem, editor, external-safety, and
performance evidence.
