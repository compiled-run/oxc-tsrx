# Custom JavaScript plugin prototypes

These files prove the two available integration boundaries without claiming
that released Oxlint can already parse `.tsrx`.

## Vite: available now

Vite plugins cannot replace Rolldown's parser or return a custom AST. They can
transform custom files, and Vite officially recommends that approach for
custom file types. `tsrxParserService()` therefore runs before the framework
compiler, parses the raw `.tsrx` once, and retains that authored AST for other
plugins in the same Vite process.

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

The order is parser service, parser-aware consumers, then the existing
framework transform. Rolldown still parses the framework plugin's generated
JavaScript; the service does not patch or replace Vite internals.

## ESLint: AST-only rules work now

`tsrx-eslint-parser.mjs` adapts `@oxc-tsrx/parser` to the public
`parseForESLint` contract. It supplies authored ranges and locations, comments,
parser services, and visitor keys including `JSXIfExpression`,
`JSXForExpression`, and the other TSRX nodes. The included
`eslint-plugin-tsrx-demo` proves a JavaScript rule can visit those nodes.

The parser API does not expose tokens yet, so the prototype deliberately
supports AST-only rules. Rules using `SourceCode` token methods need a real
authored token stream before this can become a production parser package.
Framework-aware scope semantics also need a static scope contract instead of
assuming every custom node behaves like ordinary ESTree.

## Oxlint: runnable against the upstream draft

Oxlint 1.74 supports JavaScript rules but its current documentation explicitly
lists custom file formats and parsers as unsupported. A draft upstream change,
[oxc-project/oxc#24262](https://github.com/oxc-project/oxc/pull/24262), adds
ESLint-compatible `overrides[].languageOptions.parser`. When that contract
lands, the adapter shape above fits the proposed configuration:

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

That syntax is not valid released Oxlint configuration. It is now exercised
by the VS Code lint demo against a local build of the draft: a small LSP
launcher lets the official OXC extension register `.tsrx`, the parser returns
the authored TSRX AST, and `tsrx-demo/no-tsrx-if` appears as an `oxc` editor
diagnostic. The broader
[Oxlint language-plugins RFC](https://github.com/oxc-project/oxc/discussions/21936)
is the production-grade destination for cached parsing, typed visitor schemas,
faithful virtual TS, source mappings, and type-aware rules.

The native `oxc-tsrx-lsp` remains an in-process Rust host and does not execute
JavaScript. For the source-only experiment,
`scripts/oxlint-custom-parser-lsp-proxy.mjs` forwards the official extension's
LSP stream to draft Oxlint and dynamically registers `.tsrx` document sync
and pull diagnostics. No companion VS Code extension is present in the
retained proof. Shipping this lane waits for upstream custom-parser support to
be released.
