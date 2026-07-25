#!/usr/bin/env node
// Build the one multi-call native executable, then materialize the `argv[0]`
// aliases it still answers to.
//
// `crates/oxc_tsrx_cli` used to produce three release binaries. They are now a
// single busybox-style executable that picks its tool from `argv[0]` or from a
// leading subcommand. Tools that spawn `target/release/oxc-tsrx` with a
// subcommand need nothing extra, but a caller that can only name a file still
// needs the old name to exist. `benchmarks/native-format/budgets.json` is one:
// its `candidateBinary` is byte-frozen evidence (the file is SHA-256 pinned by
// tests/acceptance/performance-contract.test.mjs and embedded in
// docs/acceptance/performance-report.json), so the path has to be made valid
// rather than edited. Pointing it at plain `oxc-tsrx` would silently benchmark
// the linter, because that is the no-tool default.
//
// The aliases are a local build convenience only. The published platform
// package still ships exactly one binary: scripts/package-native.mjs stages
// `binaryStems = ["oxc-tsrx"]` and ignores everything else in the bin
// directory.
//
// They are rewritten from scratch on every build, and a plain copy is used
// rather than a hardlink or symlink. Cargo replaces the binary with a new inode
// instead of writing through the old one, so a hardlink made once would keep
// serving a stale build forever; that is exactly the failure mode that once let
// a formatter lane pass against a binary nobody had rebuilt. A copy is also the
// only form that needs no elevation on Windows.

import { spawnSync } from "node:child_process";
import { chmodSync, copyFileSync, existsSync, mkdirSync, rmSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const executableSuffix = process.platform === "win32" ? ".exe" : "";

// Only the names something in this repository actually spawns by file name.
// `oxc-tsrx-lsp` is deliberately absent: every language-server caller here
// passes the `lsp` subcommand to `oxc-tsrx`, and an alias nothing reads is an
// unrebuilt file waiting to mislead someone.
const aliases = ["oxc-tsrx-fmt"];

const passthrough = process.argv.slice(2);
const cargoArguments = [
  "build",
  "--release",
  "--locked",
  "-p",
  "oxc_tsrx_cli",
  "--bins",
  ...passthrough,
];

const build = spawnSync("cargo", cargoArguments, { cwd: root, stdio: "inherit" });
if (build.error) {
  console.error(`build:native: unable to run cargo: ${build.error.message}`);
  process.exit(1);
}
if (build.status !== 0) {
  process.exit(build.status ?? 1);
}

// `--target <triple>` moves the output under target/<triple>/release.
const targetIndex = passthrough.indexOf("--target");
const triple = targetIndex === -1 ? null : passthrough[targetIndex + 1];
const binaryDirectory = triple
  ? join(root, "target", triple, "release")
  : join(root, "target", "release");
const binary = join(binaryDirectory, `oxc-tsrx${executableSuffix}`);

if (!existsSync(binary)) {
  console.error(`build:native: cargo did not produce ${binary}`);
  process.exit(1);
}

mkdirSync(binaryDirectory, { recursive: true });
for (const alias of aliases) {
  const aliasPath = join(binaryDirectory, `${alias}${executableSuffix}`);
  rmSync(aliasPath, { force: true });
  copyFileSync(binary, aliasPath);
  chmodSync(aliasPath, 0o755);

  // Prove the copy dispatches as the tool its name claims. Without this the
  // alias is an unchecked file that could quietly run the linter.
  if (process.platform === "win32") {
    console.log(`build:native: wrote ${aliasPath} (argv[0] dispatch unchecked on Windows)`);
    continue;
  }
  const identity = spawnSync(aliasPath, ["--version"], { cwd: root, encoding: "utf8" });
  if (identity.status !== 0 || !identity.stdout?.startsWith(`${alias} `)) {
    console.error(
      `build:native: ${aliasPath} did not identify as ${alias}: ${
        identity.stdout?.trim() || identity.stderr?.trim() || identity.error?.message
      }`,
    );
    process.exit(1);
  }
  console.log(`build:native: wrote ${aliasPath} (${identity.stdout.trim()})`);
}
