---
title: Parsing
description: Parse .tsrx and ordinary JS/TS into a real AST with oxc-tsrx/parser, the oxc-parser style API with TSRX support.
---

# Parsing

`oxc-tsrx/parser` is the parser behind the lint and format tools, exposed as a
library you can call yourself. It has the same API shape as
[`oxc-parser`](https://www.npmjs.com/package/oxc-parser), OXC's official npm
parser, and it handles `.tsrx` alongside ordinary `js`, `jsx`, `ts`, `tsx`, and
`dts`.

Reach for it when you are building something that has to understand `.tsrx`
source: a codemod, a bundler plugin, an editor feature, an analysis tool.

## Install

You need Node.js 20.19 or newer:

<!-- pm-install -->
```sh
npm install oxc-tsrx@0.2.0
```

Like the CLI tools, the parser is native Rust code. Your package manager
downloads a ready-made binary for your platform during this normal install.
There are no install scripts, and nothing is downloaded later.

## Parse a file

Two arguments, a file name and its source:

```js
import { parseSync } from "oxc-tsrx/parser";

const result = parseSync("View.tsrx", source);
```

The file name decides the language, so `.tsrx` gets TSRX and `.ts` gets
TypeScript. Pass `{ lang: "tsrx" }` if the name cannot tell you, for instance
when the source came from a string.

There is an async entry point with the same signature:

```js
import { parse } from "oxc-tsrx/parser";

const result = await parse("View.tsrx", source);
```

## What comes back

`result` has four fields, each converted from the native side only the first
time you read it. A tool that only wants the import list never pays for
building the whole AST.

| Field | What it holds |
| --- | --- |
| `result.program` | the AST |
| `result.module` | imports and exports: `staticImports`, `staticExports`, `dynamicImports`, `importMetas`, each with positions |
| `result.comments` | every line and block comment |
| `result.errors` | everything wrong with the code you parsed |

Positions are plain JavaScript string indexes, so `source.slice(node.start,
node.end)` always gives back the exact text of a node, even in files with emoji
or other multi-byte characters. They point into the string you passed in, never
into an internal copy.

## Walking the tree

To visit nodes, reach for [`oxc-walker`](https://www.npmjs.com/package/oxc-walker).
It walks this AST with no special handling, TSRX nodes included, so you do not
have to hand-roll a recursive `find`.

Here it is inside a Vite plugin. A bundler hands you the module id and its
source, which is the shape most tools using this parser end up in:

```js
import { parseSync } from "oxc-tsrx/parser";
import { walk } from "oxc-walker";

export function tsrxKeyedLoops() {
  return {
    name: "tsrx-keyed-loops",
    transform: {
      filter: { id: /\.tsrx$/ },
      handler(code, id) {
        const { program } = parseSync(id, code);
        const unkeyed = [];

        walk(program, {
          enter(node) {
            if (node.type === "JSXForExpression" && !node.key) {
              unkeyed.push(node);
            }
          },
        });

        for (const node of unkeyed) {
          this.warn({ message: "@for without a key", pos: node.start });
        }

        return null;
      },
    },
  };
}
```

Three things there are worth copying:

- `filter` is applied in Rust before your handler runs, which is cheaper than an
  early `return` in JavaScript;
- it returns `null`, because reading the tree is all it does. Compiling `.tsrx`
  belongs to your framework's plugin, which this one runs alongside;
- it collects nodes during the walk and reports afterwards, because `walk`
  rebinds `this`, so `this.warn` is not available inside `enter`.

<!-- terminal-demo:parsing-quickstart -->

The top level is ordinary ESTree node types, so code that already walks ASTs
walks this one with no special cases. The `@for` came back as a real
`JSXForExpression` node, not a comment or a placeholder.

## The tree is the code you wrote

Every TSRX control comes back as its own node type, shaped like the JSX nodes
you already know:

| You write | You get back |
| --- | --- |
| `@{ }` statement containers | `JSXCodeBlock` |
| `@if` / `@else` | `JSXIfExpression` |
| `@for` / `@empty` | `JSXForExpression` |
| `@switch` / `@case` / `@default` | `JSXSwitchExpression` |
| `@try` / `@pending` / `@catch` | `JSXTryExpression` |
| `<{expr}>` dynamic tags | `JSXElement` whose tag name is a `JSXExpressionContainer` |

Everything else, from elements and attributes to TypeScript types, uses the same
node shapes as `oxc-parser`. The package re-exports the `@oxc-project/types`
definitions so your editor can autocomplete them.

## How a parse works

<!-- diagram:parser-paths -->

Four steps, for a `.tsrx` file:

1. **Scan.** Read the file once and find the TSRX-only syntax.
2. **Copy.** Build a valid TSX copy in memory, with the TSRX controls replaced
   by placeholders and your code copied over unchanged.
3. **Parse.** OXC parses that copy, once. There is no second parser and no fork.
4. **Rebuild.** Placeholders become TSRX nodes again, and every position maps
   back to your original source.

Ordinary `.js`, `.jsx`, `.ts`, and `.tsx` files skip steps 1, 2, and 4. They go
straight to OXC, exactly like calling `oxc-parser` yourself.

## Options

The third argument tunes the parse:

| Option | What it does |
| --- | --- |
| `lang` | Forces a language: `"js"`, `"jsx"`, `"ts"`, `"tsx"`, `"dts"`, or `"tsrx"`. Without it, the file name decides. |
| `sourceType` | `"module"`, `"script"`, `"commonjs"`, or `"unambiguous"`. |
| `astType` | `"js"` or `"ts"`: which AST flavor to produce when the extension alone does not decide it. |
| `range` | Adds a `range: [start, end]` array to every node, for tools that expect ESLint-style ranges. |
| `preserveParens` | On by default: parenthesized expressions appear as `ParenthesizedExpression` nodes. Set `false` to see through them. |
| `showSemanticErrors` | Also reports semantic problems, like declaring the same `let` twice. |
| `recovery` | `"none"` (default) or `"editor"`. |

## Two kinds of errors

**Problems in the code you parse never throw.** They land in `result.errors`,
each with a severity, a message, positions, and a ready-to-print codeframe, as
in the second run above.

**Problems running the parser itself do throw.** You get a
`ParserOperationalError` carrying a stable `code` you can match on:

| Code | What went wrong |
| --- | --- |
| `ERR_TSRX_NATIVE_NOT_INSTALLED` | there is no native binary for your platform |
| `ERR_TSRX_NATIVE_INTEGRITY` | the binary failed its checksum |
| `ERR_TSRX_CAPABILITY_RECOVERY` | you asked for something this build cannot do |

Before loading a binary, the package checks its target, its version, and its
SHA-256 digest. A failed check throws. It never quietly falls back to a
different build.

## Feature detection

`capabilities` tells you what the installed build supports, so you can check
before you rely on something:

```js
import { capabilities } from "oxc-tsrx/parser";

capabilities.languages;          // which languages it parses
capabilities.editorRecovery;     // whether recovery: "editor" works here
capabilities.cssMaterialization; // whether CSS is broken down to its components
capabilities.oxcRevision;        // the OXC version it was built from
```

Asking for something the installed build cannot do throws, rather than parsing
with less than you asked for and letting you find out later.

Each flag answers a question about **this build**, not about the language. There
are two builds, a canonical one and a compatibility one, and every flag here is
the difference between them for one option. So read a `false` narrowly. It tells
you that one option is off, and nothing more.

`cssMaterialization: false` is the one worth spelling out, because it is easy to
read as "there is no CSS tree". There is. On the canonical build a `<style>`
element always comes back with its CSS text on `css`, plus a `StyleSheet` child:

```js
const { program } = parseSync(
  "Card.tsrx",
  'const x = <style>.a, .b > p:hover { color: red }</style>;\n',
);
// The <style> element's StyleSheet child holds:
//   Rule
//     prelude: SelectorList -> ComplexSelector, ComplexSelector
//     block:   Block
// Every one of those carries start/end offsets into the CSS text.
```

That is enough to find each selector and rewrite it, which is what scoping
styles needs, and consumers do exactly that today.

What the `false` withholds is the level below. The `ComplexSelector` and `Block`
nodes arrive with empty `children`, so you get no compound-selector parts and no
node per declaration. If you need those, read them out of the `source` text on
the node or hand it to a CSS parser. The compatibility build reports `true` here
and materializes them for you.

## What is tested

The parser is the same code path the linter, the formatter, and the language
server run on, so every one of their suites exercises it too. On top of that,
`tests/parser-api` covers the boundary a library user actually depends on:

- **Nothing is lost crossing from Rust to JavaScript.** The whole program moves
  as one payload, `BigInt` and `RegExp` values included, and a malformed payload
  is rejected rather than half-decoded.
- **Text with emoji and other multi-byte characters keeps exact positions.** A
  broken surrogate pair fails at its exact offset instead of shifting everything
  after it.
- **A tampered binary never loads.** The suite mutates the packed loader's
  identity and checksums and asserts every mutation is refused.
- **It works as a real dependency.** Packed tarballs are installed into a fresh
  project, then loaded, parsed, and bundled, so a broken publish fails here
  rather than in your build.
- **Deep files do not blow the stack.** Deeply nested TSRX is built iteratively
  on a bounded stack.

All of it runs on Linux, macOS, and Windows on every pull request. The terminal
output on this page is captured by really running the code at build time, so it
cannot drift from what the parser does.

## Next steps

- See exactly which TSRX syntax the parser recognizes in
  [TSRX Syntax](/guide/tsrx-syntax).
- Lint and format the same files with the CLI tools in
  [Getting Started](/guide/getting-started).
- Curious how one parse serves lint, format, and this API? Read
  [Architecture](/architecture/rust-oxc-core).
