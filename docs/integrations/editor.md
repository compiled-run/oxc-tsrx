---
title: Editor integration
description: Native formatting, lint diagnostics, and validated quick fixes for TSRX in Visual Studio Code.
---

# Editor integration

OXC for TSRX ships one native language server, `oxc-tsrx-lsp`, and a thin
Visual Studio Code companion in `packages/vscode`. Rust owns parsing, linting,
formatting, source mapping, and fix validation. The extension owns only native
binary discovery, process lifecycle, workspace settings, and the standard
Language Client transport.

This is a companion integration, not a fork of the official OXC extension or a
framework language extension. Host-targeted VSIX production, installation, and
embedded-binary discovery are proven locally. Static manifests and the hosted
workflow cover eight targets; producing and validating all eight candidates is
a post-push gate. Registry and Marketplace availability are not claimed before
an approval-gated publication.

## Why a companion is still required

The official `oxc.oxc-vscode` 1.58.0 client has configurable Oxlint/Oxfmt
binary paths, but its language-client document selectors are hard-coded and do
not include `.tsrx` or framework language IDs such as `markless-tsrx`. Replacing
the binary does not extend those selectors, and the extension exposes no public
client API for another package to add one. Associating `.tsrx` with
`typescriptreact` would displace the framework language extension and still
would not satisfy the official linter's filename selector. Vite+ has no editor
language-client hook that changes this boundary.

The companion is consequently limited to the missing adapter: 104 lines of
authored client source select file-backed `**/*.tsrx`, launch the Rust server,
and forward workspace settings. It contributes no language ID, grammar,
completion, navigation, parser, or framework compiler. The official OXC
extension continues to own ordinary JS/TS, and each framework extension
continues to own TSRX language semantics. If the official client later exposes
a configurable selector and compatible custom-server contract, this companion
can be retired without changing the Rust core or npm command packages.

## Native server boundary

The server runs canonical OXC's public language-server transport from exact
commit `8e0ed2ebb96137fb1611cdbd5742d5cb46037d40`. All revision-specific OXC and
LSP types stay private to `crates/oxc_adapter`; the TSRX layer implements a
small project-owned interface expressed in authored UTF-8 byte ranges.

Canonical OXC does expose a public compile-time Rust embedding seam through
`ToolBuilder` and `Tool`, and OXC for TSRX already uses that seam. It is not a
runtime-configurable parser or tool loader: consuming it still means compiling
the project-owned `oxc-tsrx-lsp` binary. Stock `oxlint --lsp`, `oxfmt --lsp`,
and the official editor client cannot discover or inject that compiled tool
from configuration.

### Upstream migration watch

The upstream situation was last audited on 2026-07-16. OXC's
[Language Plugins RFC](https://github.com/oxc-project/oxc/discussions/21936)
is the strongest long-term Oxlint contract for TSRX because it proposes an
authored language AST, virtual TS/TSX, mappings, typed tooling, and cacheable
parse/load phases. Its
[implementation plan](https://github.com/oxc-project/oxc/issues/23207) is not a
release dependency here; the Phase 1
[API/configuration pull request](https://github.com/oxc-project/oxc/pull/24597)
does not yet provide the parse/load/transform runtime.

The separate draft
[`languageOptions.parser` pull request](https://github.com/oxc-project/oxc/pull/24262)
is an optional future Oxlint compatibility route, but its proposed JS parser,
dynamic traversal, and shadow-source pass do not replace the native TSRX hot
path, type-aware lane, formatter, or editor selection. The open
[Oxfmt plugin pull request](https://github.com/oxc-project/oxc/pull/20250)
routes new extensions through external Prettier plugins rather than providing
a native TSRX formatter hook. None of these changes currently makes the
official VS Code selectors accept `.tsrx`.

Before each release, capability-probe the released tools and official client
instead of trusting this dated snapshot. Retire the companion only after a
released route covers lint, format, and editor selection without weakening the
native performance and fail-closed mapping contracts.

One long-lived native process provides:

- full-document synchronization for unsaved `.tsrx` buffers;
- live Oxlint diagnostics mapped to original TSRX locations;
- Oxfmt-backed whole-document formatting;
- `quickfix` actions only when every edit has an exact authored-source mapping
  and the edited TSRX reparses successfully;
- UTF-8 byte to editor UTF-16 position conversion, including astral Unicode;
- one diagnostic lint session, one fix-enabled lint session, and one format
  session per workspace; and
- optional type-aware or type-check diagnostics through the supported
  TypeScript-Go boundary.

The server does not advertise fix-all, suggestion, or dangerous actions.
Type-aware editor diagnostics are opt-in and currently analyze each requested
authored document; cross-document unsaved project semantics are not claimed.
They require the exact supported `oxlint-tsgolint` 0.24.0 executable to be
resolvable from the workspace installation, `PATH`, or the documented native
environment override. One configured request may start one TypeScript-Go
process. The executable is not bundled into the current VS Code package.

## Framework coexistence

The companion does not register a competing `.tsrx` language. It attaches to
file-backed `**/*.tsrx` documents and therefore coexists with the language ID
owned by the installed framework extension. Its activation events recognize
`markless-tsrx`, `ripple`, and `tsrx`, plus workspaces containing `.tsrx`
files.

In a Markless workspace this leaves Markless responsible for syntax grammar,
TypeScript plugins, completions, navigation, and runtime compilation. OXC for
TSRX adds native formatting, lint diagnostics, and safe code actions to the
same document.

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

Available settings are:

| Setting | Default | Meaning |
| --- | --- | --- |
| `oxcTsrx.enable` | `true` | Start OXC for TSRX in trusted workspaces. |
| `oxcTsrx.server.path` | empty | Absolute trusted path to `oxc-tsrx-lsp`; empty uses the installed platform package. |
| `oxcTsrx.typeAware` | `false` | Opt into TypeScript-Go lint diagnostics. |
| `oxcTsrx.typeCheck` | `false` | Also publish TypeScript syntactic and semantic diagnostics; implies type-aware linting. |
| `oxcTsrx.lint.configPath` | empty | Optional Oxlint JSON/JSONC config path relative to each workspace root. |
| `oxcTsrx.format.configPath` | empty | Optional Oxfmt JSON/JSONC config path relative to each workspace root. |

The extension is a workspace extension and refuses untrusted workspaces. A
custom server path must be absolute. During source development,
`OXC_TSRX_LSP_BIN` can select the release binary for the test harness.
Changes to type and config-path settings rebuild the affected workspace tool
and refresh open-document diagnostics. Changing `enable` or the native server
path requires reloading the extension host because those settings control the
process itself.

## Failure safety

All editor requests operate on the in-memory buffer; code actions do not use a
disk-writing CLI path. Each safe Oxlint edit is mapped back through identity
segments, applied to a candidate source, and validated by reparsing before it
is exposed to the editor.

An incomplete or malformed edit publishes an authored `parse-error` diagnostic
instead of retaining stale lint results. Formatting malformed input fails the
request and returns no edit. A later valid edit restores normal diagnostics.

## Reproducible proof

Build and run the protocol/package tests with:

```sh
cargo build --release --locked -p oxc_tsrx_cli --bin oxc-tsrx-lsp
npm run build:editor
npm run test:editor
```

The retained protocol matrix exercises initialization, capabilities, live
edits, authored diagnostics, UTF-16 ranges after a four-byte emoji, formatting,
safe code actions, live config-path rebuilding, malformed-source recovery,
type-aware opt-in, shutdown, and VSIX contents. The VSIX test validates the
archive and bundle; the real host
walkthrough below loads the source extension through VS Code's development
extension path. The separate installed-artifact gate packages a target-specific
VSIX, installs it into an isolated VS Code profile, clears the native binary
override, and launches its embedded `oxc-tsrx-lsp`.

The real Extension Host walkthrough runs the companion beside the actual
Markless extension against a disposable copy of a provenance-recorded Markless
fixture:

```sh
npm run test:editor:vscode
npm run test:packaging:vscode
```

It directly observes automatic activation, exact authored diagnostics, a live
workspace config-path change, a real format-on-save, one safe code action, and
updated diagnostics. The retained artifact is
`tests/editor/markless-vscode-walkthrough.json`; the installed-VSIX evidence is
`tests/packaging/installed-vsix-report.json`. Both before/after external-
worktree fingerprints are identical and record `externalWrites: false`.

The latest clean-room direct native-LSP report is
`benchmarks/editor/results-1784242073843.json`. On the recorded Apple M5 Pro,
the retained Markless fixture plus disposable probes (1,300 bytes) measured
2.49 ms median / 2.84 ms p95 across 100 fresh server
start/initialize/open-to-diagnostics samples. Edit-to-diagnostics, formatting,
and code-action p95 were 0.124, 0.378, and 0.195 ms. One long-lived server used
11.14 MiB RSS and grew 0 MiB through a 1,000-edit soak. These are syntax-only
local stdio server round trips, not type-aware or VS Code rendering
measurements, and every frozen editor budget passed.

```sh
npm run benchmark:editor
```

The host-native npm package, clean installed-binary discovery, and static
eight-target release contracts pass their local gates. Hosted production and
execution of every target remain post-push checks. This documentation still
does not claim registry or Marketplace availability.
