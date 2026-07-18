---
title: Formatting
description: How oxc-tsrx-fmt formats .tsrx files with real Oxfmt layout and converts the result back to TSRX.
---

# Formatting

`oxc-tsrx-fmt` formats `.tsrx` files with Oxfmt, OXC's formatter, and then
converts the result back into TSRX. Ordinary JS/TS files are formatted by
Oxfmt directly, with a byte-for-byte-identical-output guarantee versus the
stock tool.

## How a format run works

<!-- pipeline:format -->

Step by step:

1. The file is scanned and projected to a valid-TSX copy, the same idea as
   [linting](/guide/linting), except here the placeholders are special
   markers designed to survive formatting.
2. Oxfmt parses and formats that copy. Once.
3. The *lift* walks the formatted output and turns it back into TSRX:
   markers become `@if`/`@for`/`@switch`/`@try` again, your code keeps its
   new formatting, dynamic closing tags are rebuilt from their opening
   expression, and raw `<style>` contents are copied from your original file
   untouched.
4. As a final safety check, the lifted result is re-scanned and must match
   the structural fingerprint of the input. If anything doesn't line up, the
   tool errors out instead of writing a broken file.

## Usage

<!-- terminal-demo:formatting-usage -->

Writes are transactional: every file in the batch must format successfully
before the first one is replaced on disk, so a crash or bad file never leaves
your project half-formatted. Symbolic links are rejected.

## Configuration

`oxc-tsrx-fmt` finds one `.oxfmtrc.json` or `.oxfmtrc.jsonc` by searching
upward, or takes `--config`/`-c`. The standard layout options work:
`printWidth`, `singleQuote`, `semi`, `useTabs`, `tabWidth`, `trailingComma`,
`arrowParens`, `bracketSpacing`, `singleAttributePerLine`, and more, plus
`overrides` and `ignorePatterns`. The full list is in
[Configuration](/integrations/configuration).

Options that could silently change TSRX output in unsupported ways are
rejected with a clear error before anything is formatted or written:
`sortImports`, `jsdoc` formatting, embedded-language formatting, experimental
flags, `.editorconfig`, and JS/TS config files.

## CSS inside `<style>` is preserved, not formatted

Bytes inside a raw `<style>` element are copied through exactly as you wrote
them. The upstream OXC CSS formatter currently can't be used without patching
OXC's dependency graph, and this project's core rule is *no patches*, so CSS
formatting waits until upstream exposes a clean package boundary.

## How we know the lift is safe

A pinned, read-only corpus of real-world TSRX (the Markless oracle) proves
that all 179 parser-valid tracked files format, re-parse, and converge
(formatting a formatted file changes nothing), and that all 12 known invalid
fixtures are rejected. Every raw `<style>` payload is compared byte-for-byte.
