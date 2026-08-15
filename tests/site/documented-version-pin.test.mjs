// Install commands in the documentation say `oxc-tsrx@latest` (owner decision,
// 2026-08-14). This policy has now flipped in each direction more than once,
// so here is the whole history in one place:
//
// - Unpinned (`npm install --save-dev oxc-tsrx`) is what comparable projects
//   print, but pnpm 11's release-age hold once resolved a bare install to the
//   broken 0.1.0, which forced the first flip to exact pins.
// - Exact pins never rot in-repo (scripts/sync-version.ts rewrote them each
//   cut), but they DO rot on the deployed site between a release and the next
//   deploy, and the owner judged that churn not worth it (2026-08-14, after
//   0.4.0 shipped while the site still showed 0.3.0).
// - `@latest` is the current policy, chosen with the known trade: pnpm applies
//   its release-age hold to the `latest` tag too — measured 2026-08-01, an
//   hour after 0.2.0 shipped, pnpm 11.18 installed 0.1.5 and printed only
//   "(0.2.0 is available)". A day-one reader on pnpm gets the previous
//   release. The owner accepted that in exchange for install lines that never
//   need a doc edit or a redeploy again.
//
// What is asserted now: every documented install command uses the `@latest`
// dist-tag (so stale exact pins cannot creep back in without flipping this
// test deliberately), and any exact pin someone adds on purpose elsewhere — a
// migration note, a reproduction — must agree with the version this
// repository ships. The second check costs nothing when there are no pins.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import test from "node:test";

const root = resolve(import.meta.dirname, "../..");

const readerFacingSources = [
  "README.md",
  "docs/guide/getting-started.md",
  "docs/guide/parsing.md",
  "docs/integrations/custom-js-plugins.md",
  "docs/integrations/configuration.md",
  "docs/integrations/editor.md",
  "docs/integrations/vite-plus.md",
  "docs/reference/cli.md",
  "docs/reference/limitations.md",
  "docs/terminal-transcripts.json",
];

test("any documented oxc-tsrx version pin names the version this repository ships", async () => {
  const shipped = JSON.parse(await readFile(join(root, "package.json"), "utf8")).version;
  assert.match(shipped, /^\d+\.\d+\.\d+$/, "the package must declare a plain semver version");

  for (const relativePath of readerFacingSources) {
    const source = await readFile(join(root, relativePath), "utf8");
    for (const match of source.matchAll(/oxc-tsrx@(\d+\.\d+\.\d+)/g)) {
      assert.equal(
        match[1],
        shipped,
        `${relativePath} sends readers to oxc-tsrx@${match[1]} while this repository ships ${shipped}`,
      );
    }
  }
});

test("the documented install commands use the @latest dist-tag, never an exact pin", async () => {
  const installLine = /(?:npm install|pnpm add|yarn add|bun add|vp install)[^\n`]*\boxc-tsrx@([^\s`]+)/g;
  for (const relativePath of readerFacingSources) {
    const source = await readFile(join(root, relativePath), "utf8");
    for (const match of source.matchAll(installLine)) {
      assert.equal(
        match[1],
        "latest",
        `${relativePath} sends readers to oxc-tsrx@${match[1]}; the documented install form is oxc-tsrx@latest — flipping back to pins is a policy change that starts in this file's header`,
      );
    }
  }
});
