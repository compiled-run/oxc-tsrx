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
import { join, resolve } from "node:path";
import { NATIVE_TARGETS } from "../packages/runtime/dist/targets.js";

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

const options = parseArguments(process.argv.slice(2));
const platform = NATIVE_TARGETS.find((candidate) => candidate.target === options.target);
if (!platform) throw new Error(`unsupported Rust target: ${options.target}`);
const source = join(root, "packages/vscode");
const sourceManifest = JSON.parse(await readFile(join(source, "package.json"), "utf8"));
const lspSource = resolve(root, options["lsp-bin"]);
const lspMetadata = await stat(lspSource).catch(() => null);
if (!lspMetadata?.isFile()) throw new Error(`language server is missing: ${lspSource}`);
const executable = platform.os === "win32" ? "oxc-tsrx-lsp.exe" : "oxc-tsrx-lsp";
const rustc = await run("rustc", ["-vV"]);
if (rustHost(rustc.stdout) === platform.target) {
  const version = await run(lspSource, ["--version"]);
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
  const sha256 = createHash("sha256").update(await readFile(lspDestination)).digest("hex");
  const manifest = {
    schemaVersion: 1,
    extensionVersion: sourceManifest.version,
    target: platform.target,
    vscodeTarget: platform.vscodeTarget,
    binary: executable,
    bytes: (await stat(lspDestination)).size,
    sha256,
    oxcRevision: revision,
    rustc: rustc.stdout.trim(),
  };
  await Promise.all([
    writeFile(join(nativeDirectory, "manifest.json"), `${JSON.stringify(manifest, null, 2)}\n`),
    copyFile(join(root, "LICENSE"), join(nativeDirectory, "LICENSE")),
    copyFile(
      join(root, "THIRD_PARTY_NOTICES.md"),
      join(nativeDirectory, "THIRD_PARTY_NOTICES.md"),
    ),
    cp(join(root, "licenses"), join(nativeDirectory, "licenses"), { recursive: true }),
  ]);
  const vsix = join(
    outDirectory,
    `oxc-tsrx-vscode-${sourceManifest.version}-${platform.vscodeTarget}.vsix`,
  );
  const vsce = join(root, "node_modules/.bin", process.platform === "win32" ? "vsce.cmd" : "vsce");
  await run(
    vsce,
    [
      "package",
      "--target",
      platform.vscodeTarget,
      "--no-dependencies",
      "--out",
      vsix,
    ],
    { cwd: stage },
  );
  process.stdout.write(
    `${JSON.stringify({
      extensionId: `${sourceManifest.publisher}.${sourceManifest.name}`,
      version: sourceManifest.version,
      target: platform.target,
      vscodeTarget: platform.vscodeTarget,
      vsix,
      lspSha256: sha256,
      bytes: (await stat(vsix)).size,
    })}\n`,
  );
} finally {
  await rm(stage, { recursive: true, force: true });
}
