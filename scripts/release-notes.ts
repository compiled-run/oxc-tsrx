#!/usr/bin/env node
// Build the body of a GitHub Release from the real commit history.
//
// `changelogen` is the primary generator, exactly as intended. But changelogen
// only understands Conventional Commits: a commit whose subject is not
// `type(scope): summary` is parsed as nothing and dropped without a warning.
// Most of this repository's history is written as prose, so over the full range
// changelogen emits roughly 40% of the commits and silently hides the rest,
// including most of the headline work.
//
// Rewriting a hundred historical commit messages to suit a parser is
// destructive and not worth doing. So this script runs changelogen for the
// grouped, emoji-headed sections it is good at, then appends one more section
// listing every commit in the range changelogen did not mention. The result is
// changelogen's output plus the remainder, and the script refuses to write a
// file unless the two together account for every commit in the range.
//
// Usage:
//   node scripts/release-notes.ts [--from <ref>] [--to <ref>] [--out <path>]
//
//   --from  defaults to the most recent tag reachable from --to, and to the
//           repository's first commit when no tag exists yet. The first release
//           cut from this repository has no previous tag, which is the case the
//           default is there for.
//   --to    defaults to HEAD. The release workflow passes HEAD~1 so the notes
//           describe the work rather than the version-bump commit itself.
//   --out   defaults to stdout.
//
// This script never commits, never tags, and never leaves a file behind:
// changelogen is pointed at a temporary directory that is removed before exit.

import { execFile as execFileCallback, execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

const execFile = promisify(execFileCallback);
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const require = createRequire(import.meta.url);

class NotesError extends Error {}

function git(args) {
  return execFileSync("git", args, { cwd: root, encoding: "utf8" }).trim();
}

function parseArguments(argv) {
  const options = { from: null, to: "HEAD", out: null };
  for (let index = 0; index < argv.length; index += 1) {
    const flag = argv[index];
    const value = argv[index + 1];
    switch (flag) {
      case "--from":
      case "--to":
      case "--out": {
        if (!value || value.startsWith("--")) {
          throw new NotesError(`${flag} needs a value`);
        }
        options[flag.slice(2)] = value;
        index += 1;
        break;
      }
      default:
        throw new NotesError(`unknown argument: ${flag}`);
    }
  }
  return options;
}

/**
 * The most recent RELEASE tag, or the first commit when there has not been a
 * release yet.
 *
 * `--match "v[0-9]*"` is not decoration. This repository already carries tags
 * that are not releases (`pre-idiomatic-merge/main`,
 * `pre-idiomatic-merge/provider`), and a bare `git describe --tags` picks one
 * of them, which would silently cut the notes down to a fraction of the
 * history. Release tags are written `v<version>` by the release workflow, so
 * that is the only shape a default range is allowed to start from.
 */
function defaultFrom(to) {
  try {
    return git(["describe", "--tags", "--abbrev=0", "--match", "v[0-9]*", to]);
  } catch {
    const first = git(["rev-list", "--max-parents=0", to]).split("\n").at(-1);
    if (!first) throw new NotesError(`no commits reachable from ${to}`);
    return first;
  }
}

/** `https://github.com/owner/repo`, for the per-commit links. */
function repositoryUrl() {
  let remote = "";
  try {
    remote = git(["remote", "get-url", "origin"]);
  } catch {
    remote = "";
  }
  if (!remote) {
    const manifest = JSON.parse(readFileSync(path.join(root, "package.json"), "utf8"));
    remote = manifest.repository?.url ?? "";
  }
  const match = remote.match(/github\.com[/:]([^/]+)\/(.+?)(?:\.git)?$/u);
  if (!match) return null;
  return `https://github.com/${match[1]}/${match[2]}`;
}

/**
 * Every commit in `from..to`, oldest first, merges included.
 *
 * One record per line: a full SHA, a space, the abbreviated SHA, a space, and
 * the subject. `%s` is by definition the first line of the message, so a
 * subject can never introduce a second line and split on the first two spaces
 * is unambiguous.
 */
function commitsInRange(from, to) {
  const raw = git(["log", "--reverse", "--format=%H %h %s", `${from}..${to}`]);
  if (!raw) return [];
  return raw
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean)
    .map((line) => {
      const firstSpace = line.indexOf(" ");
      const secondSpace = line.indexOf(" ", firstSpace + 1);
      return {
        sha: line.slice(0, firstSpace),
        short: line.slice(firstSpace + 1, secondSpace),
        subject: line.slice(secondSpace + 1),
      };
    });
}

async function runChangelogen({ from, to, outputDirectory }) {
  // changelogen's `exports` map publishes only `.`, so the CLI has to be found
  // as a sibling of the resolved entry point rather than by subpath.
  const cli = path.join(path.dirname(require.resolve("changelogen")), "cli.mjs");
  if (!existsSync(cli)) {
    throw new NotesError(
      `changelogen's CLI is not at ${cli}. Run \`pnpm install\`, or follow the package if it ` +
        `has moved its entry points.`,
    );
  }
  const output = path.join(outputDirectory, "CHANGELOG.md");
  try {
    await execFile(
      process.execPath,
      [cli, "--from", from, "--to", to, "--output", output, "--no-commit", "--no-tag"],
      { cwd: root, encoding: "utf8", maxBuffer: 32 * 1024 * 1024 },
    );
  } catch (error) {
    throw new NotesError(
      `changelogen failed (${error.code ?? "unknown"}):\n${error.stderr ?? error.message}`,
    );
  }
  return readFile(output, "utf8");
}

/**
 * Everything after the first `## ` heading, which is the section changelogen
 * wrote for this range. The file also carries a `# Changelog` title and the
 * heading itself, and both of those duplicate the release title on GitHub.
 */
function releaseSection(changelog) {
  const lines = changelog.split("\n");
  const start = lines.findIndex((line) => line.startsWith("## "));
  if (start === -1) return "";
  return lines
    .slice(start + 1)
    .join("\n")
    .trim();
}

/**
 * The commits changelogen actually mentioned.
 *
 * Only the link TEXT of a markdown link counts, because that is where
 * changelogen puts the abbreviated hash. The `[compare changes](...)` line at
 * the top of the section carries the range's endpoint SHAs inside the URL, and
 * counting those would wrongly mark the first commit of a first release as
 * already covered.
 */
function emittedShas(section, commits) {
  const found = new Set();
  for (const match of section.matchAll(/\[([0-9a-f]{7,40})\]\(/gu)) {
    const abbreviated = match[1];
    for (const commit of commits) {
      if (commit.sha.startsWith(abbreviated)) {
        found.add(commit.sha);
        break;
      }
    }
  }
  return found;
}

function otherChangesSection(commits, url) {
  const lines = commits.map((commit) => {
    const subject = commit.subject.replace(/\s+/gu, " ").trim();
    const link = url ? ` ([${commit.short}](${url}/commit/${commit.sha}))` : ` (${commit.short})`;
    return `- ${subject}${link}`;
  });
  return [
    "### 📋 Other changes",
    "",
    "_Commits changelogen could not classify, oldest first. They are here because a",
    "release note that hides most of the history is not a release note._",
    "",
    ...lines,
  ].join("\n");
}

/**
 * Put the appended section above changelogen's contributors block so the
 * credits stay last, which is where a reader expects them.
 */
function spliceSection(section, addition) {
  if (!section) return addition;
  const marker = section.indexOf("### ❤️");
  if (marker === -1) return `${section}\n\n${addition}`;
  return `${section.slice(0, marker).trimEnd()}\n\n${addition}\n\n${section.slice(marker)}`;
}

async function main(argv) {
  const options = parseArguments(argv);
  const to = options.to;
  const from = options.from ?? defaultFrom(to);

  for (const [name, ref] of [
    ["--from", from],
    ["--to", to],
  ]) {
    try {
      git(["rev-parse", "--verify", `${ref}^{commit}`]);
    } catch {
      throw new NotesError(`${name} ${ref} is not a commit this repository knows`);
    }
  }

  const commits = commitsInRange(from, to);
  if (commits.length === 0) {
    throw new NotesError(`${from}..${to} contains no commits, so there is nothing to release`);
  }

  const url = repositoryUrl();
  const outputDirectory = await mkdtemp(path.join(tmpdir(), "oxc-tsrx-release-notes-"));
  let section;
  try {
    section = releaseSection(await runChangelogen({ from, to, outputDirectory }));
  } finally {
    await rm(outputDirectory, { recursive: true, force: true });
  }

  const emitted = emittedShas(section, commits);
  const remainder = commits.filter((commit) => !emitted.has(commit.sha));

  let body = remainder.length > 0 ? spliceSection(section, otherChangesSection(remainder, url)) : section;
  body = `${body.trim()}\n`;

  // The point of the whole script, checked against the finished text rather
  // than against the bookkeeping that produced it. If a commit is in the range
  // and its hash is not in the body, the notes are lying about the history and
  // this release should not go out carrying them.
  const missing = commits.filter((commit) => !body.includes(commit.short));
  if (missing.length > 0) {
    throw new NotesError(
      `${missing.length} of ${commits.length} commit(s) do not appear in the notes: ` +
        `${missing.map((commit) => commit.short).join(", ")}`,
    );
  }

  if (options.out) {
    await writeFile(path.resolve(process.cwd(), options.out), body);
  } else {
    process.stdout.write(body);
  }

  console.error(
    `release-notes: ${from}..${to} - ${commits.length} commit(s) in range, ` +
      `${emitted.size} emitted by changelogen, ${remainder.length} appended as other changes, ` +
      `${emitted.size + remainder.length} covered.`,
  );
  if (options.out) console.error(`release-notes: wrote ${options.out}`);
  return 0;
}

try {
  process.exitCode = await main(process.argv.slice(2));
} catch (error) {
  if (error instanceof NotesError) {
    console.error(`release-notes: ${error.message}`);
    process.exitCode = 1;
  } else {
    throw error;
  }
}
