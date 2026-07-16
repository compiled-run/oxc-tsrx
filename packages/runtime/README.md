# `@oxc-tsrx/runtime`

Internal runtime shared by `oxlint-tsrx`, `oxfmt-tsrx`, and the OXC for TSRX
editor companion. It discovers the exact platform-native package, runs native
processes, discovers `.tsrx` inputs, and materializes serializable Vite+
configuration outside the Rust hot path.

Most users should install `oxlint-tsrx` and `oxfmt-tsrx`, not this package
directly. The matching `@oxc-tsrx/native-*` package is selected through exact
optional dependencies. Missing, mismatched, or incomplete native packages fail
without delegating `.tsrx` to stock tools.

The runtime contains no TSRX parser, AST, formatter, or lint engine.
