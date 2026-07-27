# Custom JavaScript plugins: the runnable copy

This directory is the project that
[Custom JavaScript plugins](../../docs/integrations/custom-js-plugins.md)
walks you through. Every code fence on that page which says "Save this as
`<name>`" is compared to the file of the same name here, byte for byte, by
`tests/plugins/custom-js-plugins-doc.test.mjs`. If the two ever disagree, that
test fails.

## The sample project

| File | What it is |
| --- | --- |
| `src/TaskList.tsrx` | The fixture. Its `@for` block deliberately has no `key`, and it has a `debugger` for the built-in rules to find. |
| `src/TaskRow.tsx` | An ordinary React component. Its `.map()` call has the same missing-key problem. |
| `explore-tsrx-ast.mjs` | Prints the node type of every TSRX control block in `src/TaskList.tsrx`. |

## Oxlint: one JavaScript plugin, both file types

`oxlint-demo-plugin.mjs` is an Oxlint JavaScript plugin, and `.oxlintrc.json`
enables it. The `oxlint` binary that `oxc-tsrx` installs runs it on
`src/TaskRow.tsx`:

```sh
npx oxlint src/TaskRow.tsx
```

Pointed at `src/TaskList.tsrx`, the same config runs the same plugin. `.tsrx`
files are linted by a native Rust process with no Node.js runtime, so `oxlint`
hands each file's legal-TSX projection to the published Oxlint binary, runs your
plugin over that, and maps every diagnostic back to the bytes you wrote. It
costs one extra parse per `.tsrx` file and says so on stderr each time.
`require-keyed-map` looks for a `.map()` call, and `src/TaskList.tsrx` has an
`@for` block, so the rule runs there and finds nothing; the docs page adds a
`src/TaskFeed.tsrx` that does have one.

Set `settings.oxcTsrx.jsPluginsOnTsrx` to `false` and the `.tsrx` half refuses
out loud with exit 2 instead, rather than dropping your rule quietly.

The same config also runs the same plugin in an editor. Open one of these files
with the official OXC extension and your rule is a squiggle beside the built-in
ones, at the positions `oxlint` reports. Your rule sees `context.filename` as
the mirror path there too, and the language server logs the extra parse once per
session. [Editor integration](../../docs/integrations/editor.md#your-own-javascript-rules-in-the-editor)
covers that half.

## ESLint: for a rule that must visit authored TSRX nodes

Your rule sees the projection above, in which `@if` and `@for` have already
become ordinary `if` and `for`. A rule keyed on `JSXIfExpression` or
`JSXForExpression` therefore cannot fire on that route. That is what the rest of
this directory is for.

`tsrx-eslint-parser.mjs` adapts the parser to the public `parseForESLint`
contract. It supplies authored ranges and locations, comments, parser services,
and visitor keys including `JSXIfExpression`, `JSXForExpression`, and the other
TSRX nodes. `demo-lint-plugin.mjs` is the plugin that visits them, and
`eslint.config.mjs` wires the two together.

The files here import the parser as `../../packages/toolchain/dist/parser.js`
so the repository's own tests can load them without an install. The docs page
tells readers to use the public `oxc-tsrx/parser` subpath instead; both resolve
to the same module, and both the transcript generator and the docs test make
exactly that one substitution before running.

The parser API does not expose tokens yet, so this is deliberately an AST-only
prototype. Rules using `SourceCode` token methods need a real authored token
stream first. Framework-aware scope semantics also need a static scope contract
instead of assuming every custom node behaves like ordinary ESTree.

## Vite: reading the authored AST during a build

Vite plugins cannot replace Rolldown's parser or return a custom AST. They can
transform custom files, and Vite officially recommends that approach for custom
file types. The `withTsrxParser` helper in `tsrx-parser-service.mjs` therefore
runs a pre-transform service before the framework compiler, parses the raw
`.tsrx` once, and retains that authored AST for other plugins in the same Vite
process. `vite-demo-lint.mjs` is the consumer.

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

The order is parser service, parser-aware consumers, then the existing framework
transform. Rolldown still parses the framework plugin's generated JavaScript,
and the service does not patch or replace Vite internals. `withTsrxParser` is
not exported by the `oxc-tsrx` package, so this is a source-local proof rather
than an installable API.

## The upstream draft

A draft upstream change,
[oxc-project/oxc#24262](https://github.com/oxc-project/oxc/pull/24262), adds
ESLint-compatible `overrides[].languageOptions.parser` to Oxlint. As of
2026-07-24 it is still a Draft. When that contract lands, the adapter shape
above fits the proposed configuration:

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

That syntax is not valid released Oxlint configuration. It is exercised by the
VS Code demo in `examples/vscode-lints` against a local build of the draft:
`scripts/oxlint-custom-parser-lsp-proxy.mjs` forwards the official OXC
extension's LSP stream to draft Oxlint and dynamically registers `.tsrx`
document sync and pull diagnostics, and `tsrx-demo/no-tsrx-if` appears as an
`oxc` editor diagnostic. No companion VS Code extension is involved. The
broader [Oxlint language-plugins RFC](https://github.com/oxc-project/oxc/discussions/21936)
is the production-grade destination for cached parsing, typed visitor schemas,
faithful virtual TS, source mappings, and type-aware rules.

Running your Oxlint plugin on `.tsrx` ships today, through the projection route
above, and it ships in the editor as well as on the command line. The native
`oxc-tsrx-lsp` is still Rust and still executes no JavaScript itself: it
projects the buffer and borrows one Node.js host per workspace, started only
when the config declares `jsPlugins`, to run the published Oxlint binary over
that projection. What still waits on released upstream custom-parser support is
a rule that visits authored TSRX node types inside Oxlint itself.
