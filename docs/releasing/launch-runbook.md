# Approval-gated 0.1.0 launch runbook

This document previews the exact launch payloads. It does not authorize an
external write. Every external surface needs its own exact approval after the
candidate source and workflow run are known.

The immutable identities used below are:

- `VERSION=0.1.0`;
- `COMMIT_SHA`, the reviewed 40-character public source commit; and
- `RUN_ID`, the successful **Build release candidate** workflow run for that
  exact commit; and
- `SITE_RUN_ID`, the successful **Build website artifact** workflow run for
  that exact commit.

If the source changes, discard the candidate, obtain a new `RUN_ID`, and repeat
the clean-room proof. Do not substitute a branch name for `COMMIT_SHA`.

## 1. Repository push

First obtain approval that names the destination and exact source:

> Approve repository push of OXC for TSRX 0.1.0 at COMMIT_SHA to
> https://github.com/markless-dev/oxc-tsrx.

Creating the repository, changing its visibility, or pushing remains outside
this local preparation task. After the approved push, verify the public source
SHA and enable Actions. No later approval implies this one.

## 2. Candidate artifacts

Manually dispatch `.github/workflows/release-candidate.yml` on `COMMIT_SHA`.
Record `RUN_ID`, download the single assembled candidate, verify
`SHA256SUMS`, and follow [the release runbook](README.md). The expected set is
9 npm tarballs and eight optional legacy-client platform VSIX files. The
workflow asserts that count itself, so a run producing any other number of
`.tgz` files has already failed.

## 3. npm publication

Obtain this exact approval:

> Approve npm publication of 0.1.0 from COMMIT_SHA and candidate run RUN_ID.

The first publication is performed from the reviewed candidate bytes in the
order recorded in `npm.publishOrder` in `v0.1.0-launch.json`. That is nine
packages, natives first:

1. `@oxc-tsrx/native-darwin-arm64`
2. `@oxc-tsrx/native-darwin-x64`
3. `@oxc-tsrx/native-linux-arm64-gnu`
4. `@oxc-tsrx/native-linux-x64-gnu`
5. `@oxc-tsrx/native-linux-arm64-musl`
6. `@oxc-tsrx/native-linux-x64-musl`
7. `@oxc-tsrx/native-win32-arm64-msvc`
8. `@oxc-tsrx/native-win32-x64-msvc`
9. `oxc-tsrx`

`@oxc-tsrx/runtime`, `@oxc-tsrx/parser`, `oxlint-tsrx`, and `oxfmt-tsrx` were
folded into `oxc-tsrx` on 2026-07-25. Do not publish them and do not expect
tarballs for them. The JSON file is authoritative if this list ever drifts.

Publish the complete set under `next`, verify exact one-package clean installs
and provenance, and only then request a separate promotion to `latest`. Never
rebuild between review and publication.

## 4. VS Code Marketplace

Obtain this exact approval:

> Approve VS Code Marketplace publication of OXC for TSRX 0.1.0 from
> COMMIT_SHA and candidate run RUN_ID.

The primary editor workflow uses the already released official OXC extension
and does not require this publication. If the owner separately approves the
optional legacy client, upload the eight reviewed target-specific VSIX files
under `thejackshelton.oxc-tsrx-vscode`. Stop if one target, checksum, version,
or embedded source identity differs.

## 5. Website deployment

The website payload is the byte-for-byte fresh `docs/dist` artifact served by
Vercel at `https://oxc-tsrx-docs.vercel.app/`. It contains the real threaded
browser WASM engine but no server process or native execution endpoint.

First manually build the artifact from the reviewed commit:

```sh
gh workflow run site-artifact.yml --ref COMMIT_SHA
gh run download SITE_RUN_ID \
  --name oxc-tsrx-docs-COMMIT_SHA \
  --dir site-artifact
```

The workflow builds WASM from the locked source graph, fails closed unless the
site is in WASM mode, runs the static browser contract, and uploads the artifact
without deploying it. Verify the Actions artifact digest and inspect
`site-artifact/vercel.json`. The downloaded directory is the Vercel project
root; its `vercel.json` supplies clean URLs plus the cross-origin isolation
headers. Do not rebuild between this review and deployment.

Obtain this exact approval:

> Approve Vercel production deployment of OXC for TSRX 0.1.0 from COMMIT_SHA
> and website artifact run SITE_RUN_ID.

After approval, deploy through the `production` GitHub environment on that same
website-artifact run. The deploy job is deliberately separate from the build
job: it checks the repository out at no point and runs no build, it only
downloads the artifact the build already proved and hands those exact bytes to
Vercel, so the deploy credential is never present in the runner that executed
pnpm postinstalls, cargo, and the docs scripts. Approve the environment only
once the artifact digest and source SHA match the approved operation, and stop
if the project identity or production-domain assignment does not.

After deployment, verify `Cross-Origin-Opener-Policy: same-origin` and
`Cross-Origin-Embedder-Policy: require-corp` on the canonical URL. Then run the
browser walkthrough against that exact deployment: confirm
`crossOriginIsolated`, exercise configured lint diagnostics, formatting, and
TSRX projection, and record zero `/api` engine requests. Also verify canonical
tags, the social card, sitemap, and the honest unavailable type-aware lint and
completion capabilities.

The workflow builds on a push to `main` and on dispatch, but building is all it
does on its own: reaching Vercel requires the `production` environment, so
deployment stays a reviewed step rather than an automatic consequence of a
push. Project setup, credentials, domain assignment, and rollback remain
separately approval-gated external actions.

## 6. Social announcement

Only after registry, Marketplace, and website readback are green, obtain this
exact approval:

> Approve posting the 0.1.0 social text and social-card.png from
> v0.1.0-launch.json.

Post the `social.text` string exactly with `docs/assets/social-card.png`. Check
the rendered link and preview before sending. Do not infer social approval from
package or website approval.

## 7. Rollback and partial failure

If any package or platform is missing, do not promote the npm tag or announce
the release. npm bytes are immutable: correct failures with a new patch version.
If the site is wrong, stop or roll back the Vercel production deployment and
prepare a new reviewed source commit and website artifact. Record the observable
registry, Marketplace, and deployment state before taking another external
action.
