#!/usr/bin/env node

// Leaf capability executor for this package's declared `lint` capability.
//
// A capability target must be a leaf executor: it performs no provider
// discovery, it does not dispatch by file extension, and it is not an entry
// point a host resolves by canonical tool name. A discovering host executes
// this file only for the files this package's provider declaration already
// claims, so it lints exactly what it is handed and nothing else. Pointing the
// capability at a general host wrapper instead would make an adopting linter
// re-enter itself without bound.
//
// The argv, output, and exit-code contract a host follows to call this is
// "Capability calling convention" in ../README.md, and is pinned by
// tests/packaging/toolchain-package.test.mjs.
try {
  const { resolveNativeCommand, runPassthrough } = await import("../runtime.js");
  const native = resolveNativeCommand("lint", process.argv.slice(2));
  const result = await runPassthrough(native.executable, native.args);
  process.exitCode = result.status;
} catch (error) {
  console.error(
    `oxc-tsrx-lint: ${error instanceof Error ? error.message : String(error)}`,
  );
  process.exitCode = 2;
}
