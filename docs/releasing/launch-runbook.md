# Approval-gated 0.1.0 launch runbook

This document previews the exact launch payloads. It does not authorize an
external write. Every external surface needs its own exact approval after the
candidate source and workflow run are known.

The immutable identities used below are:

- `VERSION=0.1.0`;
- `COMMIT_SHA`, the reviewed 40-character public source commit; and
- `RUN_ID`, the successful **Build release candidate** workflow run for that
  exact commit.

If the source changes, discard the candidate, obtain a new `RUN_ID`, and repeat
the clean-room proof. Do not substitute a branch name for `COMMIT_SHA`.

## 1. Repository push

First obtain approval that names the destination and exact source:

> Approve repository push of OXC for TSRX 0.1.0 at COMMIT_SHA to
> https://github.com/thejackshelton/oxc-tsrx.

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

The website payload is the byte-for-byte fresh `docs/dist` artifact. GitHub
Pages serves it at `https://thejackshelton.github.io/oxc-tsrx/`; it contains no
server process or native execution endpoint.

Obtain this exact approval:

> Approve GitHub Pages deployment of OXC for TSRX 0.1.0 from COMMIT_SHA.

Then manually run the pinned Pages workflow on that ref:

```sh
gh workflow run pages.yml --ref COMMIT_SHA \
  -f approval='DEPLOY WEBSITE 0.1.0'
```

The `github-pages` environment should require owner review. After deployment,
verify the workflow-reported URL, canonical tags, social card, sitemap, static
playground state, and browser/accessibility suite. The workflow has no push or
automatic trigger.

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
If the site is wrong, stop the Pages environment and prepare a new reviewed
source commit. Record the observable registry, Marketplace, and deployment
state before taking another external action.
