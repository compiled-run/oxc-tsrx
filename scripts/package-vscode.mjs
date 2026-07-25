import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import {
  chmod,
  copyFile,
  cp,
  mkdir,
  mkdtemp,
  readFile,
  rm,
  stat,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { basename, join, resolve } from "node:path";
import { NATIVE_TARGETS } from "../packages/toolchain/dist/native-targets.js";
import { resolveVsceInvocation } from "./vsce-invocation.mjs";
import { verifyAndPromoteVsix } from "./vsix-archive.mjs";

const root = resolve(import.meta.dirname, "..");
const revision = "8e0ed2ebb96137fb1611cdbd5742d5cb46037d40";

function parseArguments(argv) {
  const options = {};
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (!["--target", "--lsp-bin", "--out-dir"].includes(argument)) {
      throw new Error(`unsupported option: ${argument}`);
    }
    const value = argv[++index];
    if (!value) throw new Error(`${argument} requires a value`);
    options[argument.slice(2)] = value;
  }
  for (const name of ["target", "lsp-bin", "out-dir"]) {
    if (!options[name]) throw new Error(`--${name} is required`);
  }
  return options;
}

function run(executable, args, options = {}) {
  return new Promise((resolveRun, rejectRun) => {
    execFile(
      executable,
      args,
      { cwd: options.cwd ?? root, maxBuffer: 16 * 1024 * 1024 },
      (error, stdout, stderr) => {
        if (error) rejectRun(new Error(stderr || stdout, { cause: error }));
        else resolveRun({ stdout, stderr });
      },
    );
  });
}

function rustHost(verboseVersion) {
  return /^host:\s*(\S+)$/mu.exec(verboseVersion)?.[1] ?? null;
}

function sha256(contents) {
  return createHash("sha256").update(contents).digest("hex");
}

const options = parseArguments(process.argv.slice(2));
const platform = NATIVE_TARGETS.find((candidate) => candidate.target === options.target);
if (!platform) throw new Error(`unsupported Rust target: ${options.target}`);
const source = join(root, "packages/vscode");
await run(process.execPath, [join(source, "build.mjs"), "--check"]);
await run(process.execPath, [
  join(root, "scripts/generate-vscode-license-inventory.mjs"),
  "--check",
]);
const [sourcePackage, sourceBundle, sourceInventory, sourceReport] = await Promise.all([
  readFile(join(source, "package.json")),
  readFile(join(source, "dist/extension.bundle.cjs")),
  readFile(join(source, "licenses/bundle-dependencies.json")),
  readFile(join(source, "licenses/BUNDLE_DEPENDENCIES.md")),
]);
const sourceManifest = JSON.parse(sourcePackage);
const lspSource = resolve(root, options["lsp-bin"]);
const lspMetadata = await stat(lspSource).catch(() => null);
if (!lspMetadata?.isFile()) throw new Error(`language server is missing: ${lspSource}`);
const lspContents = await readFile(lspSource);
const lspSha256 = sha256(lspContents);
const lspBytes = lspMetadata.size;
// The VSIX embeds the one multi-call native executable, which the extension
// starts with the `lsp` subcommand. It replaced three separate binaries built
// from the same crate.
const executable = platform.os === "win32" ? "oxc-tsrx.exe" : "oxc-tsrx";
const expectedVsix = {
  bundleSha256: sha256(sourceBundle),
  inventorySha256: sha256(sourceInventory),
  reportSha256: sha256(sourceReport),
  packageSha256: sha256(sourcePackage),
  extensionName: sourceManifest.name,
  publisher: sourceManifest.publisher,
  version: sourceManifest.version,
  target: platform.target,
  vscodeTarget: platform.vscodeTarget,
  nativeBinary: executable,
  nativeLspSha256: lspSha256,
  nativeLspBytes: lspBytes,
  oxcRevision: revision,
};
const rustc = await run("rustc", ["-vV"]);
if (rustHost(rustc.stdout) === platform.target) {
  const version = await run(lspSource, ["lsp", "--version"]);
  const expected = `oxc-tsrx-lsp ${sourceManifest.version} (OXC ${revision})\n`;
  if (version.stderr || version.stdout !== expected) {
    throw new Error(`unexpected language-server identity: ${version.stdout}${version.stderr}`);
  }
}

const outDirectory = resolve(root, options["out-dir"]);
await mkdir(outDirectory, { recursive: true });
const stage = await mkdtemp(join(tmpdir(), "oxc-tsrx-vscode-package-"));
try {
  await cp(source, stage, { recursive: true });
  const nativeDirectory = join(stage, "dist/native");
  await mkdir(nativeDirectory, { recursive: true });
  const lspDestination = join(nativeDirectory, executable);
  await copyFile(lspSource, lspDestination);
  if (platform.os !== "win32") await chmod(lspDestination, 0o755);
  const stagedLsp = await readFile(lspDestination);
  if (sha256(stagedLsp) !== lspSha256 || stagedLsp.length !== lspBytes) {
    throw new Error("staged language server does not match the source binary");
  }
  const manifest = {
    schemaVersion: 1,
    extensionVersion: sourceManifest.version,
    target: platform.target,
    vscodeTarget: platform.vscodeTarget,
    binary: executable,
    bytes: lspBytes,
    sha256: lspSha256,
    oxcRevision: revision,
    rustc: rustc.stdout.trim(),
  };
  await Promise.all([
    writeFile(join(nativeDirectory, "manifest.json"), `${JSON.stringify(manifest, null, 2)}\n`),
    copyFile(join(root, "LICENSE"), join(nativeDirectory, "LICENSE")),
    copyFile(join(root, "THIRD_PARTY_NOTICES.md"), join(nativeDirectory, "THIRD_PARTY_NOTICES.md")),
    cp(join(root, "licenses"), join(nativeDirectory, "licenses"), { recursive: true }),
  ]);
  const vsix = join(
    outDirectory,
    `oxc-tsrx-vscode-${sourceManifest.version}-${platform.vscodeTarget}.vsix`,
  );
  const candidate = join(outDirectory, `.candidate-${process.pid}-${Date.now()}-${basename(vsix)}`);
  await Promise.all([rm(vsix, { force: true }), rm(candidate, { force: true })]);
  let vsixVerification;
  try {
    const invocation = resolveVsceInvocation([
      "package",
      "--target",
      platform.vscodeTarget,
      "--no-dependencies",
      "--out",
      candidate,
    ]);
    await run(invocation.executable, invocation.args, { cwd: stage });
    vsixVerification = await verifyAndPromoteVsix(candidate, vsix, expectedVsix);
  } catch (error) {
    await Promise.all([rm(candidate, { force: true }), rm(vsix, { force: true })]);
    throw error;
  }
  process.stdout.write(
    `${JSON.stringify({
      extensionId: `${sourceManifest.publisher}.${sourceManifest.name}`,
      version: sourceManifest.version,
      target: platform.target,
      vscodeTarget: platform.vscodeTarget,
      vsix,
      lspSha256,
      lspBytes,
      vsixVerification,
      bytes: (await stat(vsix)).size,
    })}\n`,
  );
} finally {
  await rm(stage, { recursive: true, force: true });
}
