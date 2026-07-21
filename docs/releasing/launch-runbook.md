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
11 npm tarballs and eight platform VSIX files.

## 3. npm publication

Obtain this exact approval:

> Approve npm publication of 0.1.0 from COMMIT_SHA and candidate run RUN_ID.

The first publication is performed from the reviewed candidate bytes in the
order recorded in `v0.1.0-launch.json`: eight native packages, runtime,
`oxlint-tsrx`, then `oxfmt-tsrx`. Publish the complete set under `next`, verify
exact clean installs and provenance, and only then request a separate promotion
to `latest`. Never rebuild between review and publication.

## 4. VS Code Marketplace

Obtain this exact approval:

> Approve VS Code Marketplace publication of OXC for TSRX 0.1.0 from
> COMMIT_SHA and candidate run RUN_ID.

Upload the eight reviewed target-specific VSIX files under the single extension
ID `thejackshelton.oxc-tsrx-vscode`. Stop if one target, checksum, version, or
embedded source identity differs.

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

There is deliberately no credential-bearing deploy workflow in this repository.
After approval, deploy the reviewed directory through the owner-controlled
Vercel project that owns the canonical URL. Stop if the project identity,
production-domain assignment, artifact digest, source SHA, or deployment
credential does not match the approved operation.

After deployment, verify `Cross-Origin-Opener-Policy: same-origin` and
`Cross-Origin-Embedder-Policy: require-corp` on the canonical URL. Then run the
browser walkthrough against that exact deployment: confirm
`crossOriginIsolated`, exercise configured lint diagnostics, formatting, and
TSRX projection, and record zero `/api` engine requests. Also verify canonical
tags, the social card, sitemap, and the honest unavailable type-aware lint and
completion capabilities.

The artifact workflow has no push or automatic trigger and cannot mutate
Vercel. Project setup, credentials, deployment, domain assignment, and rollback
remain separately approval-gated external actions.

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
