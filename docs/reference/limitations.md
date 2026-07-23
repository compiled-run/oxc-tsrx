---
title: Limitations
description: What OXC for TSRX does not support yet, and why each gap fails loudly instead of quietly.
---

# Limitations

Everything on this page fails loudly: you get a clear error, never a silently
skipped file or a wrong-but-plausible result.

## Formatting

- **CSS inside `<style>` is never reformatted.** The surrounding TSRX/JSX
  gets full Oxfmt layout, but style contents are preserved byte-for-byte.
  Using OXC's CSS formatter currently requires patching OXC's dependency
  graph, and the project's core rule is no patches, so this waits on
  upstream. See the
  [embedded CSS boundary](/architecture/embedded-css-boundary).
- `.editorconfig` and formatter options that take callbacks are rejected.
- Oxfmt options that only affect other languages (JSON, prose) do not change
  `.tsrx` output.

## Syntax

- Dynamic tags whose name expression contains more dynamic JSX (a dynamic tag
  inside a dynamic tag's name) are not supported.
- Half-typed or broken syntax is never linted or formatted approximately. The
  editor publishes a `parse-error`, returns no formatting edit, and resumes
  normal diagnostics once the buffer is valid again.

## Linting

- **JavaScript lint plugins do not run in the native TSRX CLI or native
  language server.** Released Oxlint 1.74 supports JS rules but explicitly
  does not support custom parsers/file formats, and the native Rust path does
  not embed Oxlint's Node host. A source-only VS Code experiment runs them in
  the official OXC extension through a small LSP launcher and a Node-enabled
  Oxlint build from the upstream custom-parser draft; it is tested but not a
  released product path. Token APIs, framework scope semantics, and a
  released upstream host seam remain. See [Custom JavaScript
  plugins](/integrations/custom-js-plugins).
- **Type-aware rules** require an explicit `--type-aware` or `--type-check`
  opt-in and the exact supported `oxlint-tsgolint` executable. Missing or
  mismatched tooling fails instead of silently downgrading. The editor
  analyzes each requested document; cross-document unsaved project semantics
  are not claimed.
- Not every OXC rule is guaranteed to behave identically around the TSRX
  placeholders. The tested guarantee covers the standard rules in the test
  matrix; anything that would report inside placeholder code is suppressed.
- Alternate report formats and per-directory nested configs are not done.

## CLI and configuration

- The native binaries take **explicit file paths only**. Directory walking
  and globs come from the npm companions and Vite+.
- Config files must be JSON/JSONC. JS/TS config modules are rejected, except
  through the Vite+ path, where the npm companion resolves your
  `vite.config.*` once via Vite+'s public API and hands both engines the same
  extracted `lint`/`fmt` settings. Values that cannot be serialized (like
  callbacks) fail with a clear error.

## Packaging and ecosystem

- **Prepared locally, not published from this repository state.** The eight
  native npm targets and the hosted release workflow are ready, and the host
  target has local build, install, and execution proof. npm availability is
  only claimed after an explicitly approved publication.
- The language server, VS Code extension, and installed VSIX are proven
  locally. Marketplace availability is a separate approval-gated action.
- OXC upgrades are manual: bump the adapter crate and lockfile, then pass the
  full behavior and performance suites.

## What the test suites do and do not prove

- The pinned read-only Markless corpus proves the formatter contract on real
  code: 179/179 valid files format, re-parse, and converge; 12/12
  known-invalid fixtures are rejected; every `<style>` payload survives
  byte-for-byte. It does not test Markless's own compiler or runtime.
- The Vite and Vite+ build/dev/command matrices pass for both the supported
  minimum Vite+ release and the release current when the matrix was frozen.
- A disposable-copy editor walkthrough proves automatic activation, live
  diagnostics, real format-on-save, and one validated safe action without
  changing the external worktree.
