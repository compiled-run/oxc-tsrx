import { readFileSync } from "node:fs";
import { parseSync } from "oxc-tsrx/parser";

const file = "src/TaskList.tsrx";
const result = parseSync(file, readFileSync(file, "utf8"));

function* walk(node) {
  if (Array.isArray(node)) {
    for (const item of node) yield* walk(item);
  } else if (node && typeof node === "object") {
    if (typeof node.type === "string") yield node;
    for (const value of Object.values(node)) yield* walk(value);
  }
}

for (const node of walk(result.program)) {
  if (node.type.startsWith("JSX") && node.type.endsWith("Expression")) {
    console.log(node.type);
  }
}
