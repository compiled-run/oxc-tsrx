import assert from "node:assert/strict";
import test from "node:test";
import {
  configuredLoader,
  createFixtureStat,
  isSpawnable,
  rejectConfiguredValue,
  resolveEditorLinter,
} from "../../packages/toolchain/dist/editor-resolution.js";

/**
 * These are the topologies a consumer actually ends up in, replayed against the
 * lookup chain decompiled from `oxc.oxc-vscode` 1.59.0. Each one answers a
 * question that no amount of "it works on my machine" can: given this tree and
 * this window, which binary does the editor start, and does that binary know
 * what a `.tsrx` file is?
 *
 * The value under test is never a claim about our own code. It is a claim about
 * someone else's resolution order, so every assertion below traces to a line in
 * `out/main.js`, and the fixtures are the shapes that line reacts to.
 */

const OUR_MANIFEST = JSON.stringify({
  name: "oxc-tsrx",
  version: "0.1.5",
  bin: { oxlint: "./bin/oxlint", oxfmt: "./bin/oxfmt" },
  oxc: {
    provider: {
      protocol: 1,
      id: "tsrx",
      languages: [
        {
          id: "tsrx",
          extensions: [".tsrx"],
          capabilities: { lint: { bin: "oxc-tsrx-lint" } },
        },
      ],
    },
  },
});

const VITE_PLUS_MANIFEST = JSON.stringify({
  name: "vite-plus",
  version: "1.0.0",
  bin: { oxlint: "./bin/oxlint", oxfmt: "./bin/oxfmt" },
});

const OXLINT_MANIFEST = JSON.stringify({
  name: "oxlint",
  version: "1.59.0",
  bin: { oxlint: "./bin/oxlint" },
  main: "./dist/index.js",
});

const CONSUMER_MANIFEST = JSON.stringify({
  name: "app",
  dependencies: { "oxc-tsrx": "^0.1.5", "vite-plus": "^1.0.0" },
});

/**
 * What pnpm writes into `node_modules/.bin` on POSIX: a real file, with real
 * contents, that hands off to somebody else's binary. It stats exactly like the
 * shim we want, which is the entire problem.
 */
function pnpmShim(target) {
  return {
    content: `#!/bin/sh\nbasedir=$(dirname "$0")\nexec "$basedir/${target}" "$@"\n`,
    runs: `/w/node_modules/${target.replace(/^\.\.\//u, "")}`,
  };
}

const EXECUTABLE = "#!/usr/bin/env node\n";

function resolve(options) {
  return resolveEditorLinter({ name: "oxlint", platform: "linux", ...options });
}

// --- 1. pnpm plus Vite+: the collision that started this ----------------------

test("a pnpm tree hands the editor Vite+'s oxlint, and only the setting takes it back", async () => {
  const stat = createFixtureStat({
    "/w/package.json": CONSUMER_MANIFEST,
    "/w/node_modules/.bin/oxlint": pnpmShim("../vite-plus/bin/oxlint"),
    "/w/node_modules/.bin/oxfmt": pnpmShim("../vite-plus/bin/oxfmt"),
    "/w/node_modules/vite-plus/package.json": VITE_PLUS_MANIFEST,
    "/w/node_modules/vite-plus/bin/oxlint": EXECUTABLE,
    "/w/node_modules/oxc-tsrx/package.json": OUR_MANIFEST,
    "/w/node_modules/oxc-tsrx/bin/oxlint": EXECUTABLE,
  });

  const detected = await resolve({ workspaceFolders: ["/w"], stat });
  assert.deepEqual(detected, {
    source: "workspace-node-modules",
    path: "/w/node_modules/.bin/oxlint",
    loader: "native",
    spawnable: true,
    tsrxAware: false,
    reason: "resolved",
    realPath: "/w/node_modules/vite-plus/bin/oxlint",
    attempted: "/w/node_modules/.bin/oxlint",
  });

  // Resolution "succeeded". A test that only checked for a path would be green
  // here, and the consumer would still have a silent editor.
  const configured = await resolve({
    workspaceFolders: ["/w"],
    configured: "node_modules/oxc-tsrx/bin/oxlint",
    stat,
  });
  assert.equal(configured.source, "configured");
  assert.equal(configured.path, "/w/node_modules/oxc-tsrx/bin/oxlint");
  assert.equal(configured.loader, "native");
  assert.equal(configured.tsrxAware, true);
});

// --- 2. npm: the name can go nowhere at all -----------------------------------

test("an npm tree with no .bin/oxlint at all falls through the entire chain", async () => {
  const stat = createFixtureStat({
    "/w/package.json": CONSUMER_MANIFEST,
    // The measured npm tree: `.bin/oxfmt` belongs to the third-party formatter
    // and `.bin/oxlint` is simply not there.
    "/w/node_modules/.bin/oxfmt": { link: "../oxfmt/bin/oxfmt" },
    "/w/node_modules/oxfmt/package.json": JSON.stringify({ name: "oxfmt", bin: "./bin/oxfmt" }),
    "/w/node_modules/oxfmt/bin/oxfmt": EXECUTABLE,
    "/w/node_modules/oxc-tsrx/package.json": OUR_MANIFEST,
    "/w/node_modules/oxc-tsrx/bin/oxlint": EXECUTABLE,
  });

  const result = await resolve({
    workspaceFolders: ["/w"],
    packageJsonDirectories: ["/w"],
    requireResolve: () => {
      throw new Error("Cannot find module 'oxlint'");
    },
    stat,
  });
  assert.deepEqual(result, {
    source: null,
    path: null,
    loader: null,
    spawnable: false,
    tsrxAware: false,
    reason: "not-found",
    realPath: null,
    attempted: null,
  });

  // Declaring `bin.oxlint` did not put us in the chain, so nothing about this
  // tree improves by installing harder. The setting is the only route.
  const configured = await resolve({
    workspaceFolders: ["/w"],
    configured: "node_modules/oxc-tsrx/bin/oxlint",
    stat,
  });
  assert.equal(configured.tsrxAware, true);
});

// --- 3. the tree where we did win --------------------------------------------

test("a .bin/oxlint symlinked into this package resolves as ours with no setting", async () => {
  const stat = createFixtureStat({
    "/w/package.json": CONSUMER_MANIFEST,
    "/w/node_modules/.bin/oxlint": { link: "../oxc-tsrx/bin/oxlint" },
    "/w/node_modules/oxc-tsrx/package.json": OUR_MANIFEST,
    "/w/node_modules/oxc-tsrx/bin/oxlint": EXECUTABLE,
  });

  const result = await resolve({ workspaceFolders: ["/w"], stat });
  assert.equal(result.source, "workspace-node-modules");
  assert.equal(result.path, "/w/node_modules/.bin/oxlint");
  assert.equal(result.realPath, "/w/node_modules/oxc-tsrx/bin/oxlint");
  assert.equal(result.tsrxAware, true);
  // Still `native`: a `.bin` hit is never classified as a Node script, whatever
  // it points at.
  assert.equal(result.loader, "native");
});

// --- 4. the workspace root above the project root -----------------------------

test("a workspace root above the project root never reads the project's settings.json", async () => {
  const files = {
    "/repo/package.json": JSON.stringify({ name: "monorepo", private: true }),
    "/repo/pnpm-workspace.yaml": "packages:\n  - apps/*\n",
    "/repo/apps/web/package.json": CONSUMER_MANIFEST,
    // `setup` wrote this, at the project root, where VS Code will never look.
    "/repo/apps/web/.vscode/settings.json": JSON.stringify({
      "oxc.path.oxlint": "node_modules/oxc-tsrx/bin/oxlint",
    }),
    "/repo/apps/web/node_modules/.bin/oxlint": {
      content: "#!/bin/sh\n",
      runs: "/repo/apps/web/node_modules/vite-plus/bin/oxlint",
    },
    "/repo/apps/web/node_modules/vite-plus/package.json": VITE_PLUS_MANIFEST,
    "/repo/apps/web/node_modules/vite-plus/bin/oxlint": EXECUTABLE,
    "/repo/apps/web/node_modules/oxc-tsrx/package.json": OUR_MANIFEST,
    "/repo/apps/web/node_modules/oxc-tsrx/bin/oxlint": EXECUTABLE,
  };
  const stat = createFixtureStat(files);

  // The window is `/repo`. The written key is inert, so the extension
  // auto-detects and reaches Vite+ through the `**/package.json` glob.
  const inert = await resolve({
    workspaceFolders: ["/repo"],
    packageJsonDirectories: ["/repo", "/repo/apps/web"],
    stat,
  });
  assert.equal(inert.source, "package-json-node-modules");
  assert.equal(inert.path, "/repo/apps/web/node_modules/.bin/oxlint");
  assert.equal(inert.tsrxAware, false);

  // Worse: had `/repo` somehow carried that same relative value, it would
  // resolve against `/repo` and hit nothing, and there is no fallback.
  const misplaced = await resolve({
    workspaceFolders: ["/repo"],
    packageJsonDirectories: ["/repo", "/repo/apps/web"],
    configured: "node_modules/oxc-tsrx/bin/oxlint",
    stat,
  });
  assert.equal(misplaced.source, "configured");
  assert.equal(misplaced.path, null);
  assert.equal(misplaced.reason, "configured-missing");
  assert.equal(misplaced.attempted, "/repo/node_modules/oxc-tsrx/bin/oxlint");

  // Open the project folder itself and the same value is correct.
  const opened = await resolve({
    workspaceFolders: ["/repo/apps/web"],
    configured: "node_modules/oxc-tsrx/bin/oxlint",
    stat,
  });
  assert.equal(opened.path, "/repo/apps/web/node_modules/oxc-tsrx/bin/oxlint");
  assert.equal(opened.tsrxAware, true);
});

// --- 5. multi-root, where "add the folder to the workspace" backfires ---------

test("a relative value resolves against the first workspace folder, not the project's", async () => {
  const stat = createFixtureStat({
    "/other/package.json": JSON.stringify({ name: "other" }),
    "/repo/apps/web/package.json": CONSUMER_MANIFEST,
    "/repo/apps/web/node_modules/.bin/oxlint": {
      content: "#!/bin/sh\n",
      runs: "/repo/apps/web/node_modules/vite-plus/bin/oxlint",
    },
    "/repo/apps/web/node_modules/vite-plus/package.json": VITE_PLUS_MANIFEST,
    "/repo/apps/web/node_modules/vite-plus/bin/oxlint": EXECUTABLE,
    "/repo/apps/web/node_modules/oxc-tsrx/package.json": OUR_MANIFEST,
    "/repo/apps/web/node_modules/oxc-tsrx/bin/oxlint": EXECUTABLE,
  });

  const misresolved = await resolve({
    workspaceFolders: ["/other", "/repo/apps/web"],
    configured: "node_modules/oxc-tsrx/bin/oxlint",
    stat,
  });
  assert.equal(misresolved.attempted, "/other/node_modules/oxc-tsrx/bin/oxlint");
  assert.equal(misresolved.path, null);
  // And it does not quietly land on the shim that is sitting right there: a
  // configured value replaces the chain rather than joining it.
  assert.equal(misresolved.source, "configured");
  assert.equal(misresolved.reason, "configured-missing");

  const reordered = await resolve({
    workspaceFolders: ["/repo/apps/web", "/other"],
    configured: "node_modules/oxc-tsrx/bin/oxlint",
    stat,
  });
  assert.equal(reordered.path, "/repo/apps/web/node_modules/oxc-tsrx/bin/oxlint");
  assert.equal(reordered.tsrxAware, true);
});

// --- 6. Yarn PnP --------------------------------------------------------------

test("Yarn PnP resolves through the loader, from an ancestor, and only when trusted", async () => {
  const stat = createFixtureStat({
    "/pnp/package.json": CONSUMER_MANIFEST,
    "/pnp/.pnp.cjs": "module.exports = { resolveRequest() {} };\n",
    "/pnp/apps/web/package.json": CONSUMER_MANIFEST,
    "/store/oxlint-npm-1.59.0/package.json": OXLINT_MANIFEST,
    "/store/oxlint-npm-1.59.0/dist/index.js": "",
    "/store/oxlint-npm-1.59.0/bin/oxlint": EXECUTABLE,
    "/store/oxc-tsrx-npm-0.1.5/package.json": OUR_MANIFEST,
    "/store/oxc-tsrx-npm-0.1.5/dist/index.js": "",
    "/store/oxc-tsrx-npm-0.1.5/bin/oxlint": EXECUTABLE,
  });
  const issuers = [];
  const pnp = {
    "/pnp/.pnp.cjs": {
      resolveRequest(request, issuer) {
        issuers.push([request, issuer]);
        return request === "oxlint" ? "/store/oxlint-npm-1.59.0/dist/index.js" : null;
      },
    },
  };

  const result = await resolve({ workspaceFolders: ["/pnp/apps/web"], pnp, stat });
  assert.deepEqual(issuers, [["oxlint", "/pnp/apps/web/"]]);
  assert.equal(result.source, "yarn-pnp");
  // The `bin` entry of the resolved package, not the module PnP handed back.
  assert.equal(result.path, "/store/oxlint-npm-1.59.0/bin/oxlint");
  assert.equal(result.loader, "node");
  assert.equal(result.yarnPnpLoaderPath, "/pnp/.pnp.cjs");
  assert.equal(result.tsrxAware, false);

  const ours = await resolve({
    workspaceFolders: ["/pnp"],
    pnp: { "/pnp/.pnp.cjs": { resolveRequest: () => "/store/oxc-tsrx-npm-0.1.5/dist/index.js" } },
    stat,
  });
  assert.equal(ours.path, "/store/oxc-tsrx-npm-0.1.5/bin/oxlint");
  assert.equal(ours.tsrxAware, true);

  // `ie()` is guarded by workspace trust, so an untrusted window loses PnP as
  // well as the configured value.
  const untrusted = await resolve({ workspaceFolders: ["/pnp"], pnp, trusted: false, stat });
  assert.equal(untrusted.source, null);
  assert.equal(untrusted.reason, "not-found");
});

// --- 7. ancestor shadowing ----------------------------------------------------

test("the first workspace folder's .bin shadows every folder after it", async () => {
  const stat = createFixtureStat({
    "/repo/package.json": JSON.stringify({ name: "monorepo", private: true }),
    "/repo/node_modules/.bin/oxlint": {
      content: "#!/bin/sh\n",
      runs: "/repo/node_modules/vite-plus/bin/oxlint",
    },
    "/repo/node_modules/vite-plus/package.json": VITE_PLUS_MANIFEST,
    "/repo/node_modules/vite-plus/bin/oxlint": EXECUTABLE,
    "/repo/apps/web/package.json": CONSUMER_MANIFEST,
    "/repo/apps/web/node_modules/.bin/oxlint": { link: "../oxc-tsrx/bin/oxlint" },
    "/repo/apps/web/node_modules/oxc-tsrx/package.json": OUR_MANIFEST,
    "/repo/apps/web/node_modules/oxc-tsrx/bin/oxlint": EXECUTABLE,
  });

  const shadowed = await resolve({ workspaceFolders: ["/repo", "/repo/apps/web"], stat });
  assert.equal(shadowed.path, "/repo/node_modules/.bin/oxlint");
  assert.equal(shadowed.tsrxAware, false);

  // Same tree, same files, different window order, opposite answer. Nothing
  // about the install decides this.
  const direct = await resolve({ workspaceFolders: ["/repo/apps/web", "/repo"], stat });
  assert.equal(direct.path, "/repo/apps/web/node_modules/.bin/oxlint");
  assert.equal(direct.tsrxAware, true);
});

// --- 8. Windows ---------------------------------------------------------------

test("on Windows the value setup writes resolves and still cannot be spawned", async () => {
  const stat = createFixtureStat({
    "C:\\w\\package.json": CONSUMER_MANIFEST,
    "C:\\w\\node_modules\\oxc-tsrx\\package.json": OUR_MANIFEST,
    "C:\\w\\node_modules\\oxc-tsrx\\bin\\oxlint": EXECUTABLE,
    "C:\\w\\node_modules\\oxc-tsrx\\bin\\oxlint.exe": EXECUTABLE,
  });

  const written = await resolve({
    platform: "win32",
    workspaceFolders: ["C:\\w"],
    configured: "node_modules\\oxc-tsrx\\bin\\oxlint",
    stat,
  });
  assert.equal(written.path, "C:\\w\\node_modules\\oxc-tsrx\\bin\\oxlint");
  assert.equal(written.tsrxAware, true);
  // It ends in `oxc-tsrx\bin\oxlint`, not `oxlint\bin\oxlint`, so it is not
  // classified as a Node script, and a `native` loader on Windows is spawned
  // with `shell: true`, which is `cmd.exe`, which cannot execute an
  // extensionless file. Resolution succeeds and the language server never
  // starts.
  assert.equal(written.loader, "native");
  assert.equal(written.spawnable, false);

  const explicit = await resolve({
    platform: "win32",
    workspaceFolders: ["C:\\w"],
    configured: "node_modules\\oxc-tsrx\\bin\\oxlint.exe",
    stat,
  });
  assert.equal(explicit.spawnable, true);
});

test("Windows candidates are <name> then <name>.exe, and never .cmd or .ps1", async () => {
  const shims = {
    "C:\\w\\package.json": CONSUMER_MANIFEST,
    "C:\\w\\node_modules\\.bin\\oxlint.cmd": "@ECHO off\r\n",
    "C:\\w\\node_modules\\.bin\\oxlint.ps1": "#!/usr/bin/env pwsh\r\n",
  };
  const invisible = await resolve({
    platform: "win32",
    workspaceFolders: ["C:\\w"],
    stat: createFixtureStat(shims),
  });
  // This is where this module deliberately disagrees with `inspectLinterShim`
  // in compat.js: npm and pnpm write these files, the extension does not look
  // for them, so on Windows this tree reads as a tree with no shim at all.
  assert.equal(invisible.source, null);

  const withExe = await resolve({
    platform: "win32",
    workspaceFolders: ["C:\\w"],
    stat: createFixtureStat({
      ...shims,
      "C:\\w\\node_modules\\.bin\\oxlint.exe": EXECUTABLE,
    }),
  });
  assert.equal(withExe.path, "C:\\w\\node_modules\\.bin\\oxlint.exe");
  assert.equal(withExe.spawnable, true);

  const extensionless = await resolve({
    platform: "win32",
    workspaceFolders: ["C:\\w"],
    stat: createFixtureStat({
      ...shims,
      "C:\\w\\node_modules\\.bin\\oxlint": EXECUTABLE,
      "C:\\w\\node_modules\\.bin\\oxlint.exe": EXECUTABLE,
    }),
  });
  // `<name>` is stated before `<name>.exe`, so the unspawnable one wins.
  assert.equal(extensionless.path, "C:\\w\\node_modules\\.bin\\oxlint");
  assert.equal(extensionless.spawnable, false);
});

test("a configured Windows value is retried with .exe, and .exe is stripped elsewhere", async () => {
  const retried = await resolve({
    platform: "win32",
    workspaceFolders: ["C:\\w"],
    configured: "C:\\tools\\oxlint",
    stat: createFixtureStat({ "C:\\tools\\oxlint.exe": EXECUTABLE }),
  });
  assert.equal(retried.path, "C:\\tools\\oxlint.exe");
  assert.equal(retried.loader, "native");

  // The mirror image: off Windows the extension drops a trailing `.exe` before
  // it stats anything.
  const stripped = await resolve({
    configured: "/tools/oxlint.exe",
    stat: createFixtureStat({ "/tools/oxlint": EXECUTABLE }),
  });
  assert.equal(stripped.path, "/tools/oxlint");
});

// --- the configured value: validation, trust, and the missing fallback --------

test("a configured value that fails validation kills the linter instead of falling back", async () => {
  const stat = createFixtureStat({
    "/w/package.json": CONSUMER_MANIFEST,
    "/w/node_modules/.bin/oxlint": { link: "../oxc-tsrx/bin/oxlint" },
    "/w/node_modules/oxc-tsrx/package.json": OUR_MANIFEST,
    "/w/node_modules/oxc-tsrx/bin/oxlint": EXECUTABLE,
  });

  // Without the key this tree resolves perfectly.
  assert.equal((await resolve({ workspaceFolders: ["/w"], stat })).tsrxAware, true);

  for (const [value, reason] of [
    ["../sibling/node_modules/oxc-tsrx/bin/oxlint", "configured-rejected-traversal"],
    [".\\oxlint", "configured-rejected-traversal"],
    ["/opt/my project!/oxlint", "configured-rejected-metacharacter"],
    ["/opt/$HOME/oxlint", "configured-rejected-metacharacter"],
    ["/opt/a&b/oxlint", "configured-rejected-metacharacter"],
    ["/opt/100%/oxlint", "configured-rejected-metacharacter"],
  ]) {
    const result = await resolve({ workspaceFolders: ["/w"], configured: value, stat });
    assert.equal(result.reason, reason, value);
    assert.equal(result.source, "configured", value);
    assert.equal(result.path, null, value);
    assert.equal(result.attempted, value, value);
  }

  for (const character of ["$", "&", ";", "|", "`", ">", "<", "!", "%", "^"]) {
    const result = await resolve({ configured: `/opt/x${character}y/oxlint`, stat });
    assert.equal(result.reason, "configured-rejected-metacharacter", character);
  }
});

test("an untrusted workspace drops the configured value and does not auto-detect either", async () => {
  const stat = createFixtureStat({
    "/w/package.json": CONSUMER_MANIFEST,
    "/w/node_modules/.bin/oxlint": { link: "../oxc-tsrx/bin/oxlint" },
    "/w/node_modules/oxc-tsrx/package.json": OUR_MANIFEST,
    "/w/node_modules/oxc-tsrx/bin/oxlint": EXECUTABLE,
  });

  const result = await resolve({
    workspaceFolders: ["/w"],
    configured: "node_modules/oxc-tsrx/bin/oxlint",
    trusted: false,
    stat,
  });
  assert.equal(result.source, "configured");
  assert.equal(result.path, null);
  assert.equal(result.reason, "untrusted-workspace");

  // With no key at all, an untrusted window still auto-detects: only `se()` and
  // `ie()` are trust-guarded.
  const detected = await resolve({ workspaceFolders: ["/w"], trusted: false, stat });
  assert.equal(detected.path, "/w/node_modules/.bin/oxlint");
});

test("a relative value with no workspace folder resolves to nothing", async () => {
  const result = await resolve({
    workspaceFolders: [],
    configured: "node_modules/oxc-tsrx/bin/oxlint",
    stat: createFixtureStat({}),
  });
  assert.equal(result.reason, "no-workspace-folder");
  assert.equal(result.path, null);
});

test("an empty configured value is not a value and auto-detection still runs", async () => {
  const stat = createFixtureStat({
    "/w/node_modules/.bin/oxlint": { link: "../oxc-tsrx/bin/oxlint" },
    "/w/node_modules/oxc-tsrx/package.json": OUR_MANIFEST,
    "/w/node_modules/oxc-tsrx/bin/oxlint": EXECUTABLE,
  });
  const result = await resolve({ workspaceFolders: ["/w"], configured: "", stat });
  assert.equal(result.source, "workspace-node-modules");
});

// --- the rest of the chain ----------------------------------------------------

test("require.resolve reads the resolved package's bin entry, and a missing one is not fatal", async () => {
  const stat = createFixtureStat({
    "/w/package.json": CONSUMER_MANIFEST,
    "/w/node_modules/oxlint/package.json": OXLINT_MANIFEST,
    "/w/node_modules/oxlint/dist/index.js": "",
    "/w/node_modules/oxlint/bin/oxlint": EXECUTABLE,
    "/global/lib/node_modules/.bin/oxlint": { link: "../oxc-tsrx/bin/oxlint" },
    "/global/lib/node_modules/oxc-tsrx/package.json": OUR_MANIFEST,
    "/global/lib/node_modules/oxc-tsrx/bin/oxlint": EXECUTABLE,
  });

  const requested = [];
  const viaRequire = await resolve({
    workspaceFolders: ["/w"],
    requireResolve: (request, options) => {
      requested.push([request, options.paths]);
      return "/w/node_modules/oxlint/dist/index.js";
    },
    stat,
  });
  assert.deepEqual(requested, [["oxlint", ["/w"]]]);
  assert.equal(viaRequire.source, "require-resolve");
  assert.equal(viaRequire.path, "/w/node_modules/oxlint/bin/oxlint");
  assert.equal(viaRequire.loader, "node");
  assert.equal(viaRequire.spawnable, true);

  // A package with no `bin` entry for the name throws out of the walk, which the
  // extension swallows: the step fails, the chain continues to the globals.
  const noBin = await resolve({
    workspaceFolders: ["/w"],
    requireResolve: () => "/w/node_modules/nobin/dist/index.js",
    globalRoots: ["/global/lib/node_modules"],
    stat: createFixtureStat({
      "/w/node_modules/nobin/package.json": JSON.stringify({ name: "nobin" }),
      "/w/node_modules/nobin/dist/index.js": "",
      "/global/lib/node_modules/.bin/oxlint": { link: "../oxc-tsrx/bin/oxlint" },
      "/global/lib/node_modules/oxc-tsrx/package.json": OUR_MANIFEST,
      "/global/lib/node_modules/oxc-tsrx/bin/oxlint": EXECUTABLE,
    }),
  });
  assert.equal(noBin.source, "global-node-modules");
  assert.equal(noBin.tsrxAware, true);
});

test("global roots and then PATH are the last two steps, in that order", async () => {
  const files = {
    "/global/lib/node_modules/.bin/oxlint": { link: "../oxlint/bin/oxlint" },
    "/global/lib/node_modules/oxlint/package.json": OXLINT_MANIFEST,
    "/global/lib/node_modules/oxlint/bin/oxlint": EXECUTABLE,
    "/usr/local/bin/oxlint": EXECUTABLE,
  };

  const global = await resolve({
    globalRoots: ["/global/lib/node_modules"],
    pathEntries: "/usr/bin:/usr/local/bin",
    stat: createFixtureStat(files),
  });
  assert.equal(global.source, "global-node-modules");
  assert.equal(global.path, "/global/lib/node_modules/.bin/oxlint");

  const onPath = await resolve({
    globalRoots: ["/global/lib/node_modules"],
    pathEntries: ["", "/usr/bin", "/usr/local/bin"],
    stat: createFixtureStat({ "/usr/local/bin/oxlint": EXECUTABLE }),
  });
  assert.equal(onPath.source, "path");
  assert.equal(onPath.path, "/usr/local/bin/oxlint");
  assert.equal(onPath.loader, "native");
  assert.equal(onPath.tsrxAware, false);
});

// --- the exported judgements, on their own ------------------------------------

test("the loader test keys on the package name, which is why our own value is native", async () => {
  assert.equal(configuredLoader("oxlint", "/w/node_modules/oxlint/bin/oxlint", "linux"), "node");
  assert.equal(configuredLoader("oxlint", "/w/node_modules/oxc-tsrx/bin/oxlint", "linux"), "native");
  assert.equal(configuredLoader("oxlint", "C:\\w\\oxlint\\bin\\oxlint", "win32"), "node");
  // The same path with POSIX separators is not the pattern Windows looks for.
  assert.equal(configuredLoader("oxlint", "C:/w/oxlint/bin/oxlint", "win32"), "native");
  for (const extension of [".js", ".cjs", ".mjs"]) {
    assert.equal(configuredLoader("oxlint", `/w/run${extension}`, "linux"), "node");
  }
  assert.equal(configuredLoader("oxlint", "/w/run.ts", "linux"), "native");

  assert.equal(rejectConfiguredValue("node_modules/oxc-tsrx/bin/oxlint"), null);
  assert.equal(rejectConfiguredValue("/opt/oxlint"), null);

  assert.equal(isSpawnable("/w/bin/oxlint", "native", "linux"), true);
  assert.equal(isSpawnable("C:\\w\\bin\\oxlint", "native", "win32"), false);
  assert.equal(isSpawnable("C:\\w\\bin\\oxlint.exe", "native", "win32"), true);
  assert.equal(isSpawnable("C:\\w\\bin\\oxlint", "node", "win32"), true);
  assert.equal(isSpawnable(null, "native", "linux"), false);
});

test("tsrx awareness is decided by the package that really runs, not the path that resolved", async () => {
  const stat = createFixtureStat({
    "/w/package.json": OUR_MANIFEST,
    "/w/node_modules/.bin/oxlint": {
      content: "#!/bin/sh\n",
      runs: "/w/node_modules/vite-plus/bin/oxlint",
    },
    "/w/node_modules/vite-plus/package.json": VITE_PLUS_MANIFEST,
    "/w/node_modules/vite-plus/bin/oxlint": EXECUTABLE,
  });

  // The walk stops at Vite+'s own manifest. It must not keep climbing until it
  // finds a manifest that happens to claim `.tsrx`, or every binary in a tsrx
  // repo would read as tsrx-aware.
  const result = await resolve({ workspaceFolders: ["/w"], stat });
  assert.equal(result.realPath, "/w/node_modules/vite-plus/bin/oxlint");
  assert.equal(result.tsrxAware, false);

  // A provider that claims some other extension is not tsrx-aware either.
  const otherProvider = await resolve({
    workspaceFolders: ["/w"],
    stat: createFixtureStat({
      "/w/node_modules/.bin/oxlint": { link: "../other/bin/oxlint" },
      "/w/node_modules/other/package.json": JSON.stringify({
        name: "other",
        oxc: { provider: { languages: [{ id: "svelte", extensions: [".svelte"] }] } },
      }),
      "/w/node_modules/other/bin/oxlint": EXECUTABLE,
    }),
  });
  assert.equal(otherProvider.tsrxAware, false);

  // A malformed manifest is a "no", not a crash.
  const malformed = await resolve({
    workspaceFolders: ["/w"],
    stat: createFixtureStat({
      "/w/node_modules/.bin/oxlint": { link: "../broken/bin/oxlint" },
      "/w/node_modules/broken/package.json": "{not json",
      "/w/node_modules/broken/bin/oxlint": EXECUTABLE,
    }),
  });
  assert.equal(malformed.tsrxAware, false);
});

test("the oracle refuses to run without the two things it cannot invent", async () => {
  await assert.rejects(() => resolveEditorLinter({ stat: () => null }), TypeError);
  await assert.rejects(() => resolveEditorLinter({ name: "oxlint" }), TypeError);
});
