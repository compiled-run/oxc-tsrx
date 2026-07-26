# VS Code bundle license texts

`bundle-dependencies.json` is the machine-readable inventory of every npm
package present in Rolldown's generated extension bundle.
`BUNDLE_DEPENDENCIES.md` is the matching human report. `texts/` contains the
actual byte-exact license and copyright notices read from each locked package.

The generator derives coverage from `//#region node_modules/...` markers in
`dist/extension.bundle.cjs`, so adding a bundled package cannot silently omit
its license. Regenerate with `pnpm run licenses:vscode:generate` and verify
without mutation with `pnpm run licenses:vscode:check`.
