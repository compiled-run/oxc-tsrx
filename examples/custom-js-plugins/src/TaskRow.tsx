type Task = { id: string; label: string; done: boolean };

export function TaskRow({ task }: { task: Task }) {
  return <li className={task.done ? "done" : ""}>{task.label}</li>;
}

export function TaskRows({ tasks }: { tasks: Task[] }) {
  return (
    <ul className="tasks">
      {tasks.map((task) => (
        <TaskRow task={task} />
      ))}
    </ul>
  );
}
