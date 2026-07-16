---
title: Getting Started
description: Install the native companion packages or build from source, then run your first TSRX lint and format.
---

# Getting Started

OXC for TSRX ships as two npm packages: `oxlint-tsrx` for linting and
`oxfmt-tsrx` for formatting. They give you the same `oxlint` and `oxfmt`
commands you may already know, with TSRX support built in, and they plug into
Vite+ and Visual Studio Code. Under the hood, each package runs a native Rust
binary (`oxc-tsrx`, `oxc-tsrx-fmt`, and `oxc-tsrx-lsp` for editors) that
`@oxc-tsrx/runtime` selects for your platform. The 0.1.0 package set is
released as one unit; if any required platform package is missing at that exact
version, installation fails instead of falling back to a slower or different
engine.

## Install the command packages

After the approval-gated 0.1.0 registry launch, install both companions:

```sh
npm install --save-dev oxlint-tsrx@0.1.0 oxfmt-tsrx@0.1.0
```

The package names are project-specific, while their executable names stay
compatible with OXC:

```sh
npx oxlint --format=json src/Counter.tsrx src/View.tsx
npx oxfmt --check src/Counter.tsrx src/View.tsx
npx oxfmt --write src/Counter.tsrx src/View.tsx
```

`@oxc-tsrx/runtime` is installed transitively and selects the exact native
package for the host. There is no install script and no downloaded-at-runtime
binary. If npm does not yet show the complete 0.1.0 package set, use the source
checkout below; this repository never treats local release preparation as proof
that a registry publication happened.

## Prerequisites

- A stable Rust toolchain ([rustup](https://rustup.rs), `cargo`).
- Node.js `^20.19.0 || >=22.12.0` for the JavaScript tests and companion
  packages.

## Build from source

```sh
git clone https://github.com/thejackshelton/oxc-tsrx.git
cd oxc-tsrx
cargo build --release --locked -p oxc_tsrx_cli --bins
```

Keep the `--locked` flag: it makes Cargo use the exact dependency versions in
the lockfile, which is how the project guarantees you're building against the
one pinned OXC commit (`8e0ed2ebb96137fb1611cdbd5742d5cb46037d40`). The
binaries land in `target/release/`.

## Your first lint

```sh
target/release/oxc-tsrx --format=json src/Counter.tsrx src/View.tsx
```

Two things to know:

- **List files explicitly.** The native CLI doesn't walk directories or
  expand globs yet; that's handled by the npm companions and Vite+.
- **Mixed file types are fine.** `.tsrx` files go through the TSRX engine;
  ordinary `.js`/`.ts`/`.tsx` files go straight to OXC.

Diagnostics come out as JSON, positioned at your original TSRX code. You can
tweak rule severity right on the command line:

```sh
target/release/oxc-tsrx --format=json \
  --warn no-console --deny no-debugger src/Counter.tsrx
```

## Your first format

```sh
# Check only: exits 1 and lists the files that would change.
target/release/oxc-tsrx-fmt --check src/Counter.tsrx

# Format and write files.
target/release/oxc-tsrx-fmt --write src/Counter.tsrx src/View.tsx

# Editor/stdin mode: formatted source goes to stdout.
target/release/oxc-tsrx-fmt --stdin-filepath=src/Counter.tsrx < src/Counter.tsrx
```

Write mode formats everything first and only then replaces files, so a failure
in one file never leaves your project half-written.

## Configuration

Both commands look for your normal OXC config by searching from the current
directory upward (`.oxlintrc.json`/`.oxlintrc.jsonc` for lint,
`.oxfmtrc.json`/`.oxfmtrc.jsonc` for format), or take an explicit
`--config`/`-c` path. Config is read and compiled once per run, then reused
for every file. See [Configuration](/integrations/configuration.html) for
exactly which fields are supported.

## Run the project's own checks

The repository's full proof suite:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --release --locked -p oxc_tsrx_cli --bins
npm test
npm run build:editor
npm run test:editor
npm run test:editor:vscode
```

Performance is enforced by benchmark gates; see
[Benchmarks](/reference/benchmarks.html):

```sh
cargo run --release --locked -p oxc_tsrx_benchmark -- \
  --assert benchmarks/native-lint/budgets.json
cargo run --release --locked -p oxc_tsrx_format_benchmark -- \
  --assert benchmarks/native-format/budgets.json
npm run benchmark:type-aware
npm run benchmark:editor
```

## Next steps

- See which TSRX syntax is supported in
  [TSRX Syntax](/guide/tsrx-syntax.html).
- Wire the commands into `vp lint` / `vp fmt` with
  [Vite and Vite+](/integrations/vite-plus.html).
- Configure live diagnostics and format-on-save with
  [Editor integration](/integrations/editor.html).
- Curious how it works under the hood? Read
  [Architecture](/architecture/rust-oxc-core.html).
