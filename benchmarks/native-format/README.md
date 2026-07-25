# Native formatter performance gate

Run the release-only gate after building the formatter binary:

```sh
npm run build:native
npm run benchmark:native-format
```

Use `npm run build:native`, not bare `cargo build`. The three old release
binaries are now one multi-call executable that selects its tool from `argv[0]`
or from a leading subcommand, and `budgets.json` names the candidate by file
path only, so it cannot pass a subcommand. `budgets.json` is frozen evidence:
`tests/acceptance/performance-contract.test.mjs` pins its SHA-256 and
`docs/acceptance/performance-report.json` embeds its exact bytes, so the
`candidateBinary` path is made valid instead of edited. It also cannot simply
point at `target/release/oxc-tsrx`, because with no tool in `argv[0]` and no
subcommand that binary runs the linter, which would silently measure the wrong
tool.

`scripts/build-native.mjs` therefore rebuilds a fresh copy at
`target/release/oxc-tsrx-fmt` after every build and checks that it reports
`oxc-tsrx-fmt --version` before it is used. It is a copy rather than a hardlink
because cargo replaces the binary with a new inode, so a link made once would
serve a stale build forever. The copy is a local build convenience only: the
published platform package still ships exactly one `bin/oxc-tsrx`.

Schema 2 records raw latency/RSS arrays, hardware/toolchain/OXC identity,
corpus hashes, sample policy, summaries, and every assertion in a timestamped
`results-*.json` report. It contains two in-process grammar lanes:

- the retained 1 MiB statement-control corpus preserves comparison with prior
  T015/T016 reports; and
- a 256 KiB generalized corpus repeats direct JSX-child, nested, expression,
  annotated `@for`/`@empty`, `for await`, `@switch`/`@case`/`@default`, and
  `@try`/`@pending`/`@catch` controls, plus nested dynamic tags and raw style
  payloads. Its half-size companion detects nonlinear work. The report records
  dynamic/style counts and requires one JS/TSX parse, zero hidden CSS parses,
  byte-preserving convergence, and honest embedded timing metadata.

P04 also measures canonical Oxfmt, the ordinary direct product path, and an
absolute 16.6 MiB/s floor derived from 10× a historical (non-comparable)
1.66 MiB/s Prettier result. That floor is a retained regression threshold, not
a like-for-like 10× speedup claim. P04 also measures complete default-thread
multi-file `--check` and
an isolated configured session that must load once, parse two files exactly
twice, and visibly apply its quote/semicolon options.
P05 compares fresh project TSRX stdin with official Oxfmt on equivalent TSX.
P07 compares complete-output RSS for TSRX and canonical TSX in the same Rust
binary.

The harness rejects weakened limits. Existing gates remain: ordinary median
and p95 overhead ≤1.05×/1.08×, sequential TSRX ≥15 MiB/s and ≥16.6 MiB/s
(10× the historical incumbent floor),
batch p95 throughput ≥100 MiB/s, stdin p95 ≤110 ms and ≤1.25× official Oxfmt,
and RSS ≤1.15× canonical TSX. The generalized control lane additionally
requires ≥15 MiB/s median, ≥12 MiB/s p95, ≤1.35× normalized full/half scaling,
one OXC parse, and idempotence.

Aggregate-selected representative report: `results-1784321655592.json`.

- 134.78 MiB/s median (127.00 MiB/s p95) retained sequential corpus;
- 823.10 MiB/s default-thread 16 MiB batch at p95;
- 21.79 MiB/s generalized median and 21.14 MiB/s p95 across 394 dynamic tags
  and 197 raw style payloads;
- 1.000× generalized normalized scaling;
- one timed config load for two files/two parses with applied options;
- 3.16 ms fresh stdin p95; and
- 1.143× complete-output RSS.

The report retains 30 raw samples for every sequential phase. Their p95 values
are 0.818 ms scan, 0.318 ms projection, 0.848 ms canonical parse, 4.944 ms
canonical format, and 1.085 ms checked lift. Because the RSS ratio lies inside
the policy's 3% near-threshold band around the 1.15× limit, it was adjudicated
by three explicit fresh passing runs: `results-1784321650912.json`,
`results-1784321655592.json`, and `results-1784321660260.json`. Their RSS
ratios were 1.143192×, 1.143192×, and 1.143295×. The aggregate selected
`results-1784321655592.json` by median normalized budget pressure with a stable
report-path tie-break; no threshold changed.

The stdin upstream ratio compares the direct Rust candidate executable with
the official Oxfmt npm launcher. It is a diagnostic guardrail rather than a
tool-speed claim; the absolute 110 ms p95 ceiling is the product gate.

`results-1784180050706.json` is the retained generalized red: the old repeated
search/string-shift lift reached only 0.324 MiB/s and scaled at 1.928×. Earlier
reports retain the original T015 quadratic token-lift and batch regressions.
