// Install commands in the documentation are deliberately unpinned. `npm
// install --save-dev oxc-tsrx` is what every comparable project prints, and it
// is what a reader expects to copy.
//
// Pinning was tried and reverted. It was added when an unpinned install on
// pnpm 11 could resolve the broken 0.1.0, because that resolver holds a new
// release back for its first day and 0.1.0 was the only version old enough to
// clear the bar. That hazard aged out: the fallback is now a working version.
// What the pin left behind was thirteen literal version strings across six
// files that no build step touches, all of which quietly point at the previous
// release the moment a new one ships.
//
// `@latest` is not a fix for the same problem. pnpm 11 applies its release-age
// hold to the `latest` tag too, so `oxc-tsrx@latest` resolves to exactly what
// writing nothing resolves to, while looking like a guarantee it does not make.
//
// So no pin is required. But a pin that someone adds on purpose, in a
// migration note or a reproduction, must still name a version that exists and
// agree with what this repository ships. That is what is asserted here, and it
// costs nothing when there are no pins at all.
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

test("the documented install commands pin an exact version, never a dist-tag", async () => {
  // This policy has flipped once, deliberately, in each direction; here is the
  // whole trade so it does not flip again by accident.
  //
  // Dist-tags survive a release without an edit, which is why the site once
  // required them. But pnpm applies its release-age hold to the `latest` tag
  // itself, so for about a day after every publish `oxc-tsrx@latest` silently
  // resolves the PREVIOUS release — measured 2026-08-01, an hour after 0.2.0
  // shipped: pnpm 11.18 installed 0.1.5 and printed only "(0.2.0 is
  // available)". The reader who follows the docs the day a release is
  // announced is exactly the reader the docs most need to serve.
  //
  // A named version skips the holdback on every package manager, and staleness
  // is no longer the price: scripts/sync-version.ts declares every one of
  // these pins as a slot, rewrites them at each cut, and `--check` gates CI,
  // while the sibling test above proves the pinned version is the one this
  // repository ships.
  const installLine = /(?:npm install|pnpm add|yarn add|bun add|vp install)[^\n`]*\boxc-tsrx@([^\s`]+)/g;
  for (const relativePath of readerFacingSources) {
    const source = await readFile(join(root, relativePath), "utf8");
    for (const match of source.matchAll(installLine)) {
      assert.match(
        match[1],
        /^\d+\.\d+\.\d+$/,
        `${relativePath} sends readers to oxc-tsrx@${match[1]}; name the exact shipped version — dist-tags resolve a day behind under pnpm's release-age hold`,
      );
    }
  }
});
