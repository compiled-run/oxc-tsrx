# Vite and Vite+ integration

## Install

You need Node.js 20.19 or newer. Install Vite+ plus the one public TSRX
toolchain:

<!-- pm-install -->
```sh
npm install --save-dev vite-plus oxc-tsrx
```

That install is everything a host needs in order to *discover* TSRX.
`oxc-tsrx` declares a static `oxc.provider` block in its own `package.json`, and
a host that performs provider discovery reads that JSON to find which files the
package owns (`.tsrx`) plus the parser, linter, formatter, and language server
to use for them. No alias, no `overrides` block, no activation command, and
nothing written into `node_modules` afterwards. Run
`npx oxc-tsrx providers --json` to see the index; it writes nothing.

**Vite+ is not one of those hosts yet**, so on this page the install is not the
whole story. Keep reading.

### The one extra step Vite+ needs

Released Vite+ 0.2.4 finds its lint and format tools through project-local
packages named literally `oxlint` and `oxfmt`
(`join(dirname(dirname(resolve("oxlint"))), "bin", "oxlint")`). It reads no
`oxc.provider` metadata. Activate `oxc-tsrx`'s qualified, reversible
project-local facades after installation:

```sh
npx oxc-tsrx setup
```

The command never edits `package.json`, never runs as an install lifecycle
script, refuses direct or unrecognized package collisions, and is idempotent.
Use `npx oxc-tsrx status` to inspect it and `npx oxc-tsrx remove` to restore
transitive official packages. Run `setup` again after a clean dependency
install.

**This step is permanent.** It is not a shim waiting to be deleted, and it is
not something a future `oxc-tsrx` release removes. Two facts about Vite+ make it
structural:

- Vite+ resolves a *package* named `oxlint`, the same way `import "oxlint"`
  resolves. A bin name cannot answer a package resolution, and `oxc-tsrx`
  cannot legitimately publish a package under that name.
- Vite+ pins its own `oxlint@=1.72.0` dependency, so that package slot is
  already filled at an exact version. Only a project-local package sitting in
  that slot changes what Vite+ resolves, and writing that slot is precisely
  what `setup` does.

`oxc.provider` does not change this either. It is a protocol *proposed* to OXC:
nothing has been submitted upstream, nothing has been accepted, and upstream
patching is not part of this project's plan. So no released Vite+ reads it, and
none is going to.

Everywhere else, an install is the whole story. Vite+ is the single integration
where it is not.

## Quick start

With the install and that setup step done, the ordinary Vite+ commands handle
`.tsrx` files natively:

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

The integration uses Vite+'s public project-local tool resolution. The explicit
setup command provides exact `oxlint` and `oxfmt` facades backed by
`oxc-tsrx`; those facades keep the canonical root APIs and expected
`dist/index.js` plus `bin/*` layout. Vite+ resolves them without a fork or
private imports.

Selection by literal name has real downsides. It depends on install layout and
name ownership rather than on a declared capability, and two tools cannot own
one name. `oxc.provider` is the shape this project would prefer instead, and it
is recorded as a proposal for that reason.

It is a proposal only. No released host reads it, upstream patching is not part
of this project's plan, and so the two mechanisms never overlap in practice: the
facades are how Vite+ reaches TSRX, and the provider block is read only by the
hosts inside this repository. Neither the facades nor the `oxlint`/`oxfmt` names
are ever provider capability targets.

## Consumer shape

The distributable platform layout is complete. Once the approval-gated npm
release exists, the consumer manifest is:

```json
{
  "devDependencies": {
    "vite-plus": "0.2.4",
    "oxc-tsrx": "^0.1.0"
  }
}
```

The setup command does not add dependencies to this manifest. `oxc-tsrx` lists
the eight `@oxc-tsrx/native-*` platform packages as `optionalDependencies` and
resolves the matching one itself, so you never name a platform package. During
source development, `OXC_TSRX_LINT_BIN` and
`OXC_TSRX_FORMAT_BIN` select release binaries explicitly. A missing native
artifact is an error; `.tsrx` is never silently delegated to stock tools.

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

## Optional parser-aware Vite plugins (source-local example)

This is separate from everything above, and it is separate from the Vite+
`lint` config too. It has nothing to do with `vp lint` or `jsPlugins`. It is
just a pattern for letting one of your own Vite plugins read the authored
`.tsrx` AST.

Vite does not let a plugin replace Rolldown's parser, but a pre-transform
plugin can inspect raw `.tsrx` before the framework compiler runs. The
in-repo example `examples/custom-js-plugins/tsrx-parser-service.mjs` composes
around the framework plugin, parses each raw file once, caches it, and passes
that AST to parser-aware consumers through a closure:

```js
plugins: [
  withTsrxParser(tsrxReact(), (parser) => tsrxDemoLint(parser)),
]
```

Two things to be clear about, because it is easy to assume this is more
finished than it is:

- **It is a source-local proof, not an installable feature.** In the example,
  the service imports the parser with a relative path
  (`../../packages/toolchain/dist/parser.js`) rather than from
  `oxc-tsrx/parser`. That file is the same module an install reaches through the
  `oxc-tsrx/parser` subpath, so the parser itself is public; what is not public
  is the glue. `withTsrxParser` is not exported by the `oxc-tsrx` package.
- **The `parser` argument comes from a closure the helper creates**, not from
  any Vite parser lifecycle. Vite does not hand plugins a parser.

The framework plugin still owns compilation, CSS, maps, and HMR, and Rolldown
still parses only the generated JavaScript. A real Vite build proves the
ordering and the authored custom-node observation. See
[Custom JavaScript plugins](/integrations/custom-js-plugins) for the full
example, the publish gap, and the separate Oxlint host boundary.

## Configuration boundary

When Vite+ passes `vite.config.*` as the tool config:

- The toolchain's Node boundary resolves it once through Vite+'s public
  `resolveConfig`, extracts only the `lint` or `fmt` field, and hands the
  native process a disposable JSON file. The file is removed after the
  batch; nothing is added to your project.
- Relative paths in object `extends`, override globs, and `ignorePatterns`
  resolve from where you wrote them.
- Non-serializable values (callback functions, `jsPlugins`) fail with an
  error instead of being dropped. An explicit JSON/JSONC `--config` keeps
  the direct native configuration path.

For type-aware lint, the toolchain reads the resolved
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

Read the scope carefully, because it is narrower than it might sound. The
end-to-end Vite+ command matrix runs on **npm only**. It does not claim pnpm,
Yarn, or Bun for these `vp` commands. Retained tests exercise:

- a real Vite 8.1.5 production build and dev server (filesystem watcher,
  framework recompilation, module invalidation, and emitted HMR payloads)
  with the published `@tsrx/vite-plugin-react` 0.0.72;
- literal `vp build`, `vp dev`, mixed `vp lint`, `vp fmt --check`, and
  convergent `vp check --fix`, run with npm, on the tested minimum Vite+
  0.1.24 and the pinned current Vite+ 0.2.4; and
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
| Ordinary toolchain format-check | 113.44 ms | 1.101× | ≤150 ms and ≤1.25× |
| Mixed toolchain lint | 57.91 ms | 1.813× | ≤150 ms and ≤2.5× |
| Mixed toolchain format-check | 127.11 ms | 1.234× | ≤220 ms and ≤2.0× |
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

Provider discovery is not pending for this page, though, because it was never
going to serve it. No released OXC, Oxlint, Oxfmt, Vite+, or `oxc.oxc-vscode`
build reads `oxc.provider` metadata, and nothing is being submitted upstream to
change that. The reference implementation in this repository discovers
providers; the protocol only records what a host could do.

So the setup step above is not waiting on anything. It is the permanent shape of
the Vite+ integration.
