import { DEPENDENCY_FIELDS, extensionOf, findProjectRoot } from "./provider-resolve.js";
import { spawnCommand } from "./spawn-command.js";
import { createRequire } from "node:module";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { open, readFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
//#region src/canonical-command.ts
/**
* Arbitration for the canonical command names this package also publishes.
*
* `oxc-tsrx` ships `oxlint` and `oxfmt` bins because that is the only thing
* that makes a plain `npm install oxc-tsrx` reach released hosts: the installer
* links `node_modules/.bin/oxlint`, and the released `oxc.oxc-vscode`
* extension probes exactly that path first. Nothing else about the shipped
* artifact reaches an unmodified host.
*
* The cost is a name collision. When a project *also* declares the official
* `oxlint` or `oxfmt` package, whichever package wins `node_modules/.bin` is
* decided by the installer, and installers disagree: npm 11 links this
* package's launcher and pnpm 10 links the official one. Measured for T016.
*
* So the launcher decides for itself instead of inheriting the race. A direct
* dependency on the official package in the project's own manifest is an
* explicit statement about what that command name means, and it wins: the
* launcher hands the whole invocation to the exact binary that package
* declares. The observable behaviour of `oxlint`/`oxfmt` in such a project is
* then identical under every installer, and identical to what it was before
* `oxc-tsrx` was added.
*
* A transitive official package (Vite+ depends on `oxlint`, for instance) is
* not such a statement, so it does not take the command name away.
*/
const OWNED_COMMANDS = Object.freeze({
	oxlint: Object.freeze({ leafBin: "oxc-tsrx-lint" }),
	oxfmt: Object.freeze({ leafBin: "oxc-tsrx-fmt" })
});
/** The extension this package's provider block claims. */
const PROVIDED_EXTENSION = ".tsrx";
function isPlainObject(value) {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}
function declaresDirectly(manifest, name) {
	for (const field of DEPENDENCY_FIELDS) {
		const declared = manifest?.[field];
		if (isPlainObject(declared) && typeof declared[name] === "string") return field;
	}
	return null;
}
function declaredBin(manifest, command) {
	if (typeof manifest.bin === "string") return manifest.name === command ? manifest.bin : null;
	if (!isPlainObject(manifest.bin)) return null;
	const declared = manifest.bin[command];
	return typeof declared === "string" && declared.length > 0 ? declared : null;
}
/** True for the package-name facade `oxc-tsrx setup` writes into that slot. */
function isCompatibilityFacade(manifest) {
	return manifest?.oxcTsrxCompatibility?.provider === "oxc-tsrx";
}
/**
* Decide who owns `command` for the project `cwd` belongs to.
*
* Never throws for an ordinary project: an unreadable or absent manifest simply
* means nothing took the name away. It throws only for the one genuinely
* ambiguous case — the project declares the official package and that package
* is not installed — because guessing there would silently change which linter
* or formatter a pinned project runs.
*/
async function decideCanonicalCommand(command, options = {}) {
	if (OWNED_COMMANDS[command] === void 0) throw new Error(`unknown canonical command: ${command}`);
	const cwd = options.cwd ?? process.cwd();
	let projectRoot;
	let manifest;
	try {
		projectRoot = await findProjectRoot(cwd);
		manifest = JSON.parse(await readFile(join(projectRoot, "package.json"), "utf8"));
	} catch {
		return {
			command,
			owner: "oxc-tsrx",
			reason: "no-project-manifest",
			projectRoot: null
		};
	}
	const field = declaresDirectly(manifest, command);
	if (field === null) return {
		command,
		owner: "oxc-tsrx",
		reason: "not-directly-declared",
		projectRoot
	};
	let manifestPath;
	try {
		manifestPath = createRequire(join(projectRoot, "package.json")).resolve(`${command}/package.json`);
	} catch {
		throw new Error(`${projectRoot}/package.json declares the official ${command} package in ${field}, but ${command} is not installed. Install dependencies, or remove that dependency to let oxc-tsrx own the ${command} command.`);
	}
	const officialManifest = JSON.parse(await readFile(manifestPath, "utf8"));
	if (isCompatibilityFacade(officialManifest)) return {
		command,
		owner: "oxc-tsrx",
		reason: "compatibility-facade",
		projectRoot,
		officialRoot: dirname(manifestPath)
	};
	const declared = declaredBin(officialManifest, command);
	if (declared === null) throw new Error(`${projectRoot}/package.json declares the official ${command} package in ${field}, but the installed ${officialManifest.name ?? command} does not declare a ${command} binary. Remove that dependency to let oxc-tsrx own the ${command} command.`);
	return {
		command,
		owner: "project",
		reason: `declared-in-${field}`,
		projectRoot,
		officialRoot: dirname(manifestPath),
		officialVersion: typeof officialManifest.version === "string" ? officialManifest.version : null,
		binPath: resolve(dirname(manifestPath), declared)
	};
}
/** Arguments that name a file this package's provider block claims. */
function providedArguments(args) {
	return args.filter((argument) => typeof argument === "string" && !argument.startsWith("-") && extensionOf(argument) === PROVIDED_EXTENSION);
}
/**
* One actionable line, only when the caller actually asked about a `.tsrx`
* file. Silence for every ordinary invocation is the point: a project that
* pinned official Oxlint or Oxfmt must not gain new output because `oxc-tsrx`
* is installed somewhere in its tree.
*/
function deferralNotice(decision, args) {
	if (decision.owner !== "project") return null;
	const provided = providedArguments(args);
	if (provided.length === 0) return null;
	const owned = OWNED_COMMANDS[decision.command];
	const pinned = decision.officialVersion === null ? `the official ${decision.command} package` : `official ${decision.command} ${decision.officialVersion}`;
	return `${decision.command} (oxc-tsrx): this project depends on ${pinned}, so the ${decision.command} command runs it unchanged and will not read ${provided.join(", ")}. Run \`npx ${owned.leafBin}\` for .tsrx files, or drop the direct ${decision.command} dependency to let oxc-tsrx serve both.`;
}
/**
* A declared `bin` entry may be a JavaScript wrapper or a native executable.
* Reading the shebang is a static file read, not an execution of the package.
*
* The BOM strip is not decoration. A UTF-8 byte-order mark is common in files
* authored on Windows, and it sits in front of the `#!`, so without stripping
* it a perfectly ordinary Node wrapper would be classified as a native
* executable and spawned — which on Windows fails outright, because an
* extensionless file is not something `CreateProcess` can run.
*/
async function usesNodeInterpreter(path) {
	let handle;
	try {
		handle = await open(path, "r");
		const buffer = Buffer.alloc(128);
		const { bytesRead } = await handle.read(buffer, 0, 128, 0);
		const shebang = buffer.subarray(0, bytesRead).toString("utf8").replace(/^\uFEFF/u, "").split("\n", 1)[0];
		return shebang.startsWith("#!") && /\bnode(?:\.exe)?\b/u.test(shebang);
	} catch {
		return false;
	} finally {
		await handle?.close();
	}
}
/**
* Execute the official binary, preserving its exact behaviour. A Node wrapper
* runs in this process so argv, stdio, exit code, and signal handling are the
* program's own; anything else is spawned with inherited stdio and its status
* is mirrored.
*
* `pathToFileURL` is load-bearing rather than tidy: on Windows a bare absolute
* path such as `C:\project\node_modules\oxlint\bin\oxlint` is not a valid
* import specifier, and `import()` rejects it with ERR_UNSUPPORTED_ESM_URL_SCHEME
* because it reads `C:` as a URL scheme. Every host reaches the in-process
* branch, so this is the difference between the launcher working on Windows and
* failing on the first `oxlint` in a project that pinned the official package.
*
* `spawnCommand` covers the other branch: a declared `bin` may be a `.cmd` or
* `.bat` launcher, which only a command interpreter can run. See
* ./spawn-command.js for why that is done inline and without `shell: true`.
*
* A spawn that never starts is reported rather than thrown as an unhandled
* `error` event, so the caller's `catch` turns it into this package's one-line
* message instead of a stack trace from inside `node:child_process`.
*/
async function runOfficialCommand(decision, options = {}) {
	if (await usesNodeInterpreter(decision.binPath)) {
		await import(pathToFileURL(decision.binPath).href);
		return;
	}
	const spawnProcess = options.spawn ?? spawn;
	const child = spawnCommand(decision.binPath, process.argv.slice(2), { stdio: "inherit" }, spawnProcess);
	let failure = null;
	child.on("error", (error) => failure ??= error);
	const [status, signal] = await once(child, "close").catch(() => [null, null]);
	if (failure !== null) throw new Error(`could not execute ${decision.binPath}, the ${decision.command} binary declared by ${decision.officialRoot ?? "the official package"}: ${failure.message}`);
	process.exitCode = signal === null ? status ?? 0 : 2;
}
//#endregion
export { decideCanonicalCommand, deferralNotice, providedArguments, runOfficialCommand, usesNodeInterpreter };
