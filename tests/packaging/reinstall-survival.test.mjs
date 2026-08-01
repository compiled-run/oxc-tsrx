import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import {
  chmod,
  lstat,
  mkdir,
  readFile,
  realpath,
  rm,
  stat as statPath,
  writeFile,
} from "node:fs/promises";
import { createRequire } from "node:module";
import { basename, dirname, isAbsolute, join, relative, resolve, sep } from "node:path";
import test from "node:test";
import { pathToFileURL } from "node:url";
import { parseNpmPackResponse } from "../helpers/npm-pack-response.mjs";
import { startLocalRegistry } from "./local-registry.mjs";
import { temporaryDirectory } from "./temporary-directory.mjs";

/**
 * "If it doesn't survive install it's useless."
 *
 * This file is that sentence as a test. A consumer installs this package, runs
 * `setup` exactly once, and then does the ordinary things people do to a
 * dependency graph: add something, remove something, install again, install
 * again from the frozen lockfile, rebuild. After every one of those, with
 * `setup` never run again, the editor must still resolve `.tsrx` linting to a
 * binary inside `node_modules/oxc-tsrx`.
 *
 * The one mechanism this test is forbidden from leaning on is
 * `node_modules/.bin/oxlint`. pnpm 10.33 rewrites that shim on install,
 * `--frozen-lockfile`, `add`, `remove` and `rebuild`, and in a Vite+ tree it
 * does not belong to this package in the first place. So the fixture ships a
 * synthetic collider that declares `bin.oxlint` and `bin.oxfmt` and wins pnpm's
 * tie-break, exactly as Vite+ does. Every read of `.bin/oxlint` below asserts
 * that the shim is *not* ours: they are there to prove that whatever keeps
 * working, it is not the shim.
 *
 * What is left is the `oxc.path.oxlint` key in `.vscode/settings.json` plus the
 * `oxc-tsrx` dependency itself, and the oracle in
 * `packages/toolchain/dist/editor-resolution.js` is what says whether that pair
 * would really make the official OXC extension spawn our linter.
 *
 * The last subtest is the important one. It deletes the key, then rewrites the
 * key to point at the collider, and asserts that the survival check *fails*
 * both times, then restores it and asserts it passes again. A reinstall test
 * that cannot fail when the wiring breaks is worth nothing.
 */

const root = resolve(import.meta.dirname, "../..");
// Read the version from the package this test packs, the way
// `toolchain-compat.test.mjs` does, so cutting a release does not need another
// registered slot in `scripts/sync-version.ts`.
const toolchainVersion = JSON.parse(
  await readFile(resolve(root, "packages/toolchain/package.json"), "utf8"),
).version;

const npm = process.platform === "win32" ? "npm.cmd" : "npm";
const pnpm = process.platform === "win32" ? "pnpm.cmd" : "pnpm";

/**
 * Named to lose nothing to chance: it declares the same two bin names Vite+
 * declares, and it sorts after `oxc-tsrx`, which is the shape that was measured
 * to take both names under pnpm 10.33.2. If a future pnpm flips the tie-break,
 * the precondition assertion below fails loudly rather than letting the rest of
 * the file quietly prove nothing.
 */
const COLLIDER = "vite-plus-bin-collider";
const COLLIDER_VERSION = "9.9.9";
/** Something to `pnpm add` and `pnpm remove` that has no opinion about bins. */
const FILLER = "oxc-tsrx-reinstall-filler";
const FILLER_VERSION = "1.0.0";

const EDITOR_KEY = "oxc.path.oxlint";
const EDITOR_VALUE = "node_modules/oxc-tsrx/bin/oxlint";

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
  assert.equal(result.status, 0, `${executable} ${args.join(" ")}\n${result.stderr || result.stdout}`);
  return result;
}

async function pack(packageDirectory, artifacts, cache) {
  const result = await mustRun(
    npm,
    ["pack", "--json", "--pack-destination", artifacts, packageDirectory],
    { cwd: root, env: { ...process.env, npm_config_cache: cache } },
  );
  const packed = parseNpmPackResponse(result.stdout);
  const manifest = JSON.parse(
    await readFile(join(packageDirectory, "package.json"), "utf8"),
  );
  return {
    manifest,
    tarball: join(artifacts, packed.filename),
    integrity: packed.integrity,
    shasum: packed.shasum,
  };
}

function isInside(directory, candidate) {
  const offset = relative(directory, candidate);
  return offset !== "" && offset !== ".." && !offset.startsWith(`..${sep}`) && !isAbsolute(offset);
}

/**
 * The oracle takes every filesystem touch as a parameter, so a real install is
 * driven through this adapter rather than through a fixture object. `realPath`
 * is what makes a pnpm text shim readable as the binary it actually runs, and
 * `content` is only paid for on `package.json`, which is the only file the
 * oracle ever reads.
 */
async function filesystemStat(candidate) {
  let info;
  try {
    info = await statPath(candidate);
  } catch {
    return null;
  }
  const real = await realpath(candidate).catch(() => candidate);
  const content =
    info.isFile() && basename(candidate) === "package.json"
      ? await readFile(candidate, "utf8").catch(() => null)
      : null;
  return { realPath: real, content };
}

/** A single-folder VS Code window opened on the consumer directory. */
function editorWindow(consumer, configured) {
  return {
    name: "oxlint",
    configured,
    workspaceFolders: [consumer],
    packageJsonDirectories: [consumer],
    // Deliberately empty. A global install or a `PATH` hit would be a different
    // binary on a different machine, and this test is about what the consumer's
    // own tree guarantees.
    globalRoots: [],
    pathEntries: [],
    trusted: true,
    stat: filesystemStat,
  };
}

async function readSettings(consumer) {
  return readFile(join(consumer, ".vscode/settings.json"), "utf8");
}

async function writeSettings(consumer, text) {
  await mkdir(join(consumer, ".vscode"), { recursive: true });
  await writeFile(join(consumer, ".vscode/settings.json"), text);
}

/**
 * Whatever the package manager last wrote into `node_modules/.bin/oxlint`,
 * described without any claim that it matters. Used twice: once to prove the
 * collider really took the name, and once per mutation to prove it still has
 * it, so a green run can never be explained by the shim quietly becoming ours.
 */
async function describeLinterShim(consumer, providerReal) {
  const shim = join(consumer, "node_modules/.bin/oxlint");
  const info = await lstat(shim).catch(() => null);
  if (!info) return { present: false, ours: false, detail: "absent" };
  const target = await realpath(shim).catch(() => null);
  if (target && isInside(providerReal, target)) {
    return { present: true, ours: true, detail: `symlink -> ${target}` };
  }
  const source = info.isFile() && !info.isSymbolicLink()
    ? await readFile(shim, "utf8").catch(() => "")
    : "";
  if (/oxc-tsrx[\\/]bin[\\/]oxlint/u.test(source)) {
    return { present: true, ours: true, detail: "text shim naming oxc-tsrx" };
  }
  return { present: true, ours: false, detail: target ?? "text shim naming another package" };
}

test(
  "the editor wiring survives pnpm add, remove, install, --frozen-lockfile and rebuild without re-running setup",
  { timeout: 900_000 },
  async (context) => {
    assert.equal(
      spawnSync(pnpm, ["--version"], { stdio: "ignore" }).status,
      0,
      "this suite is about what pnpm does to an installed tree, so pnpm is required rather than skipped",
    );

    const temporary = await temporaryDirectory("oxc-tsrx-reinstall-");
    const artifacts = join(temporary, "artifacts");
    const cache = join(temporary, "pack-cache");
    await mkdir(artifacts, { recursive: true });

    // A synthetic Vite+: it publishes the two bin names Vite+ publishes, and
    // both of its shims fail loudly, so a `.tsrx` invocation that ever reached
    // one of them would be unmistakable rather than merely diagnostic-free.
    const colliderSource = join(temporary, "sources", COLLIDER);
    await mkdir(join(colliderSource, "bin"), { recursive: true });
    await writeFile(
      join(colliderSource, "package.json"),
      `${JSON.stringify(
        {
          name: COLLIDER,
          version: COLLIDER_VERSION,
          description: "Stands in for Vite+, which owns node_modules/.bin/oxlint in a real scaffold",
          type: "module",
          bin: { oxlint: "./bin/oxlint", oxfmt: "./bin/oxfmt" },
          files: ["bin"],
          license: "MIT",
        },
        null,
        2,
      )}\n`,
    );
    for (const name of ["oxlint", "oxfmt"]) {
      const executable = join(colliderSource, "bin", name);
      await writeFile(
        executable,
        `#!/usr/bin/env node\nconsole.error("${COLLIDER} owns ${name}; it knows nothing about .tsrx");\nprocess.exit(3);\n`,
      );
      await chmod(executable, 0o755);
    }

    const fillerSource = join(temporary, "sources", FILLER);
    await mkdir(fillerSource, { recursive: true });
    await writeFile(
      join(fillerSource, "package.json"),
      `${JSON.stringify(
        {
          name: FILLER,
          version: FILLER_VERSION,
          description: "Something to add and remove so the dependency graph really moves",
          type: "module",
          main: "./index.js",
          files: ["index.js"],
          license: "MIT",
        },
        null,
        2,
      )}\n`,
    );
    await writeFile(join(fillerSource, "index.js"), "export const filler = true;\n");

    let registry;
    try {
      const packages = [];
      for (const directory of [resolve(root, "packages/toolchain"), colliderSource, fillerSource]) {
        packages.push(await pack(directory, artifacts, cache));
      }
      registry = await startLocalRegistry(packages);

      const consumer = join(temporary, "consumer");
      await mkdir(consumer, { recursive: true });
      await writeFile(
        join(consumer, "package.json"),
        `${JSON.stringify(
          {
            name: "oxc-tsrx-reinstall-consumer",
            private: true,
            type: "module",
            devDependencies: {
              "oxc-tsrx": toolchainVersion,
              [COLLIDER]: COLLIDER_VERSION,
            },
          },
          null,
          2,
        )}\n`,
      );

      const environment = {
        ...process.env,
        NO_COLOR: "1",
        npm_config_cache: join(temporary, "npm-cache"),
        npm_config_registry: registry.url,
        XDG_CACHE_HOME: join(temporary, "xdg-cache"),
        XDG_DATA_HOME: join(temporary, "xdg-data"),
        XDG_STATE_HOME: join(temporary, "xdg-state"),
        PNPM_HOME: join(temporary, "pnpm-home"),
      };
      delete environment.NODE_PATH;
      for (const key of Object.keys(environment)) {
        if (key.startsWith("OXC_TSRX_") || key.startsWith("OXLINT_TSGOLINT")) {
          delete environment[key];
        }
      }

      // `pnpm remove` and `pnpm rebuild` reject `--registry` and
      // `--ignore-scripts` outright, so the registry is carried by
      // `npm_config_registry` in the environment and only the commands that
      // accept the flags are given them.
      const installFlags = ["--ignore-scripts", `--registry=${registry.url}`];
      const inConsumer = { cwd: consumer, env: environment };

      await mustRun(pnpm, ["install", "--no-frozen-lockfile", ...installFlags], inConsumer);

      const cli = join(consumer, "node_modules/oxc-tsrx/bin/oxc-tsrx");
      const providerReal = await realpath(join(consumer, "node_modules/oxc-tsrx"));

      // The oracle is loaded out of the *installed* package rather than out of
      // the repo, so a run that passes also proves the module is in the tarball
      // the consumer actually received.
      const installedManifest = createRequire(join(consumer, "package.json")).resolve(
        "oxc-tsrx/package.json",
      );
      const { resolveEditorLinter } = await import(
        pathToFileURL(join(dirname(installedManifest), "dist/editor-resolution.js")).href
      );

      // --- the precondition, before a single survival claim ------------------
      // Without this the whole file is theatre: if the collider does not take
      // `node_modules/.bin/oxlint`, then `.bin` ownership was never taken away
      // and "the setting survived" proves nothing about the setting.
      const initialShim = await describeLinterShim(consumer, providerReal);
      assert.equal(
        initialShim.present && !initialShim.ours,
        true,
        `${COLLIDER} must take node_modules/.bin/oxlint under pnpm, otherwise this test proves nothing: ${JSON.stringify(initialShim)}`,
      );
      const initialAutoDetect = await resolveEditorLinter(editorWindow(consumer, null));
      assert.equal(initialAutoDetect.source, "workspace-node-modules");
      assert.equal(
        initialAutoDetect.tsrxAware,
        false,
        "the editor's own lookup must land on the collider before setup runs, which is the failure this package exists to fix",
      );

      const version = await mustRun(process.execPath, [cli, "--version"], inConsumer);
      assert.equal(version.stdout, `oxc-tsrx ${toolchainVersion}\n`);

      // --- setup, exactly once ----------------------------------------------
      const setupReport = JSON.parse(
        (await mustRun(process.execPath, [cli, "setup", "--json"], inConsumer)).stdout,
      );
      assert.equal(setupReport.editorSlot.linterShim.owner, "other");
      assert.equal(setupReport.editorSlot.state, "active");
      assert.ok(setupReport.changed.includes(EDITOR_KEY));
      assert.deepEqual(JSON.parse(await readSettings(consumer)), { [EDITOR_KEY]: EDITOR_VALUE });

      /**
       * Everything a green run is allowed to mean. Throws an `AssertionError` on
       * the first thing that is no longer true, which is what makes the negative
       * subtest at the bottom able to observe a real failure.
       */
      async function assertWiringHolds(label) {
        // (d) The shim is still not ours. Nothing below may be explained by it.
        const shim = await describeLinterShim(consumer, providerReal);
        assert.equal(
          shim.ours,
          false,
          `${label}: node_modules/.bin/oxlint became ours, so this run would not prove the setting survived (${JSON.stringify(shim)})`,
        );

        const configured = JSON.parse(await readSettings(consumer))[EDITOR_KEY] ?? null;

        // (a) The oracle, replaying the official extension's lookup, lands
        // inside node_modules/oxc-tsrx and on a package that claims `.tsrx`.
        const resolution = await resolveEditorLinter(editorWindow(consumer, configured));
        assert.equal(
          resolution.source,
          "configured",
          `${label}: the extension resolved through "${resolution.source}" instead of reading ${EDITOR_KEY}`,
        );
        assert.equal(
          resolution.reason,
          "resolved",
          `${label}: the configured value did not resolve (${resolution.reason}), and a configured value has no fallback`,
        );
        assert.equal(
          resolution.path,
          join(consumer, "node_modules/oxc-tsrx/bin/oxlint"),
          `${label}: the editor would spawn ${resolution.path}`,
        );
        assert.equal(
          isInside(providerReal, resolution.realPath),
          true,
          `${label}: resolved to ${resolution.realPath}, which is outside ${providerReal}`,
        );
        assert.equal(
          resolution.tsrxAware,
          true,
          `${label}: the editor would spawn ${resolution.realPath}, which does not claim .tsrx`,
        );
        if (process.platform !== "win32") {
          // N1: on win32 this same value is extensionless with a `native`
          // loader, which cmd.exe cannot execute. That is T004's report to make,
          // not a survival question, and the CI lane for this file is Linux.
          assert.equal(resolution.spawnable, true, label);
        }

        // (b) The file the editor would spawn exists and runs.
        assert.equal(
          (await statPath(resolution.path).catch(() => null))?.isFile(),
          true,
          `${label}: ${resolution.path} is not a file`,
        );
        const executed = await run(process.execPath, [resolution.path, "--version"], inConsumer);
        assert.equal(
          executed.status,
          0,
          `${label}: node ${resolution.path} --version exited ${executed.status}\n${executed.stderr}`,
        );
        assert.match(executed.stdout, /\d+\.\d+\.\d+/u, label);

        // (c) The package's own status agrees, without setup being re-run.
        const status = JSON.parse(
          (await mustRun(process.execPath, [cli, "status", "--json"], inConsumer)).stdout,
        );
        assert.equal(status.editorSlot.state, "active", `${label}: ${status.editorSlot.state}`);
        assert.equal(status.editorSlot.currentValue, EDITOR_VALUE, label);
        assert.equal(status.editorSlot.linterShim.owner, "other", label);
        assert.equal(status.providerVersion, toolchainVersion, label);

        return { resolution, status };
      }

      await context.test("immediately after setup", async () => {
        await assertWiringHolds("after setup");
      });

      // --- the mutations ------------------------------------------------------
      // Every one of these was measured to rewrite `node_modules/.bin/oxlint`
      // under pnpm 10.33, and `pnpm add` and `pnpm remove` additionally do not
      // fire a consumer `prepare` script, so nothing gets a chance to repair
      // itself between them.
      const mutations = [
        { label: "pnpm add", args: ["add", "-D", `${FILLER}@${FILLER_VERSION}`, ...installFlags] },
        { label: "pnpm remove", args: ["remove", FILLER] },
        { label: "pnpm install", args: ["install", ...installFlags] },
        { label: "pnpm install --frozen-lockfile", args: ["install", "--frozen-lockfile", ...installFlags] },
        { label: "pnpm rebuild", args: ["rebuild"] },
      ];

      for (const mutation of mutations) {
        await context.test(mutation.label, async () => {
          await mustRun(pnpm, mutation.args, inConsumer);
          // setup is never run again. The only things that carried across this
          // command are the settings key and the dependency itself.
          await assertWiringHolds(mutation.label);
        });
      }

      // --- is this test load-bearing? -----------------------------------------
      // A survival test that stays green when the wiring is destroyed is worse
      // than no test, because it certifies the thing it stopped checking. Each
      // case breaks the mechanism a different way, asserts that the check above
      // reports it, and then restores the file and re-asserts green so the
      // failure is attributable to the break and not to a consumer left in
      // rubble by the previous case.
      await context.test("the survival check actually fails when the wiring breaks", async () => {
        const intact = await readSettings(consumer);

        const breakages = [
          {
            label: "the key is deleted",
            settings: `${JSON.stringify({ "editor.tabSize": 2 }, null, 2)}\n`,
            // No key means auto-detection, and auto-detection is the collider.
            expected: /resolved through "workspace-node-modules"/u,
          },
          {
            label: "the key is rewritten to the collider's shim",
            // The exact thing `docs/guide/getting-started.md:213` claims is ours.
            settings: `${JSON.stringify({ [EDITOR_KEY]: "node_modules/.bin/oxlint" }, null, 2)}\n`,
            expected: /would spawn .*node_modules[\\/]\.bin[\\/]oxlint\b/u,
          },
          {
            label: "the key points at a path that no longer exists",
            settings: `${JSON.stringify({ [EDITOR_KEY]: "node_modules/oxc-tsrx/bin/oxlint-gone" }, null, 2)}\n`,
            expected: /did not resolve \(configured-missing\)/u,
          },
        ];

        for (const breakage of breakages) {
          await writeSettings(consumer, breakage.settings);
          let failure = null;
          try {
            await assertWiringHolds(breakage.label);
          } catch (error) {
            failure = error;
          }
          assert.ok(
            failure,
            `${breakage.label}: the survival check passed anyway, so it is not load-bearing`,
          );
          assert.equal(
            failure.name,
            "AssertionError",
            `${breakage.label}: expected an assertion failure, got ${failure.stack}`,
          );
          // Not just "it failed": it failed *for this reason*. Otherwise a
          // broken harness would read as a load-bearing test.
          assert.match(failure.message, breakage.expected, breakage.label);
        }

        await writeSettings(consumer, intact);
        await assertWiringHolds("restored");
      });
    } finally {
      await registry?.close();
      await rm(temporary, { recursive: true, force: true });
    }
  },
);
