# OXC for TSRX for Visual Studio Code

This companion extension connects `.tsrx` documents to the native
`oxc-tsrx-lsp` server. It provides Oxfmt-backed format-on-save, Oxlint-backed
live diagnostics, and only source-mapped, validation-passed safe quick fixes.

It is additive to framework language extensions. In Markless projects it
attaches to the existing `markless-tsrx` language and leaves Markless's
TypeScript plugins, completions, navigation, and runtime compilation alone.

During source development, set `OXC_TSRX_LSP_BIN` to the absolute release
binary. Published platform packages will be discovered automatically. The
extension never runs in an untrusted workspace.

Type and config-path changes refresh the native workspace tool. Changes to the
enable switch or server executable path require an extension-host reload.
