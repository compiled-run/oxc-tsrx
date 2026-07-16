# OXC for TSRX

## Objective

Build and prove the best practical OXC/Vite+ toolchain integration for TSRX: a maintained project named **OXC for TSRX** that gives `.tsrx` files production-usable formatting, linting, fixes, bundler integration, and editor-on-save/live-diagnostic behavior comparable to `.tsx`, preserves OXC's native-performance advantages, and documents any unavoidable boundary with stock OXC binaries precisely.

## Original Request

Implement the full Vite+ toolchain—including oxlint and oxfmt—for TSRX, test-first, and prove that a real project such as `/Users/jacksm5pro/dev/open-source/markless` can format TSRX on save and show TSX-like lint diagnostics.

## Intake Summary

- Input shape: `existing_plan`
- Audience: TSRX framework authors and application developers, beginning with Markless
- Authority: `requested`
- Proof type: `test`
- Completion proof: a fresh-checkout, receipt-backed acceptance run demonstrates build/install, deterministic formatting, lint diagnostics and safe fixes on original TSRX locations, Vite/Vite+ integration, editor format-on-save/live diagnostics against read-only representative Markless `.tsrx` sources, and frozen native/end-to-end/incremental performance budgets, with all owned tests green.
- Goal oracle: the correctness and performance acceptance matrices remain green after every implementation package, and a final clean-room walkthrough using a temporary copy of representative Markless sources records formatter edits and lint diagnostics without modifying Markless itself.
- Likely misfire: shipping a transpile-and-lint prototype or wrapper that passes synthetic unit tests while autofixes, formatting fidelity, editor activation, Vite+ commands, source spans, configuration compatibility, performance, or real Markless syntax remain broken.
- Blind spots considered: the workspace begins empty and is not a Git worktree; exact TSRX/OXC/Vite+ source locations and versions are unresolved; stock oxlint/oxfmt parser limitations must be distinguished from compatibility delivered by project-owned adapters; OXC currently lacks public custom-language hooks for the complete requested surface; source mapping and fix safety are harder than diagnostics; third-party Vite plugin ordering and HMR must be exercised; native parsing speed can be erased by redundant parsing, allocation, AST conversion, serialization, process startup, or JS/NAPI boundary costs; benchmark comparisons must separate raw native work from complete AST materialization and end-to-end tool latency; licensing, naming, release packaging, platform binaries, and upstream coordination need validation.
- Existing plan facts: the project is called `OXC for TSRX`; TSRX is a whole-file TSX extension with interleaved syntax; transformation to legal TSX is an available architectural boundary; the desired surface includes Vite/Vite+, Vite plugins, oxlint-compatible rules/config/plugins, oxfmt-like formatting, type-aware lint where practical, and editor behavior; implementation must use full TDD and make success visibly demonstrable; `/Users/jacksm5pro/dev/open-source/markless` is a read-only real-world acceptance target; the result must stay in OXC's performance class and treat Yuku's data-oriented TSRX parser/code generator and reproducible benchmarks as concrete candidates and evidence for avoiding accidental allocation, cache-locality, AST-transfer, or serialization regressions; OXC for TSRX must never vendor, fork, or maintain a source patch queue for OXC.

## Goal Oracle

The oracle for this goal is:

`From a clean checkout, the documented install/build path and automated correctness and performance matrices pass; an editor integration formats a temporary copy of a real representative Markless .tsrx file on save and displays deliberately seeded lint diagnostics at correct TSRX source locations without changing Markless; CLI formatter output is idempotent and semantics-preserving; CLI lint config, diagnostics, and safe fixes behave comparably to TSX; Vite/Vite+ build and plugin-chain tests pass; native parsing/lowering, end-to-end CLI or NAPI, memory, and incremental-editor measurements remain within frozen evidence-backed budgets; and the final Judge audit maps each claim to a command, artifact, benchmark, or captured walkthrough.`

The PM must keep comparing task receipts to this oracle. Planning, discovery, a passing tiny slice, or a clean-looking board is not enough. The goal finishes only when a final Judge/PM audit maps receipts and verification back to this oracle and records `full_outcome_complete: true`.

## Goal Kind

`existing_plan`

## Current Tranche

Continuously discover, design, implement, package, and verify successive safe vertical slices until the complete OXC for TSRX developer workflow is working. The current tranche corrects both rejected extremes: no imported OXC source fork and no unrelated Zig/Yuku production toolchain. Build a separate Rust-native TSRX core against one coherent OXC workspace graph from one exact canonical Git revision behind a narrow compatibility adapter. Required `publish = false` engines resolve allocator, AST, span, syntax, parser, and semantic siblings from that workspace; substituting crates.io copies would create duplicate Cargo package identities and incompatible nominal Rust types. All twelve direct adapter dependencies therefore use the same canonical Git URL and full `rev`; a future crates.io-only graph is permitted only when the complete required engine closure resolves coherently from that single source. Keep Yuku only as a performance oracle, expose only thin JavaScript/TypeScript shells for npm, Vite/Vite+, and editor APIs, and advance through coherent Worker packages without stopping at a parser proof, CLI prototype, benchmark-only prototype, or plan.

## Non-Negotiable Constraints

- Use test-driven development: add or identify a failing behavioral test before each implementation package, then make it pass and retain it as regression proof.
- Optimize for the best practical user experience now, while describing stock-binary versus OXC-compatible/project-owned behavior honestly.
- Do not claim oxlint, oxfmt, Vite+, editor, plugin, fix, or configuration compatibility without an observable acceptance test for that surface.
- Preserve TSRX source locations and semantics. Never apply an autofix when span mapping or edit safety is uncertain.
- Formatter output must parse, preserve behavior, converge after one pass, and handle real TSRX constructs—not just toy fixtures.
- Do not write to Markless or any repository outside `/Users/jacksm5pro/dev/open-source/oxc-tsrx`. Markless may only be read, benchmarked without mutation, or copied into this repository or a disposable temporary workspace for acceptance tests. Any external write requires a new, exact approval from the user first.
- Do not edit upstream OXC, Vite+, TSRX, Yuku, or Markless repositories. Inspect them read-only and implement only project-owned adapters inside this repository unless the user separately authorizes a precisely scoped external change. Vendoring, patching, copying, or forking OXC/Oxlint/Oxfmt/Vite+ remains forbidden.
- Never vendor, fork, copy, or maintain a downstream patch queue for OXC, Oxlint, Oxfmt, or Vite+. OXC Rust crates are allowed only behind a project-owned compatibility adapter. The current graph must keep every direct OXC dependency and its canonical workspace closure on one exact full-`rev` Git source so Cargo exposes one allocator/AST/span/syntax type identity. A future crates.io-only graph is allowed only if every required engine and sibling resolves coherently from crates.io. Source snapshots, local/file-relative dependencies, mixed OXC source identities, Cargo patches, forks, copied generated layouts, and undocumented private modules are forbidden.
- The production language core is Rust and must use OXC-compatible arena/indexed data structures and canonical OXC crates where they provide the required parser, semantic, linter, formatter, or diagnostic capability. Yuku is a benchmark/design oracle, not a production parser, formatter, build dependency, or distributed artifact.
- JavaScript/TypeScript is limited to ecosystem boundaries such as npm loading, Vite/Vite+ hooks, configuration discovery, and editor clients. Parsing, semantic traversal, lint rule execution, formatting layout, fix safety, and source mapping must not materialize a whole language AST in JavaScript.
- Pin the OXC crate set compiled into every native release so a newly published upstream release cannot break an installed artifact. Deliberate OXC upgrades must be isolated to the compatibility adapter and pass minimum/current/next compile, behavior, and performance lanes before release.
- Pin or record the tested dependency revisions, but prove a supported-version matrix and capability-based fallbacks so installing a newer official Oxlint/Oxfmt release does not silently corrupt or disable TSRX behavior.
- Include installable packaging and editor activation/configuration in the product, not only library APIs.
- Keep ordinary `.js`, `.jsx`, `.ts`, and `.tsx` behavior delegated to or compatible with the upstream toolchain and cover regression boundaries.
- Keep the hot path native and data-oriented. Avoid full AST JSON serialization, duplicate whole-file parses, per-node heap allocation, unnecessary source/string copies, and eager JS object materialization. One contiguous native legal-TSX projection buffer is permitted only for `.tsrx`, because the public OXC parser accepts a contiguous `&str`, and only while its separately reported scan/allocation cost remains inside P02/P03/P04/P07; ordinary JS/TS paths must bypass it. Any other boundary exception requires same-machine evidence and a stronger review.
- Measure before and after every performance-sensitive slice. Separate raw native parser/lower timings from semantic/lint/format passes, CLI startup, JS/NAPI or IPC transfer, peak memory/allocation behavior, and editor incremental latency so one fast microbenchmark cannot hide a slow product path.
- Preserve upstream OXC performance on ordinary JS/TS/JSX/TSX paths within a pre-implementation, evidence-backed regression budget. TSRX-specific overhead must be measured against equivalent emitted TSX and justified by work unique to TSRX.
- Treat Yuku as a performance-design and benchmark-methodology reference, not an automatic implementation dependency or an apples-to-oranges score. Reproduce comparable baselines on the same machine, revisions, inputs, outputs, warmup policy, and materialization boundary before drawing conclusions.
- Do not silently broaden into publishing packages, pushing branches, opening PRs, or changing external repositories; those actions require their own authority.

## Performance Contract

Performance is part of correctness for this goal.

Before the first non-fork implementation Worker is activated, PM/Scout must retain the reproducible baselines and freeze revised numeric pass/fail budgets for the actual adapter architecture. At minimum the benchmark suite must cover:

- raw native parse and TSRX parse/lower throughput, median and tail latency;
- complete semantic/lint and format operations, including diagnostic/fix or formatted-output production;
- warm end-to-end CLI and any JS/NAPI or IPC boundary, plus cold startup separately;
- peak resident memory and practical allocation/copy indicators on small, medium, large, and stress files;
- incremental editor latency for initial open, ordinary edits, diagnostics, and format-on-save;
- representative TSRX fixtures and a read-only Markless corpus, with equivalent emitted TSX where comparison is meaningful;
- upstream OXC on ordinary JS/TS/JSX/TSX, the incumbent TSRX/ESLint/Prettier path where available, and Yuku or Acorn only on genuinely comparable syntax/output boundaries.

The suite must retain raw results, pinned revisions, build flags, hardware/OS metadata, warmup/sample policy, and benchmark source. A performance regression blocks completion unless Judge records why the comparison is invalid and replaces it with a stronger measurement.

## Stop Rule

Stop only when a final audit proves the full original outcome is complete.

Do not stop after planning, discovery, or Judge selection if the user asked for working software or automation and a safe Worker task can be activated.

Do not stop after a single verified Worker package when the broader owner outcome still has safe local follow-up work. Advance the board to the next highest-leverage safe Worker package and continue unless a phase, risk, rejected-verification, ambiguity, or final-completion review is due.

Do not create one Worker/Judge pair per repeated fixture, rule, command, or helper. Put repeated same-shape work into one Worker package and review the package as a whole.

## Slice Sizing

Safe means bounded, explicit, verified, and reversible. It does not mean tiny. A good task is the largest safe useful vertical slice, such as one end-to-end formatter path or lint path with CLI, config, mapping, tests, packaging, and representative fixtures together.

Tiny tasks are allowed when the failure is isolated, the risk is high, the source surface is unknown, or the task unlocks a larger slice. Reorient after two tiny tasks that do not change user-observable behavior.

If a slice needs owner input, credentials, production access, destructive operations, external-repository writes, or a policy decision, mark that exact slice blocked with a receipt and continue all local, non-destructive work that still advances the oracle.

## Board Health

The PM owns board health. If the board looks stale, misleading, offline, or inconsistent, run:

```bash
node /Users/jacksm5pro/.codex/plugins/cache/goalbuddy/goalbuddy/0.4.0/skills/goal-prep/scripts/check-goal-state.mjs docs/goals/oxc-for-tsrx
```

If the local board is running, compare `state.yaml` to the live board API. Repair only GoalBuddy control files unless an active Worker or PM task explicitly allows product-file edits.

## Canonical Board

Machine truth lives at:

`docs/goals/oxc-for-tsrx/state.yaml`

If this charter and `state.yaml` disagree, `state.yaml` wins for task status, active task, receipts, verification freshness, and completion truth.

## Run Command

```text
/goal Follow docs/goals/oxc-for-tsrx/goal.md.
```

## PM Loop

On every `/goal` continuation:

1. Read this charter and the GoalBuddy execution contract.
2. Read `state.yaml`.
3. Re-check the intake, original request, proof requirements, plan facts, risks, and likely misfire.
4. Work only on the active task, using its assigned Scout, Judge, Worker, or PM role.
5. Write a compact receipt and update the board.
6. Keep the acceptance matrix live; add a failing behavioral test before implementation.
7. Choose the next largest safe reversible vertical slice and continue while safe local work remains.
8. Review only at phase, risk, rejected-verification, ambiguity, or final-completion boundaries.
9. Finish only with a final audit receipt that maps every oracle clause to fresh evidence and records `full_outcome_complete: true`.
