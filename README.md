<p align="center">
  <br>
  <a href="https://oxc-tsrx-docs.vercel.app/">
    <img alt="OXC for TSRX" width="600" src="https://raw.githubusercontent.com/markless-dev/oxc-tsrx/HEAD/.github/assets/readme-hero.png">
  </a>
  <br>
</p>

<p align="center">
  <a href="https://www.npmjs.com/package/oxc-tsrx"><img alt="npm version" src="https://img.shields.io/npm/v/oxc-tsrx.svg"></a>
  <a href="https://nodejs.org/en/about/previous-releases"><img alt="supported Node.js versions" src="https://img.shields.io/node/v/oxc-tsrx.svg"></a>
  <a href="https://github.com/markless-dev/oxc-tsrx/actions/workflows/ci.yml"><img alt="CI status" src="https://github.com/markless-dev/oxc-tsrx/actions/workflows/ci.yml/badge.svg?branch=main"></a>
  <a href="LICENSE"><img alt="MIT license" src="https://img.shields.io/npm/l/oxc-tsrx.svg"></a>
</p>

OXC for TSRX is a linter, formatter, and language server for `.tsrx` files,
written in Rust. A `.tsrx` file is TypeScript and JSX plus template control flow
like `@if` and `@for`, and OXC is the toolchain behind the Oxlint linter and the
Oxfmt formatter.

_OXC for TSRX is an independent community project. It is not affiliated with,
endorsed by, or a product of VoidZero or the OXC team._

[**Docs**](https://oxc-tsrx-docs.vercel.app/) &nbsp;·&nbsp; [**Getting started**](https://oxc-tsrx-docs.vercel.app/guide/getting-started) &nbsp;·&nbsp; [**Playground**](https://oxc-tsrx-docs.vercel.app/playground)

- **One install for the command line and your editor.** You get `oxlint` and
  `oxfmt` commands that understand `.tsrx`. No config file, no ignore file, no
  install script.
- **Errors point at what you wrote.** Real Oxlint rules run on a temporary copy
  of your file, but every line and column you see is in your own `.tsrx` source.
- **Your other files are untouched.** `.js`, `.ts`, `.jsx`, and `.tsx` take the
  official OXC code paths, at official Oxlint speed ([the
  numbers](docs/acceptance/matrix.md)).
- **No fork and no patches.** OXC is an ordinary pinned dependency, and every
  call into it lives in one small adapter crate. [How that
  works](https://oxc-tsrx-docs.vercel.app/architecture/rust-oxc-core).
- **You do not need Rust installed.** A normal install downloads one prebuilt
  binary, the one matching your platform.
- **Works with the official OXC [VS Code
  extension](https://oxc-tsrx-docs.vercel.app/integrations/editor) and with
  [Vite+](https://oxc-tsrx-docs.vercel.app/integrations/vite-plus).**

## Install

```sh
npm install --save-dev oxc-tsrx@latest
```

Node.js 20.19+ (in the 20.x line) or 22.12+. Prebuilt binaries cover macOS,
Linux (glibc and musl), and Windows, on x64 and arm64. There is no JavaScript or
WebAssembly fallback, so any other platform has to [build from
source](https://oxc-tsrx-docs.vercel.app/guide/getting-started). Those eight
targets are not equally tested, and [platform
support](https://oxc-tsrx-docs.vercel.app/reference/platform-support) says
exactly which get a real lint, format, and language-server run on every change.

## Usage

Save this as `src/Cart.tsrx`. `Props`, `Row`, and `Empty` stand in for your own
type and components, and the `var total` and `debugger` lines are there to give
the linter something to catch.

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

Always give these commands a path. A bare `npx oxlint` also lints
`node_modules`, and `--fix` will rewrite files in there. The [getting started
guide](https://oxc-tsrx-docs.vercel.app/guide/getting-started) has the full
output of every command above.

## What formatting does

```tsx
type Item={id:string;label:string};export function Rows({items}:{items:Item[]})@{<ul>@for(item of items;index i;key item.id){<li data-index={i}>{item.label}</li>}@empty{<li>Empty</li>}</ul>}
```

becomes:

```tsx
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

Those are the committed fixtures in
[tests/fixtures/control](tests/fixtures/control), asserted by the format tests.

## What works today

`.tsrx` support covers `@{` statement containers, `@if` / `@else if` / `@else`,
`@for` / `@empty`, `@switch` / `@case` / `@default`, `@try` / `@pending` /
`@catch`, dynamic JSX tags written as `<{expression}>`, and lowercase raw
`<style>` elements. Anything outside that list **fails closed**: the command
stops and says what it found and where, rather than guessing and maybe producing
wrong output.

Four limits are worth knowing before you start:

- **This package compiles nothing.** It lints, formats, parses, and powers your
  editor. Building and running `.tsrx` belongs to your framework's TSRX plugin,
  which the TSRX toolchain ships for React, Preact, Solid, Vue, Ripple, and
  Octane across Vite, Rspack, Turbopack, and Bun. See
  [tsrx.dev/getting-started](https://tsrx.dev/getting-started). Without one,
  your bundler reads `.tsrx` as ordinary TypeScript and fails on the first `@{`.
- CSS inside a raw `<style>` element is preserved exactly. It is not
  reformatted and not validated.
- Your own JavaScript lint plugins do run on `.tsrx`, on the command line and in
  the editor, but they see a TSX copy of your file. That costs one extra parse
  per `.tsrx` file, which is announced every time, and your rule sees `if` and
  `for` where you wrote `@if` and `@for`.
- A dynamic tag whose name expression contains more dynamic JSX is not supported
  yet.

The full list is in
[limitations](https://oxc-tsrx-docs.vercel.app/reference/limitations), and every
supported block is in [TSRX
syntax](https://oxc-tsrx-docs.vercel.app/guide/tsrx-syntax).

## In your editor

Install the official OXC extension, `oxc.oxc-vscode`. With `oxc-tsrx` in the
project there is nothing else to install or configure: `.tsrx` diagnostics,
formatting, quick fixes, and your own plugin rules, while ordinary TypeScript
stays on official Oxlint.

One thing to know, and it is not optional: the official extension's activation
events are 21 `onLanguage:` entries and none of them is `.tsrx`'s language. The
TSRX toolchain's own extension contributes `.tsrx` under its own language id, so
opening a `.tsrx` file activates that extension and not OXC's. Open any
JavaScript, TypeScript, or JSON file in the workspace once, and `.tsrx` is
served for the rest of the session. The [editor
guide](https://oxc-tsrx-docs.vercel.app/integrations/editor) has the settings
and the rest of the path.

## Vite and Vite+

This project adds no Vite transform and no Vite parser. Your framework's own
TSRX Vite plugin still owns compilation, CSS, source maps, and HMR, and your
build and dev server are unchanged. What `oxc-tsrx` adds is `.tsrx` linting and
formatting, and Vite+ is the one place where installing it is not enough:

```sh
pnpm add -D vite-plus oxc-tsrx@latest
pnpm exec oxc-tsrx setup
```

`setup` is explicit, reversible with `oxc-tsrx remove`, never edits your
`package.json`, and has to be run again after a clean dependency install,
because `node_modules` is disposable. The [Vite+
guide](https://oxc-tsrx-docs.vercel.app/integrations/vite-plus) explains what it
writes.

## Documentation

- [Getting started](https://oxc-tsrx-docs.vercel.app/guide/getting-started): install, first file, first run.
- [TSRX syntax](https://oxc-tsrx-docs.vercel.app/guide/tsrx-syntax): every supported block.
- [Configuration](https://oxc-tsrx-docs.vercel.app/integrations/configuration): every supported setting.
- [CLI reference](https://oxc-tsrx-docs.vercel.app/reference/cli): commands, flags, and exit codes.
- [Platform support](https://oxc-tsrx-docs.vercel.app/reference/platform-support): which platforms are tested on every change.
- [Limitations](https://oxc-tsrx-docs.vercel.app/reference/limitations): what is not claimed yet.
- [Upstream map](https://oxc-tsrx-docs.vercel.app/architecture/upstreaming-to-oxc): what a submission to OXC would have contained, and why it is parked. Nothing has been submitted.

## Contributing

Issues and pull requests are welcome at [the issue
tracker](https://github.com/markless-dev/oxc-tsrx/issues). The source layout and
the OXC boundary are described in [the Rust and OXC
core](https://oxc-tsrx-docs.vercel.app/architecture/rust-oxc-core).

## License

[MIT](LICENSE).
