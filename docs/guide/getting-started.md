---
title: Getting Started
description: Install oxc-tsrx, then parse, lint, format, and edit your first TSRX file.
---

# Getting Started

OXC for TSRX is prepared as one public package, `oxc-tsrx`. It gives you the
`oxlint` and `oxfmt` commands you may already know, an OXC-shaped parser API,
support for your own [custom JavaScript lint
plugins](/integrations/custom-js-plugins) on `.tsrx` as well as on ordinary
files, and a native language server. It plugs into
[Vite+](/integrations/vite-plus) and the released
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

### The minimum steps, per host

This is the complete list of things you have to run. Each row was measured
against the published `oxc-tsrx` 0.1.0 installed from the registry into a clean
project on darwin-arm64.

| Where you use it | Steps | What you run |
| --- | --- | --- |
| Command line (`oxlint`, `oxfmt`) | 1 | `npm install --save-dev oxc-tsrx` |
| Editor, through released `oxc.oxc-vscode` | 1 | the same install, and nothing else |
| [Vite+](/integrations/vite-plus) (`vp lint`, `vp fmt`) | 2 | the same install, then `oxc-tsrx setup` |

The Vite+ second step is permanent, and you run it again after every clean
dependency install. Vite+ resolves a *package* named `oxlint` and pins
`oxlint@=1.72.0`, and a command name cannot answer a package resolution. See
[Vite and Vite+](/integrations/vite-plus) for the full reasoning.

No row asks you to create a config file, add an ignore file, or add a lifecycle
script. `oxc-tsrx` writes nothing into your project during install.

### What the install adds to `node_modules/.bin`

A clean install links seven commands. Only the first three are ones you type:

| Command | What it is |
| --- | --- |
| `oxlint` | the linter you run. Handles `.tsrx` plus ordinary files |
| `oxfmt` | the formatter you run. Same split |
| `oxc-tsrx` | `providers`, `status`, `setup`, and `remove`. See the [CLI reference](/reference/cli) |
| `oxc-tsrx-lint`, `oxc-tsrx-fmt`, `oxc-tsrx-lsp` | the native leaf commands `oxlint` and `oxfmt` dispatch to. Useful directly only if your project pins official `oxlint` |
| `tsgolint` | not part of this project. It comes from the `oxlint-tsgolint` dependency, the official type-aware runner behind `--type-aware`. You never call it yourself, and calling it prints its own "unsupported entrypoint" warning |

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

### Editors need no extra install

Install `oxc-tsrx` and install nothing else, and the released official OXC
extension (measured at `oxc.oxc-vscode` 1.59.0) gives you `.tsrx` diagnostics
that refresh as you type, formatting, quick fixes you can apply, and your own
Oxlint JavaScript plugin rules, while ordinary TypeScript files keep going to
canonical Oxlint. This was measured in a real editor session on darwin-arm64.

There is one step in that session that is not an install, and skipping it looks
like the integration is broken. The official extension's activation events
never mention `.tsrx`, and Ripple's extension contributes `.tsrx` as the
language id `ripple`, so opening a `.tsrx` file first does not start OXC's
extension. Open any JavaScript, TypeScript, or JSON file in the workspace once,
and `.tsrx` is served for the rest of the session. See
[the editor integration page](/integrations/editor#what-a-plain-install-actually-covers)
for what that session covered.

### Using Vite+? (compatibility step)

Released [Vite+](/integrations/vite-plus) does not discover providers. It finds
its lint and format tools through project-local *packages* named literally
`oxlint` and `oxfmt`. A command name cannot satisfy that, and this project
cannot legitimately publish a package under either name, so Vite+ needs the
project-local slots that `oxc-tsrx setup` writes:

<!-- pm-install -->
```sh
npm install --save-dev vite-plus oxc-tsrx
npx oxc-tsrx setup
```

Pick your own package manager in the tabs above and run both lines with it.
`npx` belongs to the npm tab only: `vp create` writes a
`devEngines.packageManager` block into `package.json`, and npm refuses to run
in a project that names a different manager, exiting with `EBADDEVENGINES`.
Any modern version of your package manager works, and Corepack is not required.

`setup` is explicit, idempotent, reversible, and never edits `package.json`.
Run it again after a clean dependency install, because `node_modules` is
disposable.

After that, `vp lint`, `vp fmt`, and `vp check --fix` handle `.tsrx` files, with
one config edit first: a project scaffolded by `vp create` enables
`options.typeAware`/`typeCheck` in the `lint` block of `vite.config.ts`, and the
native TSRX path refuses that lane, failing the run before anything is linted.
Delete that one key and keep the rest, including the template's `jsPlugins`
entry, which runs on both halves of the project.
The [Vite and Vite+ page](/integrations/vite-plus#one-template-default-you-have-to-turn-off-first)
explains why and shows the before and after.

Vite+ is the only place an install alone is not enough. Every other route in
this guide works with no command at all.

#### `oxc-tsrx status` is about those facades only

`status` reports on the Vite+ compatibility facades and on nothing else. In a
command-line or editor project it prints this, exit code 0:

```text
oxc-tsrx 0.1.1 compatibility (npm)
- oxc-parser: missing
- oxlint: missing
- oxfmt: missing
```

Three `missing` lines are the correct result there. They mean the Vite+ facades
are not installed, which is what you want when you do not use Vite+. Nothing is
broken and there is nothing to fix.

To check that your install is working, run `npx oxc-tsrx providers` instead. The
line to look for is `routed extensions: .tsrx -> oxc-tsrx`.

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

The commands above name a file on purpose. `npx oxlint` with no path at all
lints everything it can find under the current directory, and that includes
`node_modules`.

In a scratch folder created with `mkdir demo && npm init -y`, a measured bare
run reported **9260 warnings, 9257 of them from `node_modules`**. The three
warnings from `src/` were the ones the user wanted.

This is canonical Oxlint behavior, not something TSRX adds. Running the official
`oxlint` binary from the same install produces the same wall. Two things keep it
away from you:

- **A `.gitignore` listing `node_modules` fixes it completely.** Oxlint honors
  that file even when the folder is not a git repository, so the measured run
  above drops straight to its 3 real warnings once the file exists. Almost every
  real project already has one. A brand new scratch folder does not, and that is
  the only place this bites.
- **Naming a path works either way.** `npx oxlint src` lints your sources and
  nothing else.

`oxc-tsrx` ships no ignore file and no config of its own, and it never writes one
into your project. Your own `.gitignore` and `.oxlintrc.json` are the only
inputs.

> **Do not run `npx oxlint --fix` where `node_modules` is still in scope.** With
> nothing narrowing the run, `--fix` rewrites files inside your dependency tree.
> A measured run in a project with no source files at all changed 15 files under
> `node_modules` and still exited 0. Official Oxlint changed 13 in the same
> folder, so this is upstream parity rather than a TSRX defect, but your
> dependency tree is modified either way. Fix a path you own
> (`npx oxlint --fix src`), or make sure `node_modules` is ignored first.

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
