---
title: CLI Reference
description: Every flag accepted by oxc-tsrx and oxc-tsrx-fmt, with exit codes and environment variables.
---

# CLI Reference

Both native commands take **explicit source files**; directory walking and glob
expansion belong to the `oxc-tsrx` npm toolchain and Vite+, not the native binaries. Any
unsupported option is rejected with an actionable error rather than ignored.

## What a plain install puts on your path

`npm install --save-dev oxc-tsrx` is the whole setup for the command line and
for the editor. Vite+ needs one more command; see
[the minimum steps per host](/guide/getting-started#the-minimum-steps-per-host).

The install links seven commands into `node_modules/.bin`:

| Command | Kind | What it is |
| --- | --- | --- |
| `oxlint` | you type it | The linter. Sends `.tsrx` to the TSRX engine and everything else to canonical Oxlint. |
| `oxfmt` | you type it | The formatter, with the same split. |
| `oxc-tsrx` | you type it | `providers`, `status`, `setup`, `remove`. Described in the next section. |
| `oxc-tsrx-lint` | leaf executor | The native linter, given explicit files only. `oxlint` dispatches to it. It prints JSON, and it has no `--help`. |
| `oxc-tsrx-fmt` | leaf executor | The native formatter. `oxfmt` dispatches to it. `--help` works here. |
| `oxc-tsrx-lsp` | leaf executor | The native language server. Editors launch it through `oxlint --lsp`. |
| `tsgolint` | not this project | It arrives with the `oxlint-tsgolint` dependency, the official type-aware runner used by `--type-aware` and `--type-check`. You never call it directly, and calling it prints upstream's own "unsupported entrypoint" warning. |

Reach for a leaf executor directly only when your project pins official `oxlint`
or `oxfmt`, because those command names then belong to the pinned package.

The `oxc-tsrx` sections further down this page describe the **native binary** of
the same name, which is what you get from a source build. It is a different
program from the `oxc-tsrx` npm command above.

### `npx oxlint` with no path also lints `node_modules`

A bare `npx oxlint` walks the current directory, and `node_modules` is part of
it unless something narrows the run. Measured in a scratch project made with
`npm init -y`: 9260 warnings, 9257 of them from `node_modules`. Official Oxlint
from the same install produces the same wall, so this is upstream parity and not
something TSRX introduces.

A `.gitignore` containing `node_modules` removes it completely, with no git
repository required, and naming a path (`npx oxlint src`) avoids it too. The
same scope rule applies to `--fix`, which will rewrite files inside
`node_modules` if you let it reach them. `oxfmt` is unaffected; it skips
`node_modules` unless you pass `--with-node-modules`. There is more detail, with
the measured numbers, in
[Getting Started](/guide/getting-started#give-the-linter-a-path-or-an-empty-folder-will-bury-you).

## `oxc-tsrx` (npm command)

```text
Usage: oxc-tsrx providers [--project <directory>] [--json]
       oxc-tsrx setup     [--project <directory>] [--dry-run] [--json]
       oxc-tsrx status    [--project <directory>] [--json]
       oxc-tsrx remove    [--project <directory>] [--dry-run] [--json]
```

| Subcommand | What it does |
| --- | --- |
| `providers` | Reads the `oxc.provider` block of your direct dependencies and prints the index. It writes nothing and spawns nothing. `routed extensions: .tsrx -> oxc-tsrx` is the line that proves your install works. |
| `setup` | Writes the project-local `oxlint`, `oxfmt`, and `oxc-parser` facades that Vite+ resolves. Only Vite+ needs it. |
| `status` | Reports whether those facades are present. |
| `remove` | Removes them and restores any transitive official package it displaced. |

Run it with no subcommand for the same usage block. A wrong subcommand names the
bad word and exits 2. Note that `--help` and `--version` are not accepted here;
use no arguments, or `help`.

### `status` says `missing` three times in a healthy project

```text
$ npx oxc-tsrx status
oxc-tsrx 0.1.1 compatibility (npm)
- oxc-parser: missing
- oxlint: missing
- oxfmt: missing
```

That output is correct, and the exit code is 0. `status` only ever talks about
the Vite+ compatibility facades, so `missing` means "no facades installed",
which is the right state for every command-line and editor user. There is
nothing to fix, and running `setup` is not the answer unless you use Vite+.

Use `npx oxc-tsrx providers` to check that TSRX support is actually wired up.

## `oxc-tsrx` (lint)

```text
Usage: oxc-tsrx [OPTIONS] FILE...
```

| Option | Description |
| --- | --- |
| `--format=json` | Emit JSON diagnostics. `json` is the only supported value in the current native CLI. |
| `--config PATH`, `-c PATH` | Use an explicit JSON/JSONC Oxlint configuration; may be given once. Bypasses upward `.oxlintrc.json`/`.oxlintrc.jsonc` discovery. |
| `--allow RULE`, `-A RULE` | Set a rule to `allow` (off), overriding configuration. |
| `--warn RULE`, `-W RULE` | Set a rule to `warn`, overriding configuration. |
| `--deny RULE`, `-D RULE` | Set a rule to `deny` (error), overriding configuration. |
| `--fix` | Apply identity-only safe fixes to original TSRX bytes, each validated by a post-edit reparse before write. |
| `--type-aware` | Opt into official tsgolint type-aware rules. Runs one TypeScript-Go process per batch over in-memory `.tsrx.tsx` virtual files; requires the supported `oxlint-tsgolint` executable. |
| `--type-check` | Everything `--type-aware` does, plus TypeScript syntactic and semantic compiler diagnostics. |
| `--config-base PATH` | Resolve a materialized config's relative paths (extends, ignores) against this directory. Used by the Vite+ host when the JSON payload lives in a disposable directory; requires `--config`. |
| `--version` | Show the package version and canonical OXC revision. |

At least one explicit source file is required. Files matching configured
`ignorePatterns` are skipped.

**Exit codes**

| Code | Meaning |
| --- | --- |
| `0` | No errors, warning policy satisfied. |
| `1` | At least one error diagnostic, or `options.denyWarnings`/`options.maxWarnings` failed. |
| `2` | Usage, configuration, or engine error. |

## `oxc-tsrx-fmt` (format)

```text
Usage: oxc-tsrx-fmt [--write | --check] [--threads=INT] PATH...
       oxc-tsrx-fmt [--config=PATH] --stdin-filepath=PATH
```

| Option | Description |
| --- | --- |
| `--write` | Format and write explicit files (default for files). Transactional: all reads and formats finish before any original is replaced. |
| `--check` | Exit `1` and list files that differ; never write. |
| `--stdin-filepath=PATH` | Read source from stdin, infer the source type from `PATH`, print formatted source to stdout. |
| `--config PATH`, `-c PATH` | Use an explicit JSON/JSONC Oxfmt configuration. Bypasses upward `.oxfmtrc.json`/`.oxfmtrc.jsonc` discovery. |
| `--threads=INT` | Worker count for explicit multi-file formatting. |
| `-h`, `--help` | Show help. |
| `-V`, `--version` | Show the package version and canonical OXC revision. |

**Exit codes**

| Code | Meaning |
| --- | --- |
| `0` | Formatted successfully, or `--check` found no differences. |
| `1` | `--check` found files that differ. |
| `2` | Usage, configuration, or engine error. |

## `oxc-tsrx-lsp` (language server)

The third binary hosts the same Rust lint and format sessions behind canonical
OXC's language-server transport (stdio). It is launched by an editor client,
not by hand. The public `oxlint --lsp` command starts it beside canonical
Oxlint and dynamically registers `.tsrx` with the released official OXC
extension. It provides live authored-span diagnostics, whole-document
formatting, and validated quick fixes, with opt-in type-aware diagnostics. See
[Editor integration](/integrations/editor) for architecture and proof commands.

## npm direct upstream route

The public `oxlint` and `oxfmt` commands from `oxc-tsrx` recognize a small set
of canonical delegate flags. They also conservatively recognize explicit batches
containing only existing ordinary JS/JSX/TS/TSX files. Those invocations load
the pinned package's manifest-declared Oxlint or Oxfmt launcher in the same
Node process instead of initializing the TSRX bridge:

| Command | Delegate-only flags |
| --- | --- |
| `oxlint` | `--help`, `-h`, `--version`, `-V`, `--rules`, `--init` |
| `oxfmt` | `--help`, `-h`, `--version`, `-V`, `--init`, `--migrate`, `--lsp` |

This preserves canonical diagnostics, config/plugin loading, fixes, stdin,
signals, and lifecycle behavior while avoiding a second process. Ambiguous
paths, directories, globs, unknown options, and any `.tsrx` input remain on the
TSRX-aware bridge.

`oxlint --lsp` is the deliberate exception. The public launcher multiplexes
canonical Oxlint and `oxc-tsrx-lsp`: ordinary document traffic remains
canonical, `.tsrx` traffic is routed to the native server, and both directions'
request IDs are isolated. Ordinary non-LSP CLI invocations never enter this
multiplexer.

## Environment variables

The `oxc-tsrx` toolchain's internal command packages locate native binaries
through platform packages; during source development these
overrides select release binaries explicitly:

| Variable | Description |
| --- | --- |
| `OXC_TSRX_LINT_BIN` | Absolute path to the native `oxc-tsrx` binary. |
| `OXC_TSRX_FORMAT_BIN` | Absolute path to the same native `oxc-tsrx` binary; the wrapper selects the formatter with its `fmt` subcommand. |
| `OXC_TSRX_LSP_BIN` | Absolute path to the same native `oxc-tsrx` binary, used by the editor test harness during source development. The editor client starts it with `lsp`. |

All three point at one executable. The linter, the formatter, and the language
server are one native binary that dispatches on a leading subcommand (`fmt`,
`lsp`, or `lint`, which is also the default) and on the name it was invoked
under. Three separate binaries linked the same OXC engines three times.

A missing native artifact is an error; `.tsrx` is never silently delegated to
stock tools.
