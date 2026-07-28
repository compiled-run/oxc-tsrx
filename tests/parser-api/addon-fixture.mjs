import { rm } from "node:fs/promises";

/**
 * Remove a fixture directory holding an addon this process has already loaded.
 *
 * Windows keeps a native module mapped for the lifetime of the process that
 * required it, so the `.node` file cannot be unlinked until that process exits
 * and `rm` fails with `EPERM: operation not permitted, unlink`. Retrying does
 * not help: `maxRetries` covers a transient lock, and this one is held until
 * exit by design. scripts/package-native.mjs sidesteps it by probing an addon
 * in a child process, and these suites cannot, because what they assert is
 * in-process identity: the same lazily materialized graph handed back twice.
 *
 * So on Windows the mapped addon is left for the operating system to reclaim
 * with the rest of the temporary directory. Everything else is still removed,
 * every other failure still throws, and no assertion is relaxed: this runs
 * after the test body, in cleanup.
 */
export async function removeAddonFixture(directory) {
  try {
    await rm(directory, { recursive: true, force: true });
  } catch (error) {
    if (process.platform !== "win32" || error?.code !== "EPERM") throw error;
  }
}
