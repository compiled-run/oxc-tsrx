type Task = { id: string; label: string; done: boolean };

export function TaskList({ tasks, ready }: { tasks: Task[]; ready: boolean }) @{
  debugger;

  @if (ready) {
    <ul class="tasks">
      @for (const task of tasks) {
        <li>{task.label}</li>;
      }
    </ul>;
  } @else {
    <p>Loading tasks</p>;
  }
}
