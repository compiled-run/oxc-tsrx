import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { constants } from "node:fs";
import { copyFile, mkdir, readFile, stat, writeFile } from "node:fs/promises";
import { dirname, isAbsolute, join, resolve } from "node:path";
import { nativeTargetForHost } from "../packages/toolchain/dist/native-targets.js";

const root = resolve(import.meta.dirname, "..");
const OXC_REVISION = "8e0ed2ebb96137fb1611cdbd5742d5cb46037d40";

function parseArguments(argv) {
  const options = {
    out: "packages/toolchain/parser.node",
    record: null,
    "target-dir": "target",
    "skip-build": false,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === "--skip-build") {
      options["skip-build"] = true;
      continue;
    }
    if (argument !== "--out" && argument !== "--record" && argument !== "--target-dir") {
      throw new Error(`unsupported option: ${argument}`);
    }
    const value = argv[++index];
    if (!value) throw new Error(`${argument} requires a value`);
    options[argument.slice(2)] = value;
  }
  return options;
}

function run(executable, args) {
  return new Promise((resolveRun, rejectRun) => {
    execFile(
      executable,
      args,
      { cwd: root, env: process.env, maxBuffer: 16 * 1024 * 1024 },
      (error, stdout, stderr) => {
        if (error) rejectRun(new Error(stderr || stdout, { cause: error }));
        else resolveRun({ stdout, stderr });
      },
    );
  });
}

function releaseLibrary() {
  if (process.platform === "darwin") return "libparser_napi_binding.dylib";
  if (process.platform === "linux") return "libparser_napi_binding.so";
  if (process.platform === "win32") return "parser_napi_binding.dll";
  throw new Error(`unsupported parser addon build host ${process.platform}-${process.arch}`);
}

function linuxLibc() {
  if (process.platform !== "linux") return undefined;
  return process.report?.getReport?.().header?.glibcVersionRuntime ? "glibc" : "musl";
}

function expectedObject(target) {
  return {
    format: { darwin: "mach-o", linux: "elf", win32: "pe" }[target.os],
    imageKind: "dynamic-library",
    bits: 64,
    architectures: [target.cpu],
    os: target.os,
    libc: target.libc ?? null,
  };
}

const options = parseArguments(process.argv.slice(2));
if (!options.out.endsWith(".node")) throw new Error("--out must end in .node");

if (!options["skip-build"]) {
  await run("cargo", [
    "build",
    "--release",
    "--locked",
    "--offline",
    "-p",
    "parser_napi_binding",
  ]);
}

const targetDirectory = isAbsolute(options["target-dir"])
  ? options["target-dir"]
  : resolve(root, options["target-dir"]);
const source = join(targetDirectory, "release", releaseLibrary());
const sourceStat = await stat(source).catch(() => null);
if (!sourceStat?.isFile()) {
  throw new Error(`release parser addon is missing: ${source}`);
}

const destination = isAbsolute(options.out) ? options.out : resolve(root, options.out);
await mkdir(dirname(destination), { recursive: true });
await copyFile(source, destination, constants.COPYFILE_FICLONE);
const contents = await readFile(destination);
const parserManifest = JSON.parse(
  await readFile(resolve(root, "packages/toolchain/package.json"), "utf8"),
);
const target = nativeTargetForHost(process.platform, process.arch, linuxLibc());
const record = {
  packageVersion: parserManifest.version,
  target: target.target,
  bytes: contents.length,
  sha256: createHash("sha256").update(contents).digest("hex"),
  object: expectedObject(target),
  nodeApi: 8,
  oxcRevision: OXC_REVISION,
  capabilities: {
    lazy: true,
    async: true,
    editorRecovery: false,
    cssMaterialization: false,
    rawTransfer: false,
  },
  role: "canonical-parser",
  file: "parser.node",
  apiVersion: 1,
  transportAbi: 1,
};
const recordDestination = options.record === null
  ? `${destination}.json`
  : isAbsolute(options.record)
    ? options.record
    : resolve(root, options.record);
await mkdir(dirname(recordDestination), { recursive: true });
await writeFile(recordDestination, `${JSON.stringify(record, null, 2)}\n`, "utf8");

process.stdout.write(
  `${JSON.stringify({
    target: target.target,
    source,
    out: destination,
    record: recordDestination,
    bytes: record.bytes,
    sha256: record.sha256,
  })}\n`,
);
