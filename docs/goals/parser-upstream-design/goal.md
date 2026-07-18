# Upstream-grade TSRX parser API design

## Objective

Produce an adversarially-reviewed design document for a JS-callable TSRX parser API in
oxc-tsrx — one that OXC maintainers would recognize as upstream-grade (following OXC's own
conventions for napi parser bindings), that preserves oxc-tsrx's non-fork philosophy, and
that meets the markless `@tsrx/core` replacement bar as its first concrete consumer.
Design only: no Rust implementation in this tranche.

## Original Request

"You're going to look into the oxc-tsrx repo, and then craft up the parser design that
ensures it's something the oxc maintainers would want upstream, while also keeping close
to the philosophy of the oxc-tsrx project. This goalbuddy board should be created inside
of the oxc-tsrx project, in a new git worktree."

## Intake Summary

- Input shape: `specific`
- Audience: owner (Jack Shelton); secondarily oxc-tsrx contributors, potential OXC upstream reviewers, and markless as consumer #1
- Authority: `requested` (design work on a worktree branch; no pushes, PRs, or upstream contact without an explicit owner directive)
- Proof type: `artifact` + `review`
- Completion proof: design doc committed on `design/upstream-parser`, surviving an adversarial critique against the T003 upstream-fit rubric, with a final audit recording `full_outcome_complete: true`
- Goal oracle: see below
- Likely misfire: a markless-convenience API spec that ignores OXC conventions, or an upstream-pleasing design that fails markless's replacement bar, or drifting into writing Rust instead of design
- Blind spots considered: "upstream" is ambiguous (contributing pieces into oxc-project/oxc vs. OXC-convention-aligned design living in oxc-tsrx) — the design must research OXC's real conventions (oxc-parser npm package, napi raw-transfer/ESTree serialization, RFC norms) and carry an explicit upstream-fit section covering both readings; the CSS-parse boundary and loose/recovering-parse needs conflict with recorded oxc-tsrx compliance decisions and require explicit design rulings, not silent reversal; UTF-8 byte spans vs markless's UTF-16 offsets; 8-target native packaging as a compiler dependency
- Existing plan facts (2026-07-17 evaluation, markless repo `goals/oxc-tsrx-replacement/notes/verdict.md`, gitignored-but-present): oxc-tsrx exposes no JS-callable parse API today; shortest credible path is napi-rs over `tsrx_syntax` + the pinned OXC parse with a `@tsrx/core`-compatible AST serializer (TSRX node reconstruction from an expanded public overlay), UTF-16 offset translation, CSS tree for `<style>`, loose/collect mode, `isEventAttribute`/`normalizeEventName`, `SyntaxError`-shaped strict failures, type subpaths, 8-target packaging. markless's full consumption contract (3 runtime symbols, ~60 AST discriminants, offset/error semantics) is in the markless-side scout receipt referenced by that note. Validate, don't rediscover.

## Goal Oracle

The oracle for this goal is:

`A design document on the design/upstream-parser branch whose adversarial critique receipt maps every criterion of the T003 upstream-fit rubric — OXC convention alignment, oxc-tsrx philosophy preservation, markless replacement-bar coverage, and explicit rulings on the CSS/loose-mode policy conflicts — to concrete design sections, with no unresolved rejection.`

The PM must keep comparing task receipts to this oracle. Planning, discovery, a passing
tiny slice, or a clean-looking board is not enough. The goal finishes only when a final
Judge/PM audit maps receipts back to this oracle and records `full_outcome_complete: true`.

## Goal Kind

`specific`

## Current Tranche

Design tranche: gather parser-seam evidence from oxc-tsrx internals and OXC upstream
conventions, consolidate design tenets and an upstream-fit rubric, draft the design
document, adversarially critique it, revise, and audit. Rust implementation, markless
migration, and any upstream proposal are explicitly later tranches.

## Non-Negotiable Constraints

- Design only: no Rust implementation, no markless edits, no npm publishing, no contact with OXC maintainers.
- oxc-tsrx philosophy is preserved: no OXC fork, source snapshot, Cargo patch, or vendor tree; one pinned canonical OXC revision; all revision-specific OXC calls isolated in `crates/oxc_adapter`; fail-closed behavior for unimplemented grammar; recorded compliance decisions (e.g. the raw-CSS boundary) may only be changed by an explicit, argued design ruling — never silently.
- The design must state, for each major choice, whether it follows an existing OXC convention (cite where) or deliberately diverges (say why).
- markless is consumer #1: the design must map to the replacement bar in the existing-plan facts, or explicitly declare which parts of the bar move into a markless-side adapter.
- This is a fable-codex session: artifact-writing and scout/critique execution go through `crew run` on gpt-5.6-sol per the standing order; the PM adjudicates receipts and owns this board. Crew packets for write tasks must declare their Workflow guidance per that repo's rules only when working inside markless; oxc-tsrx write packets state the file contract directly.
- All work stays on the `design/upstream-parser` worktree branch (`~/dev/open-source/oxc-tsrx-parser-design`); commits are fine, pushes require an explicit owner directive.
- `docs/goals/` paths may be gitignored; reference them by absolute path in packets and read with cat, not ignore-aware tools.

## Stop Rule

Stop only when a final audit proves the full original outcome is complete.

Do not stop after planning, discovery, or Judge selection when a safe Worker task exists.
Do not stop after a single verified Worker package while the design remains undrafted,
uncritiqued, or unrevised. Do not create one Worker/Judge pair per document section; the
draft is one package, the revision is one package.

## Slice Sizing

Safe means bounded, explicit, verified, and reversible. It does not mean tiny.
The draft (T004) is one whole-document slice; the revision (T006) is one whole-document
slice. Scouts return one evidence receipt each, not serial micro-findings.

## Canonical Board

Machine truth lives at:

`docs/goals/parser-upstream-design/state.yaml` (in the `oxc-tsrx-parser-design` worktree)

If this charter and `state.yaml` disagree, `state.yaml` wins for task status, active task,
receipts, verification freshness, and completion truth.

## Run Command

```text
/goal Follow docs/goals/parser-upstream-design/goal.md.
```

(Run with the worktree `/Users/jacksm5pro/dev/open-source/oxc-tsrx-parser-design` as the
working directory, or reference the absolute path.)

## PM Loop

On every `/goal` continuation:

1. Read this charter.
2. Read `state.yaml`.
3. Run the bundled GoalBuddy update checker when available and mention a newer version without blocking.
4. Re-check the intake: original request, input shape, authority, proof, blind spots, existing plan facts, and likely misfire.
5. Work only on the active board task.
6. Assign Scout, Judge, Worker, or PM according to the task; dispatch execution through crew per the fable-session constraint.
7. Write a compact task receipt.
8. Update the board.
9. If safe local work remains, choose the next largest reversible work package and continue unless blocked.
10. Review at phase, risk, rejected-verification, ambiguity, or final-completion boundaries; do not review every small Worker by habit.
11. Finish only with a Judge/PM audit receipt that maps receipts back to the original user outcome and records `full_outcome_complete: true`.
