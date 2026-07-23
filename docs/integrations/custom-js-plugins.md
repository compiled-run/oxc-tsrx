---
title: Custom JavaScript plugins
description: What works today for TSRX AST consumers in Vite and ESLint, and the exact upstream seam Oxlint still needs.
---

# Custom JavaScript plugins

There are four different integration boundaries, and they should not be
conflated:

| Host | Released status | Practical TSRX path |
| --- | --- | --- |
| Official OXC extension | Custom JS rules work against the upstream parser draft | Point `oxc.path.oxlint` at the TSRX LSP launcher; no companion extension required |
| OXC-for-TSRX companion | Native rules, formatting, and validated fixes work now | Optional native `oxc-tsrx-lsp` client |
| Vite 8 plugins | Raw-source transforms work now; no custom bundler-parser hook | Parse before the framework transform and share the authored AST with other plugins |
| Oxlint 1.74 JS plugins | JavaScript rules work, custom parsers/file formats do not | Target the upstream custom-parser draft; use ESLint as the executable proof meanwhile |

The runnable prototypes live in `examples/custom-js-plugins`, and their
retained tests use the real parser, ESLint 10, Vite 8.1.5, and
`@tsrx/vite-plugin-react` 0.0.72.

## See the custom JavaScript rule in VS Code

The custom-rule proof uses only the official OXC extension:

1. Open `examples/vscode-lints` as the VS Code workspace.
2. Install or enable `oxc.oxc-vscode`.
3. Open `oxlint-custom-parser.json` once to activate official OXC, then open
   `LintDemo.tsrx`.

`tsrx-demo(no-tsrx-if)` underlines the authored `@if … @else` block. The
committed parser adapter returns `JSXIfExpression`, and
`demo-lint-plugin.mjs` reports it. A workspace-local launcher forwards the
official extension to `target/oxlint-custom-parser/cli.js`, dynamically
registering `.tsrx` document sync and pull diagnostics.

The retained Extension Host test explicitly asserts that the companion
extension is absent, activates official OXC through its JSON config, checks
the diagnostic and authored range, applies an unsaved edit, and checks the
updated diagnostic. F5 from the repository root remains available when the
five native rules, formatting, and validated `no-var` fix are also wanted.

## Add the parser beside an existing Vite plugin

Vite's public API supports transforming custom file types, but it does not
let a plugin replace Rolldown's parser or return a caller-supplied AST.
`moduleParsed` also does not run during dev. The useful composition point is
therefore immediately before the framework plugin transforms raw `.tsrx`.

```js
import { defineConfig } from "vite";
import { tsrxReact } from "@tsrx/vite-plugin-react";
import { withTsrxParser } from "./tsrx-parser-service.mjs";
import { tsrxDemoLint } from "./vite-demo-lint.mjs";

export default defineConfig({
  plugins: [
    withTsrxParser(tsrxReact(), (parser) => tsrxDemoLint(parser)),
  ],
});
```

`withTsrxParser` returns a Vite plugin preset in this order:

1. a pre-transform service parses raw `.tsrx` and caches the
   `@oxc-tsrx/parser` result;
2. parser-aware consumers inspect the authored AST; and
3. the existing framework plugin compiles TSRX, owns CSS/HMR/source maps, and
   hands ordinary generated JavaScript to Vite/Rolldown.

The service does not modify the framework plugin and does not ask Rolldown to
understand TSRX nodes. A retained real build proves the raw file is parsed
once, a consumer sees `JSXIfExpression`, and the final bundle contains no
TSRX control syntax.

This service should become a small published helper only if more than one
consumer needs it. For a single project, the example is already a complete
Vite plugin.

## Adapt the parser to JavaScript lint rules

Oxlint's JS plugin API follows ESLint. The parser side of that ecosystem uses
`parseForESLint`, whose result needs:

- the authored `Program`;
- ranges and line/column locations;
- comments and tokens;
- visitor keys, including every custom TSRX node; and
- parser services plus correct scope semantics where framework syntax changes
  bindings.

`examples/custom-js-plugins/tsrx-eslint-parser.mjs` implements the useful
AST-only subset today. It calls `@oxc-tsrx/parser`, derives static-per-result
visitor keys, adds authored locations and comments, and lets the included
JavaScript plugin visit `JSXIfExpression` and `JSXForExpression`. ESLint 10 is
the executable proof of that contract.

Two production gaps remain:

1. `@oxc-tsrx/parser` v1 does not expose the OXC token stream, so
   `SourceCode` token APIs cannot be correct. The prototype returns an empty
   token list and labels itself AST-only.
2. Generic ESTree traversal reaches ordinary descendants, but complete
   framework-aware scope behavior needs an explicit scope contract for TSRX
   controls and bindings.

The OXC Rust parser already has an opt-in token collection mode. A production
adapter should transport authored TSRX tokens from the native parse instead
of retokenizing in JavaScript or returning projected placeholder tokens.

## What Oxlint needs

Released Oxlint documents custom parsers and file formats as unsupported.
Adding `jsPlugins` to the native TSRX config still cannot work:
`oxc-tsrx-lsp` is a separate in-process Rust host and does not embed Node.
The editor experiment instead points the official OXC extension at a
Node-enabled Oxlint draft through an LSP launcher. The launcher dynamically
registers `.tsrx` with VS Code, avoiding another editor extension.

Two active upstream designs provide the right seams:

- [custom JS parsers, draft PR #24262](https://github.com/oxc-project/oxc/pull/24262)
  adds ESLint-compatible `overrides[].languageOptions.parser`. This is the
  shortest route for AST-only TSRX JavaScript rules.
- [language plugins RFC #21936](https://github.com/oxc-project/oxc/discussions/21936)
  adds a first-class parse/load split, typed visitor schemas, transforms and
  mappings, parser services, caching, and a language identity. This is the
  stronger route for native rules, custom TSRX rules, and future type-aware
  behavior together.

If the custom-parser draft lands with its current contract, the prototype
configuration becomes:

```jsonc
{
  "overrides": [
    {
      "files": ["**/*.tsrx"],
      "languageOptions": {
        "parser": "./tsrx-eslint-parser.mjs"
      },
      "jsPlugins": ["./demo-lint-plugin.mjs"],
      "rules": {
        "tsrx-demo/require-keyed-for": "error"
      }
    }
  ]
}
```

That JSON is not accepted by released Oxlint 1.74. It is accepted by the
upstream draft and is now covered by an isolated VS Code Extension Host test
that contains only the official OXC extension and asserts the `oxc`
diagnostic through an unsaved edit. The launcher supplies the `.tsrx`
registrations missing from the official client, while the rule itself runs in
Oxlint—not ESLint. Publication still waits for the upstream contract to ship.

Last audited: 2026-07-23.
