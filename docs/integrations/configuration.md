# Oxlint and Oxfmt configuration

If you already have an `.oxlintrc.json` or `.oxfmtrc.json`, it works here
unchanged. This page is the exact support matrix: which fields the native
commands honor, how a Vite+ config participates, and what fails loudly instead
of being silently ignored.

Configuration is loaded once when a
[native session](/architecture/rust-oxc-core#configured-session-layer)
starts and reused for every file in that batch. Ordinary `.js`, `.jsx`,
`.ts`, and `.tsx` files stay on the direct canonical path. Stock `oxlint`
and `oxfmt` still do not recognize `.tsrx`.

Installed through npm? You run the `oxlint` and `oxfmt` commands from
[`oxc-tsrx`](/integrations/vite-plus), and everything on
this page applies to them too. They can also take `lint` and `fmt` options
from a Vite+ `vite.config.*`; that contract is the
[Vite+ configuration boundary](/integrations/vite-plus#configuration-boundary).
The examples below invoke the native binaries directly because this page
documents the native boundary.

This is the whole resolution story in one picture. Select a node or step
through the buttons:

<!-- diagram:config-resolution -->

The config examples on this page are annotated: hover any highlighted field
name to read what the native boundary does with it.

## Lint configuration

`oxc-tsrx` searches upward from the working directory for one
`.oxlintrc.json` or `.oxlintrc.jsonc`, or takes an explicit JSON/JSONC file
with `--config` or `-c`. One config covers the whole batch; nested
per-directory discovery is [not implemented yet](#not-yet-supported).

The native boundary supports the following.

Config file fields:

- configured built-in rules and built-in plugins, including rule options;
- `env`, `globals`, and built-in-plugin `settings` through canonical
  `ConfigStoreBuilder`;
- JSON/JSONC `extends`, resolved relative to the declaring config;
- Vite+ object `extends` materialized through canonical OXC's public
  `extends_configs` model;
- per-file `overrides` for `.tsrx` and ordinary JS/JSX/TS/TSX paths; and
- `ignorePatterns` rooted at the config directory.

CLI flags and exit policy:

- `--allow`/`-A`, `--warn`/`-W`, and `--deny`/`-D` precedence over configured
  severities; and
- `options.denyWarnings` and `options.maxWarnings` exit policy.

Output and fixes:

- explicit multi-file batches and identity-mapped `--fix`; and
- JSON diagnostic output at original TSRX byte spans.

Opt-in type lanes:

- official tsgolint rules through `--type-aware`; and
- TypeScript syntactic and semantic diagnostics through `--type-check`.

For example:

<!-- annotate-config -->

```jsonc
{
  "plugins": ["react"],
  "env": { "browser": true },
  "globals": { "frameworkGlobal": "readonly" },
  "rules": {
    "no-debugger": "error",
    "eqeqeq": ["error", "always"],
    "react/jsx-no-undef": "error"
  },
  "overrides": [
    {
      "files": ["**/*.tsrx"],
      "rules": { "no-console": "warn" }
    }
  ],
  "ignorePatterns": ["generated/**"]
}
```

In the demo below, the discovered `.oxlintrc.json` (also passed as
`config/lint.json`) is this example plus the type-aware additions described in
the next section. `src/View.tsrx` has a console call, a debugger statement, a
wrong type annotation, and an unawaited call into `src/service.tsrx`.

<!-- terminal-demo:configuration-lint -->

### Type-aware linting

The direct native command requires `--type-aware` or `--type-check` even when
the config contains `options.typeAware` or `options.typeCheck`. Config alone
fails actionably instead of unexpectedly starting a TypeScript-Go process.
`--type-check` implies the type-aware lane and additionally reports TypeScript
syntactic and semantic diagnostics. Through the npm toolchain, a resolved
Vite+ config forwards the corresponding explicit flag automatically.

<!-- annotate-config -->

```jsonc
{
  "plugins": ["typescript"],
  "rules": {
    "typescript/no-floating-promises": "off"
  },
  "overrides": [
    {
      "files": ["**/*.tsrx"],
      "rules": {
        "typescript/no-floating-promises": "error"
      }
    }
  ],
  "options": {
    "typeAware": true,
    "typeCheck": false
  }
}
```

In plain terms:

- The tool hands the type checker a temporary in-memory TSX copy of each
  `.tsrx` file, then maps every result back onto the bytes you actually
  wrote. Nothing is written to disk, and one tsgolint process covers the
  whole batch.
- Rules and overrides are resolved against your authored `.tsrx` paths
  before that copy exists, so `**/*.tsrx` overrides keep working.
- A fix is only applied when the affected text exists verbatim in your
  authored file and the result reparses as valid TSRX.

The precise projection and fix-eligibility contract lives in
[Architecture](/architecture/rust-oxc-core#opt-in-type-aware-lint-path).

Compiler diagnostics from `--type-check` use the same JSON shape as lint
diagnostics. A TypeScript compiler message has no lint rule name, so its
`rule` field falls back to `parse-error` while `code` carries the real
compiler code, such as `typescript(TS2322)`.

#### Troubleshooting tsgolint discovery

Type-aware runs need exactly `oxlint-tsgolint` 0.24.0, pinned through
`oxc-tsrx`'s lint implementation dependency. When `--type-aware` or
`--type-check` fails to
start:

- Native discovery checks the project installation and `PATH`.
- `OXLINT_TSGOLINT_PATH` selects an executable or its directory explicitly.
- A standalone executable without package metadata also requires
  `OXC_TSRX_TSGOLINT_VERSION=0.24.0`.
- A missing binary, unverifiable binary, or version mismatch fails the
  command with exit status 2 instead of silently downgrading or writing
  source.

### Not yet supported

These fail before a source parse or write; they are not silently disabled:

- JavaScript plugins (`jsPlugins`) on the `.tsrx` lane. This is a scoping
  limit, not a blanket "JS plugins never work" statement. Ordinary `.js`/`.ts`
  files can use released Oxlint JS plugins in the normal Oxlint lane (including
  through Vite+). What rejects `jsPlugins` is the native TSRX CLI and language
  server, and the serialized `.tsrx` Vite+ lane that forwards config to them,
  because that native Rust path has no Node JS-plugin host. The source-only VS
  Code experiment routes the official OXC extension through a Node-enabled
  Oxlint draft to run a JS rule on `.tsrx`; it does not make `jsPlugins` valid
  for the native CLI. See
  [Custom JavaScript plugins](/integrations/custom-js-plugins).
- JavaScript/TypeScript config modules passed to the direct native command.
- Alternate reporters and nested per-directory config discovery.

The direct native command accepts explicit source files and JSON output; the
npm toolchain adds directory/glob arguments, combined default/JSON reporting,
and ordinary-file delegation. [Limitations](/reference/limitations)
tracks every remaining boundary.

## Format configuration

`oxc-tsrx-fmt` searches upward for one `.oxfmtrc.json` or
`.oxfmtrc.jsonc`. Pass an arbitrary JSON/JSONC config with `--config` or `-c`.
One `FormatSession` resolves the base options, overrides, and ignore matcher and
reuses them for stdin or every explicit file.

Supported JS/TSX options are:

- `useTabs`, `tabWidth`, `endOfLine`, and `printWidth`;
- `singleQuote`, `jsxSingleQuote`, and `quoteProps`;
- `trailingComma`, `semi`, and `arrowParens`;
- `bracketSpacing`, `bracketSameLine`, and `objectWrap`;
- `singleAttributePerLine` and `htmlWhitespaceSensitivity`; and
- `insertFinalNewline`.

`overrides`, override `excludeFiles`, and `ignorePatterns` are supported.
Ordinary JS/TSX takes the canonical direct Oxfmt path with the same resolved
options. A `.tsrx` file uses the same options: the tool formats a temporary
TSX view of the file, then restores the TSRX syntax and verifies the result.
[Transactional multi-file writes](/guide/formatting) and raw `<style>`
payload byte preservation behave exactly as they do without a config file.

<!-- annotate-config -->

```jsonc
{
  "singleQuote": true,
  "semi": false,
  "printWidth": 100,
  "overrides": [
    {
      "files": ["**/*.tsrx"],
      "options": { "singleAttributePerLine": true }
    }
  ],
  "ignorePatterns": ["generated/**"]
}
```

In the demo below, the format configuration shown above is the discovered
`.oxfmtrc.json` and also `config/format.json`; both sample files still use
double quotes, so `--check` lists them. In the stdin output, the `@{ }`
statement container and the `;` before `<section>` are intentional TSRX
syntax, not formatter damage.

<!-- terminal-demo:configuration-format -->

### Not supported

Enabled `sortImports`, `sortTailwindcss`, `jsdoc`,
`embeddedLanguageFormatting`, experimental formatter options, unknown
TSRX-affecting keys, `.editorconfig`, and JavaScript/TypeScript config modules
fail before output or writes. Oxfmt options that affect only non-JS languages,
such as package-JSON or prose formatting, do not change `.tsrx` output. Raw CSS
is preserved, not formatted or validated.

## Performance evidence

The retained release reports pin dedicated configuration invariants: one
config load per session, unchanged parse counts and thresholds, and one
tsgolint process per eligible type-aware batch.
[Benchmarks](/reference/benchmarks) has the methodology, the full gate
matrix, and the aggregate-selected reports.
