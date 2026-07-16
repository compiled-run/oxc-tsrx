# Oxlint and Oxfmt configuration

OXC for TSRX uses the public configuration and option types from its exact
canonical OXC revision. Configuration is discovered and compiled once when a
native session starts, then reused for every `.tsrx` and ordinary JS/TS file in
that batch. Ordinary `.js`, `.jsx`, `.ts`, and `.tsx` all stay on the direct
canonical path. Configuration is not read, merged, or compiled in the per-file scan/project/
parse path.

This is configuration compatibility through the project-owned native commands.
Stock `oxlint` and `oxfmt` still do not recognize `.tsrx`.

The thin npm companions add one ecosystem-only boundary: when Vite+ supplies a
`vite.config.*`, they resolve it once through Vite+'s public API, extract the
serializable `lint` or `fmt` field, and materialize disposable JSON for the
native TSRX process with the authored config directory carried separately.
Canonical Oxlint/Oxfmt shares that JSON when no path-sensitive field is present;
otherwise it loads the authored Vite config so relative semantics remain exact.
Direct invocation of the Rust binaries continues to reject JavaScript/TypeScript
config modules.

## Lint configuration

`oxc-tsrx` searches from the working directory toward the filesystem root for
one `.oxlintrc.json` or `.oxlintrc.jsonc`. Pass an arbitrary JSON/JSONC file
with `--config` or `-c` to bypass discovery.

The current native boundary supports:

- configured built-in rules and built-in plugins, including rule options;
- `env`, `globals`, and built-in-plugin `settings` through canonical
  `ConfigStoreBuilder`;
- JSON/JSONC `extends`, resolved relative to the declaring config;
- Vite+ object `extends` materialized through canonical OXC's public
  `extends_configs` model;
- per-file `overrides` for `.tsrx` and ordinary JS/JSX/TS/TSX paths;
- `ignorePatterns` rooted at the config directory;
- `options.denyWarnings` and `options.maxWarnings` exit policy;
- `--allow`/`-A`, `--warn`/`-W`, and `--deny`/`-D` CLI precedence;
- explicit multi-file batches and identity-mapped `--fix`;
- JSON diagnostic output at original TSRX byte spans;
- opt-in official tsgolint rules through `--type-aware`; and
- opt-in TypeScript syntactic and semantic diagnostics through `--type-check`.

For example:

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

```sh
target/release/oxc-tsrx --format=json src/View.tsrx src/View.tsx
target/release/oxc-tsrx --format=json --config config/lint.json \
  --warn no-console --deny no-debugger src/View.tsrx
target/release/oxc-tsrx --format=json --type-aware \
  src/View.tsrx src/service.tsrx
target/release/oxc-tsrx --format=json --type-check src/View.tsrx
```

### Type-aware linting

The direct native command requires `--type-aware` or `--type-check` even when
the config contains `options.typeAware` or `options.typeCheck`. Config alone
fails actionably instead of unexpectedly starting a TypeScript-Go process.
`--type-check` implies the type-aware lane and additionally reports TypeScript
syntactic and semantic diagnostics. A resolved Vite+ config is different only
at the thin npm boundary: `oxlint-tsrx` sees those option fields and forwards
the corresponding explicit flag automatically.

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

Before any virtual filename is created, OXC resolves rules, severities, and
overrides against each authored path. A `.tsrx` source is then projected to an
in-memory `.tsrx.tsx` source override; explicit `.tsrx` imports keep working,
and no generated source is written to disk. Ordinary TS/TSX can participate in
the same explicit native project batch without entering the TSRX scanner. An
eligible multi-file batch uses one tsgolint process, not one process per file.
The normal syntax-only lane still performs one OXC parse per file and starts
zero type processes.

Type-aware diagnostic labels map back to authored UTF-8 TSRX byte spans. A
reported edit is eligible for `--fix` only when its complete range has one
exact identity mapping to authored source. Semantic suggestions and edits that
touch synthetic or multi-segment projection text are rejected, and every
accepted edit must survive the existing TSRX validation reparse before a
source write.

The supported protocol executable is exactly `oxlint-tsgolint` 0.24.0, which
is an exact dependency of `oxlint-tsrx`. Native discovery checks the project
installation and `PATH`; `OXLINT_TSGOLINT_PATH` can select an executable or its
directory explicitly. A standalone executable without package metadata also
requires `OXC_TSRX_TSGOLINT_VERSION=0.24.0`. A missing binary, unverifiable
binary, or version mismatch fails the command without silently downgrading or
writing source; the native CLI reports these as tool errors with exit status 2.

JavaScript plugins (`jsPlugins`) and direct-native JavaScript/TypeScript config
modules still fail before a source parse or write. They are not silently
disabled. The official JS-plugin host currently lives behind private
generated/raw-transfer application code, which this project does not import or
copy.

The direct native command currently accepts explicit source files and JSON
output. The npm companion adds Vite+'s directory/glob command shape, combined
default/JSON reporting, and ordinary-file delegation. Alternate reporters,
nested per-directory config discovery, and JavaScript plugin hosting remain
separate compatibility work.

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
options. TSRX uses the same options for its one projected Oxfmt parse, then the
checked lift restores TSRX syntax. Transactional multi-file writes and raw
`<style>` payload byte preservation are unchanged.

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

```sh
target/release/oxc-tsrx-fmt --check src/View.tsrx src/View.tsx
target/release/oxc-tsrx-fmt --write --config config/format.json src/View.tsrx
target/release/oxc-tsrx-fmt --stdin-filepath=src/View.tsrx < src/View.tsrx
```

Enabled `sortImports`, `sortTailwindcss`, `jsdoc`,
`embeddedLanguageFormatting`, experimental formatter options, unknown
TSRX-affecting keys, `.editorconfig`, and JavaScript/TypeScript config modules
fail before output or writes. Oxfmt options that affect only non-JS languages,
such as package-JSON or prose formatting, do not change `.tsrx` output. Raw CSS
is preserved, not formatted or validated.

Serializable Vite+ `fmt` fields are supported by the npm companion. Callback
functions and other non-JSON values fail actionably instead of disappearing
during serialization.

## Performance evidence

The retained release reports include dedicated configuration invariants:

- lint: one config load, one parse for the configured `.tsrx` file, and an
  observed configured `no-debugger` diagnostic;
- format: one config load, two parses for two configured files, and observed
  quote/semicolon option changes;
- type-aware: one tsgolint process for both a single-file and a two-file
  explicit-`.tsrx` import batch, with the default lane still at one OXC parse
  and zero type processes; and
- every prior throughput, latency, scaling, cold-start, and RSS threshold is
  unchanged.

See `benchmarks/native-lint/results-1784242044684.json` and
`benchmarks/native-format/results-1784242059253.json`. The ecosystem companion
boundary is independently retained in
`benchmarks/vite/results-1784242073158.json`. The opt-in TypeScript-Go boundary
is retained in `benchmarks/type-aware/results-1784242060765.json`: 23.69 ms
median / 25.04 ms p95 for one TSRX file and 23.78 ms median / 24.46 ms p95 for
a two-file project, while the default syntax lane remains at 2.62 ms p95.
