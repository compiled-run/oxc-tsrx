import { defineConfig } from "tsdown";

export default defineConfig({
  cwd: import.meta.dirname,
  entry: ["src/index.ts", "src/facade.ts", "src/style.ts"],
  unbundle: true,
  format: "esm",
  dts: false,
  sourcemap: false,
  deps: {
    neverBundle: true,
  },
  platform: "node",
  target: "node20.19",
  fixedExtension: false,
  outDir: "dist",
  copy: [
    {
      from: ["src/index.d.ts", "src/types/index.d.ts", "src/types/estree.d.ts"],
      flatten: false,
    },
  ],
});
