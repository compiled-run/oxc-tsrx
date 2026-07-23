---
title: Custom JavaScript plugins
description: What works today for TSRX AST consumers in Vite and ESLint, and the exact upstream seam Oxlint still needs.
---

# Custom JavaScript plugins

There are three different plugin boundaries, and they should not be conflated:

| Host | Released status | Practical TSRX path |
| --- | --- | --- |
| Visual Studio Code companion | Built-in Oxlint rules work now | Native `oxc-tsrx-lsp`; use the committed lint demo |
| Vite 8 plugins | Raw-source transforms work now; no custom bundler-parser hook | Parse before the framework transform and share the authored AST with other plugins |
| Oxlint 1.74 JS plugins | JavaScript rules work, custom parsers/file formats do not | Target the upstream custom-parser draft; use ESLint as the executable proof meanwhile |

The runnable prototypes live in `examples/custom-js-plugins`, and their
retained tests use the real parser, ESLint 10, Vite 8.1.5, and
`@tsrx/vite-plugin-react` 0.0.72.

## See native lint diagnostics in VS Code

The fastest visible demo uses current production behavior:

1. Open this repository in Visual Studio Code.
2. Select **Run and Debug → TSRX: lint demo**.
3. Press **F5**.

The launch target builds the language server and companion extension, opens
`examples/vscode-lints/LintDemo.tsrx`, and publishes five real diagnostics:
`no-var`, `no-unused-vars`, `no-console`, `eqeqeq`, and `no-debugger`.
`no-var` also exposes the validated safe quick fix.

This demo intentionally uses built-in rules. JavaScript plugin diagnostics
cannot enter the current in-process Rust language server yet.

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
That means adding `jsPlugins` to the native TSRX config cannot work: the
JavaScript rule host receives only ASTs produced by Oxlint's supported
language loaders, while `oxc-tsrx-lsp` is a separate in-process Rust host.

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

That JSON is deliberately forward-looking and is not accepted by Oxlint
1.74. Once upstream ships it, direct VS Code support still needs the companion
extension to start or connect to the Oxlint JS host for `.tsrx` and merge that
diagnostic lifecycle with native formatting. Until then, built-in rules remain
the honest editor path and ESLint/Vite remain the runnable custom-rule proofs.

Last audited: 2026-07-23.
