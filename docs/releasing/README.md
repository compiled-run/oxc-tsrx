# OXC for TSRX release runbook

This runbook prepares and verifies a release. It does not grant authority to
publish one. The repository's release-candidate workflow has read-only GitHub
permissions and only uploads a temporary Actions artifact. It contains no
`npm publish`, `vsce publish`, deployment, tag, push, or announcement step.

The releaser must also read:

- [platform and ABI policy](platform-abi-policy.md);
- [OXC and ecosystem upgrade policy](upgrades.md); and
- [external account prerequisites](external-prerequisites.md).

## Release identities

One version is released as a unit:

- `@oxc-tsrx/runtime`;
- eight `@oxc-tsrx/native-*` platform packages;
- `oxlint-tsrx`;
- `oxfmt-tsrx`; and
- eight target-specific VSIX files for
  `thejackshelton.oxc-tsrx-vscode`.

Every native artifact must report the same project version and canonical OXC
revision. A partial set is not a supported release.

## 1. Freeze the candidate

Use an exact commit, not a moving branch tip. Confirm that the root, runtime,
lint companion, format companion, editor extension, and generated native
manifests all carry the intended version. Confirm that the adapter still pins
all twelve direct OXC dependencies to one full canonical Git commit and that
their OXC workspace lock closure has no second source or revision.

Run the source gates from a fresh checkout with no native-binary override:

```bash
npm ci --ignore-scripts
npm run licenses:check
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --all-targets --locked
cargo build --release --locked -p oxc_tsrx_cli --bins
npm test
npm run test:packaging:unit
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
- an installed VSIX using its embedded server with no source-tree binary
  override;
- format-on-save, live diagnostics, malformed-buffer recovery, and a safe code
  action on a disposable copy of a representative Markless file;
- no change to the external Markless worktree fingerprint; and
- every correctness and performance budget.

Markless is an oracle, not a release destination. Never write to it.

## 3. Build the candidate once

After the exact commit is present on GitHub, manually dispatch **Build release
candidate** on that ref. The workflow builds on matching x64/arm64 hosts, emits
all eight native npm packages and all eight target VSIX files, then adds:

- the three platform-independent npm packages;
- `SHA256SUMS`;
- JavaScript and Rust CycloneDX SBOMs;
- the exact legal texts and locked Rust and VS Code bundle dependency
  inventories; and
- `provenance.unsigned.intoto.json`.

The provenance file is intentionally an unsigned staging statement. It binds
the subjects to the source SHA and workflow run for review, but it is not a
cryptographic attestation. npm provenance is created only by an approved npm
publish from a public repository through supported GitHub OIDC. A GitHub
artifact attestation, if later desired, is a separate external write and needs
explicit approval before adding `attestations: write`.

The npm SBOM is generated from a disposable meta-package whose direct inputs
are the 11 candidate tarballs. npm's `--force` flag is used only while creating
that package lock so one Linux runner can describe mutually exclusive
macOS/Linux/Windows `os`/`cpu` packages together. Lifecycle scripts are disabled
and no candidate binary is executed in this SBOM step. The workflow then checks
that every runtime, companion, and platform package is present in the result.

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
the candidate. Inspect every VSIX as a ZIP. There must be 11 npm tarballs and
eight VSIX files; a VSIX contains only the target LSP binary, while each native
npm package contains all three native executables.

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

## 5. Registry publication plan (not implemented)

There is deliberately no publish workflow yet. After the account prerequisites
are complete and the user gives the exact approval, add a separately reviewed
workflow protected by a GitHub `npm-release` environment. It must download the
already-reviewed candidate by run ID and digest; it must not rebuild.

For the first release, npm staged publishing cannot bootstrap a package that
does not already exist. The owner must therefore approve an initial direct
publication path for all eleven packages. Prefer GitHub-hosted OIDC trusted
publishing where npm permits it; any one-time bootstrap credential must be
short-lived, minimally scoped, never printed, and revoked immediately. Publish
all packages under a non-default `next` tag, smoke-test exact installs on each
available platform, then move the complete set to `latest` only after all
eleven versions and provenance records are present.

For later releases, use npm's stage-only trusted publishing. The CI identity
runs `npm stage publish` for each reviewed tarball, and a human reviews the
downloaded staged bytes and runs `npm stage approve STAGE_ID` with 2FA. Configure
the trust relationship to allow staging but not direct publishing. See npm's
[trusted publishing](https://docs.npmjs.com/trusted-publishers/) and
[staged publishing](https://docs.npmjs.com/staged-publishing/) documentation.

The safe package order is all eight native packages, runtime, `oxlint-tsrx`,
then `oxfmt-tsrx`. Never promote `@oxc-tsrx/runtime` to `latest` while one of its
exact optional native dependencies is absent.

Marketplace publication likewise uses the eight already-built VSIX files and
`vsce publish --packagePath PATH`. Do not publish a generic VSIX or rebuild the
extension between targets. Follow the official
[platform-specific extension](https://code.visualstudio.com/api/working-with-extensions/publishing-extension)
procedure and confirm all target variants appear under the one extension ID.

## 6. Post-publication verification

From empty directories on every available OS/CPU/libc family:

- install exact registry versions with no source-tree overrides;
- verify `oxc-tsrx`, `oxc-tsrx-fmt`, and `oxc-tsrx-lsp --version`;
- repeat the mixed lint/format/Vite+ smoke tests;
- install the matching Marketplace VSIX and repeat editor activation and
  format-on-save; and
- compare downloaded artifacts to the approved candidate checksums.

If any member of the version set is missing, mismatched, corrupted, or selects
the wrong ABI, do not paper over it with a fallback. Stop promotion, document
the registry state, and prepare a new patch version. npm versions are
immutable; never overwrite or silently substitute release bytes.
