---
title: Getting Started
description: Install oxc-tsrx, then parse, lint, format, and edit your first TSRX file.
---

# Getting Started

Everything ships in one package, `oxc-tsrx`. It gives you the `oxlint` and
`oxfmt` commands you already know, now handling `.tsrx` files, plus a parser
API, a language server, and support for your own
[custom JavaScript lint plugins](/integrations/custom-js-plugins).

It does not compile anything. Building and running `.tsrx` is your framework's
TSRX plugin's job, and you install that separately from
[tsrx.dev/getting-started](https://tsrx.dev/getting-started). The two are
independent: this package never touches your build or dev server.

## Install

You need Node.js 20.19 or newer. Install one dev dependency:

<!-- pm-install -->
```sh
npm install --save-dev oxc-tsrx@latest
```

That is the whole setup. The tools are Rust, but you get a prebuilt binary for
your platform: no Rust needed, no install scripts, nothing fetched later. It
works on CI that blocks postinstall.

Eight platforms have binaries.
[Platform Support](/reference/platform-support) says which is yours and how
well tested it is.

### The minimum steps, per host

This is the complete list of things you have to run to lint and format `.tsrx`.

| Where you use it | Steps | What you run |
| --- | --- | --- |
| Command line (`oxlint`, `oxfmt`) | 1 | `npm install --save-dev oxc-tsrx@latest` |
| Editor, through the released official OXC extension | 1 | the same install, and nothing else |
| [Vite+](/integrations/vite-plus) (`vp lint`, `vp fmt`) | 2 | the same install, then `oxc-tsrx setup` |

No row asks you to create a config file, add an ignore file, or add a lifecycle
script. `oxc-tsrx` writes nothing into your project during install.

Making TSRX a *language* in your editor belongs to the TSRX toolchain, not to
this package. It needs `@tsrx/typescript-plugin`, a framework binding, and a
`plugins` entry in the tsconfig that owns your source. `setup` reports all
three and installs none.
[Custom JavaScript plugins](/integrations/custom-js-plugins#the-whole-path-on-a-fresh-vite-project)
walks the sequence on a fresh scaffold.

### What the install adds to `node_modules/.bin`

Three commands are yours to type:

| Command | What it is |
| --- | --- |
| `oxlint` | the linter. Handles `.tsrx` plus ordinary files |
| `oxfmt` | the formatter. Same split |
| `oxc-tsrx` | `providers`, `status`, `setup`, and `remove`. See the [CLI reference](/reference/cli) |

Four more get linked that you never type: three native leaf commands, plus
`tsgolint` from a dependency.

- **Not Node-only.** npm, pnpm, yarn, and bun are covered in CI, and Deno works
  but is not. Only the wrappers need Node; the linter and formatter are one
  standalone binary at
  `node_modules/@oxc-tsrx/native-<your-platform>/bin/oxc-tsrx`.
- **Except under Vite+**, where `oxlint` and `oxfmt` are Vite+'s wrappers rather
  than ours. Use
  [`vp lint` and `vp fmt`](/integrations/vite-plus#oxlint-and-oxfmt-on-the-command-line-belong-to-vite-here).

To see what a host finds in your project, without changing anything:

```sh
npx oxc-tsrx providers --json
```

The line to look for is `routed extensions: .tsrx -> oxc-tsrx`. (Don't be
alarmed if `npx oxc-tsrx status` prints `missing` three times outside a Vite+
project. That is the correct result, and
[Limitations](/reference/limitations#cli-and-configuration) says why.)

### Editors need no extra install

Install `oxc-tsrx` and nothing else, and the released official OXC extension
gives you `.tsrx` diagnostics that refresh as you type, formatting, quick fixes
you can apply, and your own Oxlint JavaScript plugin rules. Ordinary TypeScript
files keep going to canonical Oxlint.

One step is not an install, and skipping it looks like a broken integration.
The official extension never activates on `.tsrx`. Open any JavaScript,
TypeScript, or JSON file once, and `.tsrx` is served for the rest of the
session. See
[the editor page](/integrations/editor#what-a-plain-install-actually-covers)
for what that session covered.

### Using Vite+? (compatibility step)

Released [Vite+](/integrations/vite-plus) finds its lint and format tools
through project-local *packages* named literally `oxlint` and `oxfmt`. A command
name cannot satisfy that, and this project cannot legitimately publish a package
under either name, so Vite+ needs the project-local slots that
`oxc-tsrx setup` writes:

<!-- pm-install -->
```sh
npm install --save-dev vite-plus oxc-tsrx@latest
npx oxc-tsrx setup
```

Pick your own package manager in the tabs above and run both lines with it. In a
project `vp create` scaffolded, that is almost never the npm tab: `vp create`
writes a `devEngines.packageManager` block into `package.json` naming pnpm, and
npm then refuses to run there at all.

`setup` is explicit, idempotent, reversible, and never edits `package.json`. It
works inside `node_modules`, so run it again after every clean dependency
install.

After that, `vp lint`, `vp fmt`, and `vp check --fix` handle `.tsrx`. If your
scaffold turns type-aware lint on, add one more dependency first: Vite+ and
this package can disagree about which `oxlint-tsgolint` runs.
[The type-aware template default](/integrations/vite-plus#the-type-aware-template-default)
has the failure, the fix, and the alternative.

Vite+ is the only place an install alone is not enough.

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

### Give the linter a path, or an empty folder will bury you

`npx oxlint` with no path lints everything under the current directory,
`node_modules` included. That is canonical Oxlint behavior, not something TSRX
adds. Two things avoid it:

- **A `.gitignore` listing `node_modules`.** Oxlint honors it even outside a git
  repository. Real projects have one; fresh scratch folders do not, which is the
  only place this bites.
- **Naming a path.** `npx oxlint src` lints your sources and nothing else.

This package ships no ignore file and no config, and writes none. Your
`.gitignore` and `.oxlintrc.json` are the only inputs.

> **Do not run `npx oxlint --fix` where `node_modules` is still in scope.** With
> nothing narrowing the run, `--fix` rewrites files inside your dependency tree
> and still exits 0. Official Oxlint does the same, so this is upstream parity
> rather than a TSRX defect, but your dependency tree is modified either way.
> Fix a path you own (`npx oxlint --fix src`), or make sure `node_modules` is
> ignored first.

`oxfmt` is not affected. It skips `node_modules` unless you pass
`--with-node-modules`.

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
- Keep your framework's TSRX plugin as-is; it compiles and runs `.tsrx`. See
  [tsrx.dev](https://tsrx.dev/getting-started) for it, and
  [Vite and Vite+](/integrations/vite-plus) for how the two sit side by side.
- Get live diagnostics, formatting, and quick fixes through the official OXC
  extension with [Editor integration](/integrations/editor).
- Curious how it works under the hood? Read
  [Architecture](/architecture/rust-oxc-core).
