---
title: Editor integration
description: Native formatting, lint diagnostics, and validated quick fixes for TSRX in Visual Studio Code.
---

# Editor integration

OXC for TSRX ships one native language server, `oxc-tsrx-lsp`, plus a thin
Visual Studio Code companion in `packages/vscode` that only manages the
native process and forwards workspace settings. Rust owns parsing, linting,
formatting, source mapping, and fix validation. Marketplace availability is
not claimed until an approval-gated publication.

## Why a companion is required

The official `oxc.oxc-vscode` client hard-codes its document selectors,
which exclude `.tsrx`, and exposes no public API to add one. The companion
fills that gap:

- It attaches to file-backed `**/*.tsrx` documents, launches the Rust
  server, and forwards settings. Activation recognizes `markless-tsrx`,
  `ripple`, and `tsrx` documents, plus workspaces containing `.tsrx` files.
- It registers no competing `.tsrx` language, so your framework extension
  keeps its language ID, grammar, completions, and compilation, and the
  official OXC extension keeps ordinary JS/TS.
- The `oxlint-tsrx` and `oxfmt-tsrx` npm wrappers pass `--lsp` through
  unchanged, so aliasing `oxlint` and `oxfmt` to them keeps the official
  extension working for JS/TS.

OXC's [Language Plugins RFC](https://github.com/oxc-project/oxc/discussions/21936)
may eventually let the official client handle `.tsrx`, but nothing released
does today (last audited 2026-07-16), so capability-probe the released tools
before each release. Maintainers can find the pinned source revision in the
[upstream transplant map](../architecture/upstreaming-to-oxc.md).

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
start one TypeScript-Go process, which the VS Code package does not bundle;
[troubleshooting tsgolint discovery](/integrations/configuration#troubleshooting-tsgolint-discovery)
covers the supported version and discovery rules.

Everything runs on the in-memory buffer, and code actions never touch disk.
Malformed input publishes an authored `parse-error` diagnostic instead of
stale lint results, formatting malformed input returns no edit, and a later
valid edit restores normal diagnostics.

## Visual Studio Code settings

The extension ID is `thejackshelton.oxc-tsrx-vscode`. Select it as the default
formatter for the language ID contributed by your framework:

```json
{
  "[markless-tsrx]": {
    "editor.defaultFormatter": "thejackshelton.oxc-tsrx-vscode",
    "editor.formatOnSave": true
  }
}
```

| Setting | Default | Meaning |
| --- | --- | --- |
| `oxcTsrx.enable` | `true` | Start OXC for TSRX in trusted workspaces. |
| `oxcTsrx.server.path` | empty | Absolute trusted path to `oxc-tsrx-lsp`; empty uses the installed platform package. |
| `oxcTsrx.typeAware` | `false` | Opt into TypeScript-Go lint diagnostics. |
| `oxcTsrx.typeCheck` | `false` | Also publish TypeScript syntactic and semantic diagnostics; implies type-aware linting. |
| `oxcTsrx.lint.configPath` | empty | Optional Oxlint JSON/JSONC config path relative to each workspace root. |
| `oxcTsrx.format.configPath` | empty | Optional Oxfmt JSON/JSONC config path relative to each workspace root. |

The extension refuses untrusted workspaces, and a custom server path must be
absolute. Type and config-path changes rebuild workspace tools live; changing
`enable` or the server path requires reloading the extension host. During
source development, `OXC_TSRX_LSP_BIN` points the test harness at the
release binary.

## Run the visible lint demo

The repository contains an intentionally lint-broken workspace at
`examples/vscode-lints`. Open the repository in Visual Studio Code, choose
**Run and Debug → TSRX: lint demo**, and press **F5**. The launch task checks
the two local servers, rebuilds the extension, opens `LintDemo.tsrx`, and
shows five native authored-span diagnostics plus the validated `no-var` quick
fix.

It also shows `tsrx-demo(no-tsrx-if)` on the authored `@if … @else` block.
That rule is available without the companion: open `examples/vscode-lints`
as its own workspace with the official OXC extension, open
`oxlint-custom-parser.json` once to activate OXC, then open `LintDemo.tsrx`.
Its configured LSP launcher dynamically registers `.tsrx` and forwards to a
Node-enabled Oxlint build from the upstream custom-parser draft. The
source-only setup and upstream status are documented under [Custom JavaScript
plugins](/integrations/custom-js-plugins).

## Reproducible proof

Build the server, then run the protocol and package tests, the real
Extension Host walkthrough beside the actual Markless extension, the
installed-VSIX gate, and the benchmark:

```sh
cargo build --release --locked -p oxc_tsrx_cli --bin oxc-tsrx-lsp
npm run build:editor
npm run test:editor
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
