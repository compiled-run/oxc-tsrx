Fable-Opus-Unit: pr10-review-and-release/U02-asi-before-jsx-fix
Fable-Opus-Timeout-Minutes: 45
Fable-Opus-Effort: high
Effort-Justification: Scanner state-machine change with reference-parser parity constraints; a wrong ASI rule silently changes parse semantics for comparison expressions.

## Goal

On the currently checked-out branch `codex/octane-integration-compat` (PR #10 — do not switch branches), fix issue #8's root cause with strict TDD: the TSRX scanner applies no ASI before a line that starts with `<` inside an `@{}` body, so a semicolon-less statement followed by a markup line fails with `unterminated regular expression literal` (parse) and `formatted TSRX structure differs from the input` (Oxfmt round-trip — same root cause, verified empirically; see docs/goals/pr10-review-and-release/notes/U01-empirical.md for the full evidence, exact repros, and byte offsets).

Reference behavior to match (this is the ground truth, not standard JS ASI): Octane's own compiler treats these two programs as byte-identical output — after a semicolon-less statement, a NEWLINE followed by a `<` that begins a committed JSX/markup opening starts a new statement:

```tsx
function Counter() @{
	const count = get()

	<button>
		{'Count: ' + count}
	</button>
}
```

must parse with zero errors, exactly like the variant with `;` after `get()`.

TDD protocol, in this order, with evidence retained:
1. Write failing tests FIRST and run them to observe red. Minimum coverage:
   - parse test: the fixture above yields zero errors (currently: unterminated regex at byte 78);
   - parse test: `const d = get()` newline `@if (d) { <main>a</main> }` inside `@{}` parses clean (issue #8 defect 2's shape);
   - guard test: a genuine comparison must NOT be broken — e.g. `const x = a\n< b` (space after `<`, not a JSX start) still parses as a comparison, and same-line `a < b` unchanged;
   - guard test: a line-starting `<` that is a TypeScript generic/type-parameter form (the PR's `looks_like_typescript_type_parameters` cases) is not swallowed by the new ASI rule.
   Place tests where the existing suites put equivalent coverage (crates/tsrx_parser_engine/tests/program_composition.rs has the PR's own new tests; crates/tsrx_syntax may have unit tests — follow local convention).
2. Implement the MINIMAL fix. Both scanner variants must change in lockstep (crates/tsrx_syntax/src/scanner/ and crates/tsrx_syntax/src/parser_scanner/ mirror each other in this codebase — the PR itself changed both identically; asymmetry between them is a bug). The likely shape: at the `<` decision site, when not in expression-start state but the `<` is at the start of a line (only whitespace since the last line terminator) and `looks_like_jsx_start` holds and it is not a TypeScript type-parameter form, treat it as a JSX start (statement boundary). Keep it one predicate used by both variants if feasible.
3. Run the tests green, then the full verify commands.
4. Do not fix the formatter separately — the round-trip refusal should disappear with the scanner fix. If it demonstrably does not, report that in the receipt rather than expanding scope.

## File contract

- `crates/tsrx_syntax/**`
- `crates/tsrx_parser_engine/tests/**`

## Forbidden moves

- No git commit, push, stash, branch switch, or checkout. Why: the cockpit reviews the dirty diff and owns git state.
- Do not touch crates/oxc_adapter, the reconstruct passes' source, scripts/, or any JS/TS package files. Why: the fix is scanner-level by evidence; scope must stay reviewable.
- Do not weaken or delete any existing test to get green. Why: existing behavior is the parity baseline.

## Verification

```verify
cargo test --release --locked --offline -p tsrx_syntax -p oxc_adapter -p tsrx_parser_engine
cargo clippy --release --locked --offline -p tsrx_syntax -p oxc_adapter -p tsrx_parser_engine --all-targets -- -D warnings
```

## Blocked permission

If evidence is missing, the contract conflicts with reality (e.g. the fix genuinely requires a file outside the contract), or the reference semantics are ambiguous for a guard case, return status "blocked" with the question in open_questions instead of improvising.