<p align="center">
  <a href="https://oxc-tsrx.dev/"><img alt="OXC for TSRX" width="600" src="https://raw.githubusercontent.com/markless-dev/oxc-tsrx/HEAD/.github/assets/readme-hero.png"></a>
</p>

<p align="center">
  <a href="https://www.npmjs.com/package/oxc-tsrx"><img alt="npm version" src="https://img.shields.io/npm/v/oxc-tsrx.svg"></a>
  <a href="https://nodejs.org/en/about/previous-releases"><img alt="supported Node.js versions" src="https://img.shields.io/node/v/oxc-tsrx.svg"></a>
  <a href="https://github.com/markless-dev/oxc-tsrx/actions/workflows/ci.yml"><img alt="CI status" src="https://github.com/markless-dev/oxc-tsrx/actions/workflows/ci.yml/badge.svg?branch=main"></a>
  <a href="LICENSE"><img alt="MIT license" src="https://img.shields.io/npm/l/oxc-tsrx.svg"></a>
</p>

OXC for TSRX is a linter, formatter, and language server for `.tsrx` files,
written in Rust. A `.tsrx` file is TypeScript and JSX plus template control
flow like `@if` and `@for`, and OXC is the toolchain behind Oxlint and Oxfmt.

_OXC for TSRX is an independent community project. It is not affiliated with,
endorsed by, or a product of VoidZero or the OXC team._

[**Docs**](https://oxc-tsrx.dev/) &nbsp;·&nbsp; [**Getting started**](https://oxc-tsrx.dev/guide/getting-started) &nbsp;·&nbsp; [**Playground**](https://oxc-tsrx.dev/playground)

- **One install for the command line and your editor.** You get `oxlint` and `oxfmt` commands that understand `.tsrx`. No config file, no ignore file, no install script.
- **Errors point at what you wrote.** Real Oxlint rules run on a temporary copy of your file, but every line and column you see is in your own `.tsrx` source.
- **Your other files are untouched.** `.js`, `.ts`, `.jsx`, and `.tsx` take the official OXC code paths, at official Oxlint speed ([the numbers](docs/acceptance/matrix.md)).
- **No fork and no patches.** OXC is an ordinary pinned dependency, and every call into it lives in one small adapter crate. [How that works](https://oxc-tsrx.dev/architecture/rust-oxc-core).
- **Works with the official OXC [VS Code extension](https://oxc-tsrx.dev/integrations/editor) and with [Vite+](https://oxc-tsrx.dev/integrations/vite-plus).**

## Install

```sh
npm install --save-dev oxc-tsrx@latest
```

Node.js 20.19+ (in the 20.x line) or 22.12+. You do not need Rust installed: a
normal install downloads one prebuilt binary, the one matching your platform.
Those cover macOS, Linux (glibc and musl), and Windows, on x64 and arm64. There
is no JavaScript or WebAssembly fallback, so anything else has to build from
source. [Platform support](https://oxc-tsrx.dev/reference/platform-support) says
which of the eight targets get a real run on every change.

## Usage

Save this as `src/Cart.tsrx`. `Props`, `Row`, and `Empty` stand in for your own
type and components, and `var total` and `debugger` give the linter work to do.

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

Give `oxlint` a path. A bare `npx oxlint` also lints `node_modules`, and `--fix`
will rewrite files in there. The [CLI
reference](https://oxc-tsrx.dev/reference/cli#npx-oxlint-with-no-path-also-lints-nodemodules) has the measured cost, every flag, and the exit codes.

`oxfmt` formats a `.tsrx` file the way it formats TypeScript, and CSS inside a
raw `<style>` element is copied through untouched. The [formatting
guide](https://oxc-tsrx.dev/guide/formatting) shows a committed fixture before
and after, and the settings you can change.

## What works today

`.tsrx` support covers `@{` statement containers, `@if` / `@else if` / `@else`,
`@for` / `@empty`, `@switch` / `@case` / `@default`, `@try` / `@pending` /
`@catch`, dynamic JSX tags written as `<{expression}>`, and lowercase raw
`<style>` elements. Anything outside that list **fails closed**: the command
stops and says what it found and where, rather than guessing and maybe producing
wrong output.

**This package compiles nothing.** It lints, formats, parses, and powers your
editor. Building and running `.tsrx` belongs to your framework's TSRX plugin,
which the TSRX toolchain ships for React, Preact, Solid, Vue, Ripple, and Octane
across Vite, Rspack, Turbopack, and Bun. See
[tsrx.dev/getting-started](https://tsrx.dev/getting-started). Without one, your
bundler reads `.tsrx` as ordinary TypeScript and fails on the first `@{`.

Three smaller limits are worth knowing: CSS inside a raw `<style>` element is
preserved rather than reformatted, your own JavaScript lint plugins see a TSX
copy of your file, and a dynamic tag whose name holds more dynamic JSX is not
supported yet. [Limitations](https://oxc-tsrx.dev/reference/limitations) explains each one.

## In your editor

Install the official OXC extension, `oxc.oxc-vscode`. With `oxc-tsrx` in the
project there is nothing else to install or configure. The extension does not
start on a `.tsrx` file, so open any JavaScript, TypeScript, or JSON file in the
workspace once, and `.tsrx` is served for the rest of the session. The [editor
guide](https://oxc-tsrx.dev/integrations/editor) has the settings.

## Vite and Vite+

This project adds no Vite transform and no Vite parser. Your framework's own
TSRX Vite plugin still owns compilation, CSS, source maps, and HMR, and your
build and dev server are unchanged. Vite+ needs one extra step:

```sh
npm install --save-dev vite-plus oxc-tsrx@latest
npx oxc-tsrx setup
```

Run both lines with your own package manager. `setup` is explicit, reversible
with `oxc-tsrx remove`, and works inside `node_modules`, so a later install
undoes it. The [Vite+ guide](https://oxc-tsrx.dev/integrations/vite-plus) has
the recovery and the one line `setup` writes in your own project.

## Documentation

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
