#!/usr/bin/env node

import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { resolve } from "node:path";

const registrationId = "$oxc-tsrx/register-tsrx-documents";
const root = resolve(import.meta.dirname, "..");
const upstream = resolve(
  process.env.OXC_TSRX_CUSTOM_OXLINT_BIN ??
    resolve(root, "target/oxlint-custom-parser/cli.js"),
);

if (!existsSync(upstream)) {
  throw new Error(
    `Custom-parser Oxlint is missing at ${upstream}. ` +
      "Set OXC_TSRX_CUSTOM_OXLINT_BIN or build the upstream draft into " +
      "target/oxlint-custom-parser.",
  );
}

const child = spawn(process.execPath, [upstream, ...process.argv.slice(2)], {
  cwd: process.cwd(),
  env: { ...process.env, NO_COLOR: "1" },
  stdio: ["pipe", "pipe", "pipe"],
});

function writeMessage(stream, message) {
  const body = Buffer.from(JSON.stringify(message));
  stream.write(`Content-Length: ${body.length}\r\n\r\n`);
  stream.write(body);
}

function readMessages(stream, onMessage) {
  let input = Buffer.alloc(0);
  stream.on("data", (chunk) => {
    input = Buffer.concat([input, chunk]);
    for (;;) {
      const boundary = input.indexOf("\r\n\r\n");
      if (boundary === -1) return;
      const header = input.subarray(0, boundary).toString("ascii");
      const match = /content-length:\s*(\d+)/iu.exec(header);
      if (!match) throw new Error(`Missing Content-Length header: ${header}`);
      const length = Number(match[1]);
      const end = boundary + 4 + length;
      if (input.length < end) return;
      const body = input.subarray(boundary + 4, end).toString("utf8");
      input = input.subarray(end);
      onMessage(JSON.parse(body));
    }
  });
}

function registerTsrxDocuments() {
  const documentSelector = [{ scheme: "file", pattern: "**/*.tsrx" }];
  writeMessage(process.stdout, {
    jsonrpc: "2.0",
    id: registrationId,
    method: "client/registerCapability",
    params: {
      registrations: [
        {
          id: "oxc-tsrx-did-open",
          method: "textDocument/didOpen",
          registerOptions: { documentSelector },
        },
        {
          id: "oxc-tsrx-did-change",
          method: "textDocument/didChange",
          registerOptions: { documentSelector, syncKind: 1 },
        },
        {
          id: "oxc-tsrx-did-save",
          method: "textDocument/didSave",
          registerOptions: { documentSelector },
        },
        {
          id: "oxc-tsrx-did-close",
          method: "textDocument/didClose",
          registerOptions: { documentSelector },
        },
        {
          id: "oxc-tsrx-diagnostics",
          method: "textDocument/diagnostic",
          registerOptions: {
            documentSelector,
            identifier: "oxc-tsrx-custom-parser",
            interFileDependencies: false,
            workspaceDiagnostics: false,
          },
        },
      ],
    },
  });
}

let registered = false;
readMessages(process.stdin, (message) => {
  if (message.id === registrationId && message.method === undefined) {
    if (message.error) {
      process.stderr.write(
        `OXC for TSRX client request failed: ${JSON.stringify(message.error)}\n`,
      );
    }
    return;
  }
  writeMessage(child.stdin, message);
  if (message.method === "initialized" && !registered) {
    registered = true;
    registerTsrxDocuments();
  }
});
readMessages(child.stdout, (message) => {
  writeMessage(process.stdout, message);
});

child.stderr.pipe(process.stderr);
process.stdin.on("end", () => child.stdin.end());
child.on("error", (error) => {
  process.stderr.write(`OXC for TSRX could not start Oxlint: ${error.message}\n`);
  process.exitCode = 2;
});
child.on("close", (status, signal) => {
  if (signal) process.stderr.write(`Oxlint exited with signal ${signal}\n`);
  process.exitCode = status ?? 2;
});

for (const signal of ["SIGINT", "SIGTERM"]) {
  process.on(signal, () => child.kill(signal as NodeJS.Signals));
}
