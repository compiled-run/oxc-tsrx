---
title: Formatting
description: How oxc-tsrx formats .tsrx files with real Oxfmt layout and converts the result back to TSRX.
---

# Formatting

`oxfmt` formats your `.tsrx` files with OXC's real formatter, then converts the
result back into TSRX. Ordinary JS/TS files go straight to Oxfmt, and the output
is byte-for-byte identical to running `oxfmt` yourself.

## Input and output

Here is one of the files this project tests against, exactly as it is committed
before formatting:

```tsrx
type Item={id:string;label:string};export function Rows({items}:{items:Item[]})@{<ul>@for(item of items;index i;key item.id){<li data-index={i}>{item.label}</li>}@empty{<li>Empty</li>}</ul>}
```

becomes:

```tsrx
type Item = { id: string; label: string };
export function Rows({ items }: { items: Item[] }) @{
  <ul>
    @for (item of items; index i; key item.id) {
      <li data-index={i}>{item.label}</li>;
    } @empty {
      <li>Empty</li>;
    }
  </ul>;
}
```

Both files are committed fixtures, and the test suite formats the first one and
compares it against the second on every run, so this is real output rather than
a hand-written example. You can read them in
[tests/fixtures/control](https://github.com/markless-dev/oxc-tsrx/tree/main/tests/fixtures/control).

## How a format run works

<!-- pipeline:format -->

Four steps:

1. **Copy.** Build a valid TSX copy, the same idea as
   [linting](/guide/linting), except the placeholders here are markers designed
   to survive formatting.
2. **Format.** Oxfmt parses and formats that copy, once.
3. **Convert back.** Markers become `@if`, `@for`, `@switch`, and `@try` again,
   and your code keeps its new formatting.
4. **Check.** The result is re-read and compared against the original. If the
   structure does not match, the tool errors out instead of writing a broken
   file.

Two things are carried over rather than reformatted: dynamic closing tags are
rebuilt from their opening expression, and whatever is inside a raw `<style>`
block is copied from your file untouched.

## Usage

<!-- terminal-demo:formatting-usage -->

Writes are transactional: every file in the batch must format successfully
before the first one is replaced on disk, so a crash or bad file never leaves
your project half-formatted. Symbolic links are rejected.

## Configuration

Your `.oxfmtrc.json` works as usual. `oxfmt` searches upward from the current
directory to find it, or takes a `--config` path, and the layout options you
already use all apply: `printWidth`, `singleQuote`, `semi`, `tabWidth`,
`trailingComma`, and the rest, plus `overrides` and `ignorePatterns`.
[Configuration](/integrations/configuration) has the full list.

A few options are refused with a clear error before anything is written,
because they would change `.tsrx` output in ways this project cannot yet
guarantee: `sortImports`, `jsdoc`, embedded-language formatting, experimental
flags, `.editorconfig`, and JS or TS config files.

## CSS inside `<style>` is preserved, not formatted

Bytes inside a raw `<style>` element are copied through exactly as you wrote
them. The upstream OXC CSS formatter currently can't be used without patching
OXC's dependency graph, and this project's core rule is *no patches*, so CSS
formatting waits until upstream exposes a clean package boundary.

## How we know it is safe

This is tested against a real TSRX codebase, not just fixtures. All 179 valid
files format, re-parse, and settle, meaning formatting an already-formatted file
changes nothing. All 12 broken files are rejected rather than mangled, and every
raw `<style>` block comes out byte for byte identical.
