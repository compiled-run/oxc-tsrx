---
title: Getting Started
description: Install oxlint-tsrx and oxfmt-tsrx, then lint and format your first TSRX file.
---

# Getting Started

OXC for TSRX ships as two npm packages: `oxlint-tsrx` for linting and
`oxfmt-tsrx` for formatting. They give you the same `oxlint` and `oxfmt`
commands you may already know, with TSRX support built in, and they plug into
[Vite+](/integrations/vite-plus) and
[Visual Studio Code](/integrations/editor).

## Install

You need Node.js 20.19 or newer. Install both packages as dev dependencies:

<!-- pm-install -->
```sh
npm install --save-dev oxlint-tsrx oxfmt-tsrx
```

That is the whole setup. The tools are written in Rust, and your package
manager downloads a ready-made binary for your operating system as part of
this normal install. You do not need Rust on your machine, the packages run
no install scripts, and the commands never download anything later. If your
CI blocks postinstall scripts, this install still works.

### Using Vite+?

If your project runs on [Vite+](/integrations/vite-plus), install the
same two packages under the `oxlint` and `oxfmt` names Vite+ looks for, next
to `vite-plus` itself:

<!-- pm-install -->
```sh
npm install --save-dev vite-plus \
  oxlint@npm:oxlint-tsrx \
  oxfmt@npm:oxfmt-tsrx
```

After that, `vp lint`, `vp fmt`, and `vp check --fix` handle `.tsrx` files
automatically. The [Vite and Vite+ page](/integrations/vite-plus) has
the full quick start.

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
git clone https://github.com/thejackshelton/oxc-tsrx.git
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
- Wire the commands into `vp lint` / `vp fmt` with
  [Vite and Vite+](/integrations/vite-plus).
- Get live diagnostics and format-on-save with
  [Editor integration](/integrations/editor).
- Curious how it works under the hood? Read
  [Architecture](/architecture/rust-oxc-core).
