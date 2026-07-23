# Vite and Vite+ integration

## Install

You need Node.js 20.19 or newer. Vite+ finds its lint and format tools by
looking for project-local packages named `oxlint` and `oxfmt`, so install the
TSRX-aware packages under those names using npm aliases:

<!-- pm-install -->
```sh
npm install --save-dev vite-plus \
  oxlint@npm:oxlint-tsrx \
  oxfmt@npm:oxfmt-tsrx
```

The alias syntax (`oxlint@npm:oxlint-tsrx`) installs the `oxlint-tsrx`
package under the folder name `oxlint`. Vite+ needs no other configuration
to find it.

## Quick start

Once installed, the ordinary Vite+ commands handle `.tsrx` files natively:

```sh
npx vp lint          # lint every file, .tsrx included
npx vp fmt --check   # list files the formatter would change
npx vp check --fix   # lint and format together, applying safe fixes
```

`vp lint` and `vp fmt` route ordinary `.js`/`.jsx`/`.ts`/`.tsx` files to
canonical Oxlint and Oxfmt, and `.tsrx` files to the native TSRX support.
Diagnostics point at the lines you wrote, and `vp build` / `vp dev` are
untouched because linting and formatting sit outside the build pipeline.

## Architecture

OXC for TSRX does not compile application modules for Vite. Your framework's
official TSRX Vite plugin (for example `@tsrx/vite-plugin-react` or
Markless's plugin) still owns runtime semantics, CSS extraction, source maps,
and HMR, so `vp build` and `vp dev` flow through that plugin into
Vite/Rolldown exactly as before.

The integration instead uses Vite+'s public project-local tool resolution:
when Vite+ looks up the `oxlint` and `oxfmt` packages, it finds `oxlint-tsrx`
and `oxfmt-tsrx` under those alias names. Both packages keep the canonical
package root APIs and the expected `dist/index.js` plus `bin/*` layout, so
Vite+ resolves them without patches, and they never import Vite+ internals.

## Consumer shape

The distributable platform layout is complete. Once the approval-gated npm
release exists, the consumer manifest is:

```json
{
  "devDependencies": {
    "vite-plus": "0.2.4",
    "oxlint": "npm:oxlint-tsrx@^0.1.0",
    "oxfmt": "npm:oxfmt-tsrx@^0.1.0"
  }
}
```

`@oxc-tsrx/runtime` picks the matching native binary package for your
operating system automatically; you never name a platform package. During
source development, `OXC_TSRX_LINT_BIN` and `OXC_TSRX_FORMAT_BIN` select
release binaries explicitly. A missing native artifact is an error; `.tsrx`
is never silently delegated to stock tools.

Use the framework plugin exactly as its framework documents. No OXC for TSRX
Vite plugin is required for compilation:

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

## Optional parser-aware Vite plugins

Vite does not expose a public custom-parser replacement for Rolldown, but a
pre-transform plugin can inspect raw `.tsrx` before the framework compiler.
The retained `examples/custom-js-plugins/tsrx-parser-service.mjs` composes
around the existing framework plugin, caches one authored
`@oxc-tsrx/parser` result, and exposes it to parser-aware consumers:

```js
plugins: [
  withTsrxParser(tsrxReact(), (parser) => tsrxDemoLint(parser)),
]
```

The framework plugin still owns compilation, CSS, maps, and HMR. Rolldown
still parses only the generated JavaScript. A real Vite build proves this
ordering and the authored custom-node observation; see
[Custom JavaScript plugins](/integrations/custom-js-plugins) for the complete
example and the separate Oxlint host boundary.

## Configuration boundary

When Vite+ passes `vite.config.*` as the tool config:

- The Node companion resolves it once through Vite+'s public
  `resolveConfig`, extracts only the `lint` or `fmt` field, and hands the
  native process a disposable JSON file. The file is removed after the
  batch; nothing is added to your project.
- Relative paths in object `extends`, override globs, and `ignorePatterns`
  resolve from where you wrote them.
- Non-serializable values (callback functions, `jsPlugins`) fail with an
  error instead of being dropped. An explicit JSON/JSONC `--config` keeps
  the direct native configuration path.

For type-aware lint, the companion reads the resolved
`lint.options.typeAware` and `lint.options.typeCheck` values and adds
`--type-aware` or `--type-check` to the native `.tsrx` batch automatically.
Running the Rust binary directly still requires one of those flags, so
config alone can never start a type process. `typeCheck` implies the
type-aware lane and adds TypeScript syntactic and semantic diagnostics.

All discovered `.tsrx` files for a command share one `oxlint-tsgolint`
process; canonical Oxlint still handles ordinary files in its own lane. A
missing or mismatched executable fails the command instead of silently
falling back to syntax-only lint; see
[type-aware linting](/integrations/configuration#type-aware-linting).

## Proven compatibility

Retained tests exercise:

- a real Vite 8.1.5 production build and dev server (filesystem watcher,
  framework recompilation, module invalidation, and emitted HMR payloads)
  with the published `@tsrx/vite-plugin-react` 0.0.72;
- literal `vp build`, `vp dev`, mixed `vp lint`, `vp fmt --check`, and
  convergent `vp check --fix` under both the supported minimum Vite+
  release and the release current when this matrix was frozen; and
- imported object `extends`, TSRX-specific overrides, rooted ignores, and
  the `options.typeAware` auto-opt-in with a real mapped
  `typescript/no-floating-promises` diagnostic.

An older Vite+ version is retained only as a disposable legacy control for
the read-only Markless oracle and is absent from the root and release
dependency graphs. The supported clean-install report is
`tests/packaging/vite-plus-matrix-report.json`.

## Performance

The aggregate-selected report is
`benchmarks/vite/results-1784321678410.json` on Apple M5 Pro. The ordinary
lane imports the exact manifest-declared Oxfmt launcher in the same Node
process; trace evidence records zero TSRX dispatch:

| Boundary | p95 | Canonical ratio | Budget |
| --- | ---: | ---: | ---: |
| Ordinary companion format-check | 113.44 ms | 1.101× | ≤150 ms and ≤1.25× |
| Mixed companion lint | 57.91 ms | 1.813× | ≤150 ms and ≤2.5× |
| Mixed companion format-check | 127.11 ms | 1.234× | ≤220 ms and ≤2.0× |
| Vite+ 0.2.4 mixed lint | 237.08 ms | n/a | ≤750 ms |

These are fresh-process ecosystem boundaries. Native throughput, allocation,
RSS, and cold-start gates remain independently enforced by the native lint
and format benchmarks. Vite runtime compilation has zero OXC for TSRX
transforms or parses, so framework build and HMR performance is not taxed by
this package.

## Still pending

Everything above is proven locally. Hosted production of all eight release
candidates remains a post-push release gate, registry and Marketplace
publication remain separate approval-gated actions, and JavaScript Oxlint
plugins stay blocked on a released custom-parser or language-plugin host API.
