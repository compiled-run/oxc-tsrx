<p align="center">
  <a href="https://oxc-tsrx.dev/">
    <img alt="OXC for TSRX" width="600" src="https://raw.githubusercontent.com/markless-dev/oxc-tsrx/HEAD/.github/assets/readme-hero.png">
  </a>
</p>

<p align="center">
  <a href="https://www.npmjs.com/package/oxc-tsrx"><img alt="npm version" src="https://img.shields.io/npm/v/oxc-tsrx.svg"></a>
  <a href="https://nodejs.org/en/about/previous-releases"><img alt="supported Node.js versions" src="https://img.shields.io/node/v/oxc-tsrx.svg"></a>
  <a href="https://github.com/markless-dev/oxc-tsrx/actions/workflows/ci.yml"><img alt="CI status" src="https://github.com/markless-dev/oxc-tsrx/actions/workflows/ci.yml/badge.svg?branch=main"></a>
  <a href="https://github.com/markless-dev/oxc-tsrx/blob/HEAD/LICENSE"><img alt="MIT license" src="https://img.shields.io/npm/l/oxc-tsrx.svg"></a>
</p>

`oxc-tsrx` is a linter and a formatter for `.tsrx` files, written in Rust.
A linter warns you about likely mistakes in your code. A formatter rewrites
spacing and punctuation so every file in the project looks the same. You get
both on the command line and inside your editor. A `.tsrx` file is TypeScript
with JSX markup, plus template blocks like `@if` and `@for`.

_OXC for TSRX is an independent community project. It is not affiliated with,
endorsed by, or a product of VoidZero or the OXC team._

[**Docs**](https://oxc-tsrx.dev/) &nbsp;·&nbsp; [**Getting started**](https://oxc-tsrx.dev/guide/getting-started) &nbsp;·&nbsp; [**Playground**](https://oxc-tsrx.dev/playground)

## Install

```sh
npm install --save-dev oxc-tsrx
```

That is the whole setup, for the command line and for your editor. You get
`oxlint` and `oxfmt`, the real [OXC](https://oxc.rs) commands, now able to read
`.tsrx`, with no config file, no ignore file, and no install script. [Vite+ needs
one more command](https://oxc-tsrx.dev/integrations/vite-plus).

You do not need Rust installed: the install downloads a ready-built program for
your machine, one of eight published for macOS, Linux, and Windows. See
[Platform support](https://oxc-tsrx.dev/reference/platform-support).

## Usage

```sh
npx oxlint src/Cart.tsrx        # Lint the file.
npx oxfmt --check src/Cart.tsrx # Show what formatting would change.
npx oxfmt --write src/Cart.tsrx # Apply it.
```

Always give these commands a path. A bare `npx oxlint` also checks
`node_modules`, and `--fix` (the flag that lets `oxlint` edit your files) will
rewrite files in there.

Your `.js`, `.jsx`, `.ts`, and `.tsx` files go straight to OXC, exactly as they
would without this package. Only `.tsrx` files do anything TSRX-specific.

## What works today

These are the TSRX blocks the linter and formatter understand: `@{` blocks,
`@if` / `@else if` / `@else`, `@for` / `@empty`, `@switch` / `@case` /
`@default`, `@try` / `@pending` / `@catch`, tags whose name is an expression,
`<{expression}>`, and plain `<style>` blocks
([TSRX syntax](https://oxc-tsrx.dev/guide/tsrx-syntax) shows each one). Anything
outside that list **fails closed**: the command stops and says what it found and
where, rather than guessing and maybe producing wrong output.

**This package compiles nothing.** It never builds or runs your app. Turning
`.tsrx` into something a browser can run is your framework's TSRX plugin's job,
such as `@tsrx/vite-plugin-react`. See
[tsrx.dev/getting-started](https://tsrx.dev/getting-started). Without one, your
build tool reads `.tsrx` as plain TypeScript and stops at the first `@{`.

## In your editor

Install the official OXC extension, `oxc.oxc-vscode`. With `oxc-tsrx` in the
project there is nothing else to install or configure, and your editor
underlines the same problems the terminal reports. One catch: the TSRX
toolchain's own extension owns `.tsrx`, and the OXC extension lists no
activation event for it, so it does not start on its own. Open any JavaScript,
TypeScript, or JSON file in the project once, and `.tsrx` works for the rest of
the session. See the [editor guide](https://oxc-tsrx.dev/integrations/editor).

## API

```js
import { parseSync } from "oxc-tsrx/parser";
import { defineConfig } from "oxc-tsrx/lint";
import { format } from "oxc-tsrx/format";
```

These hand you a syntax tree and formatted text, so you can build your own
tooling on the same reader the commands use. None of them compiles `.tsrx`
either; that stays your framework's TSRX plugin's job.

## Your own JavaScript lint plugins

A plugin listed in `jsPlugins` runs on `.tsrx` from the `oxlint` command and
inside your editor, but it sees a translated copy of your file rather than the
TSRX you wrote, so each `.tsrx` file is read once more. The command and the
editor both say when they have done that, and
`settings.oxcTsrx.jsPluginsOnTsrx: false` turns it off.

`oxc-tsrx-lint`, the standalone Rust command, has no Node.js to run a plugin in,
so it refuses `jsPlugins` and names `oxlint` as the command that can.
`oxc-tsrx/lint/plugins-dev` is for *writing* a plugin, since it re-exports the
`RuleTester` from `oxlint`. See [Custom JavaScript
plugins](https://oxc-tsrx.dev/integrations/custom-js-plugins).

## Documentation

- [Getting started](https://oxc-tsrx.dev/guide/getting-started): install, first file, first run.
- [Configuration](https://oxc-tsrx.dev/integrations/configuration): every supported setting.
- [CLI reference](https://oxc-tsrx.dev/reference/cli): commands, flags, and exit codes.
- [Platform support](https://oxc-tsrx.dev/reference/platform-support): which of the eight published platforms are tested on every change.
- [Limitations](https://oxc-tsrx.dev/reference/limitations): what is not claimed yet.
- [Provider protocol](https://oxc-tsrx.dev/architecture/provider-protocol): the `oxc.provider` block, what reads it today, and how a plain install reaches released hosts.

`oxc-tsrx` is the only package to depend on. The eight `@oxc-tsrx/native-*`
packages are platform binaries in `optionalDependencies`, and you never name one
yourself.

## License

[MIT](https://github.com/markless-dev/oxc-tsrx/blob/HEAD/LICENSE).
