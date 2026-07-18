---
title: Introduction
description: What OXC for TSRX is, how it works in plain terms, and what it promises.
---

# Introduction

OXC for TSRX lets you lint and format `.tsrx` files with [OXC](https://oxc.rs),
the same fast Rust tools behind Oxlint and Oxfmt, without forking or
patching OXC in any way.

## The problem it solves

TSRX is TypeScript/JSX plus template control flow: `@if`, `@for`, `@switch`,
`@try`, `@{ }` statement containers (a successor and extension of JSX
expression containers), dynamic tags like `<{expr}>`, and inline raw
`<style>` blocks.

Stock `oxlint` and `oxfmt` don't know that syntax. Point them at a `.tsrx`
file and they see a parse error at the first `@if`. So TSRX projects would
normally lose linting and formatting entirely. That's the gap this project
closes.

## How it works, in plain terms

OXC only understands regular TS/TSX, so every `.tsrx` file goes through the
same four steps.

<!-- how-it-works -->

Your file on disk is always real TSRX. The TSX copy never touches disk; it
exists only so OXC can do its job.

Ordinary `.js`, `.jsx`, `.ts`, and `.tsx` files skip all of this and go
straight to OXC, byte-for-byte identical to running the stock tools.

## What it promises

- ✅ **Real rules, real layout.** OXC's own lint rules and Oxfmt's own
  formatting, not a reimplementation.
- 🎯 **Errors point at your code.** Diagnostics only show when they map onto
  bytes you actually wrote. Anything that fires on hidden placeholder code is
  counted, not shown.
- 🛡️ **Safe fixes.** `--fix` edits your original TSRX and confirms the result
  still parses before writing anything.
- 🧠 **Opt-in type-aware rules.** `--type-aware` adds the official tsgolint
  rules, `--type-check` adds full TypeScript diagnostics. The default lane
  starts zero type processes. See [Linting](/guide/linting).
- ✏️ **Editor support.** A native language server plus a thin VS Code
  companion: live diagnostics, format-on-save, and validated quick fixes. See
  [Editor integration](/integrations/editor).
- 🔗 **No fork.** Every OXC call lives in one adapter crate pinned to a single
  upstream commit. Upgrading OXC means updating that one crate.
- 🚦 **Fail closed.** Unsupported TSRX syntax gets a clear error, never a
  silently skipped file or a half-right result.

## The commands

Install the npm packages `oxlint-tsrx` and `oxfmt-tsrx` and you get the
familiar commands, now with TSRX support:

<!-- terminal-demo:introduction-commands -->

Both read your normal Oxlint/Oxfmt JSON config once per run and reuse it for
every file. Each command runs a native Rust binary (`oxc-tsrx` and
`oxc-tsrx-fmt`; a third one, `oxc-tsrx-lsp`, serves the same engines to
editors). See [Getting Started](/guide/getting-started) to install or
build them, and the [CLI reference](/reference/cli) for every flag.

## What it deliberately is not

- **Not a Vite plugin.** Your framework plugin keeps owning compilation, CSS,
  source maps, and HMR. This project adds nothing to your build or dev
  server. See [Vite and Vite+](/integrations/vite-plus).
- **Not a CSS formatter.** Whatever is inside a raw `<style>` block is kept
  byte-for-byte, never reformatted or validated.
- **Not finished.** Some syntax and config features aren't supported yet, and
  they fail with clear errors instead of degrading quietly. See
  [Limitations](/reference/limitations).
