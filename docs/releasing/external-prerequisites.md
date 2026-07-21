# External release prerequisites

These prerequisites are account and registry state outside this repository.
Documentation and local artifacts do not prove that they exist. Verify them
with the owner immediately before adding a publication workflow.

## npm

The owner must control:

- the `@oxc-tsrx` npm organization or scope;
- `@oxc-tsrx/runtime`;
- all eight `@oxc-tsrx/native-*` names listed in the platform policy;
- the unscoped `oxlint-tsrx` name; and
- the unscoped `oxfmt-tsrx` name.

At the 2026-07-16 packaging audit these names returned no published package.
That observation is not a reservation. Recheck every name and resolve ownership
before launch. Do not publish placeholders merely to claim names without a
separate explicit approval.

Required account controls:

- owner and backup maintainer identities with 2FA;
- public access for the scoped packages;
- a public canonical GitHub repository whose case-sensitive URL matches every
  package's `repository.url`;
- a protected GitHub environment for registry writes;
- npm CLI 11.15 or newer on Node 22.14 or newer for staged publishing;
- trusted-publisher configuration for the exact repository, workflow filename,
  and environment; and
- after bootstrap, stage-only OIDC permission, token-based publication
  disabled, and obsolete automation tokens revoked.

Trusted publishing can generate npm provenance automatically for public
packages from a public repository on GitHub-hosted runners. The local unsigned
candidate statement is not a substitute. Each npm package can have only one
trusted publisher configuration, so record and review the exact workflow name
before changing it.

New package names cannot use npm staged publishing until the package exists.
The initial bootstrap therefore needs a separately approved direct-publication
method. Do not decide that credential path in code or store a token in the
repository. Prefer an owner-controlled, short-lived, minimally scoped bootstrap
credential or any first-publication OIDC path npm confirms at execution time;
revoke the credential immediately and move subsequent releases to stage-only
trusted publishing.

## VS Code Marketplace

The owner must verify:

- Marketplace publisher `thejackshelton` exists and is controlled by the
  intended account;
- extension ID `thejackshelton.oxc-tsrx-vscode` is available or already owned;
- the publisher can upload platform-specific VSIX files;
- the Marketplace token/credential is narrowly scoped, stored only in a
  protected environment, and rotated after bootstrap where practical; and
- display name, icon, repository, privacy/support links, license, and third-
  party notices meet Marketplace policy.

The extension is one product with eight platform variants, not eight extension
IDs. Publishing the first VSIX is an external write. Publishing the remaining
variants is also part of the same explicitly approved release operation; stop
if any variant has a different version, target manifest, embedded checksum, or
source SHA.

This is a thin `.tsrx` document-selector/server bridge, not a replacement for
the official OXC or framework extension. Before each editor release, recheck
whether `oxc.oxc-vscode` has gained a configurable custom-language selector and
compatible custom-server contract. If it has, qualify that route and retire the
companion rather than maintaining redundant editor UI.

## Vercel

The owner must verify and control the Vercel project that serves
`https://oxc-tsrx-docs.vercel.app/`, including its production-domain assignment
and rollback access. The reviewed `docs/dist` artifact is deployed as the
project root so `docs/dist/vercel.json` is present at the root of the upload and
applies clean URLs plus the required cross-origin isolation headers.

Any automated production path needs a protected environment and a narrowly
scoped Vercel authentication mechanism bound to the intended team and project.
Project identifiers, tokens, and account configuration remain external state;
do not guess them, commit them, or infer deployment approval from an artifact
build. Disable unreviewed automatic production deployments, or gate them so an
exact approved source commit and website artifact run are the only deployable
inputs.

After every production deployment, read back both
`Cross-Origin-Opener-Policy: same-origin` and
`Cross-Origin-Embedder-Policy: require-corp`, confirm the capability endpoint is
in WASM mode, and complete the real-browser lint, format, projection, and zero
server-engine-request walkthrough. A capability JSON response alone is not
browser execution proof.

## GitHub and launch surfaces

Before any automated release, verify:

- the canonical repository exists at
  `https://github.com/markless-dev/oxc-tsrx` and the owner authorizes the push;
- Actions is enabled for all required x64/arm64 hosted-runner labels;
- required checks and protected `npm-release` / `marketplace-release`
  environments are configured;
- artifact retention is long enough for human review; and
- the chosen commit and candidate run are immutable and visible to reviewers.

This packaging tranche does not authorize repository creation, branch pushes,
tags, GitHub releases, a documentation deployment, or social posts. Those need
their own exact approvals. A missing credential or account is a blocker for
that external action, not a reason to weaken local checks or publish an
incomplete platform set.
