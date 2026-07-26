#!/bin/sh
# Attach a GitHub Actions trusted publisher to every launch package.
#
# npm configures trusted publishing per package, not per organization, so this
# is nine calls. It only works on packages that already exist, which is why the
# first publish of each name has to happen before this runs.
#
# After this, `.github/workflows/publish.yml` can publish with no credential and
# npm generates a provenance attestation automatically.
#
#   sh scripts/trust-publishers.sh          # attach
#   sh scripts/trust-publishers.sh --check   # just report current state
#
# npm's 2FA window is about five minutes, long enough for all nine.

set -u

REPO="markless-dev/oxc-tsrx"
WORKFLOW="publish.yml"
CHECK=0
[ "${1:-}" = "--check" ] && CHECK=1

# Read the exact set from the launch contract so this cannot drift from what
# actually ships.
NAMES=$(node -e 'process.stdout.write(require("./docs/releasing/v0.1.0-launch.json").npm.publishOrder.join("\n"))')

if ! npm whoami >/dev/null 2>&1; then
  echo "not logged in to npm: run 'npm login' first" >&2
  exit 1
fi

ok=0
skipped=0
failed=0

for name in $NAMES; do
  if ! npm view "$name" version >/dev/null 2>&1; then
    echo "  not published yet, skipping   $name"
    skipped=$((skipped + 1))
    continue
  fi

  if [ "$CHECK" -eq 1 ]; then
    current=$(npm trust list "$name" 2>&1 | head -3)
    echo "--- $name"
    echo "$current" | sed 's/^/    /'
    continue
  fi

  if npm trust github "$name" \
    --repo "$REPO" \
    --file "$WORKFLOW" \
    --allow-publish \
    --yes >/dev/null 2>&1
  then
    echo "  trusted   $name"
    ok=$((ok + 1))
  else
    echo "  FAILED    $name"
    npm trust github "$name" --repo "$REPO" --file "$WORKFLOW" --allow-publish --yes 2>&1 \
      | grep -i "error" | head -2 | sed 's/^/    /'
    failed=$((failed + 1))
  fi
  # npm rate limits bursts of writes.
  sleep 2
done

[ "$CHECK" -eq 1 ] && exit 0

echo
echo "trusted $ok, skipped $skipped, failed $failed"
if [ "$failed" -eq 0 ] && [ "$skipped" -eq 0 ]; then
  echo
  echo "Every launch package now trusts $REPO/$WORKFLOW."
  echo "Future releases: gh workflow run $WORKFLOW  (no credential, with provenance)"
fi
