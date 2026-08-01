import { spawn } from "node:child_process";
import { appendFileSync } from "node:fs";

function traceRunStart(trace, started, executable, args) {
  appendFileSync(
    trace,
    `${JSON.stringify({
      event: "start",
      pid: process.pid,
      ppid: process.ppid,
      started,
      executable,
      args,
      host: {
        vpVersion: process.env.VP_VERSION ?? null,
        vpCommand: process.env.VP_COMMAND ?? null,
        packageManager: process.env.NODE_PACKAGE_MANAGER ?? null,
        tsgolint: process.env.OXLINT_TSGOLINT_PATH ?? null,
      },
    })}\n`,
  );
}

function traceRunEnd(trace, started, executable, args, status, signal) {
  appendFileSync(
    trace,
    `${JSON.stringify({
      event: "end",
      pid: process.pid,
      ppid: process.ppid,
      started,
      ended: Date.now(),
      executable,
      args,
      status,
      signal,
    })}\n`,
  );
}

export function runCaptured(executable, args, options: any = {}) {
  return new Promise<any>((resolveRun, rejectRun) => {
    const trace = process.env.OXC_TSRX_TRACE_FILE;
    const started = Date.now();
    if (trace) traceRunStart(trace, started, executable, args);
    const child = spawn(executable, args, {
      cwd: options.cwd,
      env: options.env ?? process.env,
      stdio: ["pipe", "pipe", "pipe"],
    });
    let stdout = "";
    let stderr = "";
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => (stdout += chunk));
    child.stderr.on("data", (chunk) => (stderr += chunk));
    child.on("error", rejectRun);
    child.on("close", (status, signal) => {
      if (trace) traceRunEnd(trace, started, executable, args, status, signal);
      resolveRun({ status: status ?? 2, signal, stdout, stderr });
    });
    if (options.input === undefined) child.stdin.end();
    else child.stdin.end(options.input);
  });
}

// Long-lived interactive modes such as --lsp must inherit the wrapper's stdio
// instead of closing stdin and buffering output until the child exits.
export function runPassthrough(executable, args, options: any = {}) {
  return new Promise<any>((resolveRun, rejectRun) => {
    const trace = process.env.OXC_TSRX_TRACE_FILE;
    const started = Date.now();
    if (trace) traceRunStart(trace, started, executable, args);
    const child = spawn(executable, args, {
      cwd: options.cwd,
      env: options.env ?? process.env,
      stdio: "inherit",
    });
    child.on("error", rejectRun);
    child.on("close", (status, signal) => {
      if (trace) traceRunEnd(trace, started, executable, args, status, signal);
      resolveRun({ status: status ?? 2, signal });
    });
  });
}
