import { mkdtemp, rm } from "node:fs/promises";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { basename, join } from "node:path";
import { build } from "rolldown";

export async function transpileCts(source) {
  const directory = await mkdtemp(join(tmpdir(), "oxc-tsrx-test-cts-"));
  const modulePath = join(directory, `${basename(source, ".cts")}.cjs`);
  try {
    await build({
      input: source,
      platform: "node",
      output: {
        file: modulePath,
        format: "cjs",
        codeSplitting: false,
        sourcemap: false,
      },
    });
  } catch (error) {
    await rm(directory, { recursive: true, force: true });
    throw error;
  }
  return {
    modulePath,
    dispose: () => rm(directory, { recursive: true, force: true }),
  };
}

export async function requireCts(source) {
  const artifact = await transpileCts(source);
  try {
    return createRequire(import.meta.url)(artifact.modulePath);
  } finally {
    await artifact.dispose();
  }
}
