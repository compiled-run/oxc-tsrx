// The published support matrix has to name exactly the platforms the packaging
// scripts really build, and it has to keep naming them after someone adds or
// drops a target.
//
// `packages/toolchain/dist/native-targets.js` is the canonical list: the build
// scripts, the generated npm manifests, the release matrix, and the publish gate
// all read it. A scan of this repository found that same eight-target list
// hand-duplicated in ten other places. The docs copy is number eleven, and a
// stale docs copy is worse than a stale workflow copy, because a workflow fails
// loudly and a page just quietly tells a reader their platform is supported.
//
// So the set is asserted, not the tiers. Which tier a target sits in is a
// judgment about what is actually run on it and stays a human decision; that a
// target appears on the page at all, under one tier, with its real Rust triple,
// is mechanical and lives here.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import test from "node:test";
import { NATIVE_TARGETS } from "../../packages/toolchain/dist/native-targets.js";
import siteConfig from "../../docs/site.config.mjs";

const root = resolve(import.meta.dirname, "../..");
const MATRIX_PAGE = "docs/reference/platform-support.md";
const MATRIX_LINK = "/reference/platform-support";

const NUMBER_WORDS = [
  "zero",
  "one",
  "two",
  "three",
  "four",
  "five",
  "six",
  "seven",
  "eight",
  "nine",
  "ten",
  "eleven",
  "twelve",
];

// A target row on the page is one flat bullet, `suffix` then its Rust triple in
// parentheses. That is uv's shape, which is a flat list under a tier heading
// rather than a table, and it is why this parser is five lines instead of a
// markdown table reader.
const TARGET_ROW = /^-\s+`([a-z0-9][a-z0-9-]*)`\s+\(`([a-z0-9_][a-z0-9_-]*)`\)/u;
const HEADING = /^(#{2,3})\s+(.*)$/u;

/** Every target bullet on the page, with the tier and heading it sits under. */
export function parseMatrix(markdown) {
  const rows = [];
  let heading = null;
  let tier = null;
  for (const line of markdown.split(/\r?\n/u)) {
    const headingMatch = HEADING.exec(line);
    if (headingMatch) {
      heading = headingMatch[2].trim();
      const tierMatch = /\bTier (\d+)\b/u.exec(heading);
      // A `##` section with no tier in its title ends the tier; a `###`
      // subsection inside one (the musl carve-out) keeps it.
      tier = tierMatch ? Number(tierMatch[1]) : headingMatch[1] === "##" ? null : tier;
      continue;
    }
    const rowMatch = TARGET_ROW.exec(line);
    if (rowMatch && tier !== null) {
      rows.push({ suffix: rowMatch[1], triple: rowMatch[2], tier, heading });
    }
  }
  return rows;
}

/**
 * Everything wrong between a parsed page and a canonical target list, as
 * printable strings. Kept separate from the assertions so the failure path can
 * be exercised below against a deliberately mutated list, rather than only
 * being trusted to work.
 */
export function matrixProblems(rows, targets) {
  const problems = [];
  const byTarget = new Map(targets.map((target) => [target.packageSuffix, target]));
  const seen = new Set();

  for (const row of rows) {
    if (seen.has(row.suffix)) {
      problems.push(`${row.suffix} is listed more than once, so it has no single tier`);
      continue;
    }
    seen.add(row.suffix);
    const target = byTarget.get(row.suffix);
    if (!target) {
      problems.push(`${row.suffix} is published on the page but is not a target this project builds`);
      continue;
    }
    if (row.triple !== target.target) {
      problems.push(
        `${row.suffix} is printed as \`${row.triple}\` but is built as \`${target.target}\``,
      );
    }
    if (![1, 2, 3].includes(row.tier)) {
      problems.push(`${row.suffix} sits under "${row.heading}", which is not one of the three tiers`);
    }
  }

  for (const target of targets) {
    if (!seen.has(target.packageSuffix)) {
      problems.push(
        `${target.packageSuffix} is built and published but appears in no tier on the page`,
      );
    }
  }
  return problems;
}

async function matrixPage() {
  return await readFile(join(root, MATRIX_PAGE), "utf8");
}

test("the published support matrix names exactly the targets this project builds", async () => {
  const rows = parseMatrix(await matrixPage());
  assert.ok(rows.length > 0, `${MATRIX_PAGE} listed no targets at all; the parser found nothing`);
  assert.deepEqual(
    matrixProblems(rows, NATIVE_TARGETS),
    [],
    `${MATRIX_PAGE} has drifted from packages/toolchain/dist/native-targets.js`,
  );
});

// The check above is only worth having if it fails when the two lists diverge,
// and both directions of divergence are real: a target added to the build and
// not to the page, and a target dropped from the build and left on the page.
test("the drift check fails when the canonical target list and the page diverge", async () => {
  const rows = parseMatrix(await matrixPage());

  const added = [
    ...NATIVE_TARGETS,
    { target: "riscv64gc-unknown-linux-gnu", packageSuffix: "linux-riscv64-gnu" },
  ];
  assert.deepEqual(matrixProblems(rows, added), [
    "linux-riscv64-gnu is built and published but appears in no tier on the page",
  ]);

  const [dropped, ...kept] = NATIVE_TARGETS;
  assert.deepEqual(matrixProblems(rows, kept), [
    `${dropped.packageSuffix} is published on the page but is not a target this project builds`,
  ]);

  const retriped = NATIVE_TARGETS.map((target, index) =>
    index === 0 ? { ...target, target: "aarch64-apple-ios" } : target,
  );
  assert.deepEqual(matrixProblems(rows, retriped), [
    `${NATIVE_TARGETS[0].packageSuffix} is printed as \`${NATIVE_TARGETS[0].target}\` but is built as \`aarch64-apple-ios\``,
  ]);

  const duplicated = [...rows, rows[0]];
  assert.deepEqual(matrixProblems(duplicated, NATIVE_TARGETS), [
    `${rows[0].suffix} is listed more than once, so it has no single tier`,
  ]);
});

// musl is Tier 2 by an explicit decision, and the decision came with two limits
// that a generic "built, not continuously tested" line would hide: nothing has
// ever run on a musl userland, and the addon has never been loaded anywhere,
// because a musl-linked `.node` cannot be dlopen'd by a glibc Node at all. So
// the carve-out gets its own subsection and its own words, and folding it back
// into the plain Tier 2 list fails here.
test("the musl targets are carved out of the plain Tier 2 list, with both limits stated", async () => {
  const page = await matrixPage();
  const rows = parseMatrix(page);
  const musl = rows.filter((row) => row.suffix.endsWith("-musl"));
  const canonicalMusl = NATIVE_TARGETS.filter((target) => target.libc === "musl");

  assert.equal(musl.length, canonicalMusl.length);
  assert.deepEqual(
    musl.map((row) => row.suffix).sort(),
    canonicalMusl.map((target) => target.packageSuffix).sort(),
  );

  const headings = new Set(musl.map((row) => row.heading));
  assert.equal(headings.size, 1, "the musl targets are split across headings");
  const [muslHeading] = headings;
  assert.match(muslHeading, /musl/iu, "the musl targets sit under a heading that does not name musl");

  const others = rows.filter((row) => !row.suffix.endsWith("-musl"));
  for (const row of others) {
    assert.notEqual(
      row.heading,
      muslHeading,
      `${row.suffix} is folded into the musl carve-out section`,
    );
  }

  // The section body, from the carve-out heading to the next heading.
  const section = page.split(/^#{2,3}\s+/mu).find((part) => part.startsWith(muslHeading));
  assert.ok(section, "could not read the musl section back out of the page");
  // Line wrapping is not the subject, so the prose is compared unwrapped.
  const prose = section.replace(/\s+/gu, " ");
  assert.match(
    prose,
    /(?:never|neither has ever) been executed on a musl system/iu,
    "the musl section stopped saying that nothing has run on a musl system",
  );
  assert.match(
    prose,
    /without ever being loaded/iu,
    "the musl section stopped saying that the parser addon is never loaded",
  );
  assert.match(
    prose,
    /dlopen/u,
    "the musl section stopped saying why the addon cannot be loaded",
  );
});

test("Tier 3 is present and honest about being empty", async () => {
  const page = await matrixPage();
  const rows = parseMatrix(page);
  assert.match(page, /^## Tier 3$/mu, "the page dropped Tier 3 instead of saying it is empty");
  const tier3 = rows.filter((row) => row.tier === 3);
  const section = page.split(/^#{2,3}\s+/mu).find((part) => part.startsWith("Tier 3"));
  if (tier3.length === 0) {
    assert.match(
      section,
      /empty/iu,
      "Tier 3 lists no targets and does not say it is empty, which reads as an omission",
    );
  } else {
    assert.doesNotMatch(section, /empty/iu, "Tier 3 lists targets and still calls itself empty");
  }
});

// A matrix a reader never reaches is not published. The page has to be in the
// site navigation, and the two pages a reader is most likely to be standing on
// when the question comes up have to point at it.
test("the matrix is reachable from the site navigation and from the pages that raise the question", async () => {
  const sidebarLinks = siteConfig.sidebar.flatMap((group) => group.items.map((item) => item.link));
  assert.ok(
    sidebarLinks.includes(MATRIX_LINK),
    `${MATRIX_LINK} is not in the docs sidebar, so no reader will find it`,
  );

  for (const page of ["docs/reference/limitations.md", "docs/guide/getting-started.md"]) {
    const text = await readFile(join(root, page), "utf8");
    assert.ok(text.includes(MATRIX_LINK), `${page} does not link to the support matrix`);
  }

  const readme = await readFile(join(root, "README.md"), "utf8");
  assert.ok(
    readme.includes(MATRIX_LINK),
    "README.md does not link to the support matrix",
  );
});

// Both pages count the platform packages in words, in prose the list check
// above cannot see, and a target added to the build changes that count.
// Nothing else would notice.
test("the README and the matrix page agree with the canonical list about how many platforms ship", async () => {
  const word = NUMBER_WORDS[NATIVE_TARGETS.length];
  assert.ok(word, `no number word for ${NATIVE_TARGETS.length} targets; extend NUMBER_WORDS`);

  const counted = [
    ["README.md", [`${word} targets`]],
    [MATRIX_PAGE, [`${word} native packages`, `all ${word}`, `${word} targets`]],
  ];
  for (const [page, phrases] of counted) {
    // Prose wraps. Comparing against collapsed whitespace keeps this test about
    // the count rather than about where a line happens to break.
    const text = (await readFile(join(root, page), "utf8")).replace(/\s+/gu, " ");
    for (const phrase of phrases) {
      assert.ok(
        text.includes(phrase),
        `${page} no longer says "${phrase}", and ${NATIVE_TARGETS.length} targets are published`,
      );
    }
  }

  const readme = await readFile(join(root, "README.md"), "utf8");

  // Whatever the README calls Tier 1 has to be a real target, and has to be
  // Tier 1 on the page. A README that promotes a platform on its own is exactly
  // the drift this file exists to stop.
  const tier1 = parseMatrix(await matrixPage()).filter((row) => row.tier === 1);
  const claimed = [...readme.matchAll(/`([a-z0-9]+-[a-z0-9-]+)`/gu)]
    .map((match) => match[1])
    .filter((name) => NATIVE_TARGETS.some((target) => target.packageSuffix === name));
  // A README that names no target cannot drift from the page, so the check is
  // conditional: it fires only once the README starts claiming specific ones.
  if (claimed.length > 0) {
    assert.deepEqual(
      [...new Set(claimed)].sort(),
      tier1.map((row) => row.suffix).sort(),
      "README.md names a different set of targets than the page calls Tier 1",
    );
  }
});
