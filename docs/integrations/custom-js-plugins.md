---
title: Custom JavaScript plugins
description: Write a custom lint rule with the oxlint that oxc-tsrx installs, and see exactly where a JavaScript rule can and cannot run against .tsrx today.
---

# Custom JavaScript plugins

`npm install oxc-tsrx` puts an `oxlint` command on your PATH, and that command
already lints `.tsrx`. So this page starts with the linter you have, not with a
second one you would have to install.

Keep one split in your head, the same split a Vite user already thinks in: file
types. On ordinary `.js`, `.ts`, `.jsx`, and `.tsx` files, this `oxlint` runs
your own JavaScript plugins today. On `.tsrx` files it runs OXC's built-in Rust
rules only, and it refuses a JavaScript plugin with an explicit error rather
than skipping it quietly. If you need a JavaScript rule on `.tsrx` today, jump
to [the ESLint route](#if-you-need-a-javascript-rule-on-tsrx-today-eslint).

## Set up the project

You need Node.js 20.19 or newer, and one install:

<!-- pm-install -->
```sh
npm install oxc-tsrx
```

Save this as `src/TaskList.tsrx`:

```tsrx
type Task = { id: string; label: string; done: boolean };

export function TaskList({ tasks, ready }: { tasks: Task[]; ready: boolean }) @{
  debugger;

  @if (ready) {
    <ul class="tasks">
      @for (const task of tasks) {
        <li>{task.label}</li>;
      }
    </ul>;
  } @else {
    <p>Loading tasks</p>;
  }
}
```

The `@for` block has no `key`, on purpose, and the `debugger` statement is
there on purpose too.

## Run the linter you already have

<!-- terminal-demo:custom-plugins-first-run -->

No config file, no second linter, no build step. `oxlint` read your authored
`.tsrx` and reported a built-in OXC rule against the line you wrote. Those
built-in rules are Rust, so you cannot add one yourself. What you can add is a
JavaScript plugin, and the rest of this page is about where that plugin runs.

## What are the nodes called?

A lint rule is a set of callbacks named after node types, so the first question
is always the same: what are the nodes called? Answer it by parsing the file and
printing what comes back.

Save this as `explore-tsrx-ast.mjs`:

```js
import { readFileSync } from "node:fs";
import { parseSync } from "oxc-tsrx/parser";

const file = "src/TaskList.tsrx";
const result = parseSync(file, readFileSync(file, "utf8"));

function* walk(node) {
  if (Array.isArray(node)) {
    for (const item of node) yield* walk(item);
  } else if (node && typeof node === "object") {
    if (typeof node.type === "string") yield node;
    for (const value of Object.values(node)) yield* walk(value);
  }
}

for (const node of walk(result.program)) {
  if (node.type.startsWith("JSX") && node.type.endsWith("Expression")) {
    console.log(node.type);
  }
}
```

<!-- terminal-demo:custom-plugins-explore -->

There are your two node names. Every TSRX control block gets its own node type,
shaped like the JSX nodes you already know:

| You write | The node you visit |
| --- | --- |
| `@if` / `@else` | `JSXIfExpression` |
| `@for` / `@empty` | `JSXForExpression` |
| `@switch` / `@case` / `@default` | `JSXSwitchExpression` |
| `@try` / `@pending` / `@catch` | `JSXTryExpression` |
| `@{ }` statement containers | `JSXCodeBlock` |

Everything else, including elements, attributes, statements, and TypeScript
types, uses the same shapes as `oxc-parser`. The [Parsing
guide](/guide/parsing) covers the tree in more depth.

## Write an oxlint JavaScript plugin

Your plugin runs on the ordinary half of the project, so give the project an
ordinary half: a plain React component. Save this as `src/TaskRow.tsx`:

```tsx
type Task = { id: string; label: string; done: boolean };

export function TaskRow({ task }: { task: Task }) {
  return <li className={task.done ? "done" : ""}>{task.label}</li>;
}

export function TaskRows({ tasks }: { tasks: Task[] }) {
  return (
    <ul className="tasks">
      {tasks.map((task) => (
        <TaskRow task={task} />
      ))}
    </ul>
  );
}
```

Its `.map()` call has the same missing-key problem as the `@for` block in the
`.tsrx` file. One rule idea, two file types: that is the whole tour.

An Oxlint plugin is an ES module whose default export is `{ meta, rules }`.
Every rule gets a `create(context)` that returns a visitor object keyed by node
type. If you have written an ESLint rule before, this will look familiar.

Save this as `oxlint-demo-plugin.mjs`:

```js
// An Oxlint JavaScript plugin. The default export is `{ meta, rules }`, and
// each rule's `create(context)` returns a visitor keyed by node type. Oxlint
// runs this on ordinary .js/.ts/.jsx/.tsx files.

function hasKeyProp(element) {
  return element.openingElement.attributes.some(
    (attribute) => attribute.type === "JSXAttribute" && attribute.name.name === "key",
  );
}

const requireKeyedMap = {
  meta: {
    type: "problem",
    docs: { description: "Require a key prop on JSX returned straight from .map()" },
    messages: { missing: "JSX returned from .map() should declare a `key` prop." },
    schema: [],
  },
  create(context) {
    return {
      CallExpression(node) {
        if (node.callee.type !== "MemberExpression") return;
        if (node.callee.property.name !== "map") return;
        const returned = node.arguments[0]?.body;
        if (returned?.type !== "JSXElement" || hasKeyProp(returned)) return;
        context.report({ node: returned, messageId: "missing" });
      },
    };
  },
};

export default {
  meta: { name: "tsrx-demo", version: "0.1.0" },
  rules: { "require-keyed-map": requireKeyedMap },
};
```

Reading it top to bottom:

- `create(context)` returns an object whose keys are node type names. Oxlint
  calls `CallExpression(node)` for every call expression it walks past.
- `context.report({ node, messageId })` is how you raise a problem. Passing
  `node` is what gives you the line and column of the code you wrote.
- `meta.messages` holds the wording, keyed by id, so the text lives in one place.
- `meta.name` on the plugin is the prefix your rules are configured under, so
  this one is `tsrx-demo/require-keyed-map`.

Oxlint only loads a plugin you list, and only turns on a rule you enable. Save
this as `.oxlintrc.json`:

```json
{
  "jsPlugins": ["./oxlint-demo-plugin.mjs"],
  "rules": {
    "tsrx-demo/require-keyed-map": "error"
  }
}
```

<!-- terminal-demo:custom-plugins-oxlint-plugin -->

That is your own JavaScript, running inside the `oxlint` that `oxc-tsrx`
installed, with no other linter involved.

## The wall: a JavaScript plugin does not run on `.tsrx` yet

Leave everything exactly as it is and point the same command at the `.tsrx`
file instead:

<!-- terminal-demo:custom-plugins-tsrx-wall -->

That is exit code 2, and it is deliberate rather than a bug. `.tsrx` files are
linted by a separate native Rust process, and that process has no Node.js
plugin host to run your module in. OXC for TSRX will not silently parse your
file a second time in Node just to run a plugin, so it refuses out loud instead
of dropping your rule and reporting success.

The [configuration guide](/integrations/configuration) has the full support
matrix for what the native `.tsrx` lane accepts.

You get the same refusal when you point one command at a directory holding both
file types:

<!-- terminal-demo:custom-plugins-mixed-directory -->

The ordinary half is still linted and still reported in the normal format, the
`.tsrx` half refuses out loud, and the command exits 2. Nothing is silently
dropped in either direction.

## If you need a JavaScript rule on `.tsrx` today: ESLint

This is an escape hatch, not the recommended default. ESLint is a second linter
to install and configure, it is not part of `oxc-tsrx`, and it does not reuse
any of the native `.tsrx` work above. What it does have is a public parser slot,
so you can hand it the authored TSRX tree yourself.

<!-- pm-install -->
```sh
npm install --save-dev eslint
```

### Copy the parser adapter once

ESLint does not know what a `.tsrx` file is. You fix that with a *parser
adapter*: a module exporting `parseForESLint`, which ESLint calls instead of its
own parser. Copy `examples/custom-js-plugins/tsrx-eslint-parser.mjs` from this
repository into your project as `tsrx-eslint-parser.mjs`. It is about 120 lines,
and most of it is offset bookkeeping. This is the part that matters:

```js
export function parseForESLint(sourceText, options = {}) {
  const filePath = options.filePath ?? "input.tsrx";
  const result = parseSync(filePath, sourceText, {
    astType: "ts",
    lang: "tsrx",
    preserveParens: false,
    range: true,
    sourceType: options.sourceType === "script" ? "script" : "module",
  });
  if (result.errors.length > 0) throw syntaxError(sourceText, result.errors[0]);
  if (!result.program) throw new SyntaxError(`TSRX parser returned no Program for ${filePath}`);

  const visitorKeys = prepareForEslint(result.program, result.comments, sourceText);
  return {
    ast: result.program,
    visitorKeys,
    services: {
      isTsrx: true,
      parser: "oxc-tsrx/parser",
    },
  };
}
```

The rest of the file does four supporting jobs, each for a reason:

- **Adds `range` and `loc` to every node**, because that is how ESLint knows
  where to underline a problem.
- **Builds `visitorKeys`**, a map of node type to child property names. ESLint
  uses it to walk into node types it has never heard of. Without it, your rules
  would never be called for TSRX nodes.
- **Converts comments**, so directives like `// eslint-disable-next-line` work.
- **Turns parse errors into `SyntaxError`**, which ESLint reports as an ordinary
  `Parsing error` message rather than a crash.

The in-repo copy imports the parser by relative path
(`../../packages/toolchain/dist/parser.js`) so the repository's own tests can
load it without an install. In your project, change that one line to
`oxc-tsrx/parser`. Both resolve to the same module.

### Write the rules

The rules themselves are ordinary ESLint rules that happen to name TSRX nodes.
Save this as `demo-lint-plugin.mjs`:

```js
const noTsrxIf = {
  meta: {
    type: "suggestion",
    docs: {
      description: "Demo a JavaScript rule visiting authored TSRX control syntax",
    },
    messages: {
      avoid: "Demo rule: prefer a declarative component over this TSRX @if block.",
    },
    schema: [],
  },
  create(context) {
    return {
      JSXIfExpression(node) {
        context.report({ node, messageId: "avoid" });
      },
    };
  },
};

const requireKeyedFor = {
  meta: {
    type: "problem",
    docs: {
      description: "Require a key expression on TSRX @for blocks",
    },
    messages: {
      missing: "TSRX @for blocks should declare `key <expression>`.",
    },
    schema: [],
  },
  create(context) {
    return {
      JSXForExpression(node) {
        if (node.key == null) context.report({ node, messageId: "missing" });
      },
    };
  },
};

export default {
  meta: {
    name: "eslint-plugin-tsrx-demo",
    version: "0.1.0",
  },
  rules: {
    "no-tsrx-if": noTsrxIf,
    "require-keyed-for": requireKeyedFor,
  },
};
```

`requireKeyedFor` reads `node.key`, a real field on `JSXForExpression`. That is
the whole trick: once a parser hands you the authored tree, checking TSRX syntax
is ordinary JavaScript.

### Wire it up and run it

Save this as `eslint.config.mjs`:

```js
import tsrxDemo from "./demo-lint-plugin.mjs";
import tsrxParser from "./tsrx-eslint-parser.mjs";

export default [
  {
    files: ["**/*.tsrx"],
    languageOptions: { parser: tsrxParser, sourceType: "module" },
    plugins: { "tsrx-demo": tsrxDemo },
    rules: {
      "tsrx-demo/no-tsrx-if": "warn",
      "tsrx-demo/require-keyed-for": "error",
    },
  },
];
```

The `files: ["**/*.tsrx"]` line matters more than it looks. ESLint only lints
extensions a config block claims. Leave it out and ESLint reports the file as
ignored because no matching configuration was supplied, which is not an obvious
way of saying "no block matched your extension".

<!-- terminal-demo:custom-plugins-eslint -->

Both rules fired, on the `@if` and the `@for` you wrote. Now give the `@for`
block the key it was missing:

```tsrx
@for (const task of tasks; key task.id) {
```

<!-- terminal-demo:custom-plugins-eslint-fixed -->

The error is gone and the warning you asked for stays. That is a complete
custom rule for `.tsrx`, running in a standard tool.

## What works, and what does not yet

The ESLint route is **AST-only**. Rules that read the tree work. Two things do
not, and it is better to hit them here than halfway through writing a rule.

**There are no tokens.** The v1 parser API does not expose OXC's token stream,
so the adapter sets `program.tokens = []` rather than faking it.
`sourceCode.getText()` works, but `sourceCode.getFirstToken(node)` returns
`null`, and any rule built on token methods cannot be correct here.

**Scope is not guaranteed.** Ordinary ESTree descendants are traversed
normally, but there is no framework scope contract yet, so binding and scope
behavior around TSRX control syntax is not something to rely on.

## Where a custom check can run today

There is no single "TSRX plugin" format. Each row below is a different program
that could run a check, with the parser that feeds it and how real it is:

| Where the check runs | Parser it uses | Plugin shape | How real today |
| --- | --- | --- | --- |
| The `oxlint` `oxc-tsrx` installs, on ordinary `.js`/`.ts`/`.tsx` | OXC's own parser | An Oxlint JS plugin | Shipping; this is the walkthrough above |
| The `oxlint` `oxc-tsrx` installs, on `.tsrx` | The native Rust TSRX parser | Native Rust rules only | Shipping; it lints `.tsrx` and refuses JS plugins |
| Upstream `oxlint`, on `.tsrx` | none | none | Released upstream Oxlint cannot parse `.tsrx` at all |
| ESLint (its own process) | A `parseForESLint` adapter you copy | A normal ESLint plugin | Works for AST-only rules; proven by an ESLint 10 test |
| A Vite plugin (dev/build process) | The repo's TSRX parser service | An ordinary Vite plugin calling `this.warn` | Works, but only as a source-local example in this repo |
| Draft upstream Oxlint, on `.tsrx` | The same `parseForESLint` adapter | Oxlint JS plugins plus a draft custom-parser hook | An unmerged upstream draft, built from source |
| Native `oxc-tsrx-lsp` | The native Rust TSRX projection | Native Rust rules only | Shipping today, but Rust only; it runs no JavaScript |
| `oxc-tsrx/lint/plugins-dev` | none | Re-exports Oxlint's `RuleTester` | Real, and useful for unit-testing a rule; it is not a host |

Two more things trip people up often enough to state plainly:

- **`oxc-tsrx/lint/plugins-dev` is not a host.** It is one export, Oxlint's
  `RuleTester`, for testing a rule you wrote. It does not run one against
  `.tsrx`.
- **The official OXC VS Code extension is a client, not a rule runtime.** When a
  custom TSRX rule shows a squiggle, the extension is only displaying it.

A Vite plugin can read the authored TSRX AST too, through a pre-transform parser
service that parses each `.tsrx` file once and caches it. That is a source-local
example rather than an installable API; see
[Vite and Vite+](/integrations/vite-plus) for how it composes. Vite+ surfaces
Oxlint's `jsPlugins` in its `lint` block, which is the same split as everywhere
else on this page: ordinary files get your JS plugins, `.tsrx` does not.

The runnable version of everything above lives in
`examples/custom-js-plugins`. Its tests use the real parser, ESLint 10.7.0,
Vite 8.1.5, and `@tsrx/vite-plugin-react` 0.0.72. Oxlint in this repository is
pinned and tested at 1.74.0; public releases may have moved past that.

## Status and what is coming

Running a JavaScript rule against `.tsrx` *inside Oxlint* is an upstream draft,
not a release. OXC PR
[#24262](https://github.com/oxc-project/oxc/pull/24262) adds
`overrides[].languageOptions.parser` routing for Oxlint's JS-plugin host. As of
2026-07-24 it is still a Draft, and it is a local source build, not something
you can install. The wider language-plugin idea ([discussion
#21936](https://github.com/oxc-project/oxc/discussions/21936)) is still a
discussion.

`examples/vscode-lints/README.md` has an editor demo of that draft: the official
OXC VS Code extension, pointed at a workspace-local launcher, showing
`tsrx-demo(no-tsrx-if)` on an authored `@if` block. It proves you do not need a
second VS Code extension; it does not make the upstream draft released.

Last audited: 2026-07-26.
