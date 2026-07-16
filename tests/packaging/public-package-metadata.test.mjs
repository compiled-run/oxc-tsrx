import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import test from "node:test";

const root = resolve(import.meta.dirname, "../..");

for (const [directory, name, binary, canonical] of [
  ["oxlint", "oxlint-tsrx", "oxlint", ["oxlint-current", "npm:oxlint@1.74.0"]],
  ["oxfmt", "oxfmt-tsrx", "oxfmt", ["oxfmt-current", "npm:oxfmt@0.59.0"]],
]) {
  test(`${name} has complete public package metadata and exact delegate identity`, async () => {
    const packageRoot = join(root, "packages", directory);
    const manifest = JSON.parse(await readFile(join(packageRoot, "package.json"), "utf8"));
    assert.equal(manifest.name, name);
    assert.equal(manifest.version, "0.1.0");
    assert.equal(manifest.bin[binary], `./bin/${binary}`);
    assert.equal(manifest.dependencies["@oxc-tsrx/runtime"], "0.1.0");
    assert.equal(manifest.dependencies[canonical[0]], canonical[1]);
    assert.equal(manifest.repository.directory, `packages/${directory}`);
    assert.match(manifest.homepage, /^https:\/\//);
    assert.match(manifest.bugs.url, /^https:\/\//);
    assert.ok(manifest.keywords.includes("tsrx"));
    assert.deepEqual(manifest.publishConfig, { access: "public", provenance: true });
    assert.equal(manifest.scripts, undefined);
    for (const file of ["README.md", "LICENSE", "THIRD_PARTY_NOTICES.md"]) {
      assert.ok(manifest.files.includes(file));
      assert.ok((await readFile(join(packageRoot, file), "utf8")).length > 100);
    }
  });
}
