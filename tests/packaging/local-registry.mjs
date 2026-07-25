import assert from "node:assert/strict";
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";

function requestBody(request) {
  return new Promise((resolveBody, rejectBody) => {
    const chunks = [];
    request.on("data", (chunk) => chunks.push(chunk));
    request.on("error", rejectBody);
    request.on("end", () => resolveBody(Buffer.concat(chunks)));
  });
}

function listen(server) {
  return new Promise((resolveListen, rejectListen) => {
    server.once("error", rejectListen);
    server.listen(0, "127.0.0.1", () => {
      server.off("error", rejectListen);
      resolveListen();
    });
  });
}

function close(server) {
  return new Promise((resolveClose, rejectClose) => {
    server.close((error) => (error ? rejectClose(error) : resolveClose()));
  });
}

function packageName(pathname) {
  const decoded = decodeURIComponent(pathname.slice(1));
  if (!decoded || decoded.startsWith("__local__/")) return null;
  return decoded;
}

async function proxy(request, response, upstream) {
  const body = await requestBody(request);
  const target = new URL(request.url, upstream);
  const headers = {};
  for (const name of [
    "accept",
    "content-encoding",
    "content-type",
    "npm-auth-type",
    "npm-command",
    "user-agent",
  ]) {
    const value = request.headers[name];
    if (value) headers[name] = value;
  }
  const proxied = await fetch(target, {
    method: request.method,
    headers,
    body: ["GET", "HEAD"].includes(request.method) ? undefined : body,
    redirect: "manual",
  });
  response.statusCode = proxied.status;
  for (const name of ["content-type", "location", "npm-notice"]) {
    const value = proxied.headers.get(name);
    if (value) response.setHeader(name, value);
  }
  if (request.method === "HEAD") {
    response.end();
  } else {
    response.end(Buffer.from(await proxied.arrayBuffer()));
  }
}

/**
 * Serve untouched local package tarballs as ordinary registry releases while
 * proxying unrelated packages and audit requests to the public npm registry.
 */
export async function startLocalRegistry(entries, options = {}) {
  const upstream = options.upstream ?? "https://registry.npmjs.org/";
  const packages = new Map();
  const tarballs = new Map();
  for (const [index, entry] of entries.entries()) {
    assert.equal(typeof entry.manifest?.name, "string");
    assert.equal(typeof entry.manifest?.version, "string");
    assert.equal(typeof entry.tarball, "string");
    const tarballPath = `/__local__/${index}/${encodeURIComponent(entry.manifest.name)}-${entry.manifest.version}.tgz`;
    packages.set(entry.manifest.name, { ...entry, tarballPath });
    tarballs.set(tarballPath, entry.tarball);
  }

  let origin;
  const requests = [];
  const server = createServer(async (request, response) => {
    try {
      const url = new URL(request.url, origin ?? "http://127.0.0.1");
      requests.push({ method: request.method, pathname: url.pathname });
      const tarball = tarballs.get(url.pathname);
      if (tarball && ["GET", "HEAD"].includes(request.method)) {
        response.statusCode = 200;
        response.setHeader("content-type", "application/octet-stream");
        response.end(request.method === "HEAD" ? undefined : await readFile(tarball));
        return;
      }

      const name = packageName(url.pathname);
      const entry = name ? packages.get(name) : null;
      if (entry && ["GET", "HEAD"].includes(request.method)) {
        const manifest = {
          ...entry.manifest,
          _id: `${entry.manifest.name}@${entry.manifest.version}`,
          dist: {
            tarball: `${origin}${entry.tarballPath.slice(1)}`,
            integrity: entry.integrity,
            shasum: entry.shasum,
          },
        };
        const contents = Buffer.from(
          `${JSON.stringify({
            name: entry.manifest.name,
            "dist-tags": { latest: entry.manifest.version },
            versions: { [entry.manifest.version]: manifest },
          })}\n`,
        );
        response.statusCode = 200;
        response.setHeader("content-type", "application/json");
        response.end(request.method === "HEAD" ? undefined : contents);
        return;
      }

      await proxy(request, response, upstream);
    } catch (error) {
      response.statusCode = 502;
      response.setHeader("content-type", "text/plain");
      response.end(error instanceof Error ? error.stack : String(error));
    }
  });
  await listen(server);
  server.unref();
  const address = server.address();
  assert.ok(address && typeof address === "object");
  origin = `http://127.0.0.1:${address.port}/`;
  return {
    url: origin,
    requests,
    close: () => close(server),
  };
}
