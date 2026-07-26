import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { build } from "rolldown";

const root = import.meta.dirname;
const committedBundle = resolve(root, "dist/extension.bundle.cjs");
const check = process.argv.slice(2).includes("--check");
const unsupported = process.argv.slice(2).filter((argument) => argument !== "--check");
if (unsupported.length > 0) {
  throw new Error(`unsupported option(s): ${unsupported.join(", ")}`);
}

// Rolldown's retained module-region paths are relative to process.cwd(). Keep
// them stable whether this script is invoked directly or through a pnpm filter;
// the generated third-party license inventory keys those regions to the lockfile.
process.chdir(resolve(root, "../.."));

async function buildBundle(file) {
  await build({
    input: resolve(root, "dist/extension.cjs"),
    platform: "node",
    external: ["vscode"],
    output: {
      file,
      format: "cjs",
      codeSplitting: false,
      sourcemap: false,
    },
  });
}

if (check) {
  const temporaryRoot = await mkdtemp(join(tmpdir(), "oxc-tsrx-vscode-bundle-check-"));
  try {
    const generatedBundle = join(temporaryRoot, "extension.bundle.cjs");
    await buildBundle(generatedBundle);
    const [expected, actual] = await Promise.all([
      readFile(generatedBundle),
      readFile(committedBundle).catch(() => null),
    ]);
    if (!actual || !expected.equals(actual)) {
      throw new Error(
        "packages/vscode/dist/extension.bundle.cjs is stale; run pnpm run build:editor",
      );
    }
    process.stdout.write("verified fresh VS Code extension bundle\n");
  } finally {
    await rm(temporaryRoot, { recursive: true, force: true });
  }
} else {
  await buildBundle(committedBundle);
}
