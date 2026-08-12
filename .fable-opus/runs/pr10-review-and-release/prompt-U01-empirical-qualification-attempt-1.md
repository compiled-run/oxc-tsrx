Fable-Opus-Unit: pr10-review-and-release/U01-empirical-qualification
Fable-Opus-Timeout-Minutes: 45

## Goal

Empirically qualify PR #10 of oxc-tsrx (branch `codex/octane-integration-compat`, ALREADY checked out in this repo — do not switch branches) and write a findings report. Four questions, each answered with exact commands and observed output:

1. **Do the PR's own validation claims hold?** Run:
   - `cargo test --release --locked --offline -p tsrx_syntax -p oxc_adapter -p tsrx_parser_engine`
   - `cargo clippy --release --locked --offline -p tsrx_syntax -p oxc_adapter -p tsrx_parser_engine --all-targets -- -D warnings`
   - `pnpm test:parser-api` (check package.json for the exact script name; build prerequisites first if the script needs them, e.g. `node scripts/build-parser-native.ts`)
   - `OXC_TSRX_PARSER_ADDON=packages/toolchain/parser.node pnpm test:parser-addon`
   If `--offline` fails because the registry cache is cold, retry without `--offline` and note it.

2. **Does this branch fix issue #8 defect 1 (no ASI before a `<` line in an `@{}` body)?** Using the LOCALLY BUILT parser (never the published npm package), parse this fixture and report the `errors` array:
   ```tsx
   function Counter() @{
   	const count = get()

   	<button>
   		{'Count: ' + count}
   	</button>
   }
   ```
   On v0.2.3 this errors with "unterminated regular expression literal starting at byte 78". Also test the variant with a semicolon after `get()` (should be 0 errors both before and after).

3. **Does this branch fix issue #8 defect 2 (Oxfmt structural refusal on trailing `@if`)?** Using the locally built fmt path (find the local binary/entry point — the published bins are `oxc-tsrx-fmt`; check packages/ for how to invoke the local equivalent), run a format --check of:
   ```tsx
   function D() @{
   	const d = get()
   	@if (d) {
   		<main>a</main>
   	}
   }
   ```
   Report whether "formatted TSRX structure differs from the input" still occurs at default options and with `{"semi":false}` config.

4. **Ripple-TS/ripple#1417 equivalence:** upstream Ripple now treats `<` in markup text as literal text when the next character cannot start a tag. Parse these two snippets (inside a valid component body) with the locally built parser and report exact diagnostics or success:
   - `<span><3</span>`
   - `<span>a < b</span>`
   State plainly: does oxc-tsrx on this branch accept them, and if not, what error.

Write the report to `docs/goals/pr10-review-and-release/notes/U01-empirical.md` with: exact commands run, trimmed real output (never paraphrase a passing/failing line), and a PASS/FAIL/NOT-COVERED verdict per question. Create any repro `.tsrx` fixtures in a temp directory outside the repo (e.g. mktemp -d), not in the working tree.

## File contract

- `docs/goals/pr10-review-and-release/notes/U01-empirical.md`

## Forbidden moves

- Do not modify any tracked file: no source, test, config, or git-state changes; no commit, push, branch switch, stash, or checkout. Why: this is a measurement unit and the PR branch must remain byte-identical to what was pushed.
- Builds may write only gitignored artifacts (`target/`, `packages/toolchain/parser.node`, `node_modules`, dist outputs). Do not put repro fixtures inside the repo. Why: keeps the retrospective diff clean.
- Do not run the full Octane corpus replay or clone the Octane repo. Why: too heavy for this unit; the four questions above are the scope.

## Verification

```verify
test -s docs/goals/pr10-review-and-release/notes/U01-empirical.md
```

## Blocked permission

If evidence is missing, the contract conflicts with reality, or you need a file outside the contract, return status "blocked" with the question in open_questions instead of improvising.