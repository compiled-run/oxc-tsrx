---
title: Introduction
description: What OXC for TSRX is, how it works in plain terms, and what it promises.
---

# Introduction

[OXC](https://oxc.rs) is a JavaScript and TypeScript toolchain written in
Rust: a parser, the `oxlint` linter, and the `oxfmt` formatter. It does the
same jobs as ESLint and Prettier, just much faster.

OXC for TSRX teaches those tools to read `.tsrx` files, so you can lint and
format them like any other file in your project. The parser behind them is
also available as a library, [`oxc-tsrx/parser`](/guide/parsing), for
building your own tooling.

## The problem it solves

TSRX is TypeScript/JSX plus template control flow: `@if`, `@for`, `@switch`,
`@try`, `@{ }` statement containers (a successor and extension of JSX
expression containers), dynamic tags like `<{expr}>`, and inline raw
`<style>` blocks.

OXC on its own doesn't understand TSRX. Point `oxlint` or `oxfmt` at a
`.tsrx` file and they see a parse error at the first `@if`. So TSRX projects
would normally lose linting and formatting entirely. That's the gap this
project closes.

## How it works, in plain terms

OXC only understands regular TS/TSX, so every `.tsrx` file goes through the
same four steps.

<!-- how-it-works -->

Your file on disk is always real TSRX. The TSX copy never touches disk; it
exists only so OXC can do its job.

Ordinary `.js`, `.jsx`, `.ts`, and `.tsx` files skip all of this and go
straight to OXC, byte-for-byte identical to running `oxlint` and `oxfmt`
yourself.

## What it promises

- ✅ **Real rules, your config.** OXC's own lint rules and Oxfmt's own
  formatting, driven by the `.oxlintrc.json` and `.oxfmtrc.json` you already
  have. In a Vite+ project, `vp lint` reads the `lint` block of your
  `vite.config.ts` instead.
- 🎯 **Errors point at your code.** Every warning lands on a line you actually
  wrote. Ones that fire on the hidden placeholder code are counted, not shown.
- 🛡️ **Safe fixes.** `--fix` edits your original TSRX and confirms the result
  still parses before writing anything.
- 🧠 **Opt-in type-aware rules.** `--type-aware` adds the official tsgolint
  rules, `--type-check` adds full TypeScript diagnostics. The default lane
  starts zero type processes. See [Linting](/guide/linting).
- ✏️ **Editor support.** The official OXC extension picks up the project-local
  `oxlint` that `oxc-tsrx` installs, and your `.tsrx` files get live
  diagnostics, formatting, and quick fixes. Your `.ts` and `.tsx` files keep
  working exactly as before. See [Editor integration](/integrations/editor).
- 🔗 **No fork.** Nothing is snapshotted and nothing is patched. Every OXC call
  lives in one small adapter crate, so upgrading OXC means updating that one
  crate.
- 🚦 **Fail closed.** Unsupported TSRX syntax gets a clear error, never a
  silently skipped file or a half-right result.

## The commands

Install `oxc-tsrx` and you get the familiar commands, now with TSRX support:

<!-- terminal-demo:introduction-commands -->

Every command is the same native Rust binary, `oxc-tsrx`: the linter, the
formatter, and the editor language server in one download, picked by
subcommand. See [Getting Started](/guide/getting-started) to install it and the
[CLI reference](/reference/cli) for every flag.

## What it deliberately is not

- **Not a compiler.** Your framework's TSRX plugin, installed from
  [tsrx.dev/getting-started](https://tsrx.dev/getting-started), owns
  compilation, CSS, source maps, and HMR. Nothing here touches your build or dev
  server. See [Try it with Vite+](/guide/getting-started#try-it-with-vite).
- **Not a CSS formatter.** Anything inside a raw `<style>` block is left alone,
  byte for byte.
- **Not finished.** Some syntax and config options aren't supported yet. They
  fail with a clear error instead of quietly doing the wrong thing. See
  [Limitations](/reference/limitations).
