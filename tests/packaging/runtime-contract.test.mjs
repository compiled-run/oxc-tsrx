import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import test from "node:test";
import {
  NATIVE_TARGETS,
  nativePackageName,
} from "../../packages/runtime/dist/targets.js";

const root = resolve(import.meta.dirname, "../..");

test("runtime owns one exact optional native package for every supported target", async () => {
  const runtime = JSON.parse(await readFile(join(root, "packages/runtime/package.json"), "utf8"));
  const expected = Object.fromEntries(
    NATIVE_TARGETS.map((platform) => [nativePackageName(platform), runtime.version]),
  );
  assert.deepEqual(runtime.optionalDependencies, expected);
  assert.equal(runtime.publishConfig.access, "public");
  assert.equal(runtime.publishConfig.provenance, true);
  assert.ok(runtime.files.includes("README.md"));
  assert.ok(runtime.files.includes("THIRD_PARTY_NOTICES.md"));
});

test("the platform matrix is unique and covers the eight launch targets", async () => {
  assert.equal(NATIVE_TARGETS.length, 8);
  for (const key of ["target", "packageSuffix", "vscodeTarget"]) {
    assert.equal(new Set(NATIVE_TARGETS.map((platform) => platform[key])).size, 8);
  }
  assert.deepEqual(
    new Set(NATIVE_TARGETS.map((platform) => platform.os)),
    new Set(["darwin", "linux", "win32"]),
  );
});
