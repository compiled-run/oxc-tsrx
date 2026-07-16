# Third-party notices

The bundled Visual Studio Code client includes the following packages:

- `vscode-languageclient` 10.1.0, `vscode-jsonrpc` 9.0.1,
  `vscode-languageserver-protocol` 3.18.2,
  `vscode-languageserver-textdocument` 1.0.13, and
  `vscode-languageserver-types` 3.18.0: MIT, Microsoft Corporation.
- `balanced-match` 4.0.4, `brace-expansion` 5.0.7, `fdir` 6.5.0,
  `picomatch` 4.0.5, and `tinyglobby` 0.2.17: MIT.
- `minimatch` 10.2.5: BlueOak-1.0.0.
- `semver` 7.8.5: ISC.

The package list above is also derived mechanically from every
`node_modules` module region in the generated Rolldown bundle. Exact locked
versions, npm integrity values, license-text SHA-256 values, and the actual
byte-exact license/copyright texts (including BlueOak, ISC, and the Microsoft
notices) are shipped in `licenses/`. The deterministic inventory gate fails if
a bundle region is not covered.

The platform VSIX also embeds `oxc-tsrx-lsp`. Its `dist/native` directory
contains the project license, native third-party notice, exact OXC revision,
target identity, binary checksum, the byte-exact canonical OXC legal files,
and the generated locked Rust dependency license inventory. The native
implementation consumes canonical OXC under the MIT License without copying,
patching, or forking OXC source in this repository.

OXC for TSRX is an independent community integration. It is not an official
OXC project and is not affiliated with or endorsed by VoidZero Inc. or the OXC
contributors.
