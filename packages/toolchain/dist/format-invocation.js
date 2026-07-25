import { statSync } from "node:fs";
import { resolve } from "node:path";

export { importDeclaredPackageBinary } from "./package-binary.js";

// This small front router intentionally recognizes only the options supported
// by the pinned Oxfmt CLI. Unknown future syntax falls back to the TSRX-aware
// bridge instead of risking a false ordinary-only classification.
export const VALUE_OPTIONS = new Set([
  "-c",
  "--config",
  "--migrate",
  "--stdin-filepath",
  "--ignore-path",
  "--threads",
]);

export const DELEGATE_ONLY = new Set([
  "--help",
  "-h",
  "--version",
  "-V",
  "--init",
  "--migrate",
  "--lsp",
]);

const FLAG_OPTIONS = new Set([
  "--write",
  "--check",
  "--list-different",
  "--disable-nested-config",
  "--with-node-modules",
  "--no-error-on-unmatched-pattern",
]);

export function parseOxfmtOption(argument) {
  const equals = argument.indexOf("=");
  if (equals !== -1) {
    return { name: argument.slice(0, equals), value: argument.slice(equals + 1) };
  }
  if (argument.startsWith("-c") && argument.length > 2) {
    return { name: "-c", value: argument.slice(2) };
  }
  return { name: argument, value: null };
}

export function parseOxfmtInvocation(args) {
  const positionals = [];
  let positionalOnly = false;
  let delegateOnly = false;
  let known = true;
  let stdinFilepath = null;

  for (let index = 0; index < args.length; index += 1) {
    const argument = args[index];
    if (positionalOnly) {
      positionals.push(argument);
      continue;
    }
    if (argument === "--") {
      positionalOnly = true;
      continue;
    }
    if (!argument.startsWith("-") || argument === "-") {
      positionals.push(argument);
      continue;
    }

    const { name, value: inlineValue } = parseOxfmtOption(argument);
    if (DELEGATE_ONLY.has(name)) delegateOnly = true;
    if (VALUE_OPTIONS.has(name)) {
      const value = inlineValue ?? args[++index] ?? null;
      if (name === "--stdin-filepath") stdinFilepath = value;
      continue;
    }
    if (!DELEGATE_ONLY.has(name) && !FLAG_OPTIONS.has(name)) known = false;
  }

  return { positionals, delegateOnly, known, stdinFilepath };
}

export function canRunCanonicalOxfmt(args, cwd = process.cwd()) {
  const invocation = parseOxfmtInvocation(args);
  if (invocation.delegateOnly) return true;
  if (!invocation.known) return false;
  if (invocation.stdinFilepath !== null) {
    return (
      invocation.stdinFilepath.length > 0 &&
      !invocation.stdinFilepath.split("?")[0].endsWith(".tsrx")
    );
  }
  if (invocation.positionals.length === 0) return false;

  return invocation.positionals.every((argument) => {
    if (argument.split("?")[0].endsWith(".tsrx")) return false;
    try {
      return statSync(resolve(cwd, argument)).isFile();
    } catch {
      return false;
    }
  });
}
