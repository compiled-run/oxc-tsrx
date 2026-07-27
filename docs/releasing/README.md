# OXC for TSRX release runbook

This runbook prepares and verifies a release. It does not grant authority to
publish one. The repository's release-candidate workflow has read-only GitHub
permissions and only uploads a temporary Actions artifact. It contains no
`npm publish`, `vsce publish`, deployment, tag, push, or announcement step.

Publishing itself lives in a separate, separately gated workflow,
`.github/workflows/publish.yml`, and is documented in
[the publish runbook](publish-runbook.md).

The releaser must also read:

- [platform and ABI policy](platform-abi-policy.md);
- [OXC and ecosystem upgrade policy](upgrades.md);
- [external account prerequisites](external-prerequisites.md); and
- [the publish runbook](publish-runbook.md).

## Release notes

One file per released version, written before the candidate is built and
reviewed as part of it:

- [0.1.2](v0.1.2.md) — JavaScript lint plugins run on `.tsrx` in the CLI and the
  editor, and a `jsPlugins` config no longer silences every editor diagnostic.
- [0.1.1](v0.1.1.md) — `oxc-tsrx/parser` loads, because the native packages ship
  `parser.node` again.
- [0.1.0](v0.1.0.md) — first release.

## Release identities

One version is released as a unit:

- eight `@oxc-tsrx/native-*` platform packages;
- the public `oxc-tsrx` toolchain; and
- eight target-specific VSIX files for
  `thejackshelton.oxc-tsrx-vscode`.

That is nine npm packages. `@oxc-tsrx/runtime`, `@oxc-tsrx/parser`,
`oxlint-tsrx`, and `oxfmt-tsrx` were folded into `oxc-tsrx` on 2026-07-25 and no
longer exist. `docs/releasing/v0.1.0-launch.json` holds the authoritative list
and the publish order.

Every native artifact must report the same project version and canonical OXC
revision. A partial set is not a supported release.

## 1. Freeze the candidate

Use an exact commit, not a moving branch tip. Confirm that the root, public
`oxc-tsrx` toolchain, optional legacy editor extension, and generated native
manifests all carry the intended version. Confirm that the
adapter still pins all twelve direct OXC dependencies to one full canonical Git
commit and that their OXC workspace lock closure has no second source or
revision.

Run the source gates from a fresh checkout with no native-binary override:

```bash
pnpm install --frozen-lockfile --ignore-scripts
pnpm run licenses:check
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --all-targets --locked
cargo build --release --locked -p oxc_tsrx_cli --bins
pnpm test
pnpm run test:editor:official-toolchain
pnpm run test:packaging:unit
node --test tests/packaging/clean-install.test.mjs
node --test tests/packaging/vite-plus-matrix.test.mjs
```

The required CI checks are `.github/workflows/ci.yml`. They cover the two Node
engine floors, a maintained Node/Rust pair, the locked Rust graph, legal-file
freshness, package contents, an untouched-tarball install, and the supported
Vite+ 0.1.24/0.2.4 lanes. The scheduled Vite+ 0.1.20 and OXC-main probes are
advisory; their result cannot replace a required gate.

Run the frozen performance gates on the documented stable benchmark machine:

```bash
node tests/acceptance/run-performance.mjs
```

Do not use shared GitHub-hosted runner timing as a substitute for the retained
same-machine performance reports. The aggregate command owns fresh-report
admission, identity checks, near-threshold reruns, assertion voting, and
representative selection. Individual family commands are diagnostic raw runs;
they do not produce a release decision by themselves.

## 2. Prove the owner workflow

Before packaging, the clean-room acceptance run must prove all of these against
the exact candidate:

- mixed `.tsrx` and ordinary JS/JSX/TS/TSX lint/format/check/fix behavior;
- exact authored diagnostic spans and validation-passed fixes;
- type-aware opt-in with the supported `oxlint-tsgolint` version;
- Vite build/dev/HMR plus literal Vite+ minimum/current build, dev retransform,
  lint, format, and check commands;
- a clean one-package consumer using the released official OXC extension, with
  canonical ordinary diagnostics and native TSRX diagnostics/edits;
- an optional installed legacy VSIX using its embedded server with no
  source-tree binary override;
- format-on-save, live diagnostics, malformed-buffer recovery, and a safe code
  action on a disposable copy of a representative Markless file;
- no change to the external Markless worktree fingerprint; and
- every correctness and performance budget.

Markless is an oracle, not a release destination. Never write to it.

## 3. Build the candidate once

After the exact commit is present on GitHub, manually dispatch **Build release
candidate** on that ref. The workflow builds on matching x64/arm64 hosts, emits
all eight native npm packages and all eight target VSIX files, then adds:

- the one platform-independent npm package, `oxc-tsrx`;
- `SHA256SUMS`;
- JavaScript and Rust CycloneDX SBOMs;
- the exact legal texts and locked Rust and VS Code bundle dependency
  inventories; and
- `provenance.unsigned.intoto.json`.

The provenance file is intentionally an unsigned staging statement. It binds
the subjects to the source SHA and workflow run for review, but it is not a
cryptographic attestation. npm provenance is created only by an approved npm
publish from a public repository through supported GitHub OIDC, which is what
`.github/workflows/publish.yml` does. A GitHub artifact attestation, if later
desired, is a separate external write and needs explicit approval before adding
`attestations: write`.

The npm SBOM is generated from a disposable meta-package whose direct inputs
are the nine candidate tarballs. npm's `--force` flag is used only while creating
that package lock so one Linux runner can describe mutually exclusive
macOS/Linux/Windows `os`/`cpu` packages together. Lifecycle scripts are disabled
and no candidate binary is executed in this SBOM step. The workflow then checks
that every public, implementation, and platform package is present in the
result.

Download the single assembled artifact without rebuilding anything:

```bash
gh run download RUN_ID \
  --name release-candidate-COMMIT_SHA \
  --dir candidate
cd candidate
sha256sum --check SHA256SUMS
```

On macOS, use `shasum -a 256 -c SHA256SUMS` if GNU `sha256sum` is unavailable.
Inspect every npm tarball with `npm pack --dry-run` at source and `tar -tf` on
the candidate. Inspect every VSIX as a ZIP. There must be nine npm tarballs and
eight VSIX files; a VSIX contains only the target LSP binary, while each native
npm package contains the one multi-call executable that serves lint, format, and
the language server.

Do not rebuild after review. A changed source SHA, generated file, checksum,
version, lockfile, or release manifest creates a new candidate and restarts the
gates.

## 4. Approval gates

Preparing files is not publishing. The following are independent irreversible
actions, each requiring an exact approval that names the version, source SHA,
and candidate workflow run:

1. npm registry publication;
2. VS Code Marketplace publication;
3. a Git tag, GitHub release, or repository push not already authorized;
4. website deployment; and
5. a social announcement.

Acceptable npm approval wording is:

> Approve npm publication of VERSION from COMMIT_SHA and candidate run RUN_ID.

Acceptable Marketplace approval wording is:

> Approve VS Code Marketplace publication of VERSION from COMMIT_SHA and
> candidate run RUN_ID.

Do not infer one approval from another. Website and announcement work belongs
to the launch tranche and is not covered by this runbook.

## 5. Registry publication

`.github/workflows/publish.yml` publishes the nine npm packages. It downloads
the already-reviewed candidate by run ID and commit SHA, re-checks
`SHA256SUMS`, and never rebuilds. [The publish runbook](publish-runbook.md) is
the operator procedure; read it in full before dispatching anything.

The short version:

- Authentication is npm trusted publishing over GitHub Actions OIDC. There is no
  `NPM_TOKEN` in this repository. The publish job carries
  `permissions: { id-token: write }`, and npm generates provenance automatically
  for a trusted publish from a public repository.
- The workflow runs only on `workflow_dispatch`, defaults to `mode: dry-run`,
  and refuses to publish unless the operator types `PUBLISH <version>`.
- The publish order is read from `docs/releasing/v0.1.0-launch.json`: the eight
  `@oxc-tsrx/native-*` packages first, then `oxc-tsrx`. Never publish
  `oxc-tsrx` before the platform packages it lists in `optionalDependencies`,
  and never promote it to `latest` before they are all present at the same
  version.

One thing the workflow cannot do for itself: npm attaches a trusted publisher to
a package that already exists, so the first version of each new name must be
published from the owner's machine with interactive authentication. Steps 1 and
2 of the publish runbook are that one-time bootstrap. Treat every one of them as
an irreversible registry write needing the same approval as the release itself.

For later releases, consider npm's stage-only trusted publishing. The CI
identity runs `npm stage publish` for each reviewed tarball, and a human reviews
the staged bytes and runs `npm stage approve STAGE_ID` with 2FA. Configure the
trust relationship to allow staging but not direct publishing. See npm's
[trusted publishing](https://docs.npmjs.com/trusted-publishers/) and
[staged publishing](https://docs.npmjs.com/staged-publishing/) documentation.

Marketplace publication likewise uses the eight already-built VSIX files and
`vsce publish --packagePath PATH`. Do not publish a generic VSIX or rebuild the
extension between targets. Follow the official
[platform-specific extension](https://code.visualstudio.com/api/working-with-extensions/publishing-extension)
procedure and confirm all target variants appear under the one extension ID.

## 6. Post-publication verification

From empty directories on every available OS/CPU/libc family:

- install exact registry versions with no source-tree overrides;
- verify `oxc-tsrx --version`, `oxc-tsrx fmt --version`, and `oxc-tsrx lsp --version`;
- repeat the mixed lint/format/Vite+ smoke tests;
- install `oxc-tsrx` with the released official OXC extension and repeat the
  multiplexed ordinary/TSRX editor proof; optionally validate the matching
  legacy Marketplace VSIX separately; and
- compare downloaded artifacts to the approved candidate checksums.

If any member of the version set is missing, mismatched, corrupted, or selects
the wrong ABI, do not paper over it with a fallback. Stop promotion, document
the registry state, and prepare a new patch version. npm versions are
immutable; never overwrite or silently substitute release bytes.
