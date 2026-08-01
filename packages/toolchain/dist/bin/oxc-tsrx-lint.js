#!/usr/bin/env node
//#region src/bin/oxc-tsrx-lint.ts
try {
	const { resolveNativeCommand, runPassthrough } = await import("../runtime.js");
	const native = resolveNativeCommand("lint", process.argv.slice(2));
	const result = await runPassthrough(native.executable, native.args);
	process.exitCode = result.status;
} catch (error) {
	console.error(`oxc-tsrx-lint: ${error instanceof Error ? error.message : String(error)}`);
	process.exitCode = 2;
}
//#endregion
export {};
