---
title: Getting Started
description: Install oxc-tsrx, then parse, lint, format, and edit your first TSRX file.
---

# Getting Started

OXC for TSRX is prepared as one public package, `oxc-tsrx`. It gives you the
`oxlint` and `oxfmt` commands you may already know, an OXC-shaped parser API,
helpers for *authoring* custom JavaScript lint plugins (helpers to write one,
not a host that runs one on `.tsrx`), and a native language server. It plugs
into [Vite+](/integrations/vite-plus) and the released
[official OXC extension](/integrations/editor).

> The package is built and tested, but public npm availability is only claimed
> after an approved publication. If the install command below cannot find it
> yet, use the [build-from-source path](#build-from-source-optional) until the
> registry launch is verified.

## Install

You need Node.js 20.19 or newer. Install one dev dependency:

<!-- pm-install -->
```sh
npm install --save-dev oxc-tsrx
```

That is the whole setup. The tools are written in Rust, and your package
manager downloads a ready-made binary for your operating system as part of
this normal install. You do not need Rust on your machine, the package runs
no install scripts, and the commands never download anything later. If your
CI blocks postinstall scripts, this install still works.

### Installing is the activation step

`oxc-tsrx` declares a static `oxc.provider` block in its own `package.json`. A
host that performs provider discovery reads that JSON and learns which files
this package owns (`.tsrx`) plus the parser, linter, formatter, and language
server to use for them.

If you have used Vite, the closest familiar shape is a preset that Vite finds
because it is in your dependency list, not because you pasted a config snippet.
The difference is that nothing here is executed to be discovered. A host only
resolves and reads `package.json` files.

So there is no second step: no activation command, no dependency alias, no
`overrides` block, no install script, and nothing written into `node_modules`
after the install finishes. Delete `node_modules`, reinstall from your lockfile,
and it still works, because `oxc-tsrx` is still a direct dependency.

To see exactly what a host would find in your project, without changing
anything:

```sh
npx oxc-tsrx providers --json
```

### Which hosts read that today

This part decides what works for you right now, so it is worth being exact:

- **Reads it today:** the `oxlint --lsp` multiplexer shipped inside `oxc-tsrx`,
  and this repository's own VS Code client. Discovery is proven from clean
  consumers on npm, pnpm, Bun, and Yarn Berry (both the node-modules and
  Plug'n'Play linkers), including a frozen reinstall in every lane.
- **Does not read it today:** every released build of Oxlint, Oxfmt, Vite+, and
  the official OXC extension (`oxc.oxc-vscode`). `oxc.provider` is a protocol
  *proposed* to OXC. It is a source-complete proposal: nothing has been
  submitted upstream, and nothing has been accepted.
- **The command names are what reach released tools, and that is the design.**
  The `oxlint` and `oxfmt` commands this package declares are how the official
  OXC toolchain finds TSRX from a plain install. Earlier versions of this guide
  called them compatibility-only surfaces that would be deleted once a released
  host discovered providers. That is not going to happen, and it is not the
  plan: the command names are the shipped mechanism.

Of the four capabilities declared in the block, only the language server has a
host today. The parser, lint, and format targets are declared and resolve, but
nothing runs them through discovery yet.

### Editors need no extra step

Install `oxc-tsrx` and do nothing else, and the released official OXC extension
(measured at `oxc.oxc-vscode` 1.59.0) gives you `.tsrx` diagnostics that refresh
as you type, formatting, and quick fixes you can apply, while ordinary
TypeScript files keep going to canonical Oxlint. This was measured in a real
editor session on darwin-arm64. See
[the editor integration page](/integrations/editor) for what that session
covered.

### Using Vite+? (compatibility step)

Released [Vite+](/integrations/vite-plus) does not discover providers. It finds
its lint and format tools through project-local *packages* named literally
`oxlint` and `oxfmt`. A command name cannot satisfy that, and this project
cannot legitimately publish a package under either name, so Vite+ needs the
project-local slots that `npx oxc-tsrx setup` writes:

<!-- pm-install -->
```sh
npm install --save-dev vite-plus oxc-tsrx
npx oxc-tsrx setup
```

`setup` is explicit, idempotent, reversible, and never edits `package.json`.
Run it again after a clean dependency install, because `node_modules` is
disposable. After that, `vp lint`, `vp fmt`, and `vp check --fix` handle `.tsrx`
files automatically. The [Vite and Vite+ page](/integrations/vite-plus) has the
full quick start.

Vite+ is the only place an install alone is not enough. Every other route in
this guide works with no command at all.

## Create a TSRX file

Save this as `src/Cart.tsrx`. On this site, the "Try in playground" button
under the snippet lets you explore it in your browser without installing
anything. The `var total` and `debugger` lines are there on purpose: they
give the linter something to catch.

```tsrx
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

## Lint and format it

Run the linter, then ask the formatter which files would change. This is a
recording of both commands running against this exact file:

<!-- terminal-demo -->

Every diagnostic points at line and column numbers in your original TSRX
code, never at a transformed copy. Once you have fixed the warnings, let the
formatter write its layout changes:

<!-- terminal-demo:getting-started-format-write -->

Two things to know:

- **Mixed file types are fine.** `.tsrx` files go through the TSRX engine,
  while ordinary `.js`/`.ts`/`.tsx` files go straight to OXC.
- **You can tune rule severity inline.** For example
  `npx oxlint --warn no-console --deny no-debugger src/Cart.tsrx`.

## Configuration

Both commands find your normal OXC config by searching upward from the
current directory (`.oxlintrc.json`/`.oxlintrc.jsonc` for lint,
`.oxfmtrc.json`/`.oxfmtrc.jsonc` for format), or take an explicit
`--config`/`-c` path. See
[Configuration](/integrations/configuration) for exactly which fields
are supported.

## Build from source (optional)

If you would rather build the native binaries yourself, you need a stable
Rust toolchain ([rustup](https://rustup.rs)):

```sh
git clone https://github.com/markless-dev/oxc-tsrx.git
cd oxc-tsrx
cargo build --release --locked -p oxc_tsrx_cli --bins
```

Keep the `--locked` flag: it makes Cargo build against the exact pinned OXC
commit from the lockfile. The binaries land in `target/release/`:

<!-- terminal-demo:getting-started-native -->

The native binaries emit JSON diagnostics and want explicit file paths. The
friendly text output, directory walking, and glob handling come from the npm
commands, so most projects only need those. See the
[CLI Reference](/reference/cli) for every flag.

## Next steps

- See which TSRX syntax is supported in
  [TSRX Syntax](/guide/tsrx-syntax).
- Keep using your framework's Vite build plugin as-is; `oxc-tsrx` does not
  compile modules. See the build-vs-commands split in
  [Vite and Vite+](/integrations/vite-plus).
- Wire the lint/format commands into `vp lint` / `vp fmt` (a separate concern
  from the build plugin) with
  [Vite and Vite+](/integrations/vite-plus).
- Get live diagnostics, formatting, and quick fixes through the official OXC
  extension with
  [Editor integration](/integrations/editor).
- Curious how it works under the hood? Read
  [Architecture](/architecture/rust-oxc-core).
