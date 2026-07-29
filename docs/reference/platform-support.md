---
title: Platform Support
description: The eight platforms oxc-tsrx publishes, which of them a real run exercises on every pull request, and which of them are only built.
---

# Platform Support

`oxc-tsrx` publishes eight native packages, one per platform, and your install
downloads exactly the one that matches your machine. Publishing a platform is a
weaker promise than testing it, so this page says which is which, for all eight.

The tier names are not invented here. They come from
[Rust's platform support](https://doc.rust-lang.org/rustc/platform-support.html),
by way of [uv](https://docs.astral.sh/uv/reference/policies/platforms/), which
keeps the same three tiers and prints them as plain lists:

- **Tier 1** is guaranteed to work. It is built *and* exercised on every pull
  request.
- **Tier 2** is guaranteed to build. It is built and packaged for every release,
  and the tests are not run on it continuously.
- **Tier 3** would be built and published and nothing else. It is empty here.

"Exercised" on this page means one thing: this project spawned the binary it
just built on that platform and read what it printed. A successful compile is
not an exercise, and neither is a packaging assertion.

## Tier 1

- `linux-x64-gnu` (`x86_64-unknown-linux-gnu`)
- `win32-x64-msvc` (`x86_64-pc-windows-msvc`)
- `darwin-arm64` (`aarch64-apple-darwin`)

Every pull request, and every commit that lands on `main`, builds these three
from source on a runner of that operating system and architecture, and then, on
that same machine, runs:

- a real lint through the native binary, including a mixed batch of `.tsrx` and
  ordinary TypeScript;
- a real format, including the stdin path the `oxfmt` command uses;
- two live `--lsp` stdio sessions, one for `oxlint` and one for `oxfmt`;
- a load of the `parser.node` addon built on that machine, followed by the
  parser API suite;
- the pre-publish gate, which packs that platform's own npm package, installs it
  into a project outside this repository, and then lints and parses through the
  installed copy.

That is the `install-arbitration` job in `.github/workflows/ci.yml`, on
`ubuntu-24.04`, `windows-latest`, and `macos-latest`. `linux-x64-gnu` also
carries the full product suite, the one place the editor server, the type-aware
lane, and the JavaScript plugin lane run. So Tier 1 is not a claim that every
feature has been driven on every Tier 1 platform. See
[Limitations](/reference/limitations) for the named gaps.

## Tier 2

- `darwin-x64` (`x86_64-apple-darwin`)
- `linux-arm64-gnu` (`aarch64-unknown-linux-gnu`)
- `win32-arm64-msvc` (`aarch64-pc-windows-msvc`)

Nothing runs on these three when a commit lands or a pull request opens. They
are built and packaged for every release, and they are smoked when a release
candidate is built.

That smoke is real work, not a compile check. `release-candidate.yml` gives each
target a runner of its own architecture and operating system (`macos-15-intel`,
`ubuntu-22.04-arm`, `windows-11-arm`), which runs the same lint, format, and
`--lsp` sessions as Tier 1 and loads that target's `parser.node`.

What they lack is a run tied to the change that would break them. The release
candidate starts by hand, so a regression on `linux-arm64-gnu` stays invisible
until someone cuts a release.

### musl is Tier 2, with a carve-out

- `linux-x64-musl` (`x86_64-unknown-linux-musl`)
- `linux-arm64-musl` (`aarch64-unknown-linux-musl`)

These two get their own sentence, because the Tier 2 line above would flatter
them. **Neither has ever been executed on a musl system.** There is no Alpine
container and no musl runner in any workflow here: both are cross-compiled on a
glibc runner, and the release candidate's smoke runs there too, which proves the
binary carries its own libc and nothing at all about Alpine.

Their `parser.node` addon is built, checksummed, and packaged without ever being
loaded anywhere, because a musl-linked `.node` cannot be `dlopen`'d by a glibc
Node at all. That needs a musl Node on a musl system, a runner this project does
not have yet.

If you run `oxc-tsrx` on Alpine and something breaks, please
[file it](https://github.com/markless-dev/oxc-tsrx/issues). That report is what
would buy the Alpine lane.

## Tier 3

Empty. All eight targets are built and published on every release, so no
platform sits in the "shipped with nothing behind it" tier.

## Platforms with no package at all

There is no JavaScript or WebAssembly fallback. If your platform is outside the
eight above, npm installs no binary for it, and the commands stop with a message
naming your platform:

```text
OXC for TSRX has no native package for linux-riscv64
```

That is deliberate. The alternative is a slow or subtly different fallback path
that nobody tested either. Your route is
[building from source](/guide/getting-started#build-from-source-optional), which
needs a stable Rust toolchain and no other setup.

## How this page stays true

The eight targets above are the ones `packages/toolchain/dist/native-targets.js`
declares, which is what the packaging scripts read.
`tests/site/platform-support-matrix.test.mjs` compares this page against that
file on every documentation run, so adding or dropping a target fails until this
page is updated. Which tier a target sits in stays a judgment call; that it
appears here at all is checked.
