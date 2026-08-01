/**
 * The one reader for `npm pack --json`.
 *
 * The packager and every test harness that packs a tarball share this module, so
 * there is exactly one place that knows which containers npm has printed. A
 * second copy is how the suite ended up green on one npm major and red on the
 * other.
 */

/**
 * The one packed entry `npm pack --json` reported, read from either shape npm
 * has printed.
 *
 * npm 11 and earlier printed an array of entries. npm 12 prints an object keyed
 * by package name. The per-entry fields are identical in both, and both are
 * live: release runners are still on npm 11 while developer machines have moved
 * to npm 12, so this reads whichever arrived rather than pinning one npm major.
 *
 * The strictness stays: exactly one entry, and it must name a file, because the
 * caller turns `filename` into the tarball path the release assembly consumes.
 * Anything else throws with the raw stdout, since that text is the only
 * evidence of what npm actually did: npm's own `{ "error": ... }` report, two
 * packages, a scalar, or output that is not JSON at all.
 */
export function parseNpmPackResponse(stdout) {
  let parsed;
  try {
    parsed = JSON.parse(stdout);
  } catch {
    throw new Error(`unexpected npm pack response: ${stdout}`);
  }
  const entries = Array.isArray(parsed)
    ? parsed
    : parsed && typeof parsed === "object"
      ? Object.values(parsed)
      : [];
  const [packed] = entries;
  if (
    entries.length !== 1 ||
    !packed ||
    typeof packed !== "object" ||
    typeof packed.filename !== "string"
  ) {
    throw new Error(`unexpected npm pack response: ${stdout}`);
  }
  return packed;
}
