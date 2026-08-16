# Release 0.4.1 Commands

## Summary
PR #21 "Fix Octane compiler compatibility and eager parse overhead" is merged to main at commit `b8d31d8d2f021bd00cdc3ce41ea22c4d3deab0b3`. CI is green. Ready to cut patch release **0.4.1**.

## What's being released
- Restore `@tsrx/core` AST shapes for Octane compiler compatibility
- Fix JSX metadata, template whitespace, directive blocks, source ranges
- Add support for nested, unbraced JSX element attribute values
- Remove reference-parser defaults during native eager transfer (20.5% parse speed improvement)
- 509/509 Octane tests passing, 2,200 files with identical compile status

## Pre-flight checks ✅
- [x] PR #21 merged at b8d31d8d2f021bd00cdc3ce41ea22c4d3deab0b3
- [x] CI green on merge SHA: https://github.com/compiled-run/oxc-tsrx/actions/runs/31971592933
- [x] Current version: 0.4.0 (tag v0.4.0 exists)
- [x] Changes reviewed: compatibility/bugfix patch, no release-blocking issues
- [x] Target version: 0.4.1 (patch bump)

## Commands to execute

### Step 1: Manual Release (dry-run)

Dispatch `.github/workflows/manual-release.yml` from `main` with:
- `release_type`: **patch**
- `mode`: **dry-run**

**Command:**
```bash
gh workflow run manual-release.yml --ref main \
  -f release_type=patch \
  -f mode=dry-run
```

**Expected:**
- Bumps version to 0.4.1 locally
- Runs all gates (sync-version check, test:release, test:packaging:unit, licenses:check)
- Tags v0.4.1 locally
- Generates release notes from v0.4.0..HEAD~1
- Pushes nothing (dry-run)
- Uploads artifact: `release-cut-0.4.1-dry-run` with release notes

**Action:** Review the dry-run artifact and release notes before proceeding.

---

### Step 2: Manual Release (release)

If dry-run is clean, dispatch again with:
- `release_type`: **patch**
- `mode`: **release**

**Command:**
```bash
gh workflow run manual-release.yml --ref main \
  -f release_type=patch \
  -f mode=release
```

**Expected:**
- Creates commit "chore: release v0.4.1"
- Tags v0.4.1
- Pushes commit and tag to main
- Creates GitHub Release at v0.4.1
- Does NOT publish to npm

**Capture:** Note the tag SHA (will be the commit created by this workflow)

---

### Step 3: Build release candidate

After the tag exists, dispatch `.github/workflows/release-candidate.yml` on the **exact tagged SHA**.

**Command:**
```bash
# First, get the tag SHA
TAG_SHA=$(git rev-parse v0.4.1)
echo "Tag SHA: $TAG_SHA"

# Dispatch the build
gh workflow run release-candidate.yml --ref "$TAG_SHA"
```

**Expected:**
- Builds all 8 native packages on matching runners
- Builds oxc-tsrx package
- Generates SHA256SUMS, SBOMs, provenance
- Uploads artifact: `release-candidate-<SHA>`
- Runtime: ~60 minutes

**Capture:** Note the run ID and SHA from the workflow run

**Monitor:**
```bash
gh run list --workflow=release-candidate.yml --limit 1
```

Wait for status=completed, conclusion=success.

---

### Step 4: Publish to npm (dry-run)

When candidate is green, dispatch `.github/workflows/publish.yml` with:
- `candidate_run_id`: <RUN_ID from step 3>
- `candidate_sha`: <SHA from step 3>
- `version`: **0.4.1**
- `mode`: **dry-run**
- `confirm`: leave empty

**Command:**
```bash
# Replace with actual values from step 3
CANDIDATE_RUN_ID="<run_id_here>"
CANDIDATE_SHA="<sha_here>"

gh workflow run publish.yml \
  -f candidate_run_id="$CANDIDATE_RUN_ID" \
  -f candidate_sha="$CANDIDATE_SHA" \
  -f version=0.4.1 \
  -f mode=dry-run
```

**Expected:**
- Downloads candidate artifact
- Re-checks SHA256SUMS
- Rehearses backstop (installs current latest from npm, verifies it works)
- Runs the gate (check-publish-artifacts.ts) on all 9 tarballs
- Rehearses `npm publish --dry-run` against npmjs.com
- Publishes nothing

**Action:** Review the dry-run log for any issues.

---

### Step 5: Publish to npm (release)

If dry-run is clean, dispatch again with:
- `candidate_run_id`: <same as step 4>
- `candidate_sha`: <same as step 4>
- `version`: **0.4.1**
- `mode`: **publish**
- `confirm`: **PUBLISH 0.4.1** (exact string)

**Command:**
```bash
# Use same values from step 4
gh workflow run publish.yml \
  -f candidate_run_id="$CANDIDATE_RUN_ID" \
  -f candidate_sha="$CANDIDATE_SHA" \
  -f version=0.4.1 \
  -f mode=publish \
  -f confirm="PUBLISH 0.4.1"
```

**Expected:**
- Publishes all 9 packages to npm in launch-contract order:
  1. @oxc-tsrx/native-darwin-arm64
  2. @oxc-tsrx/native-darwin-x64
  3. @oxc-tsrx/native-linux-arm64-gnu
  4. @oxc-tsrx/native-linux-x64-gnu
  5. @oxc-tsrx/native-linux-arm64-musl
  6. @oxc-tsrx/native-linux-x64-musl
  7. @oxc-tsrx/native-win32-arm64-msvc
  8. @oxc-tsrx/native-win32-x64-msvc
  9. oxc-tsrx
- Runs post-publish backstop (installs from registry, verifies)
- Uses npm trusted publishing (OIDC)
- Generates provenance attestations

---

### Step 6: Verify npm publication

**Commands:**
```bash
# Verify the main package
npm view oxc-tsrx@0.4.1 version

# Verify all 9 packages exist
for pkg in \
  @oxc-tsrx/native-darwin-arm64 \
  @oxc-tsrx/native-darwin-x64 \
  @oxc-tsrx/native-linux-arm64-gnu \
  @oxc-tsrx/native-linux-x64-gnu \
  @oxc-tsrx/native-linux-arm64-musl \
  @oxc-tsrx/native-linux-x64-musl \
  @oxc-tsrx/native-win32-arm64-msvc \
  @oxc-tsrx/native-win32-x64-msvc \
  oxc-tsrx
do
  echo -n "$pkg: "
  npm view "$pkg@0.4.1" version 2>/dev/null || echo "NOT FOUND"
done

# Verify provenance
npm view oxc-tsrx@0.4.1 dist.attestations

# Verify latest tag points to 0.4.1
npm view oxc-tsrx version
```

**Expected:** All 9 packages at version 0.4.1, `latest` points to 0.4.1, provenance present.

---

## Constraints observed
- ✅ Do NOT add `Co-authored-by: Cursor Agent` commits
- ✅ Do NOT publish VS Code Marketplace VSIX (separate approval)
- ✅ Do NOT post to X, send email, or announce
- ✅ Do NOT clone to laptop and `npm publish` from there
- ✅ Use the two-workflow npm gate (Manual Release → Build candidate → Publish)

## Reference
- Merge commit: b8d31d8d2f021bd00cdc3ce41ea22c4d3deab0b3
- PR #21: https://github.com/compiled-run/oxc-tsrx/pull/21
- CI run: https://github.com/compiled-run/oxc-tsrx/actions/runs/31971592933
- Runbooks: docs/releasing/publish-runbook.md, docs/releasing/README.md
