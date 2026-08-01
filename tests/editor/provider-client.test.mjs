import assert from "node:assert/strict";
import { mkdir, mkdtemp, readFile, realpath, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve, sep } from "node:path";
import test from "node:test";
import { requireCts } from "../helpers/require-cts.mjs";

const root = resolve(import.meta.dirname, "../..");
const modulePath = join(root, "packages/vscode/src/provider-client.cts");
const providerClient = await requireCts(modulePath);
const { discoverProviders } = await import(
  join(root, "packages/toolchain/dist/provider-resolve.js")
);

const {
  clientForDocument,
  discoverWorkspaceFolder,
  discoverWorkspaceFolders,
  documentExtension,
  loadFolderResolver,
  plannedClientStarts,
  providerDocumentSelector,
  providerLanguageClients,
} = providerClient;

/**
 * A project on disk whose dependencies are real directories. Discovery is a
 * static read of these manifests; nothing here is ever imported or executed.
 */
async function project(directory, dependencies, packages) {
  await mkdir(directory, { recursive: true });
  await writeFile(
    join(directory, "package.json"),
    `${JSON.stringify({ name: "host-fixture", private: true, dependencies }, null, 2)}\n`,
  );
  for (const [name, { manifest, files = {} }] of Object.entries(packages)) {
    const packageRoot = join(directory, "node_modules", ...name.split("/"));
    await mkdir(packageRoot, { recursive: true });
    await writeFile(
      join(packageRoot, "package.json"),
      `${JSON.stringify(manifest, null, 2)}\n`,
    );
    for (const [file, contents] of Object.entries(files)) {
      const path = join(packageRoot, file);
      await mkdir(dirname(path), { recursive: true });
      await writeFile(path, contents, { mode: 0o755 });
    }
  }
  return directory;
}

function providerPackage(name, id, extensions, { server = true, binName = `${id}-server` } = {}) {
  const capabilities = { parse: { module: "." } };
  if (server) capabilities.lsp = { bin: binName };
  return {
    manifest: {
      name,
      version: "1.0.0",
      type: "module",
      main: "./index.js",
      exports: { ".": "./index.js", "./package.json": "./package.json" },
      ...(server ? { bin: { [binName]: `./bin/${binName}` } } : {}),
      oxc: {
        provider: {
          protocol: 1,
          id,
          languages: [{ id, extensions, capabilities }],
        },
      },
    },
    files: {
      "index.js": "export const stub = true;\n",
      ...(server ? { [`bin/${binName}`]: "#!/usr/bin/env node\n" } : {}),
    },
  };
}

const ORDINARY_DOCUMENTS = [
  "src/app.ts",
  "src/app.tsx",
  "src/app.js",
  "src/app.jsx",
  "src/app.mjs",
  "src/app.cjs",
  "tsconfig.json",
  "README.md",
  "Makefile",
];

async function temporaryRoot(context, label) {
  const directory = await mkdtemp(join(tmpdir(), `oxc-provider-client-${label}-`));
  context.after(() => rm(directory, { recursive: true, force: true }));
  // Node resolution answers with real paths; keep fixtures comparable.
  return realpath(directory);
}

test("the decision module is vendor-neutral, spawns nothing, and imports no editor API", async () => {
  const source = await readFile(modulePath, "utf8");
  for (const literal of [/tsrx/iu, /oxc-tsrx/iu]) {
    assert.doesNotMatch(
      source,
      literal,
      "the decision module must carry no provider-specific literal",
    );
  }
  assert.doesNotMatch(source, /child_process/u, "the decision module must not spawn");
  assert.doesNotMatch(source, /require\(\s*["']vscode/u, "it must not import an editor API");
  assert.doesNotMatch(source, /node_modules[/\\]\.bin/u, "it must never look in a bin directory");
  assert.doesNotMatch(source, /process\.env\.PATH|env\[["']PATH/u, "it must never consult PATH");
  assert.doesNotMatch(source, /import\(/u, "it must not dynamically import a dependency");
});

test("an empty index yields no selector, no client, and no start", async (context) => {
  const directory = await temporaryRoot(context, "empty");
  const folder = await project(directory, { "plain-dependency": "1.0.0" }, {
    "plain-dependency": { manifest: { name: "plain-dependency", version: "1.0.0" } },
  });

  const [state] = await discoverWorkspaceFolders([folder], { discover: discoverProviders });
  assert.equal(state.folder, folder);
  assert.deepEqual(state.extensions, []);
  assert.deepEqual(state.selector, []);
  assert.deepEqual(state.clients, []);
  assert.deepEqual(
    state.diagnostics.filter((entry) => entry.severity === "error"),
    [],
  );
  assert.equal(state.failure, null);

  const documents = ORDINARY_DOCUMENTS.map((path) => ({ folder, path: join(folder, path) }));
  assert.deepEqual(plannedClientStarts([state], documents), []);
  for (const document of documents) {
    assert.equal(clientForDocument(state, document.path), null, document.path);
  }
});

test("a synthetic provider unrelated to this repository produces its own client", async (context) => {
  const directory = await temporaryRoot(context, "generic");
  const folder = await project(directory, { "demo-language-provider": "1.0.0" }, {
    "demo-language-provider": providerPackage("demo-language-provider", "demo", [".demo"]),
  });

  const state = await discoverWorkspaceFolder(folder, { discover: discoverProviders });
  assert.deepEqual(state.extensions, [".demo"]);
  assert.deepEqual(state.selector, [{ scheme: "file", pattern: "**/*.demo" }]);
  assert.equal(state.clients.length, 1);

  const [client] = state.clients;
  assert.equal(client.id, "demo");
  assert.equal(client.package, "demo-language-provider");
  assert.deepEqual(client.extensions, [".demo"]);
  assert.deepEqual(client.selector, [{ scheme: "file", pattern: "**/*.demo" }]);
  // The bin carries a Node shebang, so the descriptor launches it through the
  // current interpreter rather than executing it directly.
  assert.equal(client.command, process.execPath);
  assert.deepEqual(client.args, [client.executable, "--stdio"]);
  assert.equal(
    client.executable,
    join(folder, "node_modules/demo-language-provider/bin/demo-server"),
  );

  assert.equal(clientForDocument(state, join(folder, "src/View.demo")), client);
  assert.equal(clientForDocument(state, join(folder, "src/View.DEMO")), client);
  assert.deepEqual(
    plannedClientStarts([state], [{ folder, path: join(folder, "a.demo") }]),
    [{ folder, client, document: join(folder, "a.demo") }],
  );
});

test("ordinary source documents never produce a client", async (context) => {
  const directory = await temporaryRoot(context, "ordinary");
  const folder = await project(directory, { "demo-language-provider": "1.0.0" }, {
    "demo-language-provider": providerPackage("demo-language-provider", "demo", [".demo"]),
  });

  const state = await discoverWorkspaceFolder(folder, { discover: discoverProviders });
  assert.equal(state.clients.length, 1, "the provider itself is discovered");

  const documents = ORDINARY_DOCUMENTS.map((path) => ({ folder, path: join(folder, path) }));
  for (const document of documents) {
    assert.equal(clientForDocument(state, document.path), null, document.path);
  }
  assert.deepEqual(
    plannedClientStarts([state], documents),
    [],
    "a session of ordinary documents starts nothing",
  );
  assert.deepEqual(
    plannedClientStarts([state], [
      ...documents,
      { folder, path: join(folder, "src/View.demo") },
    ]).map(({ client }) => client.id),
    ["demo"],
    "the first claimed document is what starts a client",
  );
});

test("two providers produce two independent clients", async (context) => {
  const directory = await temporaryRoot(context, "two");
  const folder = await project(
    directory,
    { "alpha-language-provider": "1.0.0", "beta-language-provider": "1.0.0" },
    {
      "alpha-language-provider": providerPackage("alpha-language-provider", "alpha", [".alpha"]),
      "beta-language-provider": providerPackage("beta-language-provider", "beta", [".beta"]),
    },
  );

  const state = await discoverWorkspaceFolder(folder, { discover: discoverProviders });
  assert.deepEqual(state.extensions, [".alpha", ".beta"]);
  assert.deepEqual(state.clients.map(({ id }) => id), ["alpha", "beta"]);
  assert.deepEqual(state.selector, [
    { scheme: "file", pattern: "**/*.alpha" },
    { scheme: "file", pattern: "**/*.beta" },
  ]);

  const [alpha, beta] = state.clients;
  assert.notEqual(alpha.executable, beta.executable);
  assert.equal(clientForDocument(state, join(folder, "a.alpha")), alpha);
  assert.equal(clientForDocument(state, join(folder, "b.beta")), beta);
  assert.deepEqual(
    plannedClientStarts([state], [
      { folder, path: join(folder, "a.alpha") },
      { folder, path: join(folder, "b.beta") },
      { folder, path: join(folder, "c.alpha") },
    ]).map(({ client }) => client.id),
    ["alpha", "beta"],
    "each client is started at most once",
  );
});

test("a provider without a language-server capability contributes no client", async (context) => {
  const directory = await temporaryRoot(context, "parse-only");
  const folder = await project(directory, { "parse-only-provider": "1.0.0" }, {
    "parse-only-provider": providerPackage("parse-only-provider", "parseonly", [".pol"], {
      server: false,
    }),
  });

  const state = await discoverWorkspaceFolder(folder, { discover: discoverProviders });
  assert.deepEqual(state.extensions, [".pol"], "the extension is still indexed");
  assert.deepEqual(state.clients, [], "but no language client exists for it");
  assert.equal(clientForDocument(state, join(folder, "a.pol")), null);
});

test("every command lives inside its provider package, never in a bin directory", async (context) => {
  const directory = await temporaryRoot(context, "confined");
  const folder = await project(
    directory,
    { "alpha-language-provider": "1.0.0", "beta-language-provider": "1.0.0" },
    {
      "alpha-language-provider": providerPackage("alpha-language-provider", "alpha", [".alpha"]),
      "beta-language-provider": providerPackage("beta-language-provider", "beta", [".beta"]),
    },
  );
  // A bin shim directory exists and is deliberately never consulted.
  await mkdir(join(folder, "node_modules/.bin"), { recursive: true });
  await writeFile(join(folder, "node_modules/.bin/alpha-server"), "#!/bin/sh\nexit 9\n", {
    mode: 0o755,
  });

  const state = await discoverWorkspaceFolder(folder, { discover: discoverProviders });
  for (const client of state.clients) {
    assert.equal(
      client.executable.startsWith(join(folder, "node_modules", client.package) + sep),
      true,
      client.package,
    );
    assert.equal(client.executable.includes(join("node_modules", ".bin")), false);
    assert.equal(client.args.includes(client.package), false);
  }
  assert.equal(state.clients.length, 2);
});

test("a reserved or conflicting claim never reaches a client", async (context) => {
  const directory = await temporaryRoot(context, "loud");
  const folder = await project(
    directory,
    {
      "greedy-language-provider": "1.0.0",
      "left-language-provider": "1.0.0",
      "right-language-provider": "1.0.0",
    },
    {
      "greedy-language-provider": providerPackage("greedy-language-provider", "greedy", [".ts"]),
      "left-language-provider": providerPackage("left-language-provider", "left", [".shared"]),
      "right-language-provider": providerPackage("right-language-provider", "right", [".shared"]),
    },
  );

  const state = await discoverWorkspaceFolder(folder, { discover: discoverProviders });
  assert.deepEqual(state.extensions, [], "nothing is routed");
  assert.deepEqual(state.clients, [], "and no client is built from a rejected claim");
  assert.equal(clientForDocument(state, join(folder, "a.ts")), null);
  assert.equal(clientForDocument(state, join(folder, "a.shared")), null);

  const codes = state.diagnostics
    .filter((entry) => entry.severity === "error")
    .map((entry) => entry.code)
    .sort();
  assert.deepEqual(codes, ["extension-conflict", "reserved-extension"]);
  assert.equal(state.failure, null, "the host reports and keeps serving; it does not crash");
});

test("indexes are never merged across workspace folders", async (context) => {
  const directory = await temporaryRoot(context, "folders");
  const alphaFolder = await project(join(directory, "alpha"), {
    "alpha-language-provider": "1.0.0",
  }, {
    "alpha-language-provider": providerPackage("alpha-language-provider", "alpha", [".alpha"]),
  });
  const betaFolder = await project(join(directory, "beta"), {
    "beta-language-provider": "1.0.0",
  }, {
    "beta-language-provider": providerPackage("beta-language-provider", "beta", [".beta"]),
  });

  const states = await discoverWorkspaceFolders([alphaFolder, betaFolder], {
    discover: discoverProviders,
  });
  assert.deepEqual(states.map(({ folder }) => folder), [alphaFolder, betaFolder]);
  assert.deepEqual(states[0].extensions, [".alpha"]);
  assert.deepEqual(states[1].extensions, [".beta"]);

  // A .beta document inside the alpha folder is not claimed by anything, and a
  // document is only ever matched against the state of its own folder.
  assert.equal(clientForDocument(states[0], join(alphaFolder, "x.beta")), null);
  assert.equal(clientForDocument(states[1], join(betaFolder, "x.alpha")), null);
  assert.deepEqual(
    plannedClientStarts(states, [
      { folder: alphaFolder, path: join(alphaFolder, "x.beta") },
      { folder: betaFolder, path: join(betaFolder, "x.alpha") },
    ]),
    [],
  );
  assert.deepEqual(
    plannedClientStarts(states, [
      { folder: alphaFolder, path: join(alphaFolder, "x.alpha") },
      { folder: betaFolder, path: join(betaFolder, "x.beta") },
    ]).map(({ folder, client }) => `${folder === alphaFolder ? "alpha" : "beta"}:${client.id}`),
    ["alpha:alpha", "beta:beta"],
  );
  assert.deepEqual(
    plannedClientStarts(states, [{ folder: join(directory, "gamma"), path: "x.alpha" }]),
    [],
    "a document outside every workspace folder starts nothing",
  );
});

test("an injected (request, issuer) resolver is honored", async (context) => {
  const directory = await temporaryRoot(context, "injected");
  // The provider lives where no directory walk from the folder could find it.
  const store = await realpath(await mkdtemp(join(tmpdir(), "oxc-provider-client-store-")));
  context.after(() => rm(store, { recursive: true, force: true }));
  await project(store, {}, {
    "hidden-language-provider": providerPackage("hidden-language-provider", "hidden", [".hid"]),
  });
  const folder = await project(directory, { "hidden-language-provider": "1.0.0" }, {});

  const requests = [];
  const injected = (request, issuer) => {
    requests.push([request, issuer]);
    if (request === "hidden-language-provider/package.json") {
      return join(store, "node_modules/hidden-language-provider/package.json");
    }
    throw new Error(`unresolved ${request}`);
  };

  const state = await discoverWorkspaceFolder(folder, {
    discover: discoverProviders,
    resolve: injected,
  });
  assert.deepEqual(state.extensions, [".hid"]);
  assert.equal(state.clients.length, 1);
  assert.equal(
    state.clients[0].executable,
    join(store, "node_modules/hidden-language-provider/bin/hidden-server"),
  );
  assert.deepEqual(requests[0], [
    "hidden-language-provider/package.json",
    join(folder, "package.json"),
  ]);
});

test("a Plug'n'Play manifest at the folder root supplies the resolver", async (context) => {
  const directory = await temporaryRoot(context, "pnp");
  const calls = [];
  const api = {
    resolveRequest(request, issuer) {
      calls.push([request, issuer]);
      return null;
    },
  };
  const resolver = loadFolderResolver(directory, {
    existsSync: (path) => path === join(directory, ".pnp.cjs"),
    requireModule: (path) => (path === join(directory, ".pnp.cjs") ? api : null),
  });
  assert.equal(typeof resolver, "function");
  resolver("some-package/package.json", join(directory, "package.json"));
  assert.deepEqual(calls, [
    ["some-package/package.json", join(directory, "package.json")],
  ]);

  assert.equal(
    loadFolderResolver(directory, { existsSync: () => false }),
    undefined,
    "a folder without a manifest keeps ordinary resolution",
  );
  assert.equal(
    loadFolderResolver(directory, {
      existsSync: () => true,
      requireModule: () => ({}),
    }),
    undefined,
    "a manifest without resolveRequest is ignored",
  );
});

test("primitives are deterministic and case-insensitive", () => {
  assert.equal(documentExtension("/a/b/View.TSX"), ".tsx");
  assert.equal(documentExtension("C:\\a\\View.Demo"), ".demo");
  assert.equal(documentExtension("Makefile"), null);
  assert.equal(documentExtension(".gitignore"), null);
  assert.equal(documentExtension(undefined), null);
  assert.deepEqual(providerDocumentSelector([".b", ".a", ".b", "bad"]), [
    { scheme: "file", pattern: "**/*.a" },
    { scheme: "file", pattern: "**/*.b" },
  ]);
  assert.deepEqual(providerDocumentSelector(undefined), []);
  assert.deepEqual(providerLanguageClients(undefined), []);
  assert.deepEqual(providerLanguageClients({ providers: [], extensions: {} }), []);
});

test("a non-interpreted executable is launched directly", () => {
  const providerRoot = join(sep, "packages", "native-provider");
  const executable = join(providerRoot, "bin", "server");
  const index = {
    providers: [
      {
        name: "native-provider",
        id: "native",
        root: providerRoot,
        languages: [
          { id: "native", extensions: [".nat"], capabilities: { lsp: { kind: "bin", path: executable } } },
        ],
      },
    ],
    extensions: { ".nat": { package: "native-provider" } },
  };
  const [client] = providerLanguageClients(index, { readShebang: () => "\u007fELF" });
  assert.equal(client.command, executable);
  assert.deepEqual(client.args, ["--stdio"]);

  const escaped = providerLanguageClients(
    {
      providers: [
        {
          name: "escaping-provider",
          id: "escaping",
          root: providerRoot,
          languages: [
            {
              id: "escaping",
              extensions: [".esc"],
              capabilities: { lsp: { kind: "bin", path: join(sep, "usr", "bin", "server") } },
            },
          ],
        },
      ],
      extensions: { ".esc": { package: "escaping-provider" } },
    },
    { readShebang: () => "" },
  );
  assert.deepEqual(escaped, [], "a command outside the provider package is never built");
});
