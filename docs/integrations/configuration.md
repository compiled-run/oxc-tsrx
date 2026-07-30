# Oxlint and Oxfmt configuration

Your existing `.oxlintrc.json` and `.oxfmtrc.json` work here unchanged, and the
settings inside them mean what they always meant. Oxlint's
[config reference](https://oxc.rs/docs/guide/usage/linter/config.html) and
Oxfmt's [config reference](https://oxc.rs/docs/guide/usage/formatter/config.html)
are still the documentation for them, and this page does not repeat either one.

What this page covers is the part that is specific to `.tsrx`: how your settings
reach a file this toolchain added, and [what is refused](#what-is-refused). Your
`.js`, `.jsx`, `.ts`, and `.tsx` files behave exactly as they do under official
OXC.

Step through the diagram to see where a config is found, and hover any
highlighted field name in the examples to see how it is handled.

<!-- diagram:config-resolution -->

## Lint configuration

`oxc-tsrx` searches upward from the working directory for one `.oxlintrc.json`
or `.oxlintrc.jsonc`, or takes an explicit JSON/JSONC file with `--config` or
`-c`. That one config covers every file in the run, and it is read once. A
separate config per directory is [not supported yet](#what-is-refused).

`rules`, `plugins`, `env`, `globals`, `settings`, `extends`, and
`ignorePatterns` are resolved by Oxlint itself, so they behave on `.tsrx` as they
do anywhere else. Three settings need more than that, and have their own
sections below: [`options.typeAware` and
`options.typeCheck`](#type-aware-linting), and
[`jsPlugins`](#jsplugins-and-the-two-lanes).

`overrides` is the one to know about. Its globs are matched against the path you
wrote, before any `.tsrx` file is turned into a TSX copy, so a `**/*.tsrx`
override applies to exactly the files you would expect.

Command-line flags win over the config, as usual: `--allow`, `--warn`, and
`--deny` beat configured severities, and `--format=json` reports at your original
byte offsets. The [CLI reference](/reference/cli) has the full list.

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
`config/lint.json`) is that example plus the type-aware additions from the next
section, and `src/View.tsrx` has a console call, a debugger statement, a wrong
type annotation, and an unawaited call into `src/service.tsrx`.

<!-- terminal-demo:configuration-lint -->

### Type-aware linting

Setting `options.typeAware` or `options.typeCheck` in the config is not enough on
its own. You also pass `--type-aware` or `--type-check` on the command line, so
that no run starts a type checker you did not ask for. `--type-check` does
everything `--type-aware` does, and also reports TypeScript's own errors. In a
Vite+ project the flag is passed for you.

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

On a `.tsrx` file, the type checker is handed a temporary in-memory TSX copy and
every result is mapped back onto the bytes you wrote. Nothing is written to disk,
and one type-checker process covers the whole run.

- Your `rules` and `overrides` are matched against your `.tsrx` paths, before
  that copy exists, so `**/*.tsrx` overrides keep working.
- A fix is applied only when the text it changes appears exactly as-is in your
  file and the result still parses as valid TSRX
  ([the full contract](/architecture/rust-oxc-core#opt-in-type-aware-lint-path)).
- TypeScript's own errors from `--type-check` use the same JSON shape as lint
  diagnostics. They have no rule name, so `rule` reads `parse-error` and `code`
  carries the compiler code, such as `typescript(TS2322)`.

#### Troubleshooting tsgolint discovery

Type-aware runs need exactly `oxlint-tsgolint` 0.24.0, pinned through
`oxc-tsrx`'s lint implementation dependency. When `--type-aware` or
`--type-check` fails to start:

- Native discovery checks the project installation and `PATH`.
- `OXLINT_TSGOLINT_PATH` names an executable or its directory explicitly.
- A standalone executable with no package metadata also needs
  `OXC_TSRX_TSGOLINT_VERSION=0.24.0`.
- A missing, unverifiable, or version-mismatched binary exits 2 rather than
  quietly dropping the type rules or writing source.

### `jsPlugins` and the two lanes

Your own plugins run on `.tsrx`, but only from two of the three commands:

| Command | `jsPlugins` on a `.tsrx` file |
| --- | --- |
| `oxlint`, and the language server | Runs them. |
| `vp lint` | Runs them, and reaches this package through `oxlint`. Declare them in [`vite.config.ts`](/guide/getting-started#if-something-goes-wrong). |
| `oxc-tsrx-lint`, the standalone binary | Refuses, and exits 2 naming `oxlint` as the command that can. |

- **How they run.** Your plugins see a legal-TSX copy of the file, with your own
  config, and every diagnostic is mapped back onto the bytes you wrote. Ordinary
  files are untouched and go straight to official Oxlint.
- **Why the standalone binary cannot.** It is a Rust process with no Node.js
  runtime, so there is nowhere to run your module.
- **What it costs.** One extra parse per linted `.tsrx` file, and it is never
  silent: `oxlint` prints a line to stderr ahead of the report and repeats it in
  `--format=json` under `oxcTsrx.jsPluginProjection`, and the language server
  writes the same fact to its log once per session.
- **How to turn it off.** Set `settings.oxcTsrx.jsPluginsOnTsrx` to `false`. Your
  plugins keep running on ordinary files, and `.tsrx` gets the same refusal
  instead of quietly fewer rules.
- **What your rules see.** The copy: `context.filename` points at it, `@if` and
  `@for` arrive already compiled, and a diagnostic landing on text only the copy
  has is dropped. [Custom JavaScript
  plugins](/integrations/custom-js-plugins#what-your-rule-sees-on-tsrx) has the
  details.

## Format configuration

`oxc-tsrx-fmt` searches upward for one `.oxfmtrc.json` or `.oxfmtrc.jsonc`, or
takes any JSON/JSONC config with `--config` or `-c`. It works out your options,
`overrides`, and ignored paths once, then uses them for stdin and for every file
you pass.

Every Oxfmt layout option applies to `.tsrx`: quotes, semicolons, print width,
trailing commas, bracket spacing, `singleAttributePerLine`, and the rest, plus
`overrides` and `ignorePatterns`. The exceptions are [refused
outright](#what-is-refused) rather than ignored.

A `.tsrx` file is formatted as a temporary TSX view with those options, then the
TSRX syntax is restored and the result is checked. [Multi-file
writes](/guide/formatting) and byte-for-byte `<style>` contents work the same
with a config file as without one.

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

In the demo below, that format configuration is the discovered `.oxfmtrc.json`
and also `config/format.json`; both sample files still use double quotes, so
`--check` lists them. In the stdin output, the `@{ }` statement container and the
`;` before `<section>` are intentional TSRX syntax, not formatter damage.

<!-- terminal-demo:configuration-format -->

## What is refused

Everything below stops the command with an error before anything is parsed,
printed, or written, rather than being turned off behind your back.

Linting refuses:

- a config written as a JavaScript or TypeScript module, when you call the
  standalone binary;
- output formats other than `default`, `agent`, `github`, and `json`;
- a separate config per directory.

Formatting refuses:

- a config written as a JavaScript or TypeScript module, and `.editorconfig`;
- `sortImports`, `sortTailwindcss`, `jsdoc`, and `embeddedLanguageFormatting`
  when they are switched on;
- experimental options, and unknown keys that would affect `.tsrx`.

The standalone binaries are deliberately small: they take named files and print
JSON, and nothing else. Directories, globs, the ordinary report format, and
handing your JS and TS files to official OXC all come from the `oxc-tsrx` npm
commands.

Two things are ignored rather than refused, because neither can change your
`.tsrx` output: Oxfmt options for other languages, such as package-JSON or prose
formatting, and CSS inside `<style>`, which is kept exactly as you wrote it.
[Limitations](/reference/limitations) tracks everything still missing.

## Performance evidence

The retained release reports pin one config load per session, unchanged parse
counts and thresholds, and one tsgolint process per eligible type-aware batch.
[Benchmarks](/reference/benchmarks) has the methodology, the full gate matrix,
and the reports it selects.
