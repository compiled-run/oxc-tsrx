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
validation, and your own Oxlint JavaScript plugin rules show up as squiggles
too. See [your own JavaScript rules in the editor](#your-own-javascript-rules-in-the-editor).

One thing to get straight before the setup steps: the extension does not wake
up on a `.tsrx` file by itself. [What "a plain install" actually
covers](#what-a-plain-install-actually-covers) explains what that means in a
workspace where a framework extension already owns `.tsrx`.

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

Those dynamic registrations are written as `{ scheme: "file", pattern:
"**/*.tsrx" }`. They match on the file name, not on the language id, so it does
not matter what another extension calls the language. What does matter is
whether the official extension is running at all.

## What "a plain install" actually covers

The official extension exposes no public API, it hard-codes its document selectors,
and its activation events do not include `.tsrx`. Read from
`oxc.oxc-vscode` 1.59.0 itself, `activationEvents` is 21 `onLanguage:` entries
(`astro`, `css`, `graphql`, `handlebars`, `html`, `javascript`,
`javascriptreact`, `json`, `json5`, `jsonc`, `less`, `markdown`, `mdx`, `mjml`,
`scss`, `svelte`, `toml`, `typescript`, `typescriptreact`, `vue`, `yaml`), and
none of them is `tsrx`, `ripple`, or `markless-tsrx`.

That matters most in the workspace you are most likely to have. Ripple's own
extension (`ripple-ts.ripple-ts-vscode-plugin`, read from 2.0.63) contributes
`.tsrx` as the language id `ripple`. So opening a `.tsrx` file there activates
Ripple's extension and not OXC's, and until something else activates OXC's
extension there is no client to route `.tsrx` traffic to. **`.tsrx` diagnostics
therefore do not appear from installing the two packages and opening a `.tsrx`
file.** Open any JavaScript, TypeScript, or JSON file in the workspace once
first. After that the dynamic registrations above apply and `.tsrx` is served
for the rest of the session, whatever language id it was given.

OXC's [Language Plugins RFC](https://github.com/oxc-project/oxc/discussions/21936)
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
command. Read that measurement with the activation step included: the session
opened an ordinary TypeScript file first, waited for the extension to become
active, and only then opened the `.tsrx` file. Nothing in that run made the
extension activate on `.tsrx` itself, and nothing can, because that is fixed in
the extension's manifest. That measurement is darwin-arm64 only.

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
- one lint, one fix-enabled lint, and one format session per workspace;
- your own Oxlint JavaScript plugin rules, when `.oxlintrc.json` declares
  `jsPlugins`; and
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

## Your own JavaScript rules in the editor

If your `.oxlintrc.json` declares `jsPlugins`, those rules run on `.tsrx` in the
editor as well as on the command line, at the same positions. You configure
nothing extra:

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
tutorial; this section is only what is different inside an editor.

### It costs one extra parse per linted file

The native server is a Rust process with no Node.js runtime, so it cannot run
your module itself. What it does instead is build the same legal-TSX
*projection* of your buffer that the built-in rules already use, hand that to a
small Node.js host, and run the published Oxlint binary over it. Every
diagnostic comes back through the byte map that turns projection positions into
positions in the file you wrote.

That means **one extra parse of each `.tsrx` file the server lints**, on open,
on change, and on save. The Node.js host is started once per workspace, and
only when your config declares `jsPlugins`. The server says so once, in its
output log:

```text
oxc-tsrx-lsp: running this project's Oxlint JS plugins on .tsrx by linting each
file's TSX projection; this parses every linted .tsrx file once more. Disable
with "settings": { "oxcTsrx": { "jsPluginsOnTsrx": false } }.
```

To turn the lane off, use the key that message names:

```json
{
  "settings": {
    "oxcTsrx": {
      "jsPluginsOnTsrx": false
    }
  }
}
```

With that set, your plugins keep running on ordinary files, and `.tsrx`
publishes a single `lint-unavailable` diagnostic explaining why there are no
`.tsrx` results, rather than going quiet.

### Two differences worth knowing

**`context.filename` is the mirror path, not yours.** Your rule is handed the
projection, which lives in a throwaway directory and is named `<name>.tsrx.tsx`,
so a `src/View.tsrx` reaches the rule as
`<temporary directory>/src/View.tsrx.tsx`. The path relative to the project is
preserved, so a rule that checks for `src/` still works. The squiggle itself
still lands on `src/View.tsrx`. This is the same known difference the command
line has, and [what your rule sees on
`.tsrx`](/integrations/custom-js-plugins#what-your-rule-sees-on-tsrx) covers the
rest of it, including the fact that `@if` and `@for` reach your rule already
compiled to ordinary `if` and `for`.

**A failing plugin is reported, not hidden.** If the lane cannot start, the
Node.js host dies, or one of your rules throws, the built-in Rust diagnostics
still publish and you additionally get a `js-plugins-unavailable` warning
carrying the reason. Fewer rules running is never silent.

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

Use normal `.oxlintrc.json` and `.oxfmtrc.json` files for TSRX settings. During
source development only, `OXC_TSRX_LSP_BIN` points the harness at a release
binary.

### In a Vite+ project you must set `oxc.path.oxlint`

The official extension finds its linter by looking for `oxlint` in
`node_modules`. In a project that also installs Vite+, that lookup does not
reach this package: `node_modules/.bin/oxlint` is Vite+'s own wrapper, which
execs

```text
node_modules/.pnpm/vite-plus@<version>/node_modules/vite-plus/bin/oxlint
```

and knows nothing about `.tsrx`. `oxc-tsrx setup` fixes *package* resolution, so
`vp lint` works, but it does not own the `.bin` shim the extension reads. The
result is an editor with no `.tsrx` diagnostics at all and no error explaining
why.

Point the extension at this package explicitly:

```json
{
  "oxc.path.oxlint": "node_modules/oxc-tsrx/bin/oxlint"
}
```

Then reload the window. Outside Vite+ the ordinary lookup does find this
package and the setting is unnecessary.

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
     -> native oxc-tsrx-lsp for TSRX   (Rust; built-in rules)
        -> TSX projection of the buffer
           -> published Oxlint binary, in a Node host
              -> your JavaScript plugin rule
```

The source-only, upstream-draft path (a local proof, not a release):

```text
Official OXC extension
  -> source-local custom-parser launcher
     -> draft Node-enabled Oxlint
        -> TSRX parseForESLint adapter
        -> your JavaScript plugin rule
```

The difference is what your rule is handed. The first stack ships, needs no
build, and gives your rule the TSX projection, where `@if` and `@for` have
already become `if` and `for`. The second gives your rule the authored TSRX
tree, with `JSXIfExpression` and `JSXForExpression` intact, but it depends on an
unmerged Oxlint draft built locally, so it is not a product path.

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
`pnpm run build:native` runs that cargo build and then writes the `oxc-tsrx-fmt`
alias that the format benchmark resolves by file name.

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

Retained evidence lives in `tests/editor/markless-vscode-walkthrough.json`
and `tests/packaging/installed-vsix-report.json`, both recording
`externalWrites: false`. On the recorded Apple M5 Pro, open-to-diagnostics
measured 2.40 ms median, with sub-millisecond edit, format, and code-action
p95 and zero memory growth over a 1,000-edit soak.
[Benchmarks](/reference/benchmarks) has every frozen editor budget.
