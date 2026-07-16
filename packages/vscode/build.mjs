import { resolve } from "node:path";
import { build } from "rolldown";

const root = import.meta.dirname;
// Rolldown's retained module-region paths are relative to process.cwd(). Keep
// them stable whether this script is invoked directly or through npm workspaces;
// the generated third-party license inventory keys those regions to the lockfile.
process.chdir(resolve(root, "../.."));
await build({
  input: resolve(root, "dist/extension.cjs"),
  platform: "node",
  external: ["vscode"],
  output: {
    file: resolve(root, "dist/extension.bundle.cjs"),
    format: "cjs",
    codeSplitting: false,
    sourcemap: false,
  },
});
