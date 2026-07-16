import assert from "node:assert/strict";
import { cp, mkdtemp, readFile, readdir, realpath, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { createServer as createNetServer } from "node:net";
import test from "node:test";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const root = resolve(import.meta.dirname, "../..");
const fixture = join(root, "tests/fixtures/vite/react");

async function makeProject() {
  const project = await mkdtemp(join(tmpdir(), "oxc-tsrx-vite-react-"));
  await cp(fixture, project, { recursive: true });
  const modules = join(project, "node_modules");
  await import("node:fs/promises").then(({ mkdir }) => mkdir(modules, { recursive: true }));
  for (const dependency of ["react", "react-dom"]) {
    const packageRoot = dirname(require.resolve(`${dependency}/package.json`));
    await symlink(packageRoot, join(modules, dependency), "dir");
  }
  return realpath(project);
}

async function outputFiles(directory) {
  const files = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) files.push(...(await outputFiles(path)));
    else files.push(path);
  }
  return files;
}

function deadline(promise, milliseconds, label) {
  let timer;
  return Promise.race([
    promise,
    new Promise(
      (_, reject) =>
        (timer = setTimeout(
          () => reject(new Error(`timed out waiting for ${label}`)),
          milliseconds,
        )),
    ),
  ]).finally(() => clearTimeout(timer));
}

function waitAtMost(promise, milliseconds) {
  let timer;
  return Promise.race([
    promise,
    new Promise((resolveWait) => (timer = setTimeout(resolveWait, milliseconds))),
  ]).finally(() => clearTimeout(timer));
}

function freePort() {
  return new Promise((resolvePort, rejectPort) => {
    const probe = createNetServer();
    probe.unref();
    probe.once("error", rejectPort);
    probe.listen(0, "127.0.0.1", () => {
      const address = probe.address();
      assert.ok(address && typeof address === "object");
      probe.close((error) => {
        if (error) rejectPort(error);
        else resolvePort(address.port);
      });
    });
  });
}

test("official TSRX React compiler composes through a real Vite build without an OXC transform", async () => {
  const [{ build }, { tsrxReact }] = await Promise.all([
    import("vite"),
    import("@tsrx/vite-plugin-react"),
  ]);
  const project = await makeProject();
  const observed = [];
  const afterFramework = {
    name: "oxc-tsrx-test:after-framework",
    enforce: "post",
    transform(code, id) {
      if (id.split("?")[0].endsWith(".tsrx")) observed.push({ code, id });
      return null;
    },
  };

  try {
    await build({
      root: project,
      appType: "custom",
      configFile: false,
      logLevel: "silent",
      plugins: [tsrxReact(), afterFramework],
      build: {
        minify: false,
        outDir: "dist",
        rolldownOptions: { input: join(project, "src/main.jsx") },
      },
    });

    assert.ok(observed.length > 0, "the downstream plugin should observe compiled TSRX");
    assert.ok(observed.every(({ code }) => !code.includes("@if")));
    assert.ok(observed.some(({ code }) => code.includes("react/jsx-runtime")));

    const files = await outputFiles(join(project, "dist"));
    const output = (
      await Promise.all(
        files.filter((file) => file.endsWith(".js")).map((file) => readFile(file, "utf8")),
      )
    ).join("\n");
    assert.match(output, /OXC TSRX BUILD/);
    assert.doesNotMatch(output, /@if|@\{/);
  } finally {
    await rm(project, { recursive: true, force: true });
  }
});

test("real Vite dev server recompiles a changed .tsrx module through the framework HMR path", async () => {
  const [{ createServer }, { tsrxReact }] = await Promise.all([
    import("vite"),
    import("@tsrx/vite-plugin-react"),
  ]);
  const project = await makeProject();
  let resolveHotUpdate;
  const hotUpdate = new Promise((resolveUpdate) => {
    resolveHotUpdate = resolveUpdate;
  });
  const tracker = {
    name: "oxc-tsrx-test:hot-update-observer",
    handleHotUpdate(context) {
      if (context.file.endsWith("App.tsrx")) resolveHotUpdate(context);
    },
  };
  const port = await freePort();
  const server = await createServer({
    root: project,
    configFile: false,
    logLevel: "silent",
    plugins: [tsrxReact(), tracker],
    server: { host: "127.0.0.1", port, strictPort: true, ws: false },
  });
  const watcherReady = new Promise((resolveReady) => server.watcher.once("ready", resolveReady));

  try {
    await deadline(server.listen(), 8_000, "Vite dev server listen");
    await waitAtMost(watcherReady, 1_000);
    await deadline(server.transformRequest("/src/main.jsx"), 8_000, "Vite entry transform");
    const initial = await deadline(
      server.transformRequest("/src/App.tsrx"),
      8_000,
      "initial TSRX transform",
    );
    assert.ok(initial);
    assert.match(initial.code, /OXC TSRX BUILD/);
    assert.doesNotMatch(initial.code, /@if|@\{/);

    const sourcePath = join(project, "src/App.tsrx");
    const source = await readFile(sourcePath, "utf8");
    let resolveHotPayload;
    const hotPayload = new Promise((resolvePayload) => {
      resolveHotPayload = resolvePayload;
    });
    const originalSend = server.ws.send.bind(server.ws);
    server.ws.send = (payload, ...rest) => {
      if (payload?.type === "update" || payload?.type === "full-reload") resolveHotPayload(payload);
      return originalSend(payload, ...rest);
    };
    await writeFile(sourcePath, source.replaceAll("OXC TSRX BUILD", "OXC TSRX HMR"));
    const context = await deadline(hotUpdate, 8_000, "Vite hot update");
    assert.ok(context.modules.some((module) => module.id?.includes("App.tsrx")));
    const payload = await deadline(hotPayload, 8_000, "Vite HMR payload");
    assert.ok(payload.type === "update" || payload.type === "full-reload");

    const updated = await deadline(
      server.transformRequest(`/src/App.tsrx?t=${Date.now()}`),
      8_000,
      "updated TSRX transform",
    );
    assert.ok(updated);
    assert.match(updated.code, /OXC TSRX HMR/);
    assert.doesNotMatch(updated.code, /@if|@\{/);
  } finally {
    await deadline(server.close(), 8_000, "Vite dev server close");
    await rm(project, { recursive: true, force: true });
  }
});
