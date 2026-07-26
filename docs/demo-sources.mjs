// The canonical TSRX snippets the demo surfaces use. They live here, in one
// module, because the clickable examples derive variants from them by string
// replacement: when a snippet and its anchor drift apart the derived variant
// silently becomes a no-op, which is exactly how the hero's "Type error"
// example went dead. The assertions below turn that drift into a build error.

// Real TSRX hero snippet, highlighted with the actual TSRX grammar. This is
// oxc-tsrx-fmt's converged output, so the default demo state is format-clean.
export const heroCode = `export function TaskList({ tasks }: Props) @{
  const pending = tasks.filter((task) => !task.done);

  <section class="tasks">
    @if (pending.length > 0) {
      @for (const task of pending; key task.id) {
        <TaskRow task={task} />;
      } @empty {
        <AllDone />;
      }
    } @else {
      <SignIn />;
    }
    <style>
      .tasks { display: grid; gap: 0.5rem; }
    </style>
  </section>;
}`

// Self-contained playground default: declares its own types and components
// so the opt-in type-check lane starts clean instead of full of TS errors.
export const playgroundCode = `type Task = { id: string; label: string; done: boolean };

function TaskRow({ task }: { task: Task }) @{
  <li>{task.label}</li>;
}

export function TaskList({ tasks }: { tasks: Task[] }) @{
  const pending = tasks.filter((task) => !task.done);

  <section class="tasks">
    @if (pending.length > 0) {
      <ul>
        @for (const task of pending; key task.id) {
          <TaskRow task={task} />;
        }
      </ul>;
    } @else {
      <p>All done!</p>;
    }
  </section>;
}`

// The "Type error" example. It has to be the self-contained snippet rather
// than the hero one: the hero references Props/TaskRow/AllDone/SignIn without
// declaring them, so type-checking it buries the interesting error under five
// "Cannot find name" ones. Here the typo is the only thing TypeScript can
// complain about.
const TYPE_ERROR_ANCHOR = '{task.label}'
const TYPE_ERROR_TYPO = '{task.titel}'
if (!playgroundCode.includes(TYPE_ERROR_ANCHOR)) {
  throw new Error(
    `demo-sources: playgroundCode no longer contains ${TYPE_ERROR_ANCHOR}, so the "Type error" example would load unmodified source`,
  )
}
export const typeErrorCode = playgroundCode.replace(TYPE_ERROR_ANCHOR, TYPE_ERROR_TYPO)

// The lint scenario's anchor lives in both snippets; assert it so the shared
// "Lint findings" and "Custom config" examples cannot go quiet either.
export const LINT_ANCHOR = 'const pending = tasks.filter((task) => !task.done);'
for (const [name, snippet] of Object.entries({ heroCode, playgroundCode })) {
  if (!snippet.includes(LINT_ANCHOR)) {
    throw new Error(`demo-sources: ${name} no longer contains the lint example anchor`)
  }
}
