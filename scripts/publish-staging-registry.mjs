import { createServer } from "node:http";

/**
 * A registry that has never heard of anything.
 *
 * `npm publish --dry-run` reads the packument for the name it is about to
 * publish and refuses when the version is already there. That refusal is
 * correct against npmjs.com at release time, and it is exactly wrong for a gate
 * that must be runnable at any moment on the version the workspace currently
 * holds: every CI run would fail with "cannot publish over the previously
 * published versions" and prove nothing about the artifact.
 *
 * Pointing the rehearsal at a registry that answers 404 for every name removes
 * the version conflict and leaves everything else npm does intact: it reads the
 * tarball, validates the manifest, applies the ignore rules, prints the file
 * list, and stops before the PUT because it is a dry run. The real
 * already-published check still exists where it belongs, in the publish
 * workflow's own rehearsal against npmjs.com.
 */
export async function startStagingRegistry() {
  const requests = [];
  const server = createServer((request, response) => {
    requests.push({ method: request.method, url: request.url });
    // A dry run must never reach a write, so a write is a hard error rather
    // than a 404 that npm could interpret as "create this package".
    if (!["GET", "HEAD"].includes(request.method)) {
      response.statusCode = 500;
      response.end(`the publish rehearsal attempted ${request.method} ${request.url}`);
      return;
    }
    response.statusCode = 404;
    response.setHeader("content-type", "application/json");
    response.end(`${JSON.stringify({ error: "Not found" })}\n`);
  });
  await new Promise((resolveListen, rejectListen) => {
    server.once("error", rejectListen);
    server.listen(0, "127.0.0.1", () => {
      server.off("error", rejectListen);
      resolveListen();
    });
  });
  server.unref();
  const { port } = server.address();
  return {
    url: `http://127.0.0.1:${port}/`,
    requests,
    wrote: () => requests.some((entry) => !["GET", "HEAD"].includes(entry.method)),
    close: () =>
      new Promise((resolveClose, rejectClose) => {
        server.close((error) => (error ? rejectClose(error) : resolveClose()));
      }),
  };
}
