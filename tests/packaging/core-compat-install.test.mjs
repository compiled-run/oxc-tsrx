import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtemp, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, relative, resolve } from "node:path";
import test from "node:test";
import { parseNpmPackResponse } from "../helpers/npm-pack-response.mjs";
import { resolveNpmInvocation } from "../helpers/npm-invocation.mjs";
import { scriptNode } from "../helpers/script-node.mjs";
import { startLocalRegistry } from "./local-registry.mjs";

const root = resolve(import.meta.dirname, "../..");

// Spawning `npm.cmd` directly EINVALs on current Windows Node; every npm call
// goes through npm's manifest-declared JavaScript entry instead, exactly like
// the other packaging suites.
function runNpm(args, { cwd, env }) {
  const invocation = resolveNpmInvocation(args, { env, cwd });
  return mustRun(invocation.executable, invocation.args, { cwd, env });
}

function hostTarget() {
  if (process.platform === "darwin") {
    return `${process.arch === "arm64" ? "aarch64" : "x86_64"}-apple-darwin`;
  }
  if (process.platform === "win32") {
    return `${process.arch === "arm64" ? "aarch64" : "x86_64"}-pc-windows-msvc`;
  }
  if (process.platform === "linux" && ["arm64", "x64"].includes(process.arch)) {
    const architecture = process.arch === "arm64" ? "aarch64" : "x86_64";
    const libc = process.report?.getReport?.().header?.glibcVersionRuntime ? "gnu" : "musl";
    return `${architecture}-unknown-linux-${libc}`;
  }
  throw new Error(`unsupported core-compat install host ${process.platform}-${process.arch}`);
}

function run(executable, args, options = {}) {
  return new Promise((resolveRun, rejectRun) => {
    const child = spawn(executable, args, {
      cwd: options.cwd,
      env: options.env ?? process.env,
      stdio: ["ignore", "pipe", "pipe"],
    });
    let stdout = "";
    let stderr = "";
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => (stdout += chunk));
    child.stderr.on("data", (chunk) => (stderr += chunk));
    child.on("error", rejectRun);
    child.on("close", (status, signal) => resolveRun({ status, signal, stdout, stderr }));
  });
}

async function mustRun(executable, args, options = {}) {
  const result = await run(executable, args, options);
  assert.equal(result.status, 0, result.stderr || result.stdout);
  return result;
}

async function pack(packagePath, artifacts, cache) {
  const result = await runNpm(
    ["pack", "--json", "--pack-destination", artifacts, resolve(root, packagePath)],
    { cwd: root, env: { ...process.env, npm_config_cache: cache } },
  );
  const packed = parseNpmPackResponse(result.stdout);
  return {
    ...packed,
    manifest: JSON.parse(await readFile(join(root, packagePath, "package.json"), "utf8")),
    tarball: join(artifacts, packed.filename),
  };
}

async function inventory(directory) {
  const files = [];
  async function visit(current) {
    for (const entry of await readdir(current, { withFileTypes: true })) {
      const path = join(current, entry.name);
      if (entry.isDirectory()) await visit(path);
      else if (entry.isFile()) files.push(relative(directory, path).replaceAll("\\", "/"));
    }
  }
  await visit(directory);
  return files.sort();
}

test("the packed core-compat package works from an outside consumer", async (context) => {
  const artifacts = await mkdtemp(join(tmpdir(), "oxc-tsrx-core-compat-artifacts-"));
  const consumer = await mkdtemp(join(tmpdir(), "oxc-tsrx-core-compat-consumer-"));
  const cache = join(artifacts, ".npm-cache");
  context.after(() => rm(artifacts, { recursive: true, force: true }));
  context.after(() => rm(consumer, { recursive: true, force: true }));

  const [compatPackage, toolchainPackage] = await Promise.all([
    pack("packages/tsrx-core-compat", artifacts, cache),
    pack("packages/toolchain", artifacts, cache),
  ]);
  const nativeResult = await mustRun(
    scriptNode(),
    [
      "scripts/package-native.ts",
      "--target",
      hostTarget(),
      "--bin-dir",
      "target/release",
      "--parser-addon",
      "packages/toolchain/parser.node",
      "--out-dir",
      artifacts,
    ],
    { cwd: root, env: { ...process.env, npm_config_cache: cache } },
  );
  const nativePackage = JSON.parse(nativeResult.stdout);
  const registry = await startLocalRegistry([
    compatPackage,
    toolchainPackage,
    {
      manifest: { name: nativePackage.packageName, version: nativePackage.version },
      tarball: nativePackage.tarball,
      integrity: nativePackage.integrity,
      shasum: nativePackage.shasum,
    },
  ]);
  context.after(() => registry.close());

  await writeFile(
    join(consumer, "package.json"),
    `${JSON.stringify(
      {
        name: "core-compat-outside-consumer",
        private: true,
        type: "module",
        dependencies: { "@oxc-tsrx/tsrx-core-compat": compatPackage.version },
      },
      null,
      2,
    )}\n`,
  );
  const environment = {
    ...process.env,
    NO_COLOR: "1",
    npm_config_cache: join(consumer, ".npm-cache"),
    npm_config_registry: registry.url,
  };
  delete environment.NODE_PATH;
  await runNpm(["install", "--ignore-scripts", "--no-audit", "--no-fund"], {
    cwd: consumer,
    env: environment,
  });

  await writeFile(
    join(consumer, "runtime.mjs"),
    `import {
  isEventAttribute,
  normalizeEventName,
  parseModule,
} from "@oxc-tsrx/tsrx-core-compat";

const program = parseModule("export const answer = 42", "consumer.ts");
console.log(JSON.stringify({
  type: program.type,
  event: isEventAttribute("onClick"),
  lowercase: isEventAttribute("onclick"),
  normalized: normalizeEventName("onClickCapture"),
}));
`,
  );
  const runtime = await mustRun(process.execPath, ["runtime.mjs"], {
    cwd: consumer,
    env: environment,
  });
  assert.deepEqual(JSON.parse(runtime.stdout), {
    type: "Program",
    event: true,
    lowercase: false,
    normalized: "click",
  });

  await writeFile(
    join(consumer, "type-contract.ts"),
    `import { parseModule } from "@oxc-tsrx/tsrx-core-compat";
import type { ParseOptions, VolarMappingsResult } from "@oxc-tsrx/tsrx-core-compat/types";
import type * as AST from "@oxc-tsrx/tsrx-core-compat/types/estree";

const options: ParseOptions = { collect: true, errors: [] };
const program: AST.Program = parseModule("export {}", "consumer.ts", options);
const result: VolarMappingsResult | undefined = undefined;
void program;
void result;
`,
  );
  await writeFile(
    join(consumer, "tsconfig.json"),
    `${JSON.stringify(
      {
        compilerOptions: {
          module: "NodeNext",
          moduleResolution: "NodeNext",
          target: "ES2022",
          strict: true,
          noEmit: true,
          skipLibCheck: true,
        },
        files: ["type-contract.ts"],
      },
      null,
      2,
    )}\n`,
  );
  await mustRun(
    process.execPath,
    [join(root, "node_modules/typescript/bin/tsc"), "--noEmit", "-p", "tsconfig.json"],
    { cwd: consumer, env: environment },
  );

  const installedRoot = join(consumer, "node_modules/@oxc-tsrx/tsrx-core-compat");
  const installedManifest = JSON.parse(await readFile(join(installedRoot, "package.json"), "utf8"));
  assert.deepEqual(installedManifest.files, ["dist", "LICENSE", "README.md"]);
  const installedFiles = await inventory(installedRoot);
  assert.equal(installedFiles.some((file) => file === "src" || file.startsWith("src/")), false);
  assert.equal(
    installedFiles.some((file) => file.endsWith(".ts") && !file.endsWith(".d.ts")),
    false,
    "the package must not contain authored TypeScript sources",
  );
});
