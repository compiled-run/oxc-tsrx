import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdir, mkdtemp, readFile, realpath, rm, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import test from "node:test";
import {
  nativePackageName,
  nativeTargetForHost,
} from "../../packages/toolchain/dist/native-targets.js";
import { resolveNpmInvocation } from "../../scripts/npm-invocation.mjs";
import { parseNpmPackResponse } from "../../scripts/npm-pack-response.mjs";

const root = resolve(import.meta.dirname, "../..");

// pnpm does not hoist transitive dependencies to the repository root, so
// `<root>/node_modules/@oxc-project/types` does not exist here. Spawning npm
// with a cwd that is not there fails as `spawn npm ENOENT`, which reads like a
// missing npm rather than a missing directory. Resolve it from the workspace
// package that actually declares the dependency instead.
const typesDirectory = dirname(
  createRequire(import.meta.url).resolve("@oxc-project/types/package.json", {
    paths: [join(root, "packages/toolchain")],
  }),
);

const CASES = Object.freeze([
  { id: "missing-manifest", code: "ERR_TSRX_NATIVE_INTEGRITY" },
  { id: "wrong-target", code: "ERR_TSRX_NATIVE_VERSION" },
  { id: "swapped-role", code: "ERR_TSRX_NATIVE_VERSION" },
  { id: "version-skew", code: "ERR_TSRX_NATIVE_VERSION" },
  { id: "checksum-tamper", code: "ERR_TSRX_NATIVE_INTEGRITY" },
  { id: "native-parser-mismatch", code: "ERR_TSRX_NATIVE_VERSION" },
  { id: "addon-tamper", code: "ERR_TSRX_NATIVE_INTEGRITY" },
  { id: "api-version", code: "ERR_TSRX_NATIVE_VERSION" },
  { id: "transport-abi", code: "ERR_TSRX_NATIVE_VERSION" },
  { id: "oxc-revision", code: "ERR_TSRX_NATIVE_VERSION" },
  { id: "node-api", code: "ERR_TSRX_NATIVE_VERSION" },
  { id: "capability", code: "ERR_TSRX_NATIVE_VERSION" },
  { id: "object-header", code: "ERR_TSRX_NATIVE_INTEGRITY" },
  { id: "byte-length", code: "ERR_TSRX_NATIVE_INTEGRITY" },
  { id: "extra-dependency", code: "ERR_TSRX_NATIVE_INTEGRITY" },
]);

function linuxLibc() {
  if (process.platform !== "linux") return undefined;
  return process.report?.getReport?.().header?.glibcVersionRuntime ? "glibc" : "musl";
}

function hostTarget() {
  return nativeTargetForHost(process.platform, process.arch, linuxLibc());
}

function run(executable, args, options = {}) {
  return new Promise((resolveRun, rejectRun) => {
    execFile(
      executable,
      args,
      {
        cwd: options.cwd ?? root,
        env: options.env ?? process.env,
        maxBuffer: 32 * 1024 * 1024,
      },
      (error, stdout, stderr) => {
        if (error) rejectRun(new Error(stderr || stdout, { cause: error }));
        else resolveRun({ stdout, stderr });
      },
    );
  });
}

// npm is reached the way the product reaches it: through the JavaScript entry
// its own manifest declares, run by Node. Naming `npm.cmd` on Windows is not
// only the shim this repository's boundary forbids, it cannot run at all, because
// Node refuses to spawn a `.cmd` file without `shell: true`. This suite spelled
// it that way and nothing noticed until the addon lanes first ran on a Windows
// runner and every npm call here died with `spawn EINVAL`.
function runNpm(args, options = {}) {
  const invocation = resolveNpmInvocation(args, {
    env: options.env ?? process.env,
    cwd: options.cwd ?? root,
  });
  return run(invocation.executable, invocation.args, options);
}

function runOutcome(executable, args, options = {}) {
  return new Promise((resolveRun) => {
    const child = execFile(
      executable,
      args,
      {
        cwd: options.cwd ?? root,
        env: options.env ?? process.env,
        maxBuffer: 32 * 1024 * 1024,
      },
      (error, stdout, stderr) => {
        resolveRun({
          exitCode: error?.code ?? 0,
          pid: child.pid,
          stdout,
          stderr,
        });
      },
    );
  });
}

async function writeJson(path, value) {
  await writeFile(path, `${JSON.stringify(value, null, 2)}\n`, "utf8");
}

async function observeLoader(consumer) {
  const source = String.raw`
    import { createRequire } from "node:module";
    const require = createRequire(import.meta.url);
    let parser;
    try {
      parser = await import("oxc-tsrx/parser");
      const result = parser.parseSync("entry.ts", "export const value: number = 1");
      process.stdout.write(JSON.stringify({
        ok: true,
        program: result.program.type,
        errors: result.errors.length,
        loadedAddons: Object.keys(require.cache).filter((path) => path.endsWith("parser.node")),
      }));
    } catch (error) {
      process.stdout.write(JSON.stringify({
        ok: false,
        name: error?.name,
        code: error?.code,
        message: error?.message,
        instance: typeof parser?.ParserOperationalError === "function"
          && error instanceof parser.ParserOperationalError,
        loadedAddons: Object.keys(require.cache).filter((path) => path.endsWith("parser.node")),
      }));
      process.exitCode = 1;
    }
  `;
  const environment = { ...process.env };
  delete environment.OXC_TSRX_PARSER_ADDON;
  const outcome = await runOutcome(process.execPath, ["--input-type=module", "-e", source], {
    cwd: consumer,
    env: environment,
  });
  return {
    ...JSON.parse(outcome.stdout),
    exitCode: outcome.exitCode,
    pid: outcome.pid,
    stderr: outcome.stderr,
  };
}

test("the packed parser loader rejects every frozen identity and integrity mutation", async () => {
  assert.equal(CASES.length, 15);
  assert.equal(new Set(CASES.map(({ id }) => id)).size, CASES.length);
  const temporary = await mkdtemp(join(tmpdir(), "oxc-tsrx-parser-loader-integrity-"));
  try {
    const artifacts = join(temporary, "artifacts");
    const consumer = join(temporary, "consumer");
    const npmCache = join(temporary, "npm-cache");
    await Promise.all([mkdir(artifacts), mkdir(consumer), mkdir(npmCache)]);
    const npmEnvironment = {
      ...process.env,
      npm_config_cache: npmCache,
    };
    delete npmEnvironment.OXC_TSRX_PARSER_ADDON;

    const addon = join(temporary, "parser.node");
    await run(process.execPath, [
      "scripts/build-parser-native.mjs",
      "--skip-build",
      "--out",
      addon,
    ]);
    const nativePackage = JSON.parse(
      (
        await run(process.execPath, [
          "scripts/package-native.mjs",
          "--target",
          hostTarget().target,
          "--bin-dir",
          "target/release",
          "--parser-addon",
          addon,
          "--out-dir",
          artifacts,
        ])
      ).stdout,
    );
    const parserPack = parseNpmPackResponse(
      (
        await runNpm(["pack", "--json", "--pack-destination", artifacts], {
          cwd: join(root, "packages/toolchain"),
          env: npmEnvironment,
        })
      ).stdout,
    );
    const parserTarball = join(artifacts, parserPack.filename);
    const typesPack = parseNpmPackResponse(
      (
        await runNpm(["pack", "--json", "--pack-destination", artifacts], {
          cwd: typesDirectory,
          env: npmEnvironment,
        })
      ).stdout,
    );
    const typesTarball = join(artifacts, typesPack.filename);
    await runNpm(
      [
        "install",
        "--ignore-scripts",
        "--no-audit",
        "--no-fund",
        "--package-lock=false",
        parserTarball,
        nativePackage.tarball,
        typesTarball,
      ],
      { cwd: consumer, env: npmEnvironment },
    );

    const parserRoot = join(consumer, "node_modules", "oxc-tsrx");
    const nativeRoot = join(
      consumer,
      "node_modules",
      ...nativePackageName(hostTarget()).split("/"),
    );
    const paths = {
      parserManifest: join(parserRoot, "package.json"),
      nativeManifest: join(nativeRoot, "package.json"),
      checksums: join(nativeRoot, "checksums.json"),
      addon: join(nativeRoot, "parser.node"),
    };
    const pristine = Object.fromEntries(
      await Promise.all(
        Object.entries(paths).map(async ([name, path]) => [name, await readFile(path)]),
      ),
    );
    const restore = async () => {
      await Promise.all(
        Object.entries(paths).map(([name, path]) => writeFile(path, pristine[name])),
      );
    };
    const mutateJson = async (path, apply) => {
      const value = JSON.parse(await readFile(path, "utf8"));
      apply(value);
      await writeJson(path, value);
    };

    const clean = await observeLoader(consumer);
    assert.equal(clean.exitCode, 0);
    assert.equal(clean.stderr, "");
    assert.ok(Number.isInteger(clean.pid) && clean.pid > 0);
    assert.deepEqual(
      { ok: clean.ok, program: clean.program, errors: clean.errors },
      { ok: true, program: "Program", errors: 0 },
    );
    assert.deepEqual(clean.loadedAddons, [await realpath(paths.addon)]);

    const observedPids = new Set([clean.pid]);

    for (const row of CASES) {
      await restore();
      if (row.id === "missing-manifest") await rm(paths.checksums);
      if (row.id === "wrong-target") {
        await mutateJson(paths.nativeManifest, (value) => {
          value.os = [hostTarget().os === "linux" ? "darwin" : "linux"];
          value.cpu = [hostTarget().cpu === "x64" ? "arm64" : "x64"];
        });
      }
      if (row.id === "swapped-role") {
        await mutateJson(paths.checksums, (value) => {
          value.addons["parser.node"].role = "tsrx-core-compat";
        });
      }
      if (row.id === "version-skew") {
        await mutateJson(paths.nativeManifest, (value) => {
          value.version = "0.0.0-skew";
        });
      }
      if (row.id === "checksum-tamper") {
        await mutateJson(paths.checksums, (value) => {
          value.addons["parser.node"].sha256 = "0".repeat(64);
        });
      }
      if (row.id === "native-parser-mismatch") {
        const mismatchedVersion = "0.0.0-tamper";
        await mutateJson(paths.nativeManifest, (value) => {
          value.version = mismatchedVersion;
          value.oxcTsrx.addons["parser.node"].packageVersion = mismatchedVersion;
        });
        await mutateJson(paths.checksums, (value) => {
          value.version = mismatchedVersion;
          value.addons["parser.node"].packageVersion = mismatchedVersion;
        });
      }
      if (row.id === "addon-tamper") {
        const value = Buffer.from(pristine.addon);
        value[Math.floor(value.length / 2)] ^= 1;
        await writeFile(paths.addon, value);
      }
      if (row.id === "api-version") {
        await mutateJson(paths.checksums, (value) => {
          value.addons["parser.node"].apiVersion += 1;
        });
      }
      if (row.id === "transport-abi") {
        await mutateJson(paths.checksums, (value) => {
          value.addons["parser.node"].transportAbi += 1;
        });
      }
      if (row.id === "oxc-revision") {
        await mutateJson(paths.checksums, (value) => {
          value.addons["parser.node"].oxcRevision = "0".repeat(40);
        });
      }
      if (row.id === "node-api") {
        await mutateJson(paths.checksums, (value) => {
          value.addons["parser.node"].nodeApi += 1;
        });
      }
      if (row.id === "capability") {
        await mutateJson(paths.checksums, (value) => {
          value.addons["parser.node"].capabilities.lazy = false;
        });
      }
      if (row.id === "object-header") {
        await mutateJson(paths.checksums, (value) => {
          value.addons["parser.node"].object.bits = 32;
        });
      }
      if (row.id === "byte-length") {
        await mutateJson(paths.checksums, (value) => {
          value.addons["parser.node"].bytes += 1;
        });
      }
      if (row.id === "extra-dependency") {
        await mutateJson(paths.nativeManifest, (value) => {
          value.dependencies = { "@oxc-tsrx/forbidden": value.version };
        });
      }

      const observed = await observeLoader(consumer);
      assert.equal(observed.exitCode, 1, row.id);
      assert.equal(observed.stderr, "", row.id);
      assert.ok(Number.isInteger(observed.pid) && observed.pid > 0, row.id);
      assert.equal(observedPids.has(observed.pid), false, row.id);
      observedPids.add(observed.pid);
      assert.equal(observed.ok, false, row.id);
      assert.equal(observed.name, "ParserOperationalError", row.id);
      assert.equal(observed.code, row.code, row.id);
      assert.equal(observed.instance, true, row.id);
      assert.match(observed.message, /expected/u, row.id);
      assert.match(observed.message, /actual/u, row.id);
      assert.deepEqual(observed.loadedAddons, [], row.id);
    }

    await restore();
    const restored = await observeLoader(consumer);
    assert.equal(restored.exitCode, 0);
    assert.equal(restored.stderr, "");
    assert.equal(observedPids.has(restored.pid), false);
    assert.deepEqual(
      { ok: restored.ok, program: restored.program, errors: restored.errors },
      { ok: true, program: "Program", errors: 0 },
    );
    assert.deepEqual(restored.loadedAddons, [await realpath(paths.addon)]);
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
});
