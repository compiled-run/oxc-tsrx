---
title: Parsing
description: Parse .tsrx and ordinary JS/TS into a real AST with oxc-tsrx/parser, the oxc-parser style API with TSRX support.
---

# Parsing

`oxc-tsrx/parser` is the parser behind the lint and format tools, exposed
as a library you can call yourself. It has the same API shape as
[`oxc-parser`](https://www.npmjs.com/package/oxc-parser), OXC's official npm
parser, and it handles ordinary JavaScript and TypeScript (`js`, `jsx`, `ts`,
`tsx`, `dts`) plus `.tsrx` files.

If you are building something that needs to understand `.tsrx` source (a
bundler plugin, a codemod, an editor feature, a static analysis tool), this
is the package for you.

## Install

You need Node.js 20.19 or newer:

<!-- pm-install -->
```sh
npm install oxc-tsrx@latest
```

Like the CLI tools, the parser is native Rust code. Your package manager
downloads a ready-made binary for your platform during this normal install.
There are no install scripts, and nothing is downloaded later.

## Parse your first file

Save this as `src/View.tsrx`. The "Try in playground" button lets you explore
the file in your browser first:

```tsrx
import { Row } from "./Row";

export function View({ items }: { items: Item[] }) @{
  <ul class="list">
    @for (const item of items; key item.id) {
      <Row item={item} />
    } @empty {
      <li>No items yet</li>
    }
  </ul>
}
```

Then save this parse script as `parse.mjs`. It parses the file and walks the
tree looking for the `@for` node:

```js
import { readFileSync } from "node:fs";
import { parseSync } from "oxc-tsrx/parser";

const source = readFileSync("src/View.tsrx", "utf8");
const result = parseSync("src/View.tsrx", source);

console.log("errors:", result.errors.length);
console.log("imports:", result.module.staticImports.map((s) => s.moduleRequest.value));
console.log("top level:", result.program.body.map((node) => node.type));

function* walk(node) {
  if (Array.isArray(node)) {
    for (const item of node) yield* walk(item);
  } else if (node && typeof node === "object") {
    if (typeof node.type === "string") yield node;
    for (const value of Object.values(node)) yield* walk(value);
  }
}

const forNode = [...walk(result.program)].find((node) => node.type === "JSXForExpression");
console.log("found:", forNode.type);
console.log("its first line, straight from your file:");
console.log(source.slice(forNode.start, forNode.end).split("\n")[0]);
```

<!-- terminal-demo:parsing-quickstart -->

Three things worth noticing:

- `top level:` lists ordinary ESTree node types. This is a normal AST, and
  code that walks ASTs can walk it with no special cases.
- The `@for` came back as a `JSXForExpression` node, a real node type, not a
  comment or a placeholder.
- Slicing the source with the node's `start` and `end` returns exactly what
  you typed. Positions always point into the string you passed in, never
  into an internal copy.

## How a parse works

<!-- diagram:parser-paths -->

Step by step, for a `.tsrx` file:

1. The file is scanned once to find the TSRX-only syntax.
2. A valid-TSX copy is built in memory (the *projection*). Your code is
   copied into it unchanged; only the TSRX control syntax is replaced with
   TSX placeholders.
3. Canonical OXC parses that copy, once. There is no second parser and no
   TSRX fork of OXC.
4. The parsed tree is rebuilt into the tree you authored: the placeholders
   become TSRX nodes again, and every position is mapped back to your
   original source.

Ordinary `.js`, `.jsx`, `.ts`, and `.tsx` files skip steps 1, 2, and 4
entirely: the file goes straight to OXC, exactly like calling `oxc-parser`
yourself.

## The tree is the code you wrote

Every TSRX control comes back as its own node type, shaped like the JSX
nodes you already know:

| You write | You get back |
| --- | --- |
| `@{ }` statement containers | `JSXCodeBlock` |
| `@if` / `@else` | `JSXIfExpression` |
| `@for` / `@empty` | `JSXForExpression` |
| `@switch` / `@case` / `@default` | `JSXSwitchExpression` |
| `@try` / `@pending` / `@catch` | `JSXTryExpression` |
| `<{expr}>` dynamic tags | `JSXElement` whose tag name is a `JSXExpressionContainer` |

Everything else (elements, attributes, statements, expressions, TypeScript
types) uses the same node shapes as `oxc-parser`, and the package re-exports
the `@oxc-project/types` type definitions so your editor can autocomplete
them.

Positions are plain JavaScript string indexes, so `source.slice(node.start,
node.end)` is always the exact text of a node, even in files with emoji or
other multi-byte characters.

## Four results, each loaded lazily

`parseSync` returns a result with four independent getters:

- `result.program` is the AST.
- `result.module` is a fast summary of imports and exports: `staticImports`,
  `staticExports`, `dynamicImports`, and `importMetas`, each with positions.
- `result.comments` lists every line and block comment.
- `result.errors` lists everything wrong with the parsed code.

Each field is converted from the native side only the first time you read
it. A tool that only wants the import list never pays for building the full
JavaScript AST.

There is also an async entry point with the same signature:

```js
import { parse } from "oxc-tsrx/parser";

const result = await parse("src/View.tsrx", source);
```

## Options

The third argument tunes the parse:

| Option | What it does |
| --- | --- |
| `lang` | Forces a language: `"js"`, `"jsx"`, `"ts"`, `"tsx"`, `"dts"`, or `"tsrx"`. Without it, the file extension of the first argument decides. |
| `sourceType` | `"module"`, `"script"`, `"commonjs"`, or `"unambiguous"`. |
| `astType` | `"js"` or `"ts"`: which AST flavor to produce when the extension alone does not decide it. |
| `range` | Adds a `range: [start, end]` array to every node, for tools that expect ESLint-style ranges. |
| `preserveParens` | On by default: parenthesized expressions appear as `ParenthesizedExpression` nodes. Set `false` to see through them. |
| `showSemanticErrors` | Also reports semantic problems, like declaring the same `let` twice. |
| `recovery` | `"none"` (default) or `"editor"`. Editor recovery is a capability flag; see below. |

## Two kinds of errors

Problems in the code you parse never throw. They land in `result.errors`,
each with a severity, a message, positions, and a ready-to-print codeframe,
as you saw in the `Broken.tsrx` run above.

Problems running the parser itself do throw, as a `ParserOperationalError`
with a stable `code` you can match on. Examples: the native binary for your
platform is not installed (`ERR_TSRX_NATIVE_NOT_INSTALLED`), it failed its
integrity check (`ERR_TSRX_NATIVE_INTEGRITY`), or you asked for a feature
this build does not support (`ERR_TSRX_CAPABILITY_RECOVERY`). Before loading
the native binary, the loader verifies its package role, target, version,
and SHA-256 digest, and a failed check is an error, never a silent fallback.

## Feature detection with `capabilities`

The package exports a `capabilities` object describing exactly what the
installed native build can do, including the pinned OXC revision it was
built from:

```js
import { capabilities } from "oxc-tsrx/parser";

console.log(capabilities.languages);    // ["js", "jsx", "ts", "tsx", "dts", "tsrx"]
console.log(capabilities.oxcRevision);  // the exact OXC commit
console.log(capabilities.editorRecovery);
```

Capability-gated options like `recovery: "editor"` follow the project's
fail-closed rule: if the installed build does not support one, the call
throws a clear error instead of quietly parsing with less than you asked
for.

## What is tested

The retained `tests/parser-api` suite proves the program crosses the native
boundary as one versioned payload (including `BigInt` and `RegExp` values),
that the loader's integrity checks reject tampered binaries, that the
package stays safe to bundle with tools like Rolldown, and that lazy fields
convert correctly in files with multi-byte characters. The recorded terminal
output above is captured by really running the scripts at build time.

## Next steps

- See exactly which TSRX syntax the parser recognizes in
  [TSRX Syntax](/guide/tsrx-syntax).
- Lint and format the same files with the CLI tools in
  [Getting Started](/guide/getting-started).
- Curious how one parse serves lint, format, and this API? Read
  [Architecture](/architecture/rust-oxc-core).
