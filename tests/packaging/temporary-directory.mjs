import { mkdtemp, realpath } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

/**
 * A temporary fixture directory named the way the filesystem itself names it.
 *
 * On Windows CI `os.tmpdir()` reports the 8.3 short form of the user's profile
 * (`C:\Users\RUNNER~1\AppData\Local\Temp`), and the two realpath implementations
 * in Node disagree about it. `fs.realpathSync`, which `createRequire().resolve`
 * and the invocation resolvers use, walks the path in JavaScript and keeps
 * whatever spelling it was handed, so it returns the short form. The
 * `fs/promises` `realpath` is the libuv call, which asks Windows for the final
 * name and returns `C:\Users\runneradmin\...`. Both name the same file, but
 * comparing them as strings fails, and `path.relative` does not rescue it
 * either: the two spellings differ by more than case.
 *
 * The same split reaches package managers. pnpm records an absolute target in
 * the junction it writes for `node_modules/<name>`, so a fixture rooted on the
 * short form makes every resolved provider path short while the test's own
 * `realpath` of the project is long, and containment checks fail.
 *
 * Anchoring every fixture on its real path resolves the alias once, at the only
 * point where it is introduced, so assertions stay exact equality on paths
 * rather than being loosened into path matching. On POSIX this is the `/var` ->
 * `/private/var` resolution these tests already depended on.
 */
export async function temporaryDirectory(prefix) {
  return realpath(await mkdtemp(join(tmpdir(), prefix)));
}
