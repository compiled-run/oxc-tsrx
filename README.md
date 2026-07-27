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

OXC for TSRX is a linter, formatter, and language server for `.tsrx` files, written in Rust, that adds `.tsrx` support to the OXC toolchain without forking OXC. A `.tsrx` file is TypeScript and JSX plus template control-flow blocks like `@if` and `@for`, and OXC is the toolchain behind the Oxlint linter and the Oxfmt formatter.

_OXC for TSRX is an independent community project. It is not affiliated with, endorsed by, or a product of VoidZero or the OXC team._

[**Docs**](https://oxc-tsrx-docs.vercel.app/) &nbsp;·&nbsp; [**Getting started**](https://oxc-tsrx-docs.vercel.app/guide/getting-started) &nbsp;·&nbsp; [**Playground**](https://oxc-tsrx-docs.vercel.app/playground)

- **One install for the command line and your editor.** `npm install --save-dev oxc-tsrx` gives you `oxlint` and `oxfmt` commands that understand `.tsrx`. No config file, no ignore file, no install script, and nothing written into `node_modules` afterwards.
- **No fork, no patches, one parse.** Canonical OXC is a normal Cargo dependency, the Rust equivalent of a `package.json` dependency, pinned at one exact commit. Every call that depends on that commit lives in `crates/oxc_adapter`, and a normal lint or format is exactly one canonical OXC parse. [How that works](https://oxc-tsrx-docs.vercel.app/architecture/rust-oxc-core).
- **Real Oxlint rules, on your bytes.** Diagnostics point at line and column numbers in your original `.tsrx` source, never at a transformed copy.
- **Ordinary JavaScript and TypeScript are untouched.** `.js`, `.ts`, `.jsx`, and `.tsx` go straight to the official OXC code paths, measured at 45.92 ms next to official Oxlint's 40.68 ms on the same 1,000-file TSX corpus with one rule ([evidence](docs/acceptance/matrix.md)).
- **You do not need Rust installed.** The package declares eight optional dependency packages, each carrying one prebuilt native binary, and a normal install downloads exactly one, the one matching your platform.
- **Works with the released OXC [VS Code extension](https://oxc-tsrx-docs.vercel.app/integrations/editor) and with [Vite+](https://oxc-tsrx-docs.vercel.app/integrations/vite-plus).**

## Install

```sh
npm install --save-dev oxc-tsrx
```

You need Node.js 20.19 or newer in the 20.x line, or 22.12 or newer. Node 21 and Node 22.0 through 22.11 are not supported. The prebuilt binaries cover macOS, Linux (both glibc and musl), and Windows, on x64 and arm64. There is no JavaScript or WebAssembly fallback, so a platform outside that list has to [build from source](https://oxc-tsrx-docs.vercel.app/guide/getting-started).

## Usage

Save this as `src/Cart.tsrx`. `Props`, `Row`, and `Empty` are placeholders for your own type and components. The `var total` and `debugger` lines are there on purpose, because they give the linter something to catch.

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

Always give these commands a path. A bare `npx oxlint` also lints `node_modules`, and `--fix` will rewrite files in there. The [getting started guide](https://oxc-tsrx-docs.vercel.app/guide/getting-started) has the measured detail and the full output of every command above.

## What formatting does

### Input

```tsx
type Item={id:string;label:string};export function Rows({items}:{items:Item[]})@{<ul>@for(item of items;index i;key item.id){<li data-index={i}>{item.label}</li>}@empty{<li>Empty</li>}</ul>}
```

### Output

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

These are the exact committed fixtures in [tests/fixtures/control](tests/fixtures/control), asserted by the format tests.

## What works today

`.tsrx` support covers `@{` statement containers, `@if` / `@else if` / `@else`, `@for` / `@empty`, `@switch` / `@case` / `@default`, `@try` / `@pending` / `@catch`, matched dynamic JSX tags written as `<{expression}>`, and lowercase raw `<style>` elements. Anything outside that list **fails closed**. Instead of guessing what your code means and possibly producing wrong output, the command stops and reports what it found and where. Four limits are worth knowing before you start, and the first one is the big one:

- **You cannot build or run a `.tsrx` file with this package.** It lints, formats, parses, and powers your editor. It compiles nothing, so importing a `.tsrx` file into your app makes the bundler read it as ordinary TypeScript and fail on the first `@{`. Building and running one is [your framework's Vite plugin's job](#vite-and-vite).
- Bytes inside a raw `<style>` element are preserved exactly. They are not CSS-formatted and not CSS-validated.
- Custom JavaScript lint plugins run on your ordinary files. On `.tsrx` they fail loudly with an explicit error instead of being silently ignored.
- A dynamic tag whose name expression contains more dynamic JSX is not supported yet.

The complete list is in [limitations](https://oxc-tsrx-docs.vercel.app/reference/limitations), and every supported block is in [TSRX syntax](https://oxc-tsrx-docs.vercel.app/guide/tsrx-syntax).

## Editor setup

Install the released official OXC extension, `oxc.oxc-vscode`. With `oxc-tsrx` in the project there is no other step. You get `.tsrx` diagnostics, formatting, and quick fixes, while ordinary TypeScript stays on canonical Oxlint. One thing to know: the official extension does not list `.tsrx` in its activation events. In a workspace that contains only `.tsrx` files, open any JavaScript, TypeScript, or JSON file once to wake it up. Settings and the rest of the path are in the [editor guide](https://oxc-tsrx-docs.vercel.app/integrations/editor).

## Vite and Vite+

This project adds no Vite transform and no Vite parser. Your framework's own TSRX Vite plugin, a separate package such as `@tsrx/vite-plugin-react`, owns runtime compilation, CSS, source maps, and Hot Module Replacement (HMR), and your build and dev server flow through that plugin unchanged. What `oxc-tsrx` adds here is `.tsrx` linting and formatting, and Vite+ is the one place in the whole product where installing it is not enough:

```sh
pnpm add -D vite-plus oxc-tsrx
pnpm exec oxc-tsrx setup
```

`setup` is explicit, reversible with `oxc-tsrx remove`, never edits your `package.json`, and has to be run again after a clean dependency install, because `node_modules` is disposable. The [Vite+ guide](https://oxc-tsrx-docs.vercel.app/integrations/vite-plus) explains what it writes.

## Documentation

- [Getting started](https://oxc-tsrx-docs.vercel.app/guide/getting-started): install, first file, first run.
- [TSRX syntax](https://oxc-tsrx-docs.vercel.app/guide/tsrx-syntax): every supported block.
- [CLI reference](https://oxc-tsrx-docs.vercel.app/reference/cli): commands, flags, and exit codes.
- [Limitations](https://oxc-tsrx-docs.vercel.app/reference/limitations): what is not claimed yet.
- [Configuration](https://oxc-tsrx-docs.vercel.app/integrations/configuration): the exact supported matrix.
- [Acceptance matrix](docs/acceptance/matrix.md): the benchmark reports behind the numbers above.

## Project independence

Canonical OXC is used as an unmodified upstream dependency. There is no source snapshot, no Cargo patch, no fork, no vendor tree, and no downstream patch queue. No OXC maintainer interest is claimed, nothing has been submitted to the OXC project, and nothing is planned. The [upstream map](https://oxc-tsrx-docs.vercel.app/architecture/upstreaming-to-oxc) records what such a submission would have contained and why it is parked.

## Contributing

- Issues and pull requests are welcome at [the issue tracker](https://github.com/markless-dev/oxc-tsrx/issues).
- The source layout, the OXC revision boundary, and the module tree are described in [the Rust and OXC core](https://oxc-tsrx-docs.vercel.app/architecture/rust-oxc-core) and [the upstream map](https://oxc-tsrx-docs.vercel.app/architecture/upstreaming-to-oxc).

## License

[MIT](LICENSE).
