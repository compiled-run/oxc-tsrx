import { computed, state } from '@markless/core';

// D3 arm-relative records repro (the dead-events-inside-arms bug class):
// a plain host with an event directly inside @try, plus the need-15 shape —
// hosts inside an arm-scoped @if arm. This arm shape (fragment + @if) has no
// boundary update symbol yet (arm re-render with records lands in T102), so
// the fixture proves INITIAL-LOAD record registration, not re-settle.
export function App() @{
	let saved = state('none');
	let unlocked = state(true);
	const report = computed(async ({ signal }) => {
		await new Promise((resolve) => setTimeout(resolve, 10));
		if (signal.aborted) return { title: 'aborted' };
		return { title: 'Q3 report' };
	});

	<main>
		<output data-saved>{saved}</output>
		@try {
			<>
				<p class="done">{report.title}</p>
				<button type="button" data-save onClick={() => saved = 'saved:' + report.title}>Save</button>
				@if (unlocked) {
					<button type="button" data-share onClick={() => saved = 'shared:' + report.title}>Share</button>
				} @else {
					<button type="button" data-locked onClick={() => saved = 'locked'}>Locked</button>
				}
			</>
		} @pending {
			<p class="pending">Loading</p>
		} @catch {
			<p class="broken">Broken</p>
		}
	</main>
}
