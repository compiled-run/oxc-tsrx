import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { constants } from "node:fs";
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
import { NATIVE_TARGETS, nativePackageName } from "../packages/runtime/dist/targets.js";

const root = resolve(import.meta.dirname, "..");
const npm = process.platform === "win32" ? "npm.cmd" : "npm";
const revision = "8e0ed2ebb96137fb1611cdbd5742d5cb46037d40";
const binaryStems = ["oxc-tsrx", "oxc-tsrx-fmt", "oxc-tsrx-lsp"];

function parseArguments(argv) {
  const options = {};
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (!["--target", "--bin-dir", "--out-dir"].includes(argument)) {
      throw new Error(`unsupported option: ${argument}`);
    }
    const value = argv[++index];
    if (!value) throw new Error(`${argument} requires a value`);
    options[argument.slice(2)] = value;
  }
  for (const name of ["target", "bin-dir", "out-dir"]) {
    if (!options[name]) throw new Error(`--${name} is required`);
  }
  return options;
}

function run(executable, args, options = {}) {
  return new Promise((resolveRun, rejectRun) => {
    execFile(
      executable,
      args,
      {
        cwd: options.cwd ?? root,
        env: options.env ?? process.env,
        maxBuffer: 16 * 1024 * 1024,
      },
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

function requireBytes(contents, length, format) {
  if (contents.length < length) {
    throw new Error(`invalid ${format} executable: expected at least ${length} header bytes`);
  }
}

function cpuName(value, format) {
  const architectures = {
    "mach-o": new Map([
      [0x01000007, "x64"],
      [0x0100000c, "arm64"],
    ]),
    elf: new Map([
      [62, "x64"],
      [183, "arm64"],
    ]),
    pe: new Map([
      [0x8664, "x64"],
      [0xaa64, "arm64"],
    ]),
  };
  return architectures[format].get(value) ?? `unknown-0x${value.toString(16)}`;
}

function inspectMachO(contents, magic) {
  const thin = new Map([
    ["cefaedfe", { endian: "little", bits: 32 }],
    ["cffaedfe", { endian: "little", bits: 64 }],
    ["feedface", { endian: "big", bits: 32 }],
    ["feedfacf", { endian: "big", bits: 64 }],
  ]);
  const thinHeader = thin.get(magic);
  if (thinHeader) {
    requireBytes(contents, 16, "Mach-O");
    const readU32 =
      thinHeader.endian === "little"
        ? contents.readUInt32LE.bind(contents)
        : contents.readUInt32BE.bind(contents);
    const cpu = readU32(4);
    const fileType = readU32(12);
    if (fileType !== 2) {
      throw new Error(`invalid Mach-O executable: file type ${fileType} is not MH_EXECUTE`);
    }
    return {
      format: "mach-o",
      os: "darwin",
      bits: thinHeader.bits,
      architectures: [cpuName(cpu, "mach-o")],
    };
  }

  const fat = new Map([
    ["cafebabe", { endian: "big", recordBytes: 20 }],
    ["bebafeca", { endian: "little", recordBytes: 20 }],
    ["cafebabf", { endian: "big", recordBytes: 32 }],
    ["bfbafeca", { endian: "little", recordBytes: 32 }],
  ]).get(magic);
  if (!fat) return null;
  requireBytes(contents, 8, "fat Mach-O");
  const readU32 =
    fat.endian === "little"
      ? contents.readUInt32LE.bind(contents)
      : contents.readUInt32BE.bind(contents);
  const count = readU32(4);
  if (count === 0 || count > 64) {
    throw new Error(`invalid fat Mach-O executable: architecture count ${count}`);
  }
  requireBytes(contents, 8 + count * fat.recordBytes, "fat Mach-O");
  const architectures = new Set();
  for (let index = 0; index < count; index += 1) {
    architectures.add(cpuName(readU32(8 + index * fat.recordBytes), "mach-o"));
  }
  return { format: "mach-o", os: "darwin", bits: 64, architectures: [...architectures] };
}

function inspectElf(contents) {
  if (contents.subarray(0, 4).toString("hex") !== "7f454c46") return null;
  requireBytes(contents, 20, "ELF");
  if (![1, 2].includes(contents[4])) {
    throw new Error(`invalid ELF executable: unsupported class ${contents[4]}`);
  }
  const endian = contents[5];
  if (![1, 2].includes(endian)) {
    throw new Error(`invalid ELF executable: unsupported byte order ${endian}`);
  }
  const readU16 =
    endian === 1 ? contents.readUInt16LE.bind(contents) : contents.readUInt16BE.bind(contents);
  const fileType = readU16(16);
  if (![2, 3].includes(fileType)) {
    throw new Error(`invalid ELF executable: file type ${fileType} is not executable`);
  }
  return {
    format: "elf",
    os: "linux",
    bits: contents[4] === 2 ? 64 : 32,
    architectures: [cpuName(readU16(18), "elf")],
  };
}

function inspectPe(contents) {
  if (contents.subarray(0, 2).toString("ascii") !== "MZ") return null;
  requireBytes(contents, 0x40, "PE");
  const header = contents.readUInt32LE(0x3c);
  requireBytes(contents, header + 26, "PE");
  if (contents.subarray(header, header + 4).toString("hex") !== "50450000") {
    throw new Error("invalid PE executable: missing PE signature");
  }
  const characteristics = contents.readUInt16LE(header + 22);
  if ((characteristics & 0x0002) === 0) {
    throw new Error("invalid PE executable: executable-image flag is missing");
  }
  return {
    format: "pe",
    os: "win32",
    bits: contents.readUInt16LE(header + 24) === 0x20b ? 64 : 32,
    architectures: [cpuName(contents.readUInt16LE(header + 4), "pe")],
  };
}

function inspectExecutable(contents) {
  requireBytes(contents, 4, "native");
  const magic = contents.subarray(0, 4).toString("hex");
  const identity = inspectMachO(contents, magic) ?? inspectElf(contents) ?? inspectPe(contents);
  if (!identity) throw new Error(`unsupported executable header 0x${magic}`);
  return identity;
}

function assertObjectTarget(name, identity, platform) {
  const format = { darwin: "mach-o", linux: "elf", win32: "pe" }[platform.os];
  if (
    identity.format !== format ||
    identity.os !== platform.os ||
    identity.bits !== 64 ||
    !identity.architectures.includes(platform.cpu)
  ) {
    throw new Error(
      `${name} object target mismatch: expected ${platform.target} ` +
        `(${format}/${platform.os}/${platform.cpu}), found ` +
        `${identity.format}/${identity.os}/${identity.bits}-bit/${identity.architectures.join("+")}`,
    );
  }
}

const options = parseArguments(process.argv.slice(2));
const platform = NATIVE_TARGETS.find((candidate) => candidate.target === options.target);
if (!platform) throw new Error(`unsupported Rust target: ${options.target}`);

const rootManifest = JSON.parse(await readFile(join(root, "package.json"), "utf8"));
const runtimeManifest = JSON.parse(
  await readFile(join(root, "packages/runtime/package.json"), "utf8"),
);
if (rootManifest.version !== runtimeManifest.version) {
  throw new Error(
    `root/runtime version mismatch: ${rootManifest.version} != ${runtimeManifest.version}`,
  );
}
const version = rootManifest.version;
const packageName = nativePackageName(platform);
const binDirectory = resolve(root, options["bin-dir"]);
const outDirectory = resolve(root, options["out-dir"]);
await mkdir(outDirectory, { recursive: true });
const stage = await mkdtemp(join(tmpdir(), "oxc-tsrx-native-package-"));

try {
  const rustc = await run("rustc", ["-vV"]);
  const executableSuffix = platform.os === "win32" ? ".exe" : "";
  const binaries = {};
  await mkdir(join(stage, "bin"), { recursive: true });
  for (const stem of binaryStems) {
    const name = `${stem}${executableSuffix}`;
    const source = join(binDirectory, name);
    const metadata = await stat(source).catch(() => null);
    if (!metadata?.isFile()) {
      throw new Error(`required release binary is missing: ${source}`);
    }
    const destination = join(stage, "bin", name);
    await copyFile(source, destination, constants.COPYFILE_FICLONE);
    if (platform.os !== "win32") await chmod(destination, 0o755);
    const staged = await stat(destination);
    const contents = await readFile(destination);
    const object = inspectExecutable(contents);
    assertObjectTarget(name, object, platform);
    binaries[name] = {
      sha256: sha256(contents),
      bytes: staged.size,
      object,
    };
  }

  const host = rustHost(rustc.stdout);
  if (host === platform.target) {
    for (const stem of binaryStems) {
      const name = `${stem}${executableSuffix}`;
      const { stdout, stderr } = await run(join(stage, "bin", name), ["--version"]);
      if (stderr || stdout !== `${stem} ${version} (OXC ${revision})\n`) {
        throw new Error(`unexpected ${name} version identity: ${stdout}${stderr}`);
      }
    }
  }

  const manifest = {
    name: packageName,
    version,
    description: `OXC for TSRX native binaries for ${platform.target}`,
    license: "MIT",
    repository: {
      type: "git",
      url: "git+https://github.com/thejackshelton/oxc-tsrx.git",
      directory: "packages/native",
    },
    homepage: "https://github.com/thejackshelton/oxc-tsrx#readme",
    bugs: { url: "https://github.com/thejackshelton/oxc-tsrx/issues" },
    keywords: ["oxc", "oxlint", "oxfmt", "tsrx", "native"],
    files: ["bin", "checksums.json", "licenses", "LICENSE", "README.md", "THIRD_PARTY_NOTICES.md"],
    os: [platform.os],
    cpu: [platform.cpu],
    ...(platform.libc ? { libc: [platform.libc] } : {}),
    engines: { node: "^20.19.0 || >=22.12.0" },
    preferUnplugged: true,
    publishConfig: { access: "public", provenance: true },
    oxcTsrx: {
      schemaVersion: 1,
      nativeProtocolVersion: 1,
      target: platform.target,
      vscodeTarget: platform.vscodeTarget,
      oxcRevision: revision,
      binaries: binaryStems.map((stem) => `${stem}${executableSuffix}`),
    },
  };
  const checksums = {
    schemaVersion: 1,
    packageName,
    version,
    target: platform.target,
    oxcRevision: revision,
    rustc: rustc.stdout.trim(),
    objectVerification: "executable-header",
    verification: host === platform.target ? "host-executed" : "cross-artifact",
    binaries,
  };
  await Promise.all([
    writeFile(join(stage, "package.json"), `${JSON.stringify(manifest, null, 2)}\n`),
    writeFile(join(stage, "checksums.json"), `${JSON.stringify(checksums, null, 2)}\n`),
    copyFile(join(root, "LICENSE"), join(stage, "LICENSE")),
    copyFile(join(root, "THIRD_PARTY_NOTICES.md"), join(stage, "THIRD_PARTY_NOTICES.md")),
    copyFile(join(root, "packages/native/README.md"), join(stage, "README.md")),
    cp(join(root, "licenses"), join(stage, "licenses"), { recursive: true }),
  ]);

  const { stdout } = await run(npm, ["pack", "--json", "--pack-destination", outDirectory], {
    cwd: stage,
    env: { ...process.env, npm_config_cache: join(stage, ".npm-cache") },
  });
  const packed = JSON.parse(stdout);
  if (!Array.isArray(packed) || packed.length !== 1) {
    throw new Error(`unexpected npm pack response: ${stdout}`);
  }
  const tarball = join(outDirectory, packed[0].filename);
  process.stdout.write(
    `${JSON.stringify({
      packageName,
      version,
      target: platform.target,
      vscodeTarget: platform.vscodeTarget,
      tarball,
      filename: basename(tarball),
      integrity: packed[0].integrity,
      shasum: packed[0].shasum,
      unpackedSize: packed[0].unpackedSize,
    })}\n`,
  );
} finally {
  await rm(stage, { recursive: true, force: true });
}
