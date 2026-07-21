# OXC for TSRX native binaries

This platform package contains the Rust-native `oxc-tsrx`, `oxc-tsrx-fmt`,
and `oxc-tsrx-lsp` executables used by `oxlint-tsrx`, `oxfmt-tsrx`, and the
OXC for TSRX editor integration. Schema-2 releases also contain the canonical
`parser.node` addon used by `@oxc-tsrx/parser`.

It is selected automatically by `@oxc-tsrx/runtime`. Do not install it by
hand unless a package manager has omitted optional dependencies. The package
has no install script and does not download or compile code after installation.

`checksums.json` records every executable and addon's SHA-256 digest, byte size,
object identity, Rust target, package version, API/ABI role, Node-API version,
capabilities, and exact canonical OXC revision.
