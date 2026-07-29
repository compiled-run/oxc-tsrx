# Custom JavaScript plugins: the runnable copy

This directory holds the two projects that
[Custom JavaScript plugins](../../docs/integrations/custom-js-plugins.md)
walks you through: the plain one here at the top level, and the `vp create`
walkthrough in `vite-plus/`. Every code fence on that page which says "Save this
as `<name>`" is compared to the file of the same name in whichever of the two it
belongs to, byte for byte, by `tests/plugins/custom-js-plugins-doc.test.mjs`. If
the two ever disagree, that test fails.

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

## `vite-plus/`: the fresh-scaffold walkthrough

`vite-plus/` is the second sample project on that page, the one its
[walkthrough](../../docs/integrations/custom-js-plugins.md#the-whole-path-on-a-fresh-vite-project)
builds from a `vp create` React scaffold. It is the same Oxlint route as above,
in the shape a real Vite+ app has.

| File | What it is |
| --- | --- |
| `house-rules.mjs` | The plugin. One rule, `no-inline-style-object`, keyed on `JSXAttribute`. |
| `.oxlintrc.json` | One **top-level** `jsPlugins` entry and one rule. No `overrides` block, on purpose. |
| `src/Greeting.tsrx` | A TSRX component with an inline `style` object on line 5, column 9. |
| `src/Panel.tsx` | An ordinary React component with the same problem on line 2, column 19. |
| `vite.config.ts` | The scaffold's config after the one edit `vp lint` needs: the plugin added to `lint.jsPlugins` and `lint.rules`. `lint.options` stays exactly as the scaffold wrote it. |

`tests/plugins/custom-js-plugins-doc.test.mjs` runs the first four of these:
one `oxlint` over `src/` reporting both files at those positions, the same rule
at the same position from the language server, and the exit-2 refusal you get if
the scaffold's `lint.options` type-aware default survives. `vite.config.ts` is
checked for byte equality with the page but not executed, because running it
needs Vite+ installed.

Two facts that walkthrough settles by measurement rather than by memory:

- **No `overrides` block is needed.** One top-level `jsPlugins` declaration
  serves a mixed batch of `.tsrx` and `.tsx` correctly.
- **The `plugins` entry goes in `tsconfig.app.json`, not the root.** A `vp create`
  scaffold's root tsconfig is solution-style and owns no files, so a plugin
  declared there is inert.

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
