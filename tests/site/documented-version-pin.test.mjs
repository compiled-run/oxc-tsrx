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

test("the documented install commands use a dist-tag, never a frozen version", async () => {
  // `@latest` is fine and `oxc-tsrx` on its own is fine. Both keep working
  // when a new version ships. `oxc-tsrx@0.1.4` does not: it is the form that
  // quietly points every reader at the previous release the moment you publish.
  //
  // Worth knowing about `@latest`, because it looks stronger than it is: pnpm
  // 11 applies its release-age hold to the `latest` tag as well, so on that
  // resolver `oxc-tsrx@latest` and a bare `oxc-tsrx` land on exactly the same
  // version. The tag is a statement of intent that survives a release, not a
  // guarantee of freshness.
  const installLine = /(?:npm install|pnpm add|yarn add|bun add)[^\n`]*\boxc-tsrx@(\S+)/g;
  for (const relativePath of readerFacingSources) {
    const source = await readFile(join(root, relativePath), "utf8");
    for (const match of source.matchAll(installLine)) {
      assert.doesNotMatch(
        match[1],
        /^\d+\.\d+\.\d+/,
        `${relativePath} freezes an install command at oxc-tsrx@${match[1]}; use a dist-tag so it survives the next release`,
      );
    }
  }
});
