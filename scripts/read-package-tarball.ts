import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { gunzipSync } from "node:zlib";

/**
 * Read an npm package tarball without extracting it and without a shell.
 *
 * The pre-publish gate has to answer one question about nine files: is every
 * path this package promises actually inside it. Extracting with the host `tar`
 * would answer it too, but the three Tier 1 runners ship two different tar
 * implementations (GNU on Linux, bsdtar on macOS and Windows) with different
 * opinions about drive letters, permissions, and long names, and a gate that
 * behaves differently per platform is a gate that has to be debugged per
 * platform. Reading the stream here means the same bytes are inspected the same
 * way everywhere, and the per-entry SHA-256 comes out for free, which is what
 * lets the gate cross-check `checksums.json` against the bytes that were packed
 * rather than against a file on disk that may not be what was packed.
 */

const BLOCK_SIZE = 512;

function readField(block, offset, length) {
  const limit = offset + length;
  let end = block.indexOf(0, offset);
  if (end === -1 || end > limit) end = limit;
  return block.toString("utf8", offset, end);
}

function readOctal(block, offset, length) {
  const raw = readField(block, offset, length).trim();
  if (raw === "") return 0;
  const value = Number.parseInt(raw, 8);
  if (!Number.isFinite(value)) {
    throw new Error(`invalid octal tar header field at offset ${offset}: ${JSON.stringify(raw)}`);
  }
  return value;
}

/**
 * Pax extended headers carry the real path (and size) for entries that do not
 * fit the 100-byte ustar name field. npm writes them, so a reader that ignored
 * them would report a package as missing exactly the deeply nested files that
 * are hardest to notice by eye.
 */
function parsePaxRecords(data) {
  const records = new Map();
  let offset = 0;
  while (offset < data.length) {
    const space = data.indexOf(0x20, offset);
    if (space === -1) break;
    const length = Number.parseInt(data.toString("utf8", offset, space), 10);
    if (!Number.isFinite(length) || length <= 0) break;
    const record = data.toString("utf8", space + 1, offset + length).replace(/\n$/u, "");
    const equals = record.indexOf("=");
    if (equals > 0) records.set(record.slice(0, equals), record.slice(equals + 1));
    offset += length;
  }
  return records;
}

function isZeroBlock(block) {
  for (const byte of block) if (byte !== 0) return false;
  return true;
}

/**
 * Every entry in a tar stream, in order. `contents` is a view over the archive
 * buffer rather than a copy, so reading a 14 MB package costs one gunzip.
 */
export function readTarEntries(archive) {
  const entries = [];
  let offset = 0;
  let overrides = new Map();
  let longName = null;
  while (offset + BLOCK_SIZE <= archive.length) {
    const header = archive.subarray(offset, offset + BLOCK_SIZE);
    offset += BLOCK_SIZE;
    // Two zero blocks end the archive; anything after them is padding.
    if (isZeroBlock(header)) continue;
    const name = readField(header, 0, 100);
    const mode = readOctal(header, 100, 8);
    const size = readOctal(header, 124, 12);
    const flag = String.fromCharCode(header[156]);
    const type = flag === "\0" ? "0" : flag;
    const prefix = readField(header, 345, 155);
    const contents = archive.subarray(offset, offset + size);
    offset += Math.ceil(size / BLOCK_SIZE) * BLOCK_SIZE;

    if (type === "g") continue;
    if (type === "x" || type === "X") {
      overrides = parsePaxRecords(contents);
      continue;
    }
    if (type === "L") {
      longName = contents.toString("utf8").replace(/\0+$/u, "");
      continue;
    }

    const path = overrides.get("path") ?? longName ?? (prefix ? `${prefix}/${name}` : name);
    overrides = new Map();
    longName = null;
    entries.push({
      path: path.replace(/\/+$/u, ""),
      type: type === "5" ? "directory" : type === "0" ? "file" : `other-${type}`,
      size,
      mode,
      contents,
    });
  }
  return entries;
}

/**
 * The manifest and the packed contents of one npm tarball.
 *
 * Paths are keyed relative to the package root, which is how a `files` entry
 * and an `oxcTsrx.binaries` entry are both written, so the gate never has to
 * reason about npm's `package/` prefix.
 */
export async function readPackageTarball(tarballPath) {
  const archive = gunzipSync(await readFile(tarballPath));
  const entries = new Map();
  for (const entry of readTarEntries(archive)) {
    if (entry.path !== "package" && !entry.path.startsWith("package/")) {
      throw new Error(
        `${tarballPath}: entry outside the npm package root: ${JSON.stringify(entry.path)}`,
      );
    }
    const path = entry.path === "package" ? "" : entry.path.slice("package/".length);
    if (path === "") continue;
    entries.set(path, {
      path,
      type: entry.type,
      size: entry.size,
      mode: entry.mode,
      sha256:
        entry.type === "file" ? createHash("sha256").update(entry.contents).digest("hex") : null,
      text: () => entry.contents.toString("utf8"),
    });
  }
  const manifestEntry = entries.get("package.json");
  if (!manifestEntry) throw new Error(`${tarballPath}: no package.json inside the tarball`);
  return { tarball: tarballPath, manifest: JSON.parse(manifestEntry.text()), entries };
}
