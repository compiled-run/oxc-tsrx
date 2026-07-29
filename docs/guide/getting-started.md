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

## Using Vite+ (one extra step)

Vite+ looks for its lint and format tools as project-local *packages* named
`oxlint` and `oxfmt`, which a command name cannot satisfy. `oxc-tsrx setup`
writes those slots:

<!-- pm-install -->
```sh
npm install --save-dev vite-plus oxc-tsrx@latest
npx oxc-tsrx setup
```

Run both lines with your own package manager. A `vp create` scaffold names pnpm
in `devEngines.packageManager`, so npm will refuse to run there at all.

`setup` works inside `node_modules`, so run it again after every clean install.
It never edits `package.json`, and `oxc-tsrx remove` undoes it.

Then `vp lint`, `vp fmt`, and `vp check --fix` handle `.tsrx`. If your scaffold
turns type-aware lint on, there is one more dependency to add first:
[the type-aware template default](/integrations/vite-plus#type-aware-lint-may-need-one-dependency)
has the failure you would see and the fix.

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
  [`vp lint` and `vp fmt`](/integrations/vite-plus#oxlint-and-oxfmt-on-the-command-line-are-vites-here).

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
- **[Vite and Vite+](/integrations/vite-plus).** How this package and your
  framework's build plugin sit side by side, neither one touching the other.
- **[Architecture](/architecture/rust-oxc-core).** How one OXC parse serves
  linting, formatting, and your editor.
