// Every in-site link that carries a `#fragment` has to land on an element that
// really exists in the built page. Five of them did not, and nothing caught it:
// three pages pointed at a heading whose wording had changed, and two pointed at
// a heading whose quotes the slugifier turned into `quot`. Both classes are
// invisible in review, because a bad fragment still resolves to a real page and
// simply lands at the top of it.
import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { existsSync } from "node:fs";
import { mkdtemp, readFile, readdir } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, posix, relative, resolve } from "node:path";
import test from "node:test";

const root = resolve(import.meta.dirname, "../..");

// `docs/dist` is a build artifact and is not committed, so a checkout that has
// not run `docs:build` has nothing to walk. Build one rather than pass by
// default: a link checker that silently checks nothing is worse than none.
async function siteDirectory() {
  const dist = join(root, "docs", "dist");
  if (existsSync(join(dist, "index.html"))) return dist;
  const outDir = await mkdtemp(join(tmpdir(), "oxc-tsrx-fragments-"));
  await new Promise((resolveRun, rejectRun) => {
    execFile(
      process.execPath,
      ["docs/build.mjs"],
      { cwd: root, env: { ...process.env, OXC_TSRX_DOCS_OUT_DIR: outDir }, maxBuffer: 32 * 1024 * 1024 },
      (error, stdout, stderr) => (error ? rejectRun(new Error(stderr || stdout)) : resolveRun()),
    );
  });
  return outDir;
}

async function htmlFilesUnder(directory) {
  const files = [];
  async function visit(current) {
    for (const entry of await readdir(current, { withFileTypes: true })) {
      const path = join(current, entry.name);
      if (entry.isDirectory()) await visit(path);
      else if (entry.name.endsWith(".html")) files.push(path);
    }
  }
  await visit(directory);
  return files.sort();
}

// The route a browser resolves for a page file. The site writes
// docs/dist/integrations/vite-plus.html and serves it at /integrations/vite-plus,
// and docs/dist/index.html at /. Both the `.html` suffix and a trailing slash are
// optional in a link, so every route is compared in one canonical form.
function routeOf(dist, file) {
  const relativePath = relative(dist, file).split(/[\\/]/).join("/");
  return canonical(`/${relativePath}`);
}

function canonical(route) {
  const withoutIndex = route.replace(/\/index\.html$/, "/");
  const withoutSuffix = withoutIndex.replace(/\.html$/, "");
  const trimmed = withoutSuffix.replace(/\/+$/, "");
  return trimmed === "" ? "/" : trimmed;
}

function idsIn(html) {
  const ids = new Set();
  for (const match of html.matchAll(/\sid="([^"]+)"/g)) ids.add(match[1]);
  for (const match of html.matchAll(/\sname="([^"]+)"/g)) ids.add(match[1]);
  return ids;
}

function hrefsIn(html) {
  return [...html.matchAll(/\shref="([^"]+)"/g)].map((match) => match[1]);
}

test("every cross-page href fragment in the built site resolves to an id that exists", async () => {
  const dist = await siteDirectory();
  const files = await htmlFilesUnder(dist);
  assert.ok(files.length > 0, `no built pages under ${dist}`);

  const pages = new Map();
  for (const file of files) {
    const html = await readFile(file, "utf8");
    pages.set(routeOf(dist, file), { file, ids: idsIn(html), hrefs: hrefsIn(html) });
  }

  const failures = [];
  for (const [route, page] of pages) {
    for (const href of page.hrefs) {
      if (!href.includes("#")) continue;
      // Off-site links and the search dialog's own in-page controls are not ours.
      if (/^[a-z][a-z\d+.-]*:/i.test(href) || href.startsWith("//")) continue;
      const [target, fragment] = href.split("#");
      if (!fragment) continue;
      const normalized =
        target === "" ? route : canonical(posix.resolve(posix.dirname(`${route}/`), target));
      const targetPage = pages.get(normalized);
      if (!targetPage) {
        failures.push(`${relative(root, page.file)} -> ${href} (no such page ${normalized})`);
        continue;
      }
      if (!targetPage.ids.has(decodeURIComponent(fragment))) {
        failures.push(`${relative(root, page.file)} -> ${href} (no id "${fragment}" in ${normalized})`);
      }
    }
  }

  assert.deepEqual(failures, [], `broken link fragments:\n${failures.join("\n")}`);
});

test("headings keep their quoted words instead of collapsing them into quot", async () => {
  const files = await htmlFilesUnder(await siteDirectory());
  const offenders = [];
  for (const file of files) {
    const html = await readFile(file, "utf8");
    for (const id of idsIn(html)) {
      // `&quot;`, `&amp;`, and `&#39;` all leave their entity name behind when a
      // slugifier strips punctuation from rendered HTML instead of from text.
      if (/(^|-)(quot|amp|apos|lt|gt|nbsp|x27|39)($|-)|quot[a-z]|[a-z]quot/.test(id)) {
        offenders.push(`${relative(root, file)}: id="${id}"`);
      }
    }
  }
  assert.deepEqual(offenders, [], `heading ids carry HTML entity residue:\n${offenders.join("\n")}`);
});
