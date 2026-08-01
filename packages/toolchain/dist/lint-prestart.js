import { resolvePackageBinary } from "./package-binary.js";
import { runCaptured } from "./process.js";
//#region src/lint-prestart.ts
function startCanonicalOxlint(args, cwd = process.cwd(), env = process.env) {
	const binary = resolvePackageBinary("oxlint-current", "oxlint", import.meta.url);
	return {
		args,
		binary,
		result: runCaptured(process.execPath, [binary, ...args], {
			cwd,
			env
		})
	};
}
//#endregion
export { startCanonicalOxlint };
