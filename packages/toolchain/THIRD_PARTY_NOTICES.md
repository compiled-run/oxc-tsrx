# Third-party notices

`oxc-tsrx` is the complete OXC-shaped TSRX parser, linter, formatter, and
language-server host. It uses canonical OXC Rust crates from commit
`8e0ed2ebb96137fb1611cdbd5742d5cb46037d40` of
<https://github.com/oxc-project/oxc>. OXC is licensed under the MIT License.

OXC is consumed as an exact-revision Cargo dependency. Its source is not
copied, vendored, patched, or forked by this project. Target-native packages
contain the complete generated Rust dependency inventory and byte-exact
upstream legal files, and each `@oxc-tsrx/native-*` package carries the notices
for the linked Rust/OXC binary distribution it ships.

This package's npm dependencies are:

- `@oxc-project/types` 0.140.0, MIT licensed and maintained by the OXC project,
  for the parser declaration surface.
- `oxlint` 1.74.0, which ordinary linting delegates to unchanged.
- `oxfmt` 0.59.0, which ordinary formatting delegates to unchanged.
- `oxlint-tsgolint` 0.24.0, for type-aware linting.
- `tinyglobby` 0.2.17, MIT licensed, for file discovery.

Those packages retain their own license and notice files when installed.

OXC for TSRX is an independent community integration. It is not an official
OXC project and is not affiliated with or endorsed by VoidZero Inc. or OXC's
contributors.
