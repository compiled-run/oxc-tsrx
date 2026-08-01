#!/usr/bin/env node
//#region src/bin/oxfmt.ts
try {
	const { decideCanonicalCommand, deferralNotice, runOfficialCommand } = await import("../canonical-command.js");
	const args = process.argv.slice(2);
	const decision = await decideCanonicalCommand("oxfmt");
	if (decision.owner === "project") {
		const notice = deferralNotice(decision, args);
		if (notice !== null) console.error(notice);
		await runOfficialCommand(decision);
	} else {
		const { canRunCanonicalOxfmt, importDeclaredPackageBinary } = await import("../format-invocation.js");
		if (canRunCanonicalOxfmt(args)) await importDeclaredPackageBinary("oxfmt-current", "oxfmt", import.meta.url);
		else {
			const { runCli } = await import("../format-cli.js");
			process.exitCode = await runCli(args);
		}
	}
} catch (error) {
	console.error(`oxfmt (oxc-tsrx): ${error instanceof Error ? error.message : String(error)}`);
	process.exitCode = 2;
}
//#endregion
export {};
