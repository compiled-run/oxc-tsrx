<p align="center">
  <a href="https://oxc-tsrx.dev/"><img alt="OXC for TSRX" width="600" src="https://raw.githubusercontent.com/markless-dev/oxc-tsrx/HEAD/.github/assets/readme-hero.png"></a>
</p>

<p align="center">
  <a href="https://www.npmjs.com/package/oxc-tsrx"><img alt="npm version" src="https://img.shields.io/npm/v/oxc-tsrx.svg"></a>
  <a href="https://nodejs.org/en/about/previous-releases"><img alt="supported Node.js versions" src="https://img.shields.io/node/v/oxc-tsrx.svg"></a>
  <a href="https://github.com/markless-dev/oxc-tsrx/actions/workflows/ci.yml"><img alt="CI status" src="https://github.com/markless-dev/oxc-tsrx/actions/workflows/ci.yml/badge.svg?branch=main"></a>
  <a href="LICENSE"><img alt="MIT license" src="https://img.shields.io/npm/l/oxc-tsrx.svg"></a>
</p>

OXC for TSRX is a linter and a formatter for `.tsrx` files, written in Rust.
A linter warns you about likely mistakes in your code. A formatter rewrites
spacing and punctuation so every file in the project looks the same. You get
both on the command line and inside your editor. A `.tsrx` file is TypeScript
with JSX markup, plus template blocks like `@if` and `@for`.

_OXC for TSRX is an independent community project. It is not affiliated with,
endorsed by, or a product of VoidZero or the OXC team._

[**Docs**](https://oxc-tsrx.dev/) &nbsp;·&nbsp; [**Getting started**](https://oxc-tsrx.dev/guide/getting-started) &nbsp;·&nbsp; [**Playground**](https://oxc-tsrx.dev/playground)

- **One install for the command line and your editor.** You get `oxlint` and `oxfmt`, the real [OXC](https://oxc.rs) commands, now able to read `.tsrx`. No config file, no ignore file, no install script.
- **Errors point at what you wrote.** Your file is translated behind the scenes so OXC can read it, but every line and column number you see is in your own `.tsrx` file.
- **Your other files are untouched.** `.js`, `.ts`, `.jsx`, and `.tsx` go straight to OXC, exactly as they would without this package ([the numbers](docs/acceptance/matrix.md)).
- **Not a fork.** This does not ship a changed copy of OXC. It installs the real thing and calls it. [How that works](https://oxc-tsrx.dev/architecture/rust-oxc-core).
- **Works with the official OXC [VS Code extension](https://oxc-tsrx.dev/integrations/editor) and with [Vite+](https://oxc-tsrx.dev/integrations/vite-plus).**

## Install

```sh
npm install --save-dev oxc-tsrx@latest
```

Node.js 20.19+ (in the 20.x line) or 22.12+. You do not need Rust installed:
the install downloads a ready-built program for your machine. macOS, Linux, and
Windows are covered, eight targets in all. There is no JavaScript or
WebAssembly fallback, so any other machine has to build from source.
[Platform support](https://oxc-tsrx.dev/reference/platform-support) has the full list.

## Usage

Save this as `src/Cart.tsrx`. `Props`, `Row`, and `Empty` stand in for your
own type and components. `var total` and `debugger` are there on purpose: they
give the linter something to catch, so your first run has warnings to show you.

```tsx
export function Cart({ items }: Props) @{
  var total = 0;
  debugger;

  <section class="cart">
    @if (items.length > 0) {
      @for (const item of items; key item.id) {
        <Row item={item} />
      }
    } @else {
      <Empty />
    }
  </section>
}
```

```sh
npx oxlint src/Cart.tsrx        # Lint the file.
npx oxfmt --check src/Cart.tsrx # Show what formatting would change.
npx oxfmt --write src/Cart.tsrx # Apply it.
```

Give `oxlint` a path. With no path it also checks `node_modules`, and `--fix`
(the flag that lets `oxlint` edit your files) will rewrite files in there. The
[CLI reference](https://oxc-tsrx.dev/reference/cli#npx-oxlint-with-no-path-also-lints-nodemodules) has the count it measured, every flag, and the exit codes.

`oxfmt` formats a `.tsrx` file the way it formats TypeScript, and CSS inside a
`<style>` block is copied through untouched. The [formatting
guide](https://oxc-tsrx.dev/guide/formatting) shows a committed fixture before and after.

## What works today

These are the TSRX blocks the linter and formatter understand:
`@{` blocks, `@if` / `@else if` / `@else`, `@for` / `@empty`, `@switch` /
`@case` / `@default`, `@try` / `@pending` / `@catch`, tags whose name is an
expression, `<{expression}>`, and plain `<style>` blocks. Anything outside that
list **fails closed**: the command stops and says what it found and where,
rather than guessing and maybe producing wrong output.

**This package compiles nothing.** It never builds or runs your app. Turning
`.tsrx` into something a browser can run is your framework's TSRX plugin's job,
and the TSRX toolchain ships one for React, Preact, Solid, Vue, Ripple, and
Octane across Vite, Rspack, Turbopack, and Bun. See
[tsrx.dev/getting-started](https://tsrx.dev/getting-started). Without one, your
build tool reads `.tsrx` as plain TypeScript and stops at the first `@{`.

Three smaller limits are worth knowing: CSS inside a `<style>` block is left
alone rather than reformatted, custom JavaScript lint rules see a translated
copy of your file, so each one is read once more, and a dynamic tag name
that itself contains more markup is not supported yet.
[Limitations](https://oxc-tsrx.dev/reference/limitations) explains each one.

## In your editor

Install the official OXC extension, `oxc.oxc-vscode`. With `oxc-tsrx` in the
project there is nothing else to install or configure, and your editor
underlines the same problems the terminal reports. One catch: the TSRX
toolchain's own extension owns `.tsrx`, and the OXC extension lists no
activation event for it, so it does not start on its own. Open any
JavaScript, TypeScript, or JSON file in the project once, and `.tsrx` works
for the rest of the session. The [editor
guide](https://oxc-tsrx.dev/integrations/editor) has the settings.

## Vite and Vite+

This package changes nothing about your build. Your framework's TSRX plugin still
owns compiling, CSS, source maps, and live reload, and your build and dev server
work exactly as before. If you use [Vite+](https://oxc-tsrx.dev/integrations/vite-plus),
there is one extra command after installing, `npx oxc-tsrx setup`, and that page
has what it writes and how to undo it.

## Documentation

- [Introduction](https://oxc-tsrx.dev/guide/introduction): what this is, in plain terms.
- [Getting started](https://oxc-tsrx.dev/guide/getting-started): install, first file, first run.
- [TSRX syntax](https://oxc-tsrx.dev/guide/tsrx-syntax): every supported block.
- [Configuration](https://oxc-tsrx.dev/integrations/configuration): every supported setting.
- [Upstream map](https://oxc-tsrx.dev/architecture/upstreaming-to-oxc): what a submission to OXC would have contained, and why it is parked. Nothing has been submitted.

## Contributing

Issues and pull requests are welcome at [the issue
tracker](https://github.com/markless-dev/oxc-tsrx/issues). The source layout and
the OXC boundary are described in [the Rust and OXC
core](https://oxc-tsrx.dev/architecture/rust-oxc-core).

## License

[MIT](LICENSE).
