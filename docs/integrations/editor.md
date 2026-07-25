---
title: Editor integration
description: Native formatting, lint diagnostics, and validated quick fixes for TSRX in Visual Studio Code.
---

# Editor integration

Install `oxc-tsrx` in the project and use the released official **OXC**
extension (`oxc.oxc-vscode`). It selects the project-local `oxlint` command
supplied by `oxc-tsrx` by that literal name, not by reading provider metadata.
That name selection is how this path works, and it is what ships.

No TSRX-specific or forked extension is required, and no setup command is
either. Rust owns TSRX parsing, linting, formatting, source mapping, and fix
validation.

## How the official extension gains TSRX

The released official client starts project-local `oxlint --lsp`.
`oxc-tsrx` uses that exact invocation as a narrow multiplexer:

- canonical Oxlint continues to receive ordinary JavaScript and TypeScript;
- the native `oxc-tsrx-lsp` server receives only `.tsrx` document traffic;
- the multiplexer dynamically registers `.tsrx` full-document sync,
  formatting, and `quickfix` capabilities with the existing official client;
- diagnostics from both servers flow back through the same client; and
- both client-request and server-request IDs are isolated, so responses cannot
  cross streams.

The official extension currently hard-codes its document selectors. It exposes no public API;
its activation events still omit `.tsrx`. In a TSRX-only workspace, open any
JavaScript, TypeScript, or JSON file once before opening `.tsrx`; after
activation, the LSP's dynamic registrations apply. OXC's
[Language Plugins RFC](https://github.com/oxc-project/oxc/discussions/21936)
could eventually remove both this activation caveat and the multiplexer.
The [source-backed upstream seam audit](../architecture/upstreaming-to-oxc.md)
separates that proposed runtime contract from OXC's current compile-time
embedding points.

### Name selection and provider discovery are not the same thing

Two different mechanisms meet on this page, and keeping them apart is what keeps
the claims honest:

- **Name selection (released behavior).** The official extension looks for a
  command called `oxlint` in your project and runs it. `oxc-tsrx` is picked up
  because it declares a command with that literal name. Nothing about TSRX is
  declared to the extension.
- **Provider discovery (local reference implementation and proof).**
  `oxc-tsrx` also declares a static `oxc.provider` block in its own
  `package.json`. A host that reads that block learns which file extensions the
  package claims and which binary serves them, without running anything.

No released Oxlint, Oxfmt, Vite+, or `oxc.oxc-vscode` build reads
`oxc.provider`. Nothing has been submitted upstream, and nothing has been
accepted.

What is proven locally is narrower, and worth stating exactly. A real VS Code
session ran the released official extension against a workspace whose only
action was `npm install`, with `node_modules/.bin` deleted, `oxc-tsrx setup`
never run, and every tool name shadowed on `PATH` by a decoy proven to fire.
The extension was given one pointer to the general `oxlint` host, and from
there the discovered provider's declared language server started as a real
process and answered a real quick-fix request. Supplying that pointer is the
part no released build does on its own.

That harsh setup exists to isolate discovery, so do not read it as the install
you get. An ordinary install, with `node_modules/.bin` left alone, `PATH`
untouched, and `oxc-tsrx setup` never run, was measured separately in a real
`oxc.oxc-vscode` 1.59.0 session and served `.tsrx` with no pointer and no extra
command. That measurement is darwin-arm64 only.

Of the four declared capabilities, only the language server has a host. The
parser, lint, and format targets resolve, but nothing runs them through
discovery yet.

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

- full-document synchronization for unsaved `.tsrx` buffers;
- live Oxlint diagnostics mapped to original TSRX locations;
- Oxfmt-backed whole-document formatting;
- `quickfix` actions, only with an exact authored mapping and a clean reparse;
- UTF-8 byte to editor UTF-16 position conversion, including astral Unicode;
- one lint, one fix-enabled lint, and one format session per workspace; and
- opt-in type-aware or type-check diagnostics through TypeScript-Go.

The server does not advertise fix-all, suggestion, or dangerous actions.
Type-aware diagnostics analyze each requested document on its own and may
start one TypeScript-Go process, which the native server does not embed;
[troubleshooting tsgolint discovery](/integrations/configuration#troubleshooting-tsgolint-discovery)
covers the supported version and discovery rules.

Everything runs on the in-memory buffer, and code actions never touch disk.
Malformed input publishes an authored `parse-error` diagnostic instead of
stale lint results, formatting malformed input returns no edit, and a later
valid edit restores normal diagnostics.

## Visual Studio Code setup

1. Install the released extension with ID `oxc.oxc-vscode`.
2. Add `oxc-tsrx` to the project.
3. Open a JS, TS, or JSON file once to activate the current released client.
4. Open `.tsrx`; diagnostics, formatting, and quick fixes now come through the
   official client.

Select the official extension as the default formatter for the language ID
contributed by your framework:

```json
{
  "[markless-tsrx]": {
    "editor.defaultFormatter": "oxc.oxc-vscode",
    "editor.formatOnSave": true
  }
}
```

Use normal `.oxlintrc.json` and `.oxfmtrc.json` files for TSRX settings. The
official extension's `oxc.path.oxlint` setting may explicitly select
`node_modules/.bin/oxlint`, but the extension's normal project-local lookup of
that command name is tested and does not require it. During source development only, `OXC_TSRX_LSP_BIN` points
the harness at a release binary.

## Optional legacy client

`packages/vscode` is the older `thejackshelton.oxc-tsrx-vscode` client. It can
still provide automatic activation in a workspace that contains only `.tsrx`
files and has no ordinary file to activate the official extension. It is not
part of the primary install, and projects must not run both TSRX document
clients at once.

## Run the visible lint demo

The clean Extension Host proof installs untouched local release tarballs in an
empty consumer whose only TSRX dependency is `oxc-tsrx`. It loads the released
official extension with the legacy client absent and proves canonical
TypeScript diagnostics, native TSRX diagnostics, an unsaved buffer update,
formatting, and the validated `no-var` quick fix.

### Two editor stacks, and why they are different

Both start from the same official OXC extension, but only the first is the
product you install today. Keeping them straight avoids a common mix-up.

The current, native path (ships today):

```text
Official OXC extension
  -> project oxlint --lsp multiplexer
     -> canonical Oxlint for JS/TS
     -> native oxc-tsrx-lsp for TSRX   (Rust; runs no JavaScript rules)
```

The source-only, upstream-draft path (a local proof, not a release):

```text
Official OXC extension
  -> source-local custom-parser launcher
     -> draft Node-enabled Oxlint
        -> TSRX parseForESLint adapter
        -> your JavaScript plugin rule
```

The first uses the native Rust server, which cannot run JavaScript lint
plugins. The second is how a *JavaScript* rule can see `.tsrx` at all, but it
depends on an unmerged Oxlint draft built locally, so it is not a product path.

This second stack is the `tsrx-demo(no-tsrx-if)` experiment on an authored
`@if … @else` block. Open `examples/vscode-lints` as its own workspace with the
official OXC extension, open `oxlint-custom-parser.json` once to activate OXC,
then open `LintDemo.tsrx`. Its configured LSP launcher dynamically registers
`.tsrx` and forwards to a Node-enabled Oxlint build from the upstream
custom-parser draft. The source-only setup and upstream status are documented
under [Custom JavaScript plugins](/integrations/custom-js-plugins).

## Reproducible proof

Build the native binary, then run the protocol and package tests, the real
Extension Host walkthrough beside the actual Markless extension, the
installed-VSIX gate, and the benchmark.

There is no separate language-server executable to build. `crates/oxc_tsrx_cli`
produces one binary, `oxc-tsrx`, and the `lsp` subcommand serves an editor.
`npm run build:native` runs that cargo build and then writes the `oxc-tsrx-fmt`
alias that the format benchmark resolves by file name.

```sh
npm run build:native
npm run test:editor
npm run test:editor:official-toolchain
npm run build:editor
npm run test:editor:vscode
npm run test:packaging:vscode
npm run benchmark:editor
```

Retained evidence lives in `tests/editor/markless-vscode-walkthrough.json`
and `tests/packaging/installed-vsix-report.json`, both recording
`externalWrites: false`. On the recorded Apple M5 Pro, open-to-diagnostics
measured 2.40 ms median, with sub-millisecond edit, format, and code-action
p95 and zero memory growth over a 1,000-edit soak.
[Benchmarks](/reference/benchmarks) has every frozen editor budget.
