import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { setTimeout as sleep } from "node:timers/promises";
import { NATIVE_TARGETS, nativePackageName } from "../packages/toolchain/dist/native-targets.js";
import { hostPlatformPackage, installAndExerciseRelease } from "./installed-release-check.mjs";

/**
 * The post-publish backstop.
 *
 * npm versions are immutable and unpublish is restricted, so nothing here can
 * prevent a bad release; the prevention lives in the pre-publish gate. What
 * this can do is notice, on one platform, that the thing on the registry is not
 * the thing that was gated, which is the one failure mode a pre-publish check
 * structurally cannot see: npm's own handling of the upload.
 *
 * It replaces a step that resolved a version string with `npm view` and
 * believed the release. A version string resolving is still checked, for all
 * nine names, because that is the cheapest way to see a package that never
 * landed. It is now the first of three things rather than the only one: the
 * release is then installed from the registry into a project outside this
 * workspace, and made to produce a real diagnostic and a real AST.
 *
 * Usage:
 *   node scripts/verify-published-release.mjs --version 0.1.5
 *
 *   --version <version>     the version that was just published
 *   --order-file <path>     "<name> <path>" lines naming the published packages
 *                           (default: the eight platform packages plus oxc-tsrx)
 *   --registry <url>        default https://registry.npmjs.org/
 *   --attempts <n>          registry visibility attempts, 10s apart (default 6)
 *   --allow-unpublished     exit 0 when the version is not on the registry,
 *                           which is what a dry-run rehearsal expects
 */

const root = resolve(import.meta.dirname, "..");

function parseArguments(argv) {
  const options = {
    version: null,
    orderFile: null,
    registry: "https://registry.npmjs.org/",
    attempts: 6,
    allowUnpublished: false,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === "--allow-unpublished") {
      options.allowUnpublished = true;
      continue;
    }
    const value = argv[++index];
    if (!value) throw new Error(`${argument} requires a value`);
    if (argument === "--version") options.version = value;
    else if (argument === "--order-file") options.orderFile = value;
    else if (argument === "--registry") options.registry = value.endsWith("/") ? value : `${value}/`;
    else if (argument === "--attempts") options.attempts = Number.parseInt(value, 10);
    else throw new Error(`unsupported option: ${argument}`);
  }
  if (!options.version) throw new Error("--version is required");
  if (!Number.isInteger(options.attempts) || options.attempts < 1) {
    throw new Error("--attempts must be a positive integer");
  }
  return options;
}

function say(line = "") {
  process.stdout.write(`${line}\n`);
}

function fail(message) {
  process.stdout.write(
    `${process.env.GITHUB_ACTIONS === "true" ? "::error::" : "error: "}${message}\n`,
  );
}

async function publishedNames(options) {
  if (!options.orderFile) return [...NATIVE_TARGETS.map(nativePackageName), "oxc-tsrx"];
  const contents = await readFile(resolve(root, options.orderFile), "utf8");
  return contents
    .split("\n")
    .map((line) => line.trim().split(/\s+/u)[0])
    .filter(Boolean);
}

/** Whether `name@version` is visible, without spawning npm nine times. */
async function visible(registry, name, version) {
  const response = await fetch(new URL(encodeURIComponent(name).replace("%40", "@"), registry), {
    headers: { accept: "application/vnd.npm.install-v1+json" },
  });
  if (response.status === 404) return false;
  if (!response.ok) throw new Error(`${name}: registry answered ${response.status}`);
  const packument = await response.json();
  return Boolean(packument.versions?.[version]);
}

async function main() {
  const options = parseArguments(process.argv.slice(2));
  const names = await publishedNames(options);
  say("Post-publish backstop");
  say(`  version   ${options.version}`);
  say(`  registry  ${options.registry}`);
  say(`  packages  ${names.length}`);
  say();

  say("[1/2] every published name resolves at this version");
  const missing = new Set(names);
  // A read can hit a replica that has not caught up with the write that just
  // succeeded, so a single miss is not evidence. That was true of the `npm view`
  // check this replaces and it is still true here.
  for (let attempt = 1; attempt <= options.attempts && missing.size > 0; attempt += 1) {
    for (const name of [...missing]) {
      if (await visible(options.registry, name, options.version)) missing.delete(name);
    }
    if (missing.size === 0) break;
    if (options.allowUnpublished && missing.size === names.length) break;
    say(`  ${missing.size} not visible yet (attempt ${attempt}), waiting for the registry`);
    if (attempt < options.attempts) await sleep(10_000);
  }
  if (missing.size === names.length && options.allowUnpublished) {
    say(`  ${options.version} is not on the registry, which is what a dry run expects`);
    say();
    say("backstop: SKIPPED  nothing is published at this version to install and run");
    return;
  }
  if (missing.size > 0) {
    for (const name of missing) fail(`${name}@${options.version} is not on the registry`);
    say();
    say(`backstop: FAIL  ${missing.size} of ${names.length} packages did not land`);
    process.exitCode = 1;
    return;
  }
  say(`  all ${names.length} names resolve at ${options.version}`);
  say();

  say("[2/2] install the published release and make it do real work");
  say();
  const host = hostPlatformPackage();
  try {
    // No tarball path and no local file: this is the registry's copy, resolving
    // the platform package through the published optionalDependencies exactly
    // as a consumer's first install does.
    const installed = await installAndExerciseRelease({
      specs: [`oxc-tsrx@${options.version}`],
      registry: options.registry,
      expectedVersion: options.version,
      log: (line) => say(line),
    });
    say();
    say(
      `backstop: PASS  oxc-tsrx@${options.version} installed from the registry on ` +
        `${host.target.target}, linted ${installed.lint.diagnostics} diagnostics, and parsed through ` +
        "its own addon",
    );
  } catch (error) {
    fail(`the published release does not work when installed from the registry: ${error.message}`);
    say();
    say("backstop: FAIL  the published artifact is broken; deprecate and patch");
    process.exitCode = 1;
  }
}

await main();
