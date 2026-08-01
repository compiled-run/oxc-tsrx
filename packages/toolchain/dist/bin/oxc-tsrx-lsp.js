#!/usr/bin/env node
//#region src/bin/oxc-tsrx-lsp.ts
try {
	const { resolveNativeCommand, runPassthrough } = await import("../runtime.js");
	const native = resolveNativeCommand("server", process.argv.slice(2));
	const result = await runPassthrough(native.executable, native.args);
	process.exitCode = result.status;
} catch (error) {
	console.error(`oxc-tsrx-lsp: ${error instanceof Error ? error.message : String(error)}`);
	process.exitCode = 2;
}
//#endregion
export {};
