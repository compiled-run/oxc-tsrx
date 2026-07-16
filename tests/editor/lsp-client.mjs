import assert from "node:assert/strict";
import { spawn } from "node:child_process";

export class LspClient {
  #buffer = Buffer.alloc(0);
  #child;
  #exit;
  #messages = [];
  #nextId = 1;
  #stderr = "";
  #waiters = [];

  constructor(executable, options = {}) {
    this.#child = spawn(executable, options.args ?? [], {
      cwd: options.cwd,
      env: options.env ?? process.env,
      stdio: ["pipe", "pipe", "pipe"],
    });
    this.#child.stderr.setEncoding("utf8");
    this.#child.stderr.on("data", (chunk) => (this.#stderr += chunk));
    this.#child.stdout.on("data", (chunk) => this.#onData(chunk));
    this.#child.on("exit", (code, signal) => (this.#exit = { code, signal }));
  }

  get stderr() {
    return this.#stderr;
  }

  get pid() {
    return this.#child.pid;
  }

  notify(method, params = {}) {
    this.#write({ jsonrpc: "2.0", method, params });
  }

  async request(method, params = {}, timeout = 5000) {
    const id = this.#nextId++;
    this.#write({ jsonrpc: "2.0", id, method, params });
    let message;
    try {
      message = await this.waitFor((candidate) => candidate.id === id, timeout);
    } catch (error) {
      throw new Error(`${method}: ${error.message}`, { cause: error });
    }
    if (message.error) {
      throw new Error(`${method}: ${JSON.stringify(message.error)}\n${this.#stderr}`);
    }
    return message.result;
  }

  waitFor(predicate, timeout = 5000, label = "message") {
    const existing = this.#messages.findIndex(predicate);
    if (existing !== -1) return Promise.resolve(this.#messages.splice(existing, 1)[0]);
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        const index = this.#waiters.findIndex((waiter) => waiter.resolve === resolve);
        if (index !== -1) this.#waiters.splice(index, 1);
        reject(
          new Error(
            `LSP ${label} timeout; exit=${JSON.stringify(this.#exit)} queued=${JSON.stringify(this.#messages)}\n${this.#stderr}`,
          ),
        );
      }, timeout);
      this.#waiters.push({
        predicate,
        resolve(message) {
          clearTimeout(timer);
          resolve(message);
        },
      });
    });
  }

  async initialize(rootUri, initializationOptions = {}) {
    const result = await this.request("initialize", {
      processId: process.pid,
      rootUri,
      capabilities: {},
      workspaceFolders: [{ uri: rootUri, name: "editor-test" }],
      initializationOptions,
    });
    this.notify("initialized", {});
    return result;
  }

  async close() {
    try {
      await this.request("shutdown", null, 2000);
      this.notify("exit", {});
    } finally {
      this.#child.stdin.end();
    }
    await new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.#child.kill();
        reject(new Error(`LSP process did not exit\n${this.#stderr}`));
      }, 2000);
      this.#child.once("exit", (status) => {
        clearTimeout(timer);
        if (status === 0 || status === null) resolve();
        else reject(new Error(`LSP exited ${status}\n${this.#stderr}`));
      });
    });
  }

  terminate() {
    if (this.#child.exitCode === null) this.#child.kill();
  }

  #write(message) {
    const body = Buffer.from(JSON.stringify(message));
    this.#child.stdin.write(`Content-Length: ${body.length}\r\n\r\n`);
    this.#child.stdin.write(body);
  }

  #onData(chunk) {
    this.#buffer = Buffer.concat([this.#buffer, chunk]);
    for (;;) {
      const marker = this.#buffer.indexOf("\r\n\r\n");
      if (marker === -1) return;
      const header = this.#buffer.subarray(0, marker).toString("ascii");
      const match = /(?:^|\r\n)Content-Length:\s*(\d+)/i.exec(header);
      assert.ok(match, `Missing Content-Length in ${header}`);
      const length = Number(match[1]);
      const bodyStart = marker + 4;
      if (this.#buffer.length < bodyStart + length) return;
      const body = this.#buffer.subarray(bodyStart, bodyStart + length);
      this.#buffer = this.#buffer.subarray(bodyStart + length);
      this.#deliver(JSON.parse(body.toString("utf8")));
    }
  }

  #deliver(message) {
    if (
      message.id !== undefined &&
      (message.method === "client/registerCapability" ||
        message.method === "client/unregisterCapability")
    ) {
      this.#write({ jsonrpc: "2.0", id: message.id, result: null });
      return;
    }
    const waiter = this.#waiters.findIndex(({ predicate }) => predicate(message));
    if (waiter === -1) this.#messages.push(message);
    else this.#waiters.splice(waiter, 1)[0].resolve(message);
  }
}

export function pathToFileUri(path) {
  return new URL(`file://${path}`).href;
}

export function positionToOffset(source, position) {
  const lines = source.split("\n");
  let offset = 0;
  for (let line = 0; line < position.line; line += 1) offset += lines[line].length + 1;
  let utf16 = 0;
  for (const character of lines[position.line] ?? "") {
    if (utf16 >= position.character) break;
    offset += character.length;
    utf16 += character.length;
  }
  return offset;
}

export function applyTextEdits(source, edits) {
  const ordered = [...edits]
    .map((edit) => ({
      ...edit,
      start: positionToOffset(source, edit.range.start),
      end: positionToOffset(source, edit.range.end),
    }))
    .sort((left, right) => right.start - left.start);
  let output = source;
  for (const edit of ordered) {
    output = output.slice(0, edit.start) + edit.newText + output.slice(edit.end);
  }
  return output;
}
