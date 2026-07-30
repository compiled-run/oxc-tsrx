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

You need Node.js 20.19 or newer on the 20.x line, or Node.js 22.12 or newer.
Node 21 and Node 22.0 through 22.11 are not supported. Install one dev
dependency:

<!-- pm-install -->
```sh
npm install --save-dev oxc-tsrx@latest
```

That is the whole setup. There is no config file and no ignore file to write.
The tools are Rust, but you get a prebuilt binary for your platform: no Rust
needed, no install scripts, nothing fetched later. It works on CI that blocks
postinstall.

Eight platforms have binaries.
[Platform Support](/reference/platform-support) says which is yours and how
well tested it is.

### The minimum steps, per host

This is the complete list of things you have to run to lint and format `.tsrx`.

| Where you use it | Steps | What you run |
| --- | --- | --- |
| Command line (`oxlint`, `oxfmt`) | 1 | `npm install --save-dev oxc-tsrx@latest` |
| Editor, through the released official OXC extension | 1 | the same install, and nothing else |
| [Vite+](#try-it-with-vite) (`vp lint`, `vp fmt`) | 2 | the same install, then `oxc-tsrx setup` |

## Try it with Vite+

Paste this into an empty directory. It builds a React app, adds this package,
drops in a `.tsrx` component and one lint rule of your own, and lints them:

```sh
mkdir tsrx-vp-demo && cd tsrx-vp-demo
npm install vite-plus
export PATH="$PWD/node_modules/.bin:$PATH"

vp create vite --no-git --no-agent --no-editor --no-interactive --approve-builds \
  -- my-app --template react-ts
cd my-app

vp install -D oxc-tsrx@latest oxlint-tsgolint@0.24.0
vp exec oxc-tsrx setup

curl -sL https://github.com/markless-dev/oxc-tsrx/archive/refs/heads/main.tar.gz \
  | tar -xz --strip-components=4 oxc-tsrx-main/examples/custom-js-plugins/vite-plus

vp lint
```

A working setup reports your own rule twice, once from each component:

```text
src/Greeting.tsrx:5:9: warning house-rules(no-inline-style-object): Inline `style={{ ... }}` object. Use a class instead.
src/Panel.tsx:2:19: warning house-rules(no-inline-style-object): Inline `style={{ ... }}` object. Use a class instead.
Found 2 warnings and 0 errors.
```

The `.tsrx` line is the one that proves it. Without this package that file is
not linted at all. If you see only the `.tsx` line, your type-aware lane needs
a matching `oxlint-tsgolint`; see [If something goes wrong](#if-something-goes-wrong).

Three things in there are load-bearing. The `export PATH` line matters because
`vp create` spawns `vp install` by bare name, so without it you get a scaffold
with no `node_modules`. `vp install` and `vp exec` are used instead of `npm` and
`npx` because those two often refuse to run inside a Vite+ project. And `setup`
goes last, because it works inside `node_modules` and any install after it
quietly undoes it.

The curl line just copies five files out of
[`examples/custom-js-plugins/vite-plus`](https://github.com/markless-dev/oxc-tsrx/tree/main/examples/custom-js-plugins/vite-plus).
CI runs four of them on every change, so they stay honest. [Custom JavaScript
plugins](/integrations/custom-js-plugins#in-a-vite-project) builds them up one
at a time.

### Adding this to a project you already have

If you already have a project and it is not on Vite+ yet, these two lines add
both, and the walkthrough above is the rest of the story:

<!-- pm-install -->
```sh
npm install --save-dev vite-plus oxc-tsrx@latest
npx oxc-tsrx setup
```

If the project is *already* on Vite+, use `vp install -D` and `vp exec` instead.

Vite+ finds its linter and formatter by package name, searching `node_modules`
for packages literally called `oxlint` and `oxfmt`. A *command* named `oxlint`,
which is what installing `oxc-tsrx` gives you, is not enough. `setup` puts this
package in those two slots. It never edits your `package.json`, and
`oxc-tsrx remove` undoes it.

## In your editor

Install the official OXC extension. That is the whole setup, and your `.tsrx`
files get diagnostics, formatting, and quick fixes.

<!-- extension:oxc -->

One catch: it does not start on a `.tsrx` file. Open any JavaScript, TypeScript,
or JSON file once, and `.tsrx` works for the rest of the session.

Syntax highlighting and type checking are a different job, owned by the TSRX
toolchain rather than by this package. Its extension is what provides them:

<!-- extension:tsrx -->

See [the editor page](/integrations/editor#what-a-plain-install-actually-covers)
for what a plain install covers.

## What the install adds to `node_modules/.bin`

Three commands are yours to type:

| Command | What it is |
| --- | --- |
| `oxlint` | the linter. Handles `.tsrx` plus ordinary files |
| `oxfmt` | the formatter. Same split |
| `oxc-tsrx` | `providers`, `status`, `setup`, and `remove`. See the [CLI reference](/reference/cli) |

Four more get linked that you never type: three native leaf commands, plus
`tsgolint` from a dependency.

- **Not Node-only.** npm, pnpm, yarn, bun, and
  [Deno](https://deno.com "brand:deno") are all covered in CI. Only the thin
  wrappers need Node. The linter and formatter are one standalone binary.
- **Except under Vite+**, where `oxlint` and `oxfmt` are Vite+'s wrappers rather
  than ours. Use
  [`vp lint` and `vp fmt`](#if-something-goes-wrong).

To see what a host finds in your project, without changing anything:

<!-- pm-exec -->
```sh
npx oxc-tsrx providers --json
```

The line to look for is `routed extensions: .tsrx -> oxc-tsrx`.

Outside a Vite+ project, `npx oxc-tsrx status` prints `missing` three times.
That is the correct result, not a broken install:
[The CLI reference](/reference/cli#status-says-missing-in-a-healthy-project)
says why.

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

Mixed file types need no special handling. In a single run, `.tsrx` files go
through the TSRX engine while ordinary `.js`, `.jsx`, `.ts`, and `.tsx` files go
straight to OXC.

To change a rule's severity for one run, without touching your config, name it
on the command line:

<!-- pm-exec -->
```sh
npx oxlint --warn no-console --deny no-debugger src/Cart.tsrx
```

## Configuration

Both commands read your normal OXC config, searching upward from the current
directory:

| | Lint | Format |
| --- | --- | --- |
| Config file | `.oxlintrc.json` or `.oxlintrc.jsonc` | `.oxfmtrc.json` or `.oxfmtrc.jsonc` |
| Somewhere else | `oxlint --config path` | `oxfmt --config path` |

[Configuration](/integrations/configuration) lists exactly which fields are
supported.

## If something goes wrong

Almost everything that surprises people under Vite+ has one of these seven
shapes. Pick what you saw rather than reading all of them.

<!-- chooser -->

| What did you see? | What it means |
| --- | --- |
| `EBADDEVENGINES` | A `vp create` scaffold pins one exact version of whichever manager made it in `devEngines`, and `onFail: "download"` does not actually fetch it. Use `vp install`, `vp install -D pkg`, and `vp exec` instead of your own manager. They work when your version matches too, so there is no reason to check first. |
| `refusing to replace unowned package slot(s)` | An install wiped what `setup` wrote inside `node_modules`, and `setup` will not overwrite what it no longer owns. Rebuild the tree: `rm -rf node_modules && vp install && vp exec oxc-tsrx setup`. Installing on top of the old tree is not enough. |
| Editor misses `.tsrx`, `vp lint` sees it | Your editor needs `oxc.path.oxlint` in `.vscode/settings.json`, or the official OXC extension finds Vite+'s own `oxlint`, which knows nothing about `.tsrx`. `setup` writes that one line only when the extension would otherwise miss this package, leaves the rest of the file alone, and reports a value you set yourself rather than replacing it. |
| `setup` listed things it would not install | Not a failure. Highlighting and types for `.tsrx` belong to the TSRX toolchain, so `setup` names what is missing and stops: `@tsrx/typescript-plugin`, a framework binding, that plugin declared in the `tsconfig.json` owning your source (in a scaffold that is `tsconfig.app.json`, not the root one), and TypeScript in the `>=5.9 <6` range the plugin asks for. A current scaffold pins TypeScript 6, so everyone sees that last line. `vp lint` works either way. |
| `vp lint` reports `.tsx` and skips `.tsrx` | Type-aware lint runs on `oxlint-tsgolint`, and this package works only with the version it was built for rather than guessing at the protocol. The last line names both versions. Add the one it names as a direct dev dependency in the same `vp install` as `oxc-tsrx`, before `setup`: on its own afterwards it clears the error and switches `.tsrx` linting off in the same step. |
| A rule fires in one place but not the other | Vite+ moves any scaffolded `.oxlintrc.json` into the `lint` block of `vite.config.ts` and reads only that. Your editor still reads `.oxlintrc.json`. Write a rule you want in both places twice. |
| Bare `oxlint` says `No files found to lint` | Before `setup`, `node_modules/.bin/oxlint` is Vite+'s own and cannot see `.tsrx` at all. After `setup` it is this package's. Keep using `vp lint` and `vp fmt` anyway, because they read `vite.config.ts` while the bare commands read `.oxlintrc.json`, and here those are not the same file. |

`setup` is not going away: Vite+ resolves a package name, which a command name
cannot satisfy, and no released Vite+ reads the `oxc.provider` block that would
replace it.

The `vp` commands are tested on npm only. On the oldest supported Vite+ and the
pinned current one, the tests run a real production build and dev server with
hot reload, then `vp build`, `vp dev`, `vp lint`, `vp fmt --check`, and
`vp check --fix` across a range of configs. The report is
`tests/packaging/vite-plus-matrix-report.json`.

## Build from source (optional)

If you would rather build the native binaries yourself, you need a stable
Rust toolchain ([rustup](https://rustup.rs)):

```sh
git clone https://github.com/markless-dev/oxc-tsrx.git
cd oxc-tsrx
cargo build --release --locked -p oxc_tsrx_cli --bins
```

Keep the `--locked` flag: it makes Cargo build against the exact pinned OXC
commit from the lockfile. The binaries land in `target/release/`.

They emit JSON diagnostics and take explicit file paths only. The friendly text
output, directory walking, and glob handling live in the npm commands, so most
projects want those instead. See the [CLI Reference](/reference/cli) for every
flag.

## Next steps

- **[TSRX Syntax](/guide/tsrx-syntax).** Every block the linter and formatter
  understand, and what each one becomes.
- **[Editor integration](/integrations/editor).** Live diagnostics, formatting,
  and quick fixes while you type.
- **[Architecture](/architecture/rust-oxc-core).** How one OXC parse serves
  linting, formatting, and your editor.
