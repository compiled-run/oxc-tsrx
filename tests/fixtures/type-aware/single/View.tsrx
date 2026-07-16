declare function save(): Promise<void>;

export function View() @{
  save();
  <main>Saved</main>;
}
