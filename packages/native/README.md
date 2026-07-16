# OXC for TSRX native binaries

This platform package contains the Rust-native `oxc-tsrx`, `oxc-tsrx-fmt`,
and `oxc-tsrx-lsp` executables used by `oxlint-tsrx`, `oxfmt-tsrx`, and the
OXC for TSRX editor integration.

It is selected automatically by `@oxc-tsrx/runtime`. Do not install it by
hand unless a package manager has omitted optional dependencies. The package
has no install script and does not download or compile code after installation.

`checksums.json` records every executable's SHA-256 digest, byte size, Rust
target, package version, and exact canonical OXC revision.
