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

`oxc-tsrx` is a linter, formatter, parser, and language server for `.tsrx`
files, written in Rust. A `.tsrx` file is TypeScript and JSX plus template
control flow like `@if` and `@for`, and OXC is the toolchain behind the Oxlint
linter and the Oxfmt formatter.

_An independent community project. Not affiliated with, endorsed by, or a
product of VoidZero or the OXC team._

[**Docs**](https://oxc-tsrx.dev/) &nbsp;·&nbsp; [**Getting started**](https://oxc-tsrx.dev/guide/getting-started) &nbsp;·&nbsp; [**Playground**](https://oxc-tsrx.dev/playground)

## Install

```sh
npm install --save-dev oxc-tsrx
```

That is the whole setup, for the command line and for your editor. You get
`oxlint` and `oxfmt` commands that understand `.tsrx`, with no config file, no
ignore file, and no install script. [Vite+ needs one more
command](https://oxc-tsrx.dev/integrations/vite-plus).

You do not need Rust installed. Your package manager downloads the one prebuilt
binary that matches your platform, out of eight published ones.

## Usage

```sh
npx oxlint src/Cart.tsrx        # Lint the file.
npx oxfmt --check src/Cart.tsrx # Show what formatting would change.
npx oxfmt --write src/Cart.tsrx # Apply it.
```

Always give these commands a path. A bare `npx oxlint` also lints
`node_modules`, and `--fix` will rewrite files in there.

Your `.js`, `.jsx`, `.ts`, and `.tsx` files take the official OXC code paths
unchanged. Only `.tsrx` files do anything TSRX-specific.

## In your editor

Install the official OXC extension, `oxc.oxc-vscode`. With `oxc-tsrx` in the
project there is nothing else to install or configure: you get `.tsrx`
diagnostics, formatting, quick fixes, and your own Oxlint JavaScript plugin
rules.

One thing to know: the official extension lists no `.tsrx` activation event, and
the TSRX toolchain's extension owns `.tsrx` under its own language id, so
opening a `.tsrx` file first does not start it. Open any JavaScript, TypeScript,
or JSON file once, and `.tsrx` is served for the rest of the session. See the
[editor guide](https://oxc-tsrx.dev/integrations/editor).

## API

```js
import { parseSync } from "oxc-tsrx/parser";
import { defineConfig } from "oxc-tsrx/lint";
import { format } from "oxc-tsrx/format";
```

These give you an AST and formatted text. Neither one compiles `.tsrx`: building
and running it belongs to your framework's TSRX plugin, such as
`@tsrx/vite-plugin-react`. See
[tsrx.dev/getting-started](https://tsrx.dev/getting-started).

## Your own JavaScript lint plugins

A plugin listed in `jsPlugins` runs on `.tsrx` from the `oxlint` command and
from the language server, but it sees a legal-TSX copy of your file rather than
the TSRX you wrote. That costs one extra parse per `.tsrx` file, which both
lanes announce, and `settings.oxcTsrx.jsPluginsOnTsrx: false` turns it off.
`oxc-tsrx-lint`, the standalone binary, is a Rust process with no Node.js
runtime, so it refuses `jsPlugins` and names `oxlint` as the command that can.
`oxc-tsrx/lint/plugins-dev` is for *writing* a plugin, since it re-exports
Oxlint's `RuleTester`. See [Custom JavaScript
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
