#!/usr/bin/env node
// Publish the nine launch packages from a release-candidate run.
//
// The eight platform packages can only be built by CI, because one machine
// cannot compile all eight targets, so this downloads the artifacts from a
// successful `release-candidate.yml` run and publishes those exact tarballs.
//
//   node scripts/publish-release.mjs --dry-run        # rehearse, publishes nothing
//   node scripts/publish-release.mjs --tag next       # publish under a tag
//   node scripts/publish-release.mjs --otp 123456     # publish, 2FA code up front
//
// Order is not negotiable: `oxc-tsrx` names the eight platform packages in
// optionalDependencies and npm resolves those from the registry at install
// time, so publishing the parent first opens a window where an install
// succeeds, silently installs no binary, and fails at first use.

import { execFile, spawn } from "node:child_process";
import { mkdtemp, readdir, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");

// Thrown to unwind out of the publish loop while still running the cleanup in
// `finally`. A bare `return` is not legal at module top level.
class PublishStopped extends Error {}

function run(executable, args, options = {}) {
  return new Promise((resolveRun, rejectRun) => {
    execFile(
      executable,
      args,
      { cwd: options.cwd ?? root, env: process.env, maxBuffer: 32 * 1024 * 1024 },
      (error, stdout, stderr) => {
        if (error) rejectRun(new Error(stderr || stdout || error.message));
        else resolveRun({ stdout, stderr });
      },
    );
  });
}

// Publishing needs the real terminal. With 2FA enabled npm answers EOTP and
// prints a browser URL to authenticate against, and it can only do that if it
// owns stdio. Capturing the output instead turns the prompt into a failure.
function runInteractive(executable, args) {
  return new Promise((resolveRun, rejectRun) => {
    const child = spawn(executable, args, { cwd: root, env: process.env, stdio: "inherit" });
    child.on("error", rejectRun);
    child.on("close", (status) => {
      if (status === 0) resolveRun();
      else rejectRun(new Error(`${executable} exited with ${status}`));
    });
  });
}

function parseArguments(argv) {
  const options = { dryRun: false, tag: null, otp: null, runId: null };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === "--dry-run") options.dryRun = true;
    else if (argument === "--tag") options.tag = argv[++index];
    else if (argument === "--otp") options.otp = argv[++index];
    else if (argument === "--run-id") options.runId = argv[++index];
    else throw new Error(`unsupported option: ${argument}`);
  }
  return options;
}

const options = parseArguments(process.argv.slice(2));

const launch = JSON.parse(
  await readFile(join(root, "docs/releasing/v0.1.0-launch.json"), "utf8"),
);
const order = launch.npm.publishOrder;
const version = JSON.parse(await readFile(join(root, "package.json"), "utf8")).version;

if (order.at(-1) !== "oxc-tsrx") {
  throw new Error("the launch contract must publish oxc-tsrx last");
}

// Confirm who we are before writing anything to the registry.
let who;
try {
  who = (await run("npm", ["whoami"])).stdout.trim();
} catch {
  throw new Error("not logged in to npm: run `npm login` first");
}
console.log(`publishing as ${who}`);
console.log(`version ${version}, ${order.length} packages, tag ${options.tag ?? "latest"}`);
if (options.dryRun) console.log("DRY RUN: nothing will be published\n");

// Find the release-candidate run that produced the artifacts.
let runId = options.runId;
if (!runId) {
  const { stdout } = await run("gh", [
    "run", "list", "--workflow", "release-candidate.yml",
    "--status", "success", "--limit", "1", "--json", "databaseId,headSha",
  ]);
  const [latest] = JSON.parse(stdout);
  if (!latest) {
    // Say which, because "no successful run" reads as permanent when the usual
    // cause is simply that the build is still going.
    const { stdout: recent } = await run("gh", [
      "run", "list", "--workflow", "release-candidate.yml",
      "--limit", "3", "--json", "headSha,status,conclusion,url",
    ]);
    const runs = JSON.parse(recent);
    const running = runs.find((entry) => entry.status !== "completed");
    if (running) {
      throw new Error(
        `the release build is still running, nothing to publish yet\n` +
          `  ${running.headSha.slice(0, 7)} ${running.status}\n` +
          `  ${running.url}\n` +
          `Platform builds take about 30 minutes. Re-run this when it goes green.`,
      );
    }
    throw new Error(
      "no successful release-candidate run found. Most recent:\n" +
        runs
          .map((e) => `  ${e.headSha.slice(0, 7)}  ${e.conclusion ?? e.status}  ${e.url}`)
          .join("\n"),
    );
  }
  runId = String(latest.databaseId);
  console.log(`using release-candidate run ${runId} (${latest.headSha.slice(0, 7)})`);
}

const staging = await mkdtemp(join(tmpdir(), "oxc-tsrx-publish-"));
try {
  await run("gh", ["run", "download", runId, "-p", "release-*", "-D", staging]);

  // `gh run download` writes one directory per artifact; flatten to find the tarballs.
  const tarballs = new Map();
  const walk = async (directory) => {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const path = join(directory, entry.name);
      if (entry.isDirectory()) await walk(path);
      else if (entry.name.endsWith(".tgz")) tarballs.set(entry.name, path);
    }
  };
  await walk(staging);

  // The public package is packed from source; only the natives come from CI.
  const { stdout: packed } = await run("npm", [
    "pack", "--pack-destination", staging, "./packages/toolchain",
  ]);
  const publicTarball = packed.trim().split("\n").at(-1);
  tarballs.set(publicTarball, join(staging, publicTarball));

  const plan = order.map((name) => {
    const expected = `${name.replace("@", "").replace("/", "-")}-${version}.tgz`;
    const path = tarballs.get(expected);
    if (!path) {
      throw new Error(
        `missing tarball for ${name}: expected ${expected}\n` +
          `found: ${[...tarballs.keys()].sort().join(", ")}`,
      );
    }
    return { name, path };
  });

  console.log("\npublish order:");
  for (const [index, item] of plan.entries()) {
    console.log(`  ${String(index + 1).padStart(2)}. ${item.name}`);
  }
  console.log();

  for (const { name, path } of plan) {
    const args = ["publish", path, "--access", "public"];
    if (options.tag) args.push("--tag", options.tag);
    if (options.otp) args.push("--otp", options.otp);
    if (options.dryRun) args.push("--dry-run");
    // A laptop cannot produce provenance; CI does that once trusted publishing
    // is configured. Without this the publish fails outright.
    args.push("--no-provenance");
    console.log(`\n>>> ${name}`);
    try {
      await runInteractive("npm", args);
      console.log(`  published  ${name}`);
    } catch (error) {
      console.error(`  FAILED     ${name}: ${error.message}`);
      if (!options.dryRun) {
        console.error(
          "\nStopping. Packages already published stay published; re-run to continue,\n" +
            "already-published names will fail with EPUBLISHCONFLICT and can be ignored.",
        );
      }
      process.exitCode = 1;
      throw new PublishStopped();
    }
  }

  console.log(
    options.dryRun
      ? "\ndry run complete, nothing was published"
      : "\nall nine packages published",
  );
} catch (error) {
  if (!(error instanceof PublishStopped)) throw error;
} finally {
  await rm(staging, { recursive: true, force: true });
}
