# Vite and Vite+

Your framework's TSRX plugin builds your app: compiling `.tsrx`, CSS, source
maps, and hot reload. `vp build` and `vp dev` never touch this package. What
`oxc-tsrx` adds is `.tsrx` support inside `vp lint`, `vp fmt`, and
`vp check --fix`.

<!-- pm-install -->
```sh
npm install --save-dev vite-plus oxc-tsrx@latest
npx oxc-tsrx setup
```

## A project to copy from

```sh
# `vp` comes from vite-plus, and `npx vp` is an unrelated package. `vp create`
# starts `vp install` by bare name, so vite-plus has to be on PATH first.
mkdir vp-host && cd vp-host && npm init -y && npm install vite-plus
export PATH="$PWD/node_modules/.bin:$PATH"
vp create vite --no-git --no-agent --no-editor --no-interactive --approve-builds \
  -- my-app --template react-ts

cd my-app
# The tsgolint version is the one this package asks for today; see below.
npm install --save-dev vite-plus oxc-tsrx@latest oxlint-tsgolint@0.24.0
npx oxc-tsrx setup
# copy the five files in: .oxlintrc.json, house-rules.mjs, and vite.config.ts at
# the project root, overwriting the scaffold's vite.config.ts, and Greeting.tsrx
# and Panel.tsx under src/. Then:
vp lint
```

Install everything in one command and run `setup` last. `setup` works inside
`node_modules`, so anything you install after it puts the real `oxlint` back and
your `.tsrx` files stop being linted, with no message and exit 0. The five files
in [`examples/custom-js-plugins/vite-plus`](https://github.com/markless-dev/oxc-tsrx/tree/main/examples/custom-js-plugins/vite-plus)
are a `.tsrx` component, a `.tsx` component, one custom lint rule that runs on
both, the `.oxlintrc.json` your editor reads, and the
[`vite.config.ts`](https://github.com/markless-dev/oxc-tsrx/blob/main/examples/custom-js-plugins/vite-plus/vite.config.ts)
that `vp lint` reads. They answer "what goes where" faster than this page can,
and CI runs four of the five on every change. [Custom JavaScript
plugins](/integrations/custom-js-plugins#in-a-vite-project) builds them up step
by step.

## What surprises people

### The one extra step Vite+ needs

Vite+ looks for its linter and formatter by package name, searching
`node_modules` for packages literally called `oxlint` and `oxfmt`. Installing a
*command* called `oxlint`, which is what `oxc-tsrx` does, is not enough to be
found that way. `oxc-tsrx setup` puts `oxc-tsrx` in those two places.

Because that happens inside `node_modules`, **any install afterwards wipes it**,
and running `setup` again does not put it back: it stops with `refusing to
replace unowned package slot(s): oxfmt`. So install everything the project needs
in one go and run `setup` last. It never edits your `package.json`, and
`oxc-tsrx remove` undoes it.

If it has already happened, the way back is to build `node_modules` again from
nothing: `rm -rf node_modules && npm install && npx oxc-tsrx setup`. Keep
whatever you installed; deleting `node_modules` is the part that matters, and
installing on top of it is not enough.

### `setup` writes one file in your own project

Everything else it does is inside `node_modules`, but your editor may need one
line: `oxc.path.oxlint` in `.vscode/settings.json`. Without it the official OXC
extension finds Vite+'s own `oxlint`, which knows nothing about `.tsrx`.

`setup` writes that line only when the extension would otherwise miss this
package, and it tells you either way. In the `vp create` scaffold above it
printed `oxc.path.oxlint: unnecessary (editor)` and wrote no file at all,
because `node_modules/.bin/oxlint` already resolves into this package. When it
does write, the rest of the file is left alone, a value you set yourself is
reported instead of replaced, and `oxc-tsrx remove` takes back only that one
line. Outside Vite+, nothing is written at all.

### `setup` reports the editor prerequisites but does not install them

Making `.tsrx` a language your editor understands, with highlighting and types,
belongs to the TSRX toolchain, not to this package. So `setup` prints what is
missing and stops: `@tsrx/typescript-plugin`, a framework binding, the
`tsconfig.json` that owns your source declaring that plugin (in a scaffold that
is `tsconfig.app.json`, not the root one), and TypeScript in the `>=5.9 <6`
range that plugin asks for.

### Type-aware lint may need one dependency

A `vp create` React project turns type-aware lint on. That lane runs on a
separate tool, `oxlint-tsgolint`, and the two projects can disagree about which
version to use: this package runs only against the version it was built for, and
refuses anything else rather than guess at the protocol. When they disagree your
ordinary files get linted and your `.tsrx` files stop with a message naming both
versions:

```text
oxlint (oxc-tsrx): unsupported tsgolint version <theirs>; OXC for TSRX requires oxlint-tsgolint <ours> for protocol v2
```

Install the version that message asks for as a direct dev dependency, and both
linters use your copy with your `lint` block unchanged. It has to go in the same
`npm install` as `vite-plus` and `oxc-tsrx`, before `npx oxc-tsrx setup`:
installing it on its own afterwards clears this error and switches `.tsrx`
linting off in the same step. Removing `options` from that block also works, but
costs you type-aware lint everywhere, so it is the second choice. To see which
version your project resolves today:

```sh
node -e "console.log(require.resolve('oxlint-tsgolint/bin/tsgolint',{paths:[process.cwd()]}))"
```

### `vp lint` and your editor read different files

Vite+ moves any `.oxlintrc.json` it scaffolded into the `lint` block of
`vite.config.ts` and reads only that from then on. Your editor still reads
`.oxlintrc.json`. On a fresh project, a rule written in one of them produced
nothing in the other, so write a rule you want in both places twice.

### `oxlint` and `oxfmt` on the command line are Vite+'s here

In a scaffold you have not run `setup` in, both of those commands belong to
Vite+. `node_modules/.bin/oxlint` is Vite+'s own `oxlint`, and it does not see
`.tsrx` files: pointing it at one prints `No files found to lint` and exits 1,
while `node_modules/.bin/oxfmt` formats your ordinary files as usual. `setup`
replaces both, so after it `./node_modules/.bin/oxlint` is this package's and
lints your project, `.tsrx` included. Keep using `vp lint` and `vp fmt` anyway:
they read your `vite.config.ts`, while the command reads `.oxlintrc.json`, and
those two are not the same file here. The plain `oxlint` and `oxfmt` commands elsewhere in
these docs are for projects that do not use Vite+.

## How your config reaches the linter

When Vite+ hands over `vite.config.*`, this package reads it once through
Vite+'s public API, takes only the `lint` or `fmt` part, and writes it to a
temporary JSON file for the native tool. Nothing is added to your project.
Relative paths in `extends`, override globs, and `ignorePatterns` are resolved
from where you wrote them, and anything that cannot be turned into JSON, such as
a callback, fails with a clear error instead of being dropped.
`lint.options.typeAware` and `typeCheck` turn into the `--type-aware` and
`--type-check` flags for you.

## What is tested

The `vp` commands are tested on **npm only**. pnpm, Yarn, and Bun are not
claimed for them. On the oldest supported Vite+ and the pinned current one, the
tests run a real production build and dev server with hot
reload, then `vp build`, `vp dev`, `vp lint`, `vp fmt --check`, and
`vp check --fix`, over configs ranging from an imported `extends` to
`.tsrx`-only overrides. The report is
`tests/packaging/vite-plus-matrix-report.json`, and
[Benchmarks](/reference/benchmarks) has the timings.

`setup` is not going away. Vite+ resolves a package name, which a command name
cannot satisfy, and no released Vite+ reads the
[`oxc.provider`](/architecture/provider-protocol) block that would replace it.
