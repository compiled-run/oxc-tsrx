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

REPO="compiled-run/oxc-tsrx"
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

# `npm trust` exists before 11.15.0 but sends a payload the registry rejects
# with a bare 400 ("value must be an array"), once per package and with no
# indication that the CLI is the problem. Fail up front instead.
NPM_VERSION=$(npm --version)
if ! node -e '
  const [have, want] = [process.argv[1], "11.15.0"].map((v) => v.split(".").map(Number));
  const ok = have[0] > want[0]
    || (have[0] === want[0] && (have[1] > want[1] || (have[1] === want[1] && have[2] >= want[2])));
  process.exit(ok ? 0 : 1);
' "$NPM_VERSION"; then
  echo "npm $NPM_VERSION is too old for 'npm trust' (needs >= 11.15.0)." >&2
  echo "Run: npm install -g npm@latest" >&2
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

  # npm requires 2FA here and prints a browser URL to authenticate against, so
  # it needs the real terminal. Swallowing its output turns the prompt into a
  # silent failure.
  echo ">>> $name"

  # npm refuses to overwrite an existing trusted publisher (E409: "already
  # exists ... delete and re-create"), so a re-point (for example after a repo
  # transfer) must revoke the old config first. `npm trust list` prints the
  # trust id; revoke every one found, then create fresh below.
  existing_ids=$(npm trust list "$name" 2>/dev/null \
    | grep -oE '[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}' \
    | sort -u)
  for trust_id in $existing_ids; do
    echo "  revoking existing trust $trust_id"
    npm trust revoke "$name" --id="$trust_id" || {
      echo "  FAILED to revoke $trust_id on $name; skipping create" >&2
      failed=$((failed + 1))
      continue 2
    }
    sleep 2
  done

  if npm trust github "$name" \
    --repo "$REPO" \
    --file "$WORKFLOW" \
    --allow-publish \
    --yes
  then
    echo "  trusted   $name"
    ok=$((ok + 1))
  else
    echo "  FAILED    $name"
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
