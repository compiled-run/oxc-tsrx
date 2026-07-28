// Every install command a reader copies names an exact version, because an
// unpinned `pnpm add -D oxc-tsrx` on pnpm 11 resolves whatever version has
// aged past its release-age hold and reports success. When that hold last
// mattered the answer was 0.1.0, the one release whose parser export throws on
// every platform and whose `setup` silently skips the editor slot.
//
// The pin fixes that and creates a maintenance trap in its place: thirteen
// literal version strings across six reader-facing files, none of which any
// build step touches. Ship 0.1.5 and every one of them quietly keeps sending
// readers to the previous release. Nothing about a stale pin looks wrong in
// review, and the docs build cannot tell a deliberate pin from a forgotten
// one. So assert it here, against the version the repository actually ships.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import test from "node:test";

const root = resolve(import.meta.dirname, "../..");

// The pages a reader is sent to. Archived and generated material is excluded on
// purpose: `docs/archive/` records what was true when it was written, and
// `docs/dist/` is rebuilt from these sources.
const readerFacingSources = [
  "README.md",
  "docs/guide/getting-started.md",
  "docs/guide/parsing.md",
  "docs/integrations/custom-js-plugins.md",
  "docs/integrations/vite-plus.md",
  "docs/integrations/configuration.md",
  "docs/integrations/editor.md",
  "docs/reference/cli.md",
  "docs/reference/limitations.md",
  "docs/terminal-transcripts.json",
];

test("every documented oxc-tsrx version pin names the version this repository ships", async () => {
  const shipped = JSON.parse(await readFile(join(root, "package.json"), "utf8")).version;
  assert.match(shipped, /^\d+\.\d+\.\d+$/, "the package must declare a plain semver version");

  let pins = 0;
  for (const relativePath of readerFacingSources) {
    const source = await readFile(join(root, relativePath), "utf8");
    for (const match of source.matchAll(/oxc-tsrx@(\d+\.\d+\.\d+)/g)) {
      pins += 1;
      assert.equal(
        match[1],
        shipped,
        `${relativePath} sends readers to oxc-tsrx@${match[1]} while this repository ships ${shipped}`,
      );
    }
  }

  // A version bump that deletes the pins would otherwise pass silently, which
  // is the failure this whole test exists to prevent.
  assert.ok(
    pins >= 10,
    `expected the documented install commands to stay pinned, found only ${pins}`,
  );
});
