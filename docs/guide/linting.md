---
title: Linting
description: How oxc-tsrx runs real OXC lint rules on .tsrx files and reports errors at your actual code.
---

# Linting

`oxc-tsrx` lints `.tsrx` files (and ordinary JS/TS files) with OXC's real lint
rules. Authored descendants use the canonical rule engine; diagnostics or
fixes that would touch TSRX projection scaffolding are suppressed rather than
presented as equivalent Oxlint behavior.

## How a lint run works

<!-- pipeline:lint -->

Step by step:

1. The file is scanned once to find the TSRX-only syntax.
2. A valid-TSX copy is built in memory (the *projection*). Your code is copied
   into it unchanged; only the TSRX control syntax is replaced with TSX
   placeholders. The tool records which byte ranges in the copy correspond to
   which byte ranges in your file.
3. OXC parses and lints that copy, once.
4. Every diagnostic is translated back through those recorded ranges, so the
   error you see points at the right line and column in your `.tsrx` file.

Ordinary `.js`, `.jsx`, `.ts`, and `.tsx` files skip steps 1–2 entirely: the
file goes straight to OXC, exactly like running `oxlint` yourself.

## See the projection for yourself

The tabs below show one real file at each stage. The projected TSX is the
actual output of the projection engine, and the diagnostics are actual
`oxc-tsrx` output. Notice how the `@`-controls become scaffold comments and
wrappers in tab 2, while your code (like `var total` and `debugger;`) is
byte-for-byte identical, which is what makes exact mapping possible:

<!-- projection-explorer -->

## Usage

<!-- terminal-demo:linting-usage -->

CLI severity flags (`--allow`/`-A`, `--warn`/`-W`, `--deny`/`-D`) override
whatever the config file says for that rule. Exit codes: `0` clean, `1` when
there are errors (or the configured warning policy fails), `2` for usage or
engine errors.

## Why you never see errors in code you didn't write

The placeholders in the projected copy are code too, and sometimes a lint rule
fires on *them* instead of on your code. When that happens, the whole
diagnostic is dropped (and counted in metadata) rather than shown. The rule
is simple: a diagnostic must land entirely inside bytes you wrote, or you
don't see it. No error will ever point into invisible scaffolding.

## Safe fixes

`--fix` applies fixes directly to your original TSRX file, but only fixes
that touch purely your own code. After applying, the tool re-scans and
re-parses the result to confirm it's still valid before writing anything. A
fix that would touch the TSRX control syntax or span a placeholder boundary
is rejected.

## Configuration

`oxc-tsrx` finds one `.oxlintrc.json` or `.oxlintrc.jsonc` by searching from
your working directory upward, or takes an explicit `--config`/`-c` path. The
usual Oxlint fields work: built-in rules and plugins with options, `env`,
`globals`, `settings`, `extends`, `overrides`, `ignorePatterns`, and warning
policy.

JavaScript lint plugins and direct-native JS/TS config files are rejected up
front instead of half-working. Type-aware lint is supported only through an
explicit flag and the exact verified `oxlint-tsgolint` 0.24.0 executable; a
missing or mismatched tool fails without silently falling back. See
[Configuration](/integrations/configuration) for the exact support matrix
and [Custom JavaScript plugins](/integrations/custom-js-plugins) for the
tested parser adapters and remaining host boundary.

## What is tested

The retained test suite proves `no-debugger` and `no-unused-vars` report at
the correct original positions, and that `no-var` fixes apply correctly,
across all the control-flow forms (`@if`, `@for`, `@switch`, `@try`). It does
not claim every OXC rule behaves identically around the placeholders; that
broader claim is deliberately left unclaimed until proven. A separate matrix
proves authored type-aware labels, explicit `.tsrx` imports, one type process
per batch, and identity-safe fixes without changing the default one-OXC-parse
path.
