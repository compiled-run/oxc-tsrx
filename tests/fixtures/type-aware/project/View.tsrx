import { save } from "./service.tsrx";

export function View() @{
  save();
  <main>Saved</main>;
}
