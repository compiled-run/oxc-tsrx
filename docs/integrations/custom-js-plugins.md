---
title: Custom JavaScript plugins
description: The separate places a custom JavaScript check can read a TSRX AST today, what is released, and what is still only a local proof or an upstream draft.
---

# Custom JavaScript plugins

People ask one question in a lot of different ways: "can I write my own
JavaScript lint rule that understands `.tsrx`?" The honest answer is "it
depends which tool runs the rule." There is no single "TSRX plugin" host.
There are several tools, each with its own parser and its own plugin format,
and only some of them can see TSRX today.

So before any code, get the map straight. Each row below is a different
program that could run a check. The columns that matter are: which parser
feeds it the AST, what plugin shape it accepts, and how real it is right now.

| Where the check runs | Parser it uses | Plugin shape | How real today |
| --- | --- | --- | --- |
| A Vite plugin (in the Vite dev/build process) | The repo's TSRX parser service | An ordinary Vite plugin calling `this.warn` | Works, but only as a source-local example in this repo |
| ESLint (its own process) | A `parseForESLint` adapter in this repo | A normal ESLint plugin | Works for AST-only rules; proven by an ESLint 10 test |
| Released Oxlint, on ordinary `.js`/`.ts` | Oxlint's native OXC parser | Oxlint JS plugins | Released, but it cannot parse `.tsrx` at all |
| Draft Oxlint, on `.tsrx` | The same `parseForESLint` adapter | Oxlint JS plugins plus a draft custom-parser hook | Only an unmerged upstream draft, built locally |
| Native `oxc-tsrx-lsp` | The native Rust TSRX projection | Native Rust rules only | Shipping today, but it is Rust and runs no JavaScript |
| `oxc-tsrx/lint/plugins-dev` | none | Helpers for *authoring* a JS plugin | A real export, but it only helps you write a plugin; it does not run one |

Two things trip people up, so they are worth saying plainly:

- **The official OXC VS Code extension is a client, not a rule runtime.** When
  a custom TSRX rule shows a squiggle in the demo below, the extension is only
  displaying it. The rule actually executes inside draft Oxlint behind the
  extension.
- **`oxc-tsrx/lint/plugins-dev` is not a host.** It exports helpers for
  writing a plugin. It does not give you a place that executes your plugin
  against `.tsrx`. Running one is what the rows above are about.

The runnable examples live in `examples/custom-js-plugins`. Their tests use
the real parser, ESLint 10, Vite 8.1.5, and `@tsrx/vite-plugin-react` 0.0.72.
The Oxlint versions in this repo are pinned and tested at 1.74.0; public
releases may have moved past that.

## A Vite plugin that reads the authored AST

`examples/custom-js-plugins/vite-demo-lint.mjs` is an ordinary Vite plugin. It
walks the authored TSRX AST and calls `this.warn` when it finds something it
does not like. That is all "custom check inside Vite" means here: your own
code, running in the Vite process, reading a parse result the repo already
produced.

It reuses one shared parse. A pre-transform service parses each raw `.tsrx`
file once and caches it, and the demo plugin reads that cached AST instead of
parsing again.

Be careful what this does *not* prove. It does not show that an ESLint plugin
can be dropped into Vite, that Oxlint runs inside Vite, or that a Vite+
`lint.jsPlugins` entry would reuse this cache. Those are different hosts.

### How the composition is wired

Vite's public plugin API lets a plugin transform a custom file type, but it
does not let a plugin replace Rolldown's parser or return an AST you built
yourself, and `moduleParsed` does not run during dev. So the only useful place
to read authored TSRX is *just before* the framework plugin compiles it.

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

`withTsrxParser` builds a small array of Vite plugins in this order:

1. a pre-transform service that parses raw `.tsrx` once and caches the result;
2. your parser-aware plugins, which read that cached AST; and
3. the existing framework plugin, which compiles TSRX and keeps ownership of
   CSS, HMR, and source maps. Rolldown still parses only the generated
   JavaScript.

Two honest caveats a junior reader should not skip:

- **This is a source-local proof, not an installable feature.** Inside this
  example, `tsrx-parser-service.mjs` imports the parser with a relative path
  (`../../packages/toolchain/dist/parser.js`) rather than from
  `oxc-tsrx/parser`. That is the same file an install reaches through the
  `oxc-tsrx/parser` subpath, so the parser is public. The helper is not:
  `withTsrxParser` is *not* exported by the `oxc-tsrx` package, so you cannot
  `import { withTsrxParser } from "oxc-tsrx"` today.
- **Vite does not hand your plugin a parser.** The `parser` argument comes from
  a closure that `withTsrxParser` creates, not from any standard Vite parser
  lifecycle. This is a pattern the example sets up, not a Vite feature.

If this helper is ever published, a clean API might look like
`import { withTsrxParser } from "oxc-tsrx/vite"`. That is a suggestion for the
future, not something you can run now.

## An ESLint plugin (AST-only)

If you want a real, familiar plugin format today, ESLint is the answer. ESLint
lets you supply your own parser through `parseForESLint`, and
`examples/custom-js-plugins/tsrx-eslint-parser.mjs` does exactly that for TSRX.
It:

- returns the authored `Program`;
- supplies authored ranges and line/column locations;
- adds comments;
- derives visitor keys for the custom TSRX nodes;
- exposes a small `services` object; and
- deliberately returns an empty token list (`program.tokens = []`).

`examples/custom-js-plugins/demo-lint-plugin.mjs` is a normal ESLint plugin
whose rules visit `JSXIfExpression` and `JSXForExpression`, and
`tests/plugins/parser-integrations.test.mjs` runs ESLint 10 and proves the
authored `@if` diagnostic fires.

Two limits define why this is labeled **AST-only**, and you should not claim
past them:

1. The public parser (v1) does not expose OXC's token stream, so the adapter
   returns no tokens. Rules that rely on `SourceCode` token methods cannot be
   correct here.
2. Generic ESTree traversal reaches ordinary descendants, but there is no full
   framework scope contract, so binding/scope behavior around TSRX control
   syntax is not guaranteed.

Same source-local caveat as the Vite example: this adapter also imports the
parser with a relative path (`../../packages/toolchain/dist/parser.js`) rather
than through the published `oxc-tsrx/parser` subpath, even though both resolve
to the same module.

## Released Oxlint and Vite+

Released Oxlint supports JavaScript plugins, but only for file types its own
parser already accepts, and its
[released docs](https://oxc.rs/docs/guide/usage/linter/js-plugins.html) still
list custom parsers and custom file formats as unsupported. In other words,
released Oxlint runs your JS plugins on `.js`/`.ts`, and simply cannot read
`.tsrx`.

Vite+ surfaces Oxlint's JS-plugin configuration in its `lint` block. That
gives you the ordinary released Oxlint JS-plugin host through Vite+; it does
not add a TSRX parser. Inside `oxc-tsrx`, the Vite+ bridge splits work by file
type: ordinary files go to canonical Oxlint (so their JS plugins run), and
`.tsrx` files go to a separate native TSRX process. Because that native
process has no Node JS-plugin host, the bridge **rejects** `jsPlugins` for the
TSRX lane with a clear error instead of silently dropping it. So:

- ordinary `.js`/`.ts` can use released Oxlint JS plugins through Vite+;
- `.tsrx` in the native lane cannot run those JS plugins;
- adding `jsPlugins` does not make the Vite parser service get reused; and
- `oxc-tsrx/lint/plugins-dev` helps you author a plugin but does not run it.

## The upstream Oxlint draft for TSRX

The path that would eventually let a JavaScript rule run against `.tsrx`
*inside Oxlint* is an upstream draft, not a release. As of 2026-07-24, OXC PR
[#24262](https://github.com/oxc-project/oxc/pull/24262) is still a **Draft**
with nine commits. It adds explicit
`overrides[].languageOptions.parser` routing for Oxlint's Node-enabled
JS-plugin host.

Older docs described this PR as just a "shortest AST-only route." That is
stale. The current draft is much broader and now includes:

- `parseForESLint` / `parse` routing;
- `SourceCode` and token-store behavior;
- parser services and scope-manager integration;
- fixes and disable directives;
- editor/LSP routing;
- native-rule coverage through an offset-preserving shadow source; and
- explicit per-glob opt-in.

It still leaves real work out, so do not oversell it:

- typechecking and type-aware framework files;
- a faithful framework virtual source and its mappings;
- parse/load caching;
- generated typed walkers;
- full module-graph participation; and
- first-class language identity.

The wider language-plugin idea
([discussion #21936](https://github.com/oxc-project/oxc/discussions/21936))
remains a discussion, not a release.

If this draft lands with its current contract, the TSRX config would look like
this:

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

Released Oxlint 1.74.0 rejects that JSON. Only the draft accepts it, and the
"draft" here is a local source build of the PR at
`target/oxlint-custom-parser/cli.js`, not anything you can install.

## See the draft rule run in VS Code

The repository has an editor demo that ties this together, and it is worth
understanding exactly which piece does what:

1. the client is only the official OXC VS Code extension (no companion TSRX
   extension);
2. it is pointed at a workspace-local launcher;
3. the launcher dynamically registers `.tsrx` document sync and pull
   diagnostics with the extension;
4. it forwards LSP traffic to `target/oxlint-custom-parser/cli.js`, the local
   draft build;
5. draft Oxlint runs the TSRX `parseForESLint` adapter; and
6. the JavaScript rule reports `tsrx-demo/no-tsrx-if`.

To try it:

1. Open `examples/vscode-lints` as the VS Code workspace.
2. Install or enable `oxc.oxc-vscode`.
3. Open `oxlint-custom-parser.json` once to activate the official extension,
   then open `LintDemo.tsrx`.

`tsrx-demo(no-tsrx-if)` underlines the authored `@if … @else` block. A retained
Extension Host test asserts the companion extension is absent, activates the
official extension through its JSON config, checks the diagnostic and its
authored range, applies an unsaved edit, and checks the updated diagnostic.

The custom rule runs in **draft Oxlint**. It does not run in the official
extension client itself, in native `oxc-tsrx-lsp`, in ESLint during this
session, or in the Vite parser service. This correctly shows you do not need a
second VS Code extension; it does not make the upstream draft released.

Last audited: 2026-07-24.
