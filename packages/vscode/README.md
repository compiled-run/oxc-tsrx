# OXC for TSRX for Visual Studio Code

This is the optional legacy `.tsrx` client. The primary editor workflow needs
only the released official OXC extension (`oxc.oxc-vscode`) and a project-local
`oxc-tsrx` package: its public `oxlint --lsp` command dynamically registers
TSRX diagnostics, formatting, and quick fixes while keeping ordinary JS/TS on
canonical Oxlint.

The legacy client remains available for TSRX-only workspaces that need
automatic activation without first opening a JS, TS, or JSON file. It provides
Oxfmt-backed format-on-save, Oxlint-backed live diagnostics, and only
source-mapped, validation-passed safe quick fixes.

## How it finds a language server

The client is provider-driven, not hard-coded to one language. For each
workspace folder it reads that folder's `package.json`, resolves
`<name>/package.json` for each direct dependency, and reads the static
`oxc.provider` block those manifests declare. Every provider that declares an
`lsp` capability becomes one language client, whose document selector and
executable come from the index.

- Discovery runs **per workspace folder**, and two folders' indexes are never
  merged.
- A client is created only when a document whose extension that folder's index
  owns is actually opened. A session of ordinary `.ts`, `.tsx`, and `.js` files
  starts no provider process at all; those files stay on the official OXC path.
- The executable is a key of the provider's own `bin` map, resolved inside the
  provider package. There is no `node_modules/.bin` lookup, no `PATH` lookup, no
  setup command, and nothing is written under `node_modules`.
- A workspace folder that ships a Yarn Plug'n'Play manifest has its
  `resolveRequest` used as the resolver.

Declaring `oxc-tsrx` as a project dependency is therefore the whole
installation step for TSRX support.

If a folder's discovery finds no provider language server, the client falls back
to the compatibility chain `oxcTsrx.server.path` → `OXC_TSRX_LSP_BIN` → the
binary bundled in a platform VSIX → `oxc-tsrx`'s own platform-package
resolution, and serves `.tsrx` only. That chain is compatibility-only and is just as lazy: it starts nothing
until a `.tsrx` document is opened.

It is additive to framework language extensions. In Markless projects it
attaches to the existing `markless-tsrx` language and leaves Markless's
TypeScript plugins, completions, navigation, and runtime compilation alone.

Do not enable the legacy client for `.tsrx` at the same time as the public
toolchain's official-extension integration; choose one TSRX document client.
With both installed, a `.tsrx` buffer has two formatting providers, because the
toolchain registers `textDocument/formatting` dynamically on the `oxlint --lsp`
connection while this client registers its own. VS Code then needs
`editor.defaultFormatter` on that language id to know which one to use, and
format-on-save is ambiguous until it is set.

During source development, set `OXC_TSRX_LSP_BIN` to the absolute release
binary, `target/release/oxc-tsrx`. That one executable carries the linter, the
formatter, and the language server, and the client starts it with the `lsp`
subcommand. Published platform packages will be discovered automatically. The
extension never runs in an untrusted workspace.

Type and config-path changes refresh the native workspace tool. Changes to the
enable switch or server executable path require an extension-host reload.
