import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdir, mkdtemp, readFile, readdir, realpath, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { basename, join, resolve } from "node:path";
import test from "node:test";
import { resolveNpmInvocation } from "../../scripts/npm-invocation.mjs";

const root = resolve(import.meta.dirname, "../..");

function hostTarget() {
  if (process.platform === "darwin" && process.arch === "arm64") {
    return "aarch64-apple-darwin";
  }
  if (process.platform === "darwin" && process.arch === "x64") {
    return "x86_64-apple-darwin";
  }
  if (process.platform === "win32" && process.arch === "arm64") {
    return "aarch64-pc-windows-msvc";
  }
  if (process.platform === "win32" && process.arch === "x64") {
    return "x86_64-pc-windows-msvc";
  }
  if (process.platform === "linux" && ["arm64", "x64"].includes(process.arch)) {
    const architecture = process.arch === "arm64" ? "aarch64" : "x86_64";
    const libc = process.report?.getReport?.().header?.glibcVersionRuntime ? "gnu" : "musl";
    return `${architecture}-unknown-linux-${libc}`;
  }
  throw new Error(`unsupported packaging-test host ${process.platform}-${process.arch}`);
}

function differentArchitectureTarget() {
  const target = hostTarget();
  return target.startsWith("aarch64-")
    ? target.replace(/^aarch64-/, "x86_64-")
    : target.replace(/^x86_64-/, "aarch64-");
}

function hostObjectFormat() {
  if (process.platform === "darwin") return "mach-o";
  if (process.platform === "linux") return "elf";
  if (process.platform === "win32") return "pe";
  throw new Error(`unsupported packaging-test platform ${process.platform}`);
}

function oppositeArchitecture() {
  return process.arch === "arm64" ? "x64" : "arm64";
}

function fixtureTarget(os) {
  const cpu = process.platform === os ? oppositeArchitecture() : "arm64";
  const architecture = cpu === "arm64" ? "aarch64" : "x86_64";
  if (os === "darwin") return `${architecture}-apple-darwin`;
  if (os === "linux") return `${architecture}-unknown-linux-gnu`;
  if (os === "win32") return `${architecture}-pc-windows-msvc`;
  throw new Error(`unsupported executable fixture OS ${os}`);
}

function executableHeader(format, cpu, bits = 64) {
  if (format === "mach-o") {
    const contents = Buffer.alloc(32);
    contents.writeUInt32LE(bits === 64 ? 0xfeedfacf : 0xfeedface, 0);
    contents.writeUInt32LE(cpu === "arm64" ? 0x0100000c : 0x01000007, 4);
    contents.writeUInt32LE(2, 12);
    return contents;
  }
  if (format === "elf") {
    const contents = Buffer.alloc(64);
    contents.set([0x7f, 0x45, 0x4c, 0x46, bits === 64 ? 2 : 1, 1, 1, 0]);
    contents.writeUInt16LE(3, 16);
    contents.writeUInt16LE(cpu === "arm64" ? 183 : 62, 18);
    return contents;
  }
  if (format === "pe") {
    const contents = Buffer.alloc(0x100);
    contents.write("MZ", 0, "ascii");
    contents.writeUInt32LE(0x80, 0x3c);
    contents.set([0x50, 0x45, 0, 0], 0x80);
    contents.writeUInt16LE(cpu === "arm64" ? 0xaa64 : 0x8664, 0x84);
    contents.writeUInt16LE(0x0002, 0x96);
    contents.writeUInt16LE(bits === 64 ? 0x020b : 0x010b, 0x98);
    return contents;
  }
  throw new Error(`unsupported executable fixture format ${format}`);
}

async function writeExecutableFixtures(directory, target, format, bits = 64) {
  await mkdir(directory, { recursive: true });
  const cpu = target.startsWith("aarch64-") ? "arm64" : "x64";
  const suffix = format === "pe" ? ".exe" : "";
  const contents = executableHeader(format, cpu, bits);
  await Promise.all(
    ["oxc-tsrx", "oxc-tsrx-fmt", "oxc-tsrx-lsp"].map((name) =>
      writeFile(join(directory, `${name}${suffix}`), contents),
    ),
  );
}

function run(executable, args, options = {}) {
  return new Promise((resolveRun, rejectRun) => {
    execFile(
      executable,
      args,
      { cwd: options.cwd ?? root, env: options.env ?? process.env, maxBuffer: 16 * 1024 * 1024 },
      (error, stdout, stderr) => {
        if (error) rejectRun(new Error(stderr || stdout, { cause: error }));
        else resolveRun({ stdout, stderr });
      },
    );
  });
}

async function sha256(path) {
  return createHash("sha256")
    .update(await readFile(path))
    .digest("hex");
}

test("current native release stages a complete, checksummed, npm-installable platform package", async () => {
  const artifacts = await mkdtemp(join(tmpdir(), "oxc-tsrx-native-artifacts-"));
  const { stdout } = await run(process.execPath, [
    "scripts/package-native.mjs",
    "--target",
    hostTarget(),
    "--bin-dir",
    "target/release",
    "--out-dir",
    artifacts,
  ]);
  const packaged = JSON.parse(stdout);
  assert.equal(packaged.version, "0.1.0");
  assert.equal(packaged.target, hostTarget());
  assert.match(packaged.packageName, /^@oxc-tsrx\/native-/);
  assert.equal(resolve(packaged.tarball).startsWith(resolve(artifacts)), true);

  const consumer = await mkdtemp(join(tmpdir(), "oxc-tsrx-native-consumer-"));
  const npmInvocation = resolveNpmInvocation([
    "install",
    "--ignore-scripts",
    "--no-audit",
    "--no-fund",
    packaged.tarball,
  ]);
  await run(npmInvocation.executable, npmInvocation.args, {
    cwd: consumer,
    env: { ...process.env, npm_config_cache: join(consumer, ".npm-cache") },
  });

  const packageRoot = join(consumer, "node_modules", ...packaged.packageName.split("/"));
  assert.equal((await realpath(packageRoot)).startsWith(await realpath(consumer)), true);
  const manifest = JSON.parse(await readFile(join(packageRoot, "package.json"), "utf8"));
  assert.equal(manifest.version, "0.1.0");
  assert.equal(manifest.oxcTsrx.target, hostTarget());
  assert.equal(manifest.oxcTsrx.oxcRevision, "8e0ed2ebb96137fb1611cdbd5742d5cb46037d40");
  assert.equal(manifest.scripts, undefined);
  assert.equal(manifest.preferUnplugged, true);

  const checksums = JSON.parse(await readFile(join(packageRoot, "checksums.json"), "utf8"));
  const expected =
    process.platform === "win32"
      ? ["oxc-tsrx.exe", "oxc-tsrx-fmt.exe", "oxc-tsrx-lsp.exe"]
      : ["oxc-tsrx", "oxc-tsrx-fmt", "oxc-tsrx-lsp"];
  const lsp = process.platform === "win32" ? "oxc-tsrx-lsp.exe" : "oxc-tsrx-lsp";
  assert.equal(packaged.lspSha256, checksums.binaries[lsp].sha256);
  assert.equal(packaged.lspBytes, checksums.binaries[lsp].bytes);
  assert.deepEqual((await readdir(join(packageRoot, "bin"))).sort(), expected.sort());
  for (const binary of expected) {
    const path = join(packageRoot, "bin", binary);
    const metadata = await stat(path);
    assert.equal(metadata.isFile(), true);
    if (process.platform !== "win32") assert.notEqual(metadata.mode & 0o111, 0);
    assert.equal(checksums.binaries[binary].bytes, metadata.size);
    assert.equal(checksums.binaries[binary].sha256, await sha256(path));
    assert.equal(checksums.binaries[binary].object.format, hostObjectFormat());
    assert.equal(checksums.binaries[binary].object.os, process.platform);
    assert.equal(checksums.binaries[binary].object.bits, 64);
    assert.ok(checksums.binaries[binary].object.architectures.includes(process.arch));
  }
  assert.equal(checksums.objectVerification, "executable-header");
  assert.ok((await readdir(packageRoot)).includes("LICENSE"));
  assert.ok((await readdir(packageRoot)).includes("README.md"));
  assert.ok((await readdir(packageRoot)).includes("THIRD_PARTY_NOTICES.md"));
  assert.equal(
    basename(packaged.tarball),
    `${packaged.packageName.slice(1).replace("/", "-")}-0.1.0.tgz`,
  );
});

test("native packaging rejects current-host object files labeled as another architecture", async () => {
  const artifacts = await mkdtemp(join(tmpdir(), "oxc-tsrx-native-wrong-target-"));
  await assert.rejects(
    run(process.execPath, [
      "scripts/package-native.mjs",
      "--target",
      differentArchitectureTarget(),
      "--bin-dir",
      "target/release",
      "--out-dir",
      artifacts,
    ]),
    /object target mismatch.*expected .* found /s,
  );
});

test("cross-package verification recognizes Mach-O, ELF, and PE headers without host tools", async () => {
  for (const [os, format] of [
    ["darwin", "mach-o"],
    ["linux", "elf"],
    ["win32", "pe"],
  ]) {
    const target = fixtureTarget(os);
    const binaries = await mkdtemp(join(tmpdir(), `oxc-tsrx-${format}-fixtures-`));
    const artifacts = await mkdtemp(join(tmpdir(), `oxc-tsrx-${format}-artifacts-`));
    await writeExecutableFixtures(binaries, target, format);
    const { stdout } = await run(process.execPath, [
      "scripts/package-native.mjs",
      "--target",
      target,
      "--bin-dir",
      binaries,
      "--out-dir",
      artifacts,
    ]);
    assert.equal(JSON.parse(stdout).target, target);
  }
});

test("all supported packages reject 32-bit executable headers", async () => {
  for (const [os, format] of [
    ["darwin", "mach-o"],
    ["linux", "elf"],
    ["win32", "pe"],
  ]) {
    const target = fixtureTarget(os);
    const binaries = await mkdtemp(join(tmpdir(), `oxc-tsrx-${format}-32-bit-`));
    const artifacts = await mkdtemp(join(tmpdir(), `oxc-tsrx-${format}-32-artifacts-`));
    await writeExecutableFixtures(binaries, target, format, 32);
    await assert.rejects(
      run(process.execPath, [
        "scripts/package-native.mjs",
        "--target",
        target,
        "--bin-dir",
        binaries,
        "--out-dir",
        artifacts,
      ]),
      /object target mismatch.*32-bit/s,
    );
  }
});
