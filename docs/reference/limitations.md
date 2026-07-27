---
title: Limitations
description: What OXC for TSRX does not support yet, and why each gap fails loudly instead of quietly.
---

# Limitations

Everything on this page fails loudly: you get a clear error, never a silently
skipped file or a wrong-but-plausible result.

## You cannot build or run a `.tsrx` file yet

This is the largest limitation on the page, so it goes first.

`oxc-tsrx` is a lint, format, parse, and editor toolchain for `.tsrx`. It is not
a compiler you can ship an application with. There is no Vite plugin, no Rollup
or Rolldown plugin, and no loader. Import a `.tsrx` module from your application
and the bundler parses it as ordinary TypeScript and fails on the first `@{`:

```text
$ vp build
✗ `,` or `)` expected
```

Measured with Vite+ 0.2.6 and `oxc-tsrx` 0.1.0. Nothing is wrong with your
install when you see this.

You cannot fill the gap yourself from the public API either. The legal-TSX
projection that makes linting and formatting work happens inside Rust and is
never returned as source, and no export hands it back. `oxc-tsrx/parser` gives
you an AST, `oxc-tsrx/format` gives you formatted TSRX, and neither is code a
bundler can consume.

So the supported loop today is: author `.tsrx`, check it with `oxlint`/`oxfmt`
(or `vp lint` and `vp fmt`), and edit it with diagnostics and quick fixes in
your editor. Running the result in a browser is not part of this project yet.

## Formatting

- **CSS inside `<style>` is never reformatted.** The surrounding TSRX/JSX
  gets full Oxfmt layout, but style contents are preserved byte-for-byte.
  Using OXC's CSS formatter currently requires patching OXC's dependency
  graph, and the project's core rule is no patches, so this waits on
  upstream. See the
  [embedded CSS boundary](/architecture/embedded-css-boundary).
- `.editorconfig` and formatter options that take callbacks are rejected.
- Oxfmt options that only affect other languages (JSON, prose) do not change
  `.tsrx` output.

## Syntax

- Dynamic tags whose name expression contains more dynamic JSX (a dynamic tag
  inside a dynamic tag's name) are not supported.
- Half-typed or broken syntax is never linted or formatted approximately. The
  editor publishes a `parse-error`, returns no formatting edit, and resumes
  normal diagnostics once the buffer is valid again.

## Linting

- **JavaScript lint plugins on `.tsrx` see the projection, not your authored
  tree.** Your `jsPlugins` rules do run on `.tsrx`, from the `oxlint` command
  and in the editor, but neither runs your module against the TSRX AST. The
  native path is Rust with no Node.js runtime, so the legal-TSX projection of
  your file is linted by the published Oxlint binary instead, and diagnostics
  are mapped back to your bytes. Four consequences:
  - `@if`, `@for`, `@switch`, and `@try` reach your rule as the ordinary `if`,
    `for`, and `switch` statements they project to. A rule keyed on
    `JSXForExpression` never fires on this route.
  - `context.filename` is the mirror path, ending in `.tsrx.tsx`, not your
    authored path.
  - Each linted `.tsrx` file is parsed once more. The cost is announced on
    stderr by the CLI and in the server log by the editor, and
    `settings.oxcTsrx.jsPluginsOnTsrx: false` turns the lane off.
  - A diagnostic whose position falls on projected-only text is dropped rather
    than reported at an invented location.

  For a rule that must see authored TSRX nodes there is still no released
  Oxlint route. *The local ESLint adapter* is one escape hatch and is AST-only:
  the public parser v1 exposes no token stream, so `SourceCode` token rules
  cannot be correct, and there is no full framework scope contract. *The
  upstream custom-parser draft* is broader (it does provide `SourceCode` and
  forces token/range/location options), but it is unmerged and built locally,
  so it is not a released product path. See [Custom JavaScript
  plugins](/integrations/custom-js-plugins).
- **Type-aware rules** require an explicit `--type-aware` or `--type-check`
  opt-in and the exact supported `oxlint-tsgolint` executable. Missing or
  mismatched tooling fails instead of silently downgrading. The editor
  analyzes each requested document; cross-document unsaved project semantics
  are not claimed.
- Not every OXC rule is guaranteed to behave identically around the TSRX
  placeholders. The tested guarantee covers the standard rules in the test
  matrix; anything that would report inside placeholder code is suppressed.
- Alternate report formats and per-directory nested configs are not done.

## CLI and configuration

- **A bare `npx oxlint` lints `node_modules` too, and that is upstream
  behavior.** With no path argument and nothing else narrowing the run, Oxlint
  walks the whole current directory. Measured in a scratch project created with
  `npm init -y`: 9260 warnings, 9257 of them from `node_modules`. Official
  Oxlint from the same install reproduces it, so this is parity with canonical
  Oxlint and not something the TSRX drop-in adds.

  A `.gitignore` containing `node_modules` removes it completely, and no git
  repository is needed for that file to count. Naming a path
  (`npx oxlint src`) avoids it as well. This project ships no ignore file, no
  default config, and no postinstall that would write one for you, so an empty
  scratch folder is the one place you will meet it.
- **`npx oxlint --fix` will rewrite files inside `node_modules` if they are in
  scope.** Measured in a project with no source files of its own: 15 files
  changed under `node_modules`, exit code 0, no warning. Official Oxlint changed
  13 in the same folder, so again this is upstream parity. Point `--fix` at a
  path you own, or make sure `node_modules` is ignored first. `oxfmt` is not
  affected; it skips `node_modules` unless you pass `--with-node-modules`.
- **`npx oxc-tsrx status` reports `missing` three times in a healthy project.**
  It only inspects the Vite+ compatibility facades, so `oxc-parser: missing`,
  `oxlint: missing`, and `oxfmt: missing` are the correct state for every
  command-line and editor user. It exits 0. Use `npx oxc-tsrx providers` to
  check that TSRX support is wired up.
- **A seventh command, `tsgolint`, appears in `node_modules/.bin`.** It is not
  part of this project. It comes from the `oxlint-tsgolint` dependency, which is
  the official type-aware runner behind `--type-aware` and `--type-check`. You
  never invoke it yourself.
- The native binaries take **explicit file paths only**. Directory walking
  and globs come from the `oxc-tsrx` npm commands and Vite+.
- Config files must be JSON/JSONC. JS/TS config modules are rejected, except
  through the Vite+ path, where the toolchain resolves your
  `vite.config.*` once via Vite+'s public API and hands both engines the same
  extracted `lint`/`fmt` settings. Values that cannot be serialized (like
  callbacks) fail with a clear error.

## Packaging and ecosystem

- **Prepared locally, not published from this repository state.** The eight
  native npm targets and the hosted release workflow are ready, and the host
  target has local build, install, and execution proof. npm availability is
  only claimed after an explicitly approved publication.
- **Vite+ needs one command after install, permanently.** Vite+ finds its
  lint/format tools by the literal *package* names `oxlint` and `oxfmt`, and it
  pins its own `oxlint@=1.72.0`. A bin name cannot answer a package resolution,
  and `oxc-tsrx` cannot legitimately publish a package under either name, so
  `oxc-tsrx setup` writes those project-local slots instead.

  Because `setup` works inside `node_modules`, a clean install wipes it and you
  run it again. That rerun is real and it is not scheduled to go away.
- **Everywhere except Vite+, the install is the whole step, and the `oxlint` /
  `oxfmt` command names are how.** Counted out: one step for the command line,
  one step for the editor, two for Vite+, and the second Vite+ step repeats
  after every clean dependency install. The full table is in
  [Getting Started](/guide/getting-started#the-minimum-steps-per-host).

  `oxc-tsrx` declares bins under those names,
  which is exactly what released Vite+ and the released official OXC extension
  select by. That is the shipped delivery mechanism, not a stopgap.

  `oxc-tsrx` also declares a static `oxc.provider` block in its own
  `package.json`, and a host that reads that block can find TSRX from the
  install alone. Three separate facts hold at once here, and they are easy to
  blur:
  - discovery is implemented and proven locally from clean consumers on npm,
    pnpm, Bun, and both Yarn Berry linkers;
  - no released Oxlint, Oxfmt, Vite+, or `oxc.oxc-vscode` build reads
    `oxc.provider`. Nothing has been submitted upstream, nothing has been
    accepted, and upstream patching is not part of this project's plan, so no
    released host is going to start reading it;
  - so `oxc.provider` is a recorded proposal, and the command names plus
    `setup` are what actually deliver the product. They stay.

  Of the four capabilities that block declares, only the language server has a
  host, and that host is the `oxlint --lsp` multiplexer inside `oxc-tsrx`. The
  parser has no host at all: it is public, but you reach it by importing
  `oxc-tsrx/parser` yourself, never through discovery.
- **A project that pins official `oxlint` or `oxfmt` keeps official behavior
  for those command names.** That is deliberate: breaking a pinned setup would
  be worse. `.tsrx` is then reachable through `oxc-tsrx-lint` and
  `oxc-tsrx-fmt`, which are always installed.
- An earlier research design that would have had `setup` write project-owned
  dependency aliases, overrides, and a lockfile once is **not** the roadmap. It
  would put permanent rewrites in your own manifest to satisfy one host's
  resolution, which is worse than one explicit, reversible command. That design
  is superseded, not pending.
- The native language server and released-official-extension integration are
  proven locally. The optional legacy VSIX is also proven, but its Marketplace
  availability is a separate approval-gated action and it is not required for
  the primary editor workflow.
- OXC upgrades are manual: bump the adapter crate and lockfile, then pass the
  full behavior and performance suites.

## What the test suites do and do not prove

- The pinned read-only Markless corpus proves the formatter contract on real
  code: 179/179 valid files format, re-parse, and converge; 12/12
  known-invalid fixtures are rejected; every `<style>` payload survives
  byte-for-byte. It does not test Markless's own compiler or runtime.
- The Vite and Vite+ build/dev/command matrices pass for the tested minimum
  Vite+ 0.1.24 and the pinned current Vite+ 0.2.4. The end-to-end `vp` command
  matrix runs on npm only; pnpm, Yarn, and Bun are not claimed for those
  commands. The package-manager facade proof and the npm `vp` command proof are
  separate axes, not one combined guarantee.
- A disposable-copy editor walkthrough proves automatic activation, live
  diagnostics, real format-on-save, and one validated safe action without
  changing the external worktree. Automatic activation there is the optional
  legacy VSIX's, which declares `.tsrx` itself. The released official OXC
  extension does not: its activation events are 21 `onLanguage:` entries and
  none of them is `.tsrx`'s language, so a `.tsrx` file opened first in a
  session does not start it. Open an ordinary JavaScript, TypeScript, or JSON
  file once and the rest of the session works. See
  [Editor integration](/integrations/editor#what-a-plain-install-actually-covers).
