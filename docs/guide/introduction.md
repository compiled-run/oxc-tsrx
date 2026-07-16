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
`@try`, `@{ }` function bodies, dynamic tags like `<{expr}>`, and inline raw
`<style>` blocks.

Stock `oxlint` and `oxfmt` don't know that syntax. Point them at a `.tsrx`
file and they see a parse error at the first `@if`. So TSRX projects would
normally lose linting and formatting entirely. That's the gap this project
closes.

## How it works, in plain terms

OXC only understands regular TS/TSX, so for every `.tsrx` file the tool:

1. **Scans** the file once and records where the TSRX-only syntax is.
2. **Projects** it: builds an in-memory copy where each TSRX construct is
   swapped for equivalent, valid TSX placeholders. Your real code between
   those constructs is copied byte-for-byte, and the tool remembers exactly
   which byte ranges are "your code" and which are placeholder.
3. **Runs the real OXC** (parser, then linter or formatter) on that copy.
   Exactly once. Even dynamic tags are validated against this same parse.
4. **Maps the results back** to your original file. Lint errors point at your
   actual `.tsrx` lines and columns. For formatting, a final step (the
   *lift*) converts the formatted TSX copy back into TSRX and double-checks
   that nothing structural changed.

Your file on disk is always real TSRX. The TSX copy never touches disk; it
exists only so OXC can do its job.

Ordinary `.js`, `.jsx`, `.ts`, and `.tsx` files skip all of this and go
straight to OXC, byte-for-byte identical to running the stock tools.

## What it promises

- **Real rules, real layout.** These are OXC's own lint rules and Oxfmt's own
  formatting, not a reimplementation.
- **Errors point at your code.** A diagnostic is only shown if it maps cleanly
  onto bytes you actually wrote. If a rule fires on placeholder scaffolding
  instead, the tool hides that diagnostic (and counts it) rather than showing
  you a confusing error in code you can't see.
- **Safe fixes.** `--fix` edits your original TSRX, then re-checks the result
  parses before anything is written.
- **Opt-in type-aware rules.** `--type-aware` adds the official tsgolint
  rules, and `--type-check` adds full TypeScript compiler diagnostics, through
  one TypeScript-Go process per batch. The default lane stays syntax-only and
  starts zero type processes. See [Linting](/guide/linting.html).
- **Editor support.** A native language server (`oxc-tsrx-lsp`) and a thin
  VS Code companion provide live diagnostics, format-on-save, and validated
  quick fixes next to your framework's own extension. See
  [Editor integration](/integrations/editor.html).
- **No fork.** All OXC calls live in one adapter crate pinned to a single
  upstream commit (`8e0ed2ebb96137fb1611cdbd5742d5cb46037d40`). Upgrading OXC
  means updating that one crate and re-running the full test and benchmark
  suites.
- **Fail closed.** If a file uses TSRX syntax the tool doesn't support yet, you
  get a clear error, never a silently skipped file or a half-right result.

## The commands

Install the npm packages `oxlint-tsrx` and `oxfmt-tsrx` and you get the
familiar commands, now with TSRX support:

```sh
# Lint .tsrx and ordinary JS/TS with real OXC rules
npx oxlint --format=json src/Counter.tsrx src/View.tsx

# Format .tsrx and ordinary JS/TS with real Oxfmt layout
npx oxfmt --check src/Counter.tsrx src/View.tsx
```

Both read your normal Oxlint/Oxfmt JSON config once per run and reuse it for
every file. Each command runs a native Rust binary (`oxc-tsrx` and
`oxc-tsrx-fmt`; a third one, `oxc-tsrx-lsp`, serves the same engines to
editors). See [Getting Started](/guide/getting-started.html) to install or
build them, and the [CLI reference](/reference/cli.html) for every flag.

## What it deliberately is not

- **Not a Vite plugin.** Your framework plugin keeps owning compilation, CSS,
  source maps, and HMR. This project adds nothing to your build or dev
  server. See [Vite and Vite+](/integrations/vite-plus.html).
- **Not a CSS formatter.** Whatever is inside a raw `<style>` block is kept
  byte-for-byte, never reformatted or validated.
- **Not finished.** Some syntax and config features aren't supported yet, and
  they fail with clear errors instead of degrading quietly. See
  [Limitations](/reference/limitations.html).
