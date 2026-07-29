---
title: Editor integration
description: Native formatting, lint diagnostics, and validated quick fixes for TSRX in Visual Studio Code.
---

# Editor integration

Install `oxc-tsrx` in your project and the official OXC extension in your
editor. There is no TSRX-specific extension to add and no fork to install, and
outside Vite+ there is no setup command either.

<!-- extension:oxc -->

The extension starts your project's `oxlint --lsp`, and `oxc-tsrx` uses that one
process as a narrow multiplexer: `.tsrx` traffic goes to its native server, and
ordinary JavaScript and TypeScript keep going to official Oxlint. [Your own
Oxlint JavaScript rules](#your-own-javascript-rules-in-the-editor) show up as
squiggles too.

Two things to know before you start:

- **It does not wake up on a `.tsrx` file.** Open any JavaScript, TypeScript, or
  JSON file once per session, and `.tsrx` is served from then on.
  [Why](#what-a-plain-install-actually-covers).
- **A Vite+ project owns `node_modules/.bin/oxlint`**, which is where the
  extension looks, so it needs one setup command.
  [Which one](#in-a-vite-project-setup-writes-oxcpathoxlint).

Syntax highlighting and IntelliSense for `.tsrx` are a different job, owned by
the TSRX toolchain rather than by this package. Its extension provides them, and
the two run side by side:

<!-- extension:tsrx -->

## Setup

1. Install the official OXC extension, `oxc.oxc-vscode`.
2. Add `oxc-tsrx` to the project.
3. Open a JS, TS, or JSON file once, so the extension starts.
4. Open a `.tsrx` file. Diagnostics, formatting, and quick fixes come through
   the official client.

To format on save, make the extension the default formatter for whatever
language id your framework contributes:

```json
{
  "[markless-tsrx]": {
    "editor.defaultFormatter": "oxc.oxc-vscode",
    "editor.formatOnSave": true
  }
}
```

Settings come from your normal `.oxlintrc.json` and `.oxfmtrc.json`.

## What "a plain install" actually covers

The official extension shipped before `.tsrx` existed, so it lists no
activation event for the file type, and it picks the documents it serves from a
fixed internal list rather than from a public API. That is just what an editor
extension looks like before a new file type shows up. Meanwhile the TSRX
extension claims `.tsrx` under its own language id, so opening one on its own
does not wake OXC's client.

Open any JavaScript, TypeScript, or JSON file once. `.tsrx` is then served for
the whole session, whatever language id it has, because the registrations match
file names.

That one extra step is the whole gap, and it is why this package exists today
instead of a patch upstream: support that already works in the wild makes a
better case for `.tsrx` in OXC than an ask would. The extension finds this
package because it declares a command named `oxlint`. The `oxc.provider` block
it also declares is [our own proposal, which no released tool reads
yet](/architecture/provider-protocol). OXC's
[Language Plugins RFC](https://github.com/oxc-project/oxc/discussions/21936)
could remove both the extra step and the multiplexer, and we would rather end up
there than keep our own path; [Upstreaming to OXC](/architecture/upstreaming-to-oxc)
tracks what that needs.

## What a session looks like

Here is the whole flow from a keystroke to a squiggle. Select any node to
read what it does, or step through the buttons:

<!-- diagram:editor-session -->

And here is that session as you would actually see it. Press Play, or step
through the stages yourself. Hover the squiggles: those are the real
diagnostics the server publishes.

<!-- editor-replay -->

## What the server provides

One long-lived native process provides:

- live diagnostics on unsaved buffers, mapped to your original TSRX positions;
- Oxfmt-backed whole-document formatting;
- quick fixes, but only with an exact mapping onto your code and a clean
  reparse;
- your own Oxlint JavaScript plugin rules, when `.oxlintrc.json` declares
  `jsPlugins`; and
- opt-in type-aware diagnostics through TypeScript-Go.

Everything runs on the in-memory buffer, and code actions never touch disk.
Broken syntax publishes a `parse-error` rather than stale results, formatting it
returns no edit, and the next valid edit restores normal diagnostics.

## Your own JavaScript rules in the editor

If your `.oxlintrc.json` declares `jsPlugins`, those rules run on `.tsrx` in the
editor as well as on the command line, at the same positions, with nothing extra
to configure:

```json
{
  "jsPlugins": ["./oxlint-demo-plugin.mjs"],
  "rules": {
    "tsrx-demo/require-keyed-map": "error"
  }
}
```

Open a `.tsrx` file and your rule is a squiggle, next to the built-in Rust ones.
The [custom JavaScript plugins guide](/integrations/custom-js-plugins) is the
tutorial. Two things are specific to the editor.

**It costs one extra parse of each `.tsrx` file the server lints.** The native
server is Rust with no Node.js runtime, so it hands the TSX copy of your buffer
to a small Node.js host, runs the published Oxlint binary over it, and maps the
diagnostics back. That happens on open, on change, and on save. The host starts
once per workspace, only when your config declares `jsPlugins`, and the server
says so once in its output log, naming the key that turns it off:

```json
{
  "settings": {
    "oxcTsrx": {
      "jsPluginsOnTsrx": false
    }
  }
}
```

With that set, your plugins keep running on ordinary files and `.tsrx` publishes
one `lint-unavailable` diagnostic explaining why, rather than going quiet.

**Your rule sees the copy, not what you wrote.** `context.filename` is the
mirror path ending in `.tsrx.tsx`, and `@if` and `@for` reach your rule as
ordinary `if` and `for`, though the squiggle still lands on your file. See [what
your rule
sees](/integrations/custom-js-plugins#what-your-rule-sees-on-tsrx). If the lane
cannot start, or a rule throws, the built-in diagnostics still publish and a
`js-plugins-unavailable` warning carries the reason: fewer rules running is
never silent.

### In a Vite+ project, `setup` writes `oxc.path.oxlint`

The extension finds its linter by looking for `oxlint` in `node_modules`. In a
Vite+ project that lookup reaches Vite+'s own wrapper, which knows nothing about
`.tsrx`, so you would get no `.tsrx` diagnostics and no error explaining why.

`oxc-tsrx setup` handles it by merging one key into your `.vscode/settings.json`:

```json
{
  "oxc.path.oxlint": "node_modules/oxc-tsrx/bin/oxlint"
}
```

Reload the window afterwards. Everything else in the file is preserved, a value
you set yourself is reported rather than overwritten, and `oxc-tsrx remove`
takes back only that key. [The Vite+
page](/integrations/vite-plus#setup-writes-one-file-in-your-own-project)
has the full rules.

Outside Vite+ the ordinary lookup does find this package, so nothing is written
and `status` reports the slot as `unnecessary`.

## Reproducible proof

A clean Extension Host run installs untouched local release tarballs into an
empty consumer whose only TSRX dependency is `oxc-tsrx`, loads the released
official extension with no second TSRX client installed, and proves canonical
TypeScript diagnostics, native TSRX diagnostics, an unsaved buffer update,
formatting, and the validated `no-var` quick fix.

There is no separate language-server executable to build. `crates/oxc_tsrx_cli`
produces one binary, and its `lsp` subcommand serves an editor.

```sh
pnpm run build:native
pnpm run test:editor
pnpm run test:plugins
pnpm run test:editor:official-toolchain
pnpm run build:editor
pnpm run test:editor:vscode
pnpm run test:packaging:vscode
pnpm run benchmark:editor
```

`test:plugins` is the one that covers the JavaScript plugin lane. It drives a
real user plugin over a real `.tsrx` file twice, once through `oxlint` and once
through the language server, and fails if the two disagree about a position.

There is a second, source-only stack that hands your rule the authored TSRX tree
instead of the copy, with `JSXIfExpression` and `JSXForExpression` intact. It
depends on an unmerged Oxlint draft built locally, so it is a proof rather than
a product path. [Custom JavaScript plugins](/integrations/custom-js-plugins)
documents it.
