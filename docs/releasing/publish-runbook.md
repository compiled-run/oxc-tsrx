# Publish runbook

Rewritten 2026-07-25 for the nine-package layout and for OIDC trusted
publishing. This is the operator checklist for putting `oxc-tsrx` and its eight
platform packages on npm. It records the traps that are specific to shipping
per-platform native packages, because most of them fail quietly.

Nothing has been published yet. Nothing in this file has been executed against
the real registry.

An earlier version of this runbook described thirteen packages. Four of them
(`@oxc-tsrx/runtime`, `@oxc-tsrx/parser`, `oxlint-tsrx`, `oxfmt-tsrx`) were
folded into the single public `oxc-tsrx` package on 2026-07-25 and no longer
exist anywhere in the tree. Anyone following the old list would try an
impossible publish.

## What actually ships

Nine packages, and `docs/releasing/v0.1.0-launch.json` is the source of truth
for both the set and the order:

1. `@oxc-tsrx/native-darwin-arm64`
2. `@oxc-tsrx/native-darwin-x64`
3. `@oxc-tsrx/native-linux-arm64-gnu`
4. `@oxc-tsrx/native-linux-x64-gnu`
5. `@oxc-tsrx/native-linux-arm64-musl`
6. `@oxc-tsrx/native-linux-x64-musl`
7. `@oxc-tsrx/native-win32-arm64-msvc`
8. `@oxc-tsrx/native-win32-x64-msvc`
9. `oxc-tsrx`

Plus eight target-specific VSIX files for `thejackshelton.oxc-tsrx-vscode`,
which go to the VS Code Marketplace, not to npm, and are covered by
[the release runbook](README.md).

### The order is not negotiable

`oxc-tsrx` last, always. It lists all eight platform packages in
`optionalDependencies`, and npm resolves those at install time against whatever
is on the registry at that moment. Publish the parent first and you open a
window where an install succeeds, quietly installs no binary, and then fails at
first use with a confusing "binary not found" instead of an install error.

This is the single most damaging ordering mistake available here, because
`optionalDependencies` failures are silent by design. npm treats a missing
optional dependency as normal.

`.github/workflows/publish.yml` reads the order out of the launch contract and
refuses to run if the contract does not list exactly nine names ending in
`oxc-tsrx`, so you do not have to hold the order in your head. You do have to
not bypass the workflow.

### Version lockstep

All nine publish at the same version. `oxc-tsrx` pins each platform package to
an exact version:

```json
"optionalDependencies": {
  "@oxc-tsrx/native-darwin-arm64": "0.1.0",
  "@oxc-tsrx/native-darwin-x64": "0.1.0"
}
```

If one lands at a different version, that platform gets no binary. There is no
partial-success mode. The publish workflow checks every tarball's manifest
against the version you typed before it publishes anything.

## How publishing authenticates

There is no npm token anywhere in this repository, and adding one would be a
step backwards.

`.github/workflows/publish.yml` uses **npm trusted publishing**. The job asks
GitHub for a short-lived OpenID Connect (OIDC) token, the npm CLI notices it
automatically, exchanges it for a short-lived registry token, and publishes. If
you have only ever published with `NPM_TOKEN`, the mental model is: instead of
storing a long-lived secret that proves "I am allowed to publish", the workflow
proves "I am this specific workflow file in this specific repository" at the
moment it runs, and npm decides whether that is allowed.

What makes it work:

- `permissions: { id-token: write }` on the publish job. Without it there is no
  OIDC token and npm falls back to looking for a token that does not exist. The
  error you get is a misleading `ENEEDAUTH` or `E404`.
- npm CLI **11.5.1 or newer**, on Node **22.14.0 or newer**. Node 24.15.0, which
  this repository pins everywhere, bundles npm 11.12.1, so the floor is already
  met. The workflow still pins `npm@11.18.0` explicitly and asserts the version,
  so a future Node bump cannot silently drop below the floor.
- A GitHub-hosted runner. Self-hosted runners are not supported for trusted
  publishing.
- `repository.url` in each manifest must match the GitHub repository exactly.
  Every manifest here says `git+https://github.com/markless-dev/oxc-tsrx.git`,
  including the generated platform manifests from `scripts/package-native.mjs`.

Provenance comes for free. npm generates and publishes a provenance attestation
automatically for a trusted publish from a public repository, so you do not need
`--provenance` on the command line. Every manifest also sets
`publishConfig.provenance: true`, which is why publishing from a laptop fails:
provenance can only be produced by a supported CI.

Sources: npm's [trusted publishing docs](https://docs.npmjs.com/trusted-publishers)
and the `id-token: write` pattern used by real Solid repositories such as
[`solidjs/solid-start`](https://github.com/solidjs/solid-start/blob/main/.github/workflows/release.yml),
whose release job carries `id-token: write # Required for npm trusted publishing (OIDC)`
and no `NPM_TOKEN`.

## The one-time setup only the owner can do

**Publishing is not fully hands-off yet, and it cannot be for version 0.1.0.**
This section is the honest part of the runbook. Read it before scheduling an
announcement.

npm configures a trusted publisher **on a package that already exists**. Both
the [npm trust CLI docs](https://docs.npmjs.com/cli/v11/commands/npm-trust)
("Package must exist: The package you're configuring must already exist on the
npm registry") and the trusted publishing guide ("Navigate to your package
settings on npmjs.com") say so, and
[npm/cli#8544](https://github.com/npm/cli/issues/8544), "Allow publishing
initial version with OIDC", is still open with comments as recent as June 2026.
PyPI supports pre-registering a publisher for a name that does not exist yet.
npm does not.

All nine names are brand new. So the first publish of each name cannot use OIDC.

### Step 1: bootstrap the nine names (one time, from the owner's machine)

Do this once, from a laptop, with interactive authentication. Do not create a
long-lived automation token for it.

This writes to the registry, so it needs the owner's explicit npm-publication
approval in its own right, exactly like publishing 0.1.0 does. It is not a
preparation step that someone else can take on the owner's behalf.

```sh
npm install -g npm@^11.15.0   # npm trust needs 11.15.0 or newer
npm login                     # interactive, 2FA at the prompt
npm whoami                    # confirm the right account
```

Publish a throwaway version of each of the nine names, under a throwaway tag so
that `latest` is never pointed at a placeholder, and with provenance turned off
because a laptop cannot produce it:

```sh
cd "$(mktemp -d)"
for name in \
  @oxc-tsrx/native-darwin-arm64 @oxc-tsrx/native-darwin-x64 \
  @oxc-tsrx/native-linux-arm64-gnu @oxc-tsrx/native-linux-x64-gnu \
  @oxc-tsrx/native-linux-arm64-musl @oxc-tsrx/native-linux-x64-musl \
  @oxc-tsrx/native-win32-arm64-msvc @oxc-tsrx/native-win32-x64-msvc \
  oxc-tsrx
do
  mkdir -p bootstrap && cd bootstrap
  cat > package.json <<JSON
{
  "name": "$name",
  "version": "0.0.0-trusted-publishing-bootstrap",
  "private": false,
  "description": "Name reservation for trusted publishing setup. Do not install.",
  "license": "MIT",
  "repository": { "type": "git", "url": "git+https://github.com/markless-dev/oxc-tsrx.git" },
  "publishConfig": { "access": "public", "provenance": false }
}
JSON
  npm publish --tag bootstrap
  cd .. && rm -rf bootstrap
done
```

Two details that matter:

- `--tag bootstrap` means `latest` is never set to the placeholder, so nobody
  can accidentally install it.
- `"provenance": false` is required. With `provenance: true` a laptop publish
  fails outright, which is exactly the blocker the earlier runbook recorded.

There is a community tool, `npx setup-npm-trusted-publish <name>`, that
automates the placeholder publish. It is not vetted here, and it publishes under
the owner's account, so the manual loop above is the recommended path.

### Step 2: configure the trusted publisher on each of the nine packages

Two equivalent ways. The CLI is much faster for nine packages.

**Option A, the CLI (recommended).** npm's docs describe a 5 minute window after
the first 2FA prompt in which further `npm trust` calls do not re-prompt, which
is enough for nine packages:

```sh
for name in \
  @oxc-tsrx/native-darwin-arm64 @oxc-tsrx/native-darwin-x64 \
  @oxc-tsrx/native-linux-arm64-gnu @oxc-tsrx/native-linux-x64-gnu \
  @oxc-tsrx/native-linux-arm64-musl @oxc-tsrx/native-linux-x64-musl \
  @oxc-tsrx/native-win32-arm64-msvc @oxc-tsrx/native-win32-x64-msvc \
  oxc-tsrx
do
  npm trust github "$name" \
    --repo markless-dev/oxc-tsrx \
    --file publish.yml \
    --allow-publish \
    --yes
  sleep 2
done
npm trust list oxc-tsrx     # confirm it saved
```

`npm trust` needs npm 11.15.0 or newer, account-level 2FA enabled, and write
access to each package. Granular access tokens with the "bypass 2FA" option do
not work for it.

`scripts/trust-publishers.sh` is that loop, kept in the tree so nobody retypes
it. It reads the nine names from the launch contract rather than repeating them,
stops early if you are not logged in or if npm is too old, skips any name that
is not on the registry yet, and prints a per-package result.
`sh scripts/trust-publishers.sh --check` reports the current configuration
without changing anything. Nothing in CI runs it, and nothing should: it needs
an interactive 2FA prompt, so it stays an owner-only manual step.

**Option B, the website.** For each of the nine packages, in this order of
clicks:

1. Sign in at [npmjs.com](https://www.npmjs.com/).
2. Go to **Packages**, click the package name.
3. Open the **Settings** tab.
4. Find the **Trusted Publisher** section.
5. Under **Select your publisher**, click **GitHub Actions**.
6. **Organization or user**: `markless-dev`
7. **Repository**: `oxc-tsrx`
8. **Workflow filename**: `publish.yml` (just the filename, with the extension,
   not a path)
9. **Environment name**: leave empty. `publish.yml` does not declare a GitHub
   environment. If you later add one, you must come back and fill this in, or
   publishing breaks.
10. **Allowed actions**: tick **npm publish**. (Tick **npm stage publish** too if
    you intend to move to staged publishing later.)
11. Save.

Every field is case sensitive and npm does not validate them when you save. A
typo shows up only as a failed publish.

### Step 3: publish 0.1.0 from CI

Now the workflow works. See "Running the publish" below.

### Step 4: clean up the placeholders

Only after 0.1.0 is on the registry, so that each package still has a real
version left:

```sh
npm unpublish "$name@0.0.0-trusted-publishing-bootstrap"
```

Do not unpublish the placeholder while it is the only version of a package.
Removing the last version removes the package, and removing the package removes
its trusted publisher configuration.

### Step 5 (optional hardening, later)

Once a trusted publish has actually worked:

- On each package: **Settings → Publishing access → Require two-factor
  authentication and disallow tokens**. Trusted publishing keeps working; only
  token authentication is switched off.
- Consider switching the trusted publisher to stage-only (`--allow-stage-publish`
  without `--allow-publish`). Then CI runs `npm stage publish` and a human
  approves each release with 2FA before it becomes installable. That is the
  strongest posture, and it is a change to make for 0.1.1, not during a launch.

### So: is publishing hands-off?

**Not for 0.1.0.** Steps 1 and 2 are one-time account-level actions that no
workflow can perform. After they are done, every later release is a workflow
dispatch with no token and no laptop publish. Anyone who says the first release
is fully automated has not tried it against a name that does not exist yet.

## Still undecided: the dist-tag

`docs/releasing/v0.1.0-launch.json` says:

```json
"distTag": "next",
"installPreview": "npm install -D oxc-tsrx"
```

Those disagree. `npm install -D oxc-tsrx` resolves the `latest` tag. Published
under `next` only, the advertised command fails with E404, which is the first
thing anyone reading the announcement will run.

Pick one before publishing:

- Publish to `latest`. The advertised command works and no launch copy changes.
- Stay on `next`. Then every piece of launch copy has to say
  `npm install -D oxc-tsrx@next`, including the README, the site, and the
  announcement.

There is no third option where the short command works and the tag is `next`.
The publish workflow uses the launch contract's tag by default, prints a loud
warning when the resolved tag is not `latest`, and accepts a `dist_tag` input if
you want to override it for one run. It does not make the decision for you.

## Before you publish

| Check | How | Result 2026-07-25 |
| --- | --- | --- |
| All nine names available | `npm view <name> version` | Recheck. The last name audit predates the nine-package collapse. |
| `access: public` on every package | manifest `publishConfig` | Present. The publish workflow re-checks each tarball. |
| Versions in lockstep | every manifest at the same version | All at `0.1.0` |
| Candidate built | **Build release candidate** run is green | Required. The publish workflow refuses any other run. |

Scoped packages default to **restricted** on first publish. `access: public` is
set everywhere, so this is covered, but a newly added scoped package needs the
same field or the publish silently creates a private package nobody can install.

## Running the publish

The publish workflow never rebuilds. It downloads the artifact that was already
reviewed, re-checks `SHA256SUMS`, and publishes those exact bytes.

1. Dispatch **Build release candidate** on the exact commit. Note the run ID and
   the commit SHA.
2. Review the candidate as described in [the release runbook](README.md).
3. Dispatch **Publish to npm** with:
   - `candidate_run_id`: the run ID from step 1
   - `candidate_sha`: the commit SHA from step 1
   - `version`: `0.1.0`
   - `mode`: `dry-run`
   - `confirm`: leave empty
4. Read the dry-run log. It prints the nine packages in publish order, the
   resolved dist-tag, and the file list of every tarball. Nothing was published.
5. Dispatch it again with `mode: publish` and `confirm` set to exactly
   `PUBLISH 0.1.0`.

The gate is deliberate. `mode` defaults to `dry-run`, the publish step is
skipped unless `mode` is `publish`, and the job fails immediately if the
confirmation phrase does not match the version. A mistaken trigger cannot
publish. The workflow also runs only on `workflow_dispatch` and only in
`markless-dev/oxc-tsrx`, so no push, tag, or fork can start it.

What the workflow checks before it writes anything:

- the candidate run is a **completed, successful** run of
  `release-candidate.yml` on the SHA you named;
- `SHA256SUMS` still matches the downloaded bytes;
- the launch contract lists exactly nine packages with `oxc-tsrx` last;
- every tarball's manifest carries the version you typed, `access: public`, and
  `provenance: true`;
- `oxc-tsrx`'s `optionalDependencies` are exactly the eight platform packages,
  each pinned to that version.

Then it dry-runs all nine, and only then publishes all nine in order.

### If the publish fails with ENEEDAUTH or E404

Nearly always a trusted publisher configuration mismatch. Check in this order:

1. Workflow filename on npmjs.com is exactly `publish.yml`, with the extension
   and no directory.
2. Organization/user is `markless-dev` and repository is `oxc-tsrx`, case
   sensitive.
3. The **Environment name** field is empty, because `publish.yml` declares no
   environment.
4. `id-token: write` is present on the job (it is, on the `publish` job).
5. The package already exists. A name that has never been published cannot be
   configured, so this error on a first publish means step 1 of the setup was
   skipped.

npm's own troubleshooting notes that these errors are reported as generic 404 /
ENEEDAUTH rather than as a trusted-publishing diagnostic, so do not read the
message too literally.

## After you publish, verify from outside

Do not trust the local tree. Verify against the real registry, ideally on a
machine that has never built this project.

```sh
# 1. The advertised command actually works
mkdir /tmp/oxc-tsrx-smoke && cd /tmp/oxc-tsrx-smoke
npm init -y
npm install -D oxc-tsrx        # add @next if you published to the next tag

# 2. Exactly one platform package arrived
ls node_modules/@oxc-tsrx/     # expect native-<your-platform> and nothing else

# 3. The bins exist and run
npx oxlint --version
npx oxfmt --version

# 4. It actually handles .tsrx
printf 'let x = 1\n' > a.tsrx
npx oxlint a.tsrx

# 5. Provenance really landed
npm view oxc-tsrx@0.1.0 dist.attestations
```

Then the case that matters most for not breaking people:

```sh
# 6. A project that already pins official oxlint must keep its version
mkdir /tmp/oxc-tsrx-collision && cd /tmp/oxc-tsrx-collision
npm init -y
npm install -D oxlint@1.72.0
npx oxlint --version           # note the version
npm install -D oxc-tsrx
npx oxlint --version           # MUST still report 1.72.0
```

Step 6 is the regression that `tests/packaging/released-host-install.test.mjs`
guards. Before that fix, npm silently upgraded a pinned 1.72.0 to 1.74.0, and
pnpm behaved differently again. Confirm it against the published artifacts, not
just locally.

## Edge cases specific to per-platform native packages

- **`optionalDependencies` fail silently.** If one platform package fails to
  publish, users on that platform get no install error. They get a runtime
  "binary not found" later, which reads like a bug in your code. The workflow
  checks all nine landed, but check the registry yourself before announcing.
- **`os`/`cpu` fields decide what gets downloaded.** A wrong or missing field
  means either the wrong binary on a platform or nothing at all.
- **musl versus glibc.** `linux-x64-musl` is what Alpine and many Docker images
  need. It is easy to leave out, and the failure looks like a corrupt binary
  rather than a missing variant.
- **One binary, three names.** Each platform package now ships a single
  multi-call executable that answers to `oxc-tsrx`, `oxc-tsrx-fmt`, and
  `oxc-tsrx-lsp` through `argv[0]` and through the `fmt` / `lsp` subcommands.
  Do not expect three files in the tarball.
- **The 72 hour unpublish window.** npm allows unpublishing within 72 hours of
  first publish. After that you can only deprecate. For a first release with
  nine interdependent packages, that window is your only clean undo.
- **Provenance is per-package.** All nine publish from CI, so all nine get
  attestation. A mixed release where some have it and some do not is worse than
  none having it.
- **Two dependencies are npm alias specs.** `oxc-tsrx` depends on
  `"oxlint-current": "npm:oxlint@1.74.0"` and `"oxfmt-current": "npm:oxfmt@0.59.0"`.
  The names `oxlint-current` and `oxfmt-current` do not exist on npm and do not
  need to; the alias points at the real package. npm supports alias specs in
  published manifests, and the local clean-install and compat lanes exercise
  them. They are less common though, so confirm on pnpm, Yarn, and Bun after
  publishing rather than assuming parity. A failure here looks like an
  unresolvable `oxlint-current`.
- **Aliasing means the real `oxlint` package is present under another name.** A
  consumer ends up with real Oxlint installed as `oxlint-current`, so
  `require.resolve("oxlint")` still fails. That is exactly why Vite+ cannot see
  TSRX from a plain install, and it is expected, not a packaging bug.

## What is verified and what is not

Verified on darwin-arm64 only:

- install-only behavior for `npx oxlint`, `npx oxfmt`, and the released
  `oxc.oxc-vscode` 1.59.0 extension with no setup step;
- the pinned-version collision case under npm and pnpm;
- provider discovery across npm, pnpm, Bun, Yarn Berry node-modules, and Yarn
  Berry Plug'n'Play.

Not verified anywhere:

- **Any part of the trusted publishing flow.** No publish, dry run, or `npm
  trust` call in this runbook has been executed. The workflow's YAML parses and
  its checks are readable, and that is the whole of the evidence.
- **Windows and Linux behavior.** The `install-arbitration` CI job covering
  `windows-latest` and `ubuntu-24.04` exists but has not run. Windows
  correctness for command arbitration is argued from source, not observed.
  macOS has no CI job at all.
- Vite+ cannot be served by a plain install. It resolves the *package* name
  `oxlint`, which a bin cannot satisfy and which this project cannot legitimately
  publish. `npx oxc-tsrx setup` remains the one command that fixes it. Say that
  plainly in launch copy rather than letting people discover it.

`docs/releasing/external-prerequisites.md` still lists the pre-collapse
thirteen-name set. It was outside the scope that produced this rewrite; treat
the nine names above as authoritative.
