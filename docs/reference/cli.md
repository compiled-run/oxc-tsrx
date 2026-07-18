---
title: CLI Reference
description: Every flag accepted by oxc-tsrx and oxc-tsrx-fmt, with exit codes and environment variables.
---

# CLI Reference

Both commands take **explicit source files**; directory walking and glob
expansion belong to the npm companions and Vite+, not the native binaries. Any
unsupported option is rejected with an actionable error rather than ignored.

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
not by hand; the VS Code companion in `packages/vscode` starts it for
file-backed `.tsrx` documents. It provides live authored-span diagnostics,
whole-document formatting, and validated quick fixes, with opt-in type-aware
diagnostics. See [Editor integration](/integrations/editor) for settings,
architecture, and proof commands.

## npm direct upstream route

The npm companion packages (`oxlint-tsrx`, `oxfmt-tsrx`) recognize a small set
of delegate-only flags. They also conservatively recognize explicit batches
containing only existing ordinary JS/JSX/TS/TSX files. Those invocations load
the pinned package's manifest-declared Oxlint or Oxfmt launcher in the same
Node process instead of initializing the TSRX bridge:

| Command | Delegate-only flags |
| --- | --- |
| `oxlint` (via `oxlint-tsrx`) | `--help`, `-h`, `--version`, `-V`, `--rules`, `--lsp`, `--init` |
| `oxfmt` (via `oxfmt-tsrx`) | `--help`, `-h`, `--version`, `-V`, `--init`, `--migrate`, `--lsp` |

This preserves canonical diagnostics, config/plugin loading, fixes, stdin,
signals, and lifecycle behavior while avoiding a second process. Ambiguous
paths, directories, globs, unknown options, and any `.tsrx` input remain on the
TSRX-aware bridge.

For `--lsp`, loading the upstream launcher in-process keeps its stdio session
attached without a captured or buffered child. Editor clients can therefore
use the wrapper command itself as the server command, which is what keeps the
official OXC VS Code extension working for ordinary JS/TS when a project
aliases `oxlint`/`oxfmt` to the wrapper packages.

## Environment variables

The npm companion packages (`oxlint-tsrx`, `oxfmt-tsrx`) locate the
native binaries through platform packages; during source development these
overrides select release binaries explicitly:

| Variable | Description |
| --- | --- |
| `OXC_TSRX_LINT_BIN` | Absolute path to the native `oxc-tsrx` binary. |
| `OXC_TSRX_FORMAT_BIN` | Absolute path to the native `oxc-tsrx-fmt` binary. |
| `OXC_TSRX_LSP_BIN` | Absolute path to the native `oxc-tsrx-lsp` binary, used by the editor test harness during source development. |

A missing native artifact is an error; `.tsrx` is never silently delegated to
stock tools.
