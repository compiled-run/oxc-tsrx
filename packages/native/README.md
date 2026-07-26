# OXC for TSRX native binaries

This is a platform-specific implementation package for
[`oxc-tsrx`](https://www.npmjs.com/package/oxc-tsrx). There are eight of them,
one per supported target, and `oxc-tsrx` lists all eight as
`optionalDependencies`. Your package manager installs only the one matching
your operating system, CPU, and C library, so a normal install downloads one
prebuilt binary rather than eight.

You should not add this package to your own `package.json`. Depend on
`oxc-tsrx` and let resolution pick the platform package for you. Install it by
hand only if a package manager has been configured to skip optional
dependencies. The package has no install script and does not download or
compile anything after installation.

## What is inside

`bin/oxc-tsrx` is one Rust-native executable that carries the linter, the
formatter, and the language server together. It selects a tool from a leading
subcommand: none or `lint` to lint, `fmt` to format, `lsp` to serve an editor.
It also dispatches on the name it was invoked under, so a copy or link named
`oxc-tsrx-fmt` or `oxc-tsrx-lsp` runs that tool directly.

One executable rather than three matters for download size. Three separate
binaries linked the same OXC engines three times; the merged one is a little
over half the bytes.

Schema-2 releases also contain `parser.node`, the canonical Node-API parser
addon. `oxc-tsrx` loads it for its `oxc-tsrx/parser` export.

## Verifying what you got

`checksums.json` records, for every executable and addon in the package, its
SHA-256 digest, byte size, object identity, Rust target, package version,
API/ABI role, Node-API version, capabilities, and the exact canonical OXC
revision it was built against.
