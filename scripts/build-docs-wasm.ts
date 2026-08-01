import { execFile } from "node:child_process";
import { copyFile, readFile, rm, stat, writeFile } from "node:fs/promises";
import { dirname, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(import.meta.dirname, "..");
const tool = join(root, "docs/tools/demo-wasm");
const output = join(tool, "dist");
const lockPath = join(tool, "Cargo.lock");

if (process.argv.length > 2) {
  throw new Error(`unsupported option(s): ${process.argv.slice(2).join(", ")}`);
}

function run(executable, args) {
  return new Promise((resolveRun, rejectRun) => {
    execFile(
      executable,
      args,
      { cwd: tool, maxBuffer: 32 * 1024 * 1024 },
      (error, stdout, stderr) => {
        if (error) rejectRun(new Error(stderr || stdout, { cause: error }));
        else resolveRun({ stdout, stderr });
      },
    );
  });
}

function readU32Leb(bytes, offset) {
  let value = 0;
  let shift = 0;
  for (let index = 0; index < 5; index += 1) {
    const byte = bytes[offset + index];
    if (byte === undefined) throw new Error("truncated WebAssembly LEB128 value");
    value |= (byte & 0x7f) << shift;
    if ((byte & 0x80) === 0) return { value: value >>> 0, bytes: index + 1 };
    shift += 7;
  }
  throw new Error("oversized WebAssembly u32 LEB128 value");
}

function removeBuildIdSection(wasm) {
  if (!wasm.subarray(0, 4).equals(Buffer.from([0x00, 0x61, 0x73, 0x6d]))) {
    throw new Error("NAPI-RS output is not a WebAssembly module");
  }
  const chunks = [wasm.subarray(0, 8)];
  let removed = 0;
  let offset = 8;
  while (offset < wasm.length) {
    const sectionStart = offset;
    const sectionId = wasm[offset];
    offset += 1;
    const sectionSize = readU32Leb(wasm, offset);
    offset += sectionSize.bytes;
    const payloadStart = offset;
    const payloadEnd = payloadStart + sectionSize.value;
    if (payloadEnd > wasm.length) throw new Error("truncated WebAssembly section");
    let name = null;
    if (sectionId === 0) {
      const nameSize = readU32Leb(wasm, payloadStart);
      const nameStart = payloadStart + nameSize.bytes;
      const nameEnd = nameStart + nameSize.value;
      if (nameEnd > payloadEnd) throw new Error("truncated WebAssembly custom section name");
      name = wasm.subarray(nameStart, nameEnd).toString("utf8");
    }
    if (name === "build_id") removed += 1;
    else chunks.push(wasm.subarray(sectionStart, payloadEnd));
    offset = payloadEnd;
  }
  if (removed !== 1) {
    throw new Error(`expected one nondeterministic WebAssembly build_id section, found ${removed}`);
  }
  return Buffer.concat(chunks);
}

const manifestPath = fileURLToPath(import.meta.resolve("@napi-rs/cli/package.json"));
const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
const declaredBin = typeof manifest.bin === "string" ? manifest.bin : manifest.bin?.napi;
if (manifest.name !== "@napi-rs/cli" || typeof declaredBin !== "string") {
  throw new Error("@napi-rs/cli does not declare its public napi executable");
}
const packageRoot = dirname(manifestPath);
const cli = resolve(packageRoot, declaredBin);
const cliRelative = relative(packageRoot, cli);
if (cliRelative === "" || cliRelative === ".." || cliRelative.startsWith(`..${sep}`)) {
  throw new Error("@napi-rs/cli declared an executable outside its package");
}
if (!(await stat(cli).catch(() => null))?.isFile()) {
  throw new Error(`@napi-rs/cli executable is missing: ${cli}`);
}

const locked = await readFile(lockPath);
await rm(output, { recursive: true, force: true });
let build;
try {
  build = await run(process.execPath, [
    cli,
    "build",
    "--release",
    "--target",
    "wasm32-wasip1-threads",
    "--output-dir",
    "dist",
  ]);
} finally {
  const after = await readFile(lockPath);
  if (!after.equals(locked)) {
    await writeFile(lockPath, locked);
    throw new Error("the docs wasm build attempted to change its locked Rust dependency graph");
  }
}

const wasmPath = join(output, "demo-wasm.wasm");
const workerPath = join(output, "wasi-worker-browser.mjs");
const [wasm, worker] = await Promise.all([
  readFile(wasmPath),
  readFile(workerPath, "utf8"),
]);
if (!worker.includes("MessageHandler") || !worker.includes("instantiateNapiModule")) {
  throw new Error("NAPI-RS output is missing its browser WASI thread worker");
}
const normalizedWasm = removeBuildIdSection(wasm);
await writeFile(wasmPath, normalizedWasm);
// NAPI-RS currently emits the generic wasm filename while its generated Node
// loader references the target-qualified filename. Keep the generated output
// self-consistent so the source build can be exercised directly in Node as a
// contract test; the static site still ships only one wasm copy.
await copyFile(wasmPath, join(output, "demo-wasm.wasm32-wasi.wasm"));
const wasmMetadata = await stat(wasmPath);

process.stdout.write(build.stdout);
process.stderr.write(build.stderr);
process.stdout.write(
  `${JSON.stringify({ wasm: relative(root, wasmPath), bytes: wasmMetadata.size, worker: relative(root, workerPath) })}\n`,
);
