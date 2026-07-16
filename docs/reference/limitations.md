---
title: Limitations
description: What OXC for TSRX does not support yet, and why each gap fails loudly instead of quietly.
---

# Limitations

This project is honest about its edges. Everything on this page is
unsupported *loudly*: you get a clear error, never a silently skipped file or
a wrong-but-plausible result. The project does not call itself complete.

## Formatting

- **CSS inside `<style>` is never reformatted.** The surrounding TSRX/JSX gets
  full Oxfmt layout, but style contents are preserved byte-for-byte. Using
  OXC's CSS formatter currently requires patching OXC's dependency graph, and
  the project's core rule is no patches, so this waits on upstream. The exact
  pinned evidence and requalification gate are documented in the
  [embedded CSS boundary](/architecture/embedded-css-boundary.html).
- `.editorconfig` and formatter options that take callbacks are rejected.
- Oxfmt options that only affect other languages (JSON, prose) don't change
  `.tsrx` output.

## Syntax

- Dynamic tags whose expression *contains more dynamic JSX* (a dynamic tag
  inside a dynamic tag's name) aren't supported yet.
- Half-typed or broken syntax is not linted or formatted approximately. The
  editor publishes a `parse-error`, returns no formatting edit, and resumes
  normal diagnostics when the buffer is valid again.

## Linting

- **JavaScript lint plugins** don't work yet: OXC's own JS-plugin host
  currently lives behind private internal APIs, and reaching into private
  APIs would break the no-fork upgrade contract.
- **Type-aware rules** require an explicit `--type-aware` or `--type-check`
  opt-in and the exact supported `oxlint-tsgolint` 0.24.0 executable. Missing
  or mismatched tooling fails rather than silently downgrading. The editor
  analyzes each requested authored document; cross-document unsaved project
  semantics are not claimed.
- Not every OXC rule is guaranteed to behave identically around the TSRX
  placeholders. The tested guarantee covers the standard rules in the test
  matrix; anything that would report inside placeholder code is suppressed.
- Alternate report formats and per-directory nested configs aren't done.

## CLI and configuration

- The native binaries take **explicit file paths only**. Directory walking
  and globs come from the npm companions and Vite+.
- Config files must be JSON/JSONC. JS/TS config modules are rejected, except
  through the Vite+ path, where the npm companion resolves your
  `vite.config.*` once via Vite+'s public API and hands both engines the same
  extracted `lint`/`fmt` settings. Values that can't be serialized (like
  callbacks) fail with a clear error.

## Packaging and ecosystem

- **Prepared locally, not published by this repository state.** Manifests and
  the hosted workflow cover eight native npm targets; the host target has the
  local build, untouched-tarball installation, and execution proof. Producing
  and validating all eight candidates remains a post-push hosted gate. The
  minimum/current Vite+ matrix passes. npm availability is claimed only after
  registry readback following an explicitly approved publication.
- The native language server, companion VS Code bundle, protocol tests, real
  Extension Host walkthrough, and an installed target-specific VSIX with
  embedded native-binary discovery are proven locally. Marketplace availability
  remains a separate approval-gated external action.
- OXC upgrades are currently manual: bump the adapter crate and lockfile,
  then pass the full behavior and performance suites.

## What the test suites do and don't prove

The pinned read-only Markless corpus proves the *formatter contract* on real
code: 179/179 valid files format, re-parse, and converge; 12/12 known-invalid
fixtures are rejected; every `<style>` payload survives byte-for-byte. It
does not test Markless's own compiler or runtime. Vite 8.1.5 build/dev/HMR
and literal Vite+ 0.1.24/0.2.4 build/dev/command matrices pass. Version 0.1.24
is the supported minimum and 0.2.4 was current when the matrix was frozen.
Version 0.1.20 is kept only as a separate legacy Markless control; its
published package has advisories fixed in 0.1.24 and later. A disposable-copy
Markless editor walkthrough separately proves automatic companion activation,
live diagnostics, real format-on-save, and one validation-passed safe action
without changing the external worktree.
