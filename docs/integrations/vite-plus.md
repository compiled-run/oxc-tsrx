# Vite and Vite+ integration

## Architecture

OXC for TSRX does not compile application modules for Vite. Runtime semantics,
CSS extraction, source maps, and HMR remain owned by the selected framework
plugin, such as `@tsrx/vite-plugin-react` or Markless's Vite plugin. Adding a
generic OXC projection before that compiler would duplicate parsing and erase
framework semantics.

The integration instead occupies Vite+'s public project-local tool-resolution
seam:

```text
Vite build/dev/HMR -> official framework TSRX plugin -> Vite/Rolldown

vp lint -> project-local oxlint alias
          |- ordinary JS/JSX/TS/TSX -> canonical Oxlint
          `- .tsrx          -> native OXC for TSRX

vp fmt  -> project-local oxfmt alias
          |- ordinary JS/JSX/TS/TSX -> canonical Oxfmt
          `- .tsrx          -> native OXC for TSRX
```

The command packages are `oxlint-tsrx` and `oxfmt-tsrx`. They retain
the canonical package root APIs and expected `dist/index.js` plus `bin/*`
layout, so Vite+ resolves them when installed under npm aliases named `oxlint`
and `oxfmt`. They do not patch or import Vite+ internals.

## Consumer shape

The distributable platform layout is complete. Once the approval-gated npm
release exists, the consumer manifest is:

```json
{
  "devDependencies": {
    "vite-plus": "0.2.4",
    "oxlint": "npm:oxlint-tsrx@0.1.0",
    "oxfmt": "npm:oxfmt-tsrx@0.1.0"
  }
}
```

`@oxc-tsrx/runtime` selects the exact matching optional native package for the
host; consumers do not name a platform package. During source development,
`OXC_TSRX_LINT_BIN` and
`OXC_TSRX_FORMAT_BIN` select release binaries explicitly. A missing native
artifact is an error; `.tsrx` is never silently delegated to stock tools.

Use the framework plugin exactly as its framework documents. No OXC for TSRX
Vite plugin is added:

```js
import { tsrxReact } from '@tsrx/vite-plugin-react';
import { defineConfig } from 'vite-plus';

export default defineConfig({
  plugins: [tsrxReact()],
  lint: {
    plugins: ['typescript'],
    rules: {
      'no-debugger': 'error',
      'typescript/no-floating-promises': 'error',
    },
    options: {
      typeAware: true,
    },
  },
  fmt: {
    semi: true,
    singleQuote: true,
  },
});
```

## Configuration boundary

When Vite+ passes `vite.config.*` as the tool config, the thin Node host calls
Vite+'s public `resolveConfig` once. It extracts only `lint` or `fmt`, rejects
non-serializable values, and writes the field to a disposable JSON file for the
native TSRX process. The native session also receives the authored Vite config
directory, so object `extends`, override globs, and `ignorePatterns` keep their
original roots even though the JSON file is temporary.

For configs without path-sensitive fields, canonical Oxlint/Oxfmt consumes the
same disposable JSON and avoids another JavaScript config evaluation. When
`extends`, overrides, ignores, or plugin paths require the authored base, the
canonical process loads the original `vite.config.*` through its supported
Vite+ path while the native process uses the materialized field plus explicit
base. This is deliberately a correctness-first split: no project file is
created, no relative path is rebased approximately, and both lanes retain OXC's
own semantics. The disposable file is removed after the batch.

Config resolution remains outside per-file work. Each TSRX file has one native
scan/projection/OXC parse; ordinary files are never sent through the TSRX
engine. An explicit ordinary JSON/JSONC `--config` keeps the direct native
configuration path. Unsupported JavaScript plugins, callback values, or
private APIs still fail loudly.

For type-aware lint, the companion reads the resolved
`lint.options.typeAware` and `lint.options.typeCheck` values. It appends
`--type-aware` or `--type-check` to the native `.tsrx` batch automatically;
users do not need to duplicate that intent in a Vite+ script. Explicit command
flags remain supported, and direct invocation of the Rust binary still
requires one of those flags so config alone cannot unexpectedly start a type
process. `typeCheck` implies the type-aware lane and adds TypeScript syntactic
and semantic diagnostics.

All discovered `.tsrx` files for the command share one official tsgolint 0.24.0
process when configured type work exists. Their Rust type projections are sent
as in-memory `.tsrx.tsx` source overrides, with no generated TypeScript source
or project files.
Rules and override globs are resolved against authored `.tsrx` paths before
the virtual names are sent, so `.tsrx`-specific configuration and explicit
`.tsrx` imports retain their meaning. Canonical Oxlint still handles ordinary
files in its parallel lane; the TSRX scanner never touches them.

Diagnostics return at authored TSRX byte spans. Only exact identity-mapped
safe fixes may cross back into the source, followed by a TSRX validation
reparse; semantic suggestions and projection-crossing edits are rejected. The
`oxlint-tsrx` package pins `oxlint-tsgolint` 0.24.0 exactly. Missing,
unverifiable, or mismatched executables fail the command without silently
falling back to syntax-only lint.

## Proven compatibility

Retained tests directly exercise:

- Vite 8.1.5 production build with published
  `@tsrx/vite-plugin-react` 0.0.72;
- a real Vite dev server, filesystem watcher, framework recompilation, module
  invalidation, and emitted update/full-reload HMR payload;
- Vite+ 0.1.24, the supported minimum;
- Vite+ 0.2.4, current when this matrix was frozen;
- literal `vp build` under both versions, with compiled output and no retained
  TSRX control syntax;
- literal `vp dev` under both versions, with the served compiled module and a
  changed-source retransform observed directly;
- mixed `.tsx`/`.tsrx` `vp lint`, `vp fmt --check`, and convergent
  `vp check --fix`;
- imported object `extends`, TSRX/TSX-specific relative overrides, and rooted
  ignores under both Vite+ versions in a physical consumer package layout;
- `options.typeAware` auto-opt-in from a resolved Vite+ config in a physical
  `oxlint-tsrx` consumer layout, with a real mapped
  `typescript/no-floating-promises` diagnostic and one type process; and
- canonical root API behavior, missing-native failure, original TSRX fixes,
  and one-parse metadata.

Vite+ 0.1.20 is retained only as a disposable legacy compatibility control for
the read-only Markless oracle. Its published package has security advisories
fixed in 0.1.24 and later, so it is absent from the root and release dependency
graphs. The supported clean-install report is
`tests/packaging/vite-plus-matrix-report.json`; an unpublished next version is
advisory-only until its exact package can pass the same gate.

## Performance

The latest retained report is
`benchmarks/vite/results-1784242073158.json` on Apple M5 Pro:

| Boundary | p95 | Canonical ratio | Budget |
| --- | ---: | ---: | ---: |
| Mixed companion lint | 61.18 ms | 1.868× | ≤150 ms and ≤2.5× |
| Mixed companion format-check | 137.89 ms | 1.331× | ≤220 ms and ≤2.0× |
| Vite+ 0.2.4 mixed lint | 322.65 ms | n/a | ≤750 ms |

These are fresh-process ecosystem boundaries. Native throughput, allocation,
RSS, and cold-start gates remain independently enforced by the native lint and
format benchmarks. Vite runtime compilation has zero OXC for TSRX transforms
or parses, so framework build and HMR performance is not taxed by this package.

## Still pending

The host platform package, installed host VSIX, supported Vite+ matrix, clean
consumer, and complete clean-room acceptance run are directly proven. Static
artifact contracts cover the eight-target manifest; hosted production and
execution of all eight candidates remains a post-push release gate.
Registry/Marketplace publication remains a separate approval-gated external
action. JavaScript Oxlint plugins remain blocked on a stable public one-parse
host API and are not claimed.
