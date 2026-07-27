---
title: Custom JavaScript plugins
description: Write a custom lint rule with the oxlint that oxc-tsrx installs and run it on .tsrx files, with positions in your authored source.
---

# Custom JavaScript plugins

`npm install oxc-tsrx` puts an `oxlint` command on your PATH, and that command
already lints `.tsrx`. So this page starts with the linter you have, not with a
second one you would have to install.

You write one ordinary Oxlint JavaScript plugin, list it in `.oxlintrc.json`,
and it runs on both halves of your project: on `.js`, `.ts`, `.jsx`, and `.tsx`
directly, and on `.tsrx` through the TSX projection, with every diagnostic
reported at the line and column of the file you wrote. It runs in your editor
too, at the same positions. There is no second linter and no separate plugin
format.

The `.tsrx` half costs one extra parse per file, and `oxlint` says so on stderr
every time it does it. [Turning the extra parse
off](#turning-the-extra-parse-off) covers when you might want that. If your rule
needs to see TSRX control syntax as its own node types rather than as the
JavaScript it compiles to, read [what your rule sees on
`.tsrx`](#what-your-rule-sees-on-tsrx) before you write it.

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

## The same plugin on `.tsrx`

Leave everything exactly as it is and point the same command at the `.tsrx`
file instead:

<!-- terminal-demo:custom-plugins-tsrx-plugin -->

Two things happened there. The `oxlint (oxc-tsrx):` line is the disclosure:
linting `.tsrx` with a JavaScript plugin costs one more parse of that file, and
the command tells you so every time, naming the setting that turns it off. It
goes to stderr, so in a terminal it arrives before the report; the transcript
above prints stdout first and stderr after it, which is why it reads last there.

The other thing is that your rule ran and found nothing, because
`require-keyed-map` looks for a `.map()` call and `TaskList.tsrx` has an `@for`
block instead. The built-in `no-debugger` rule still reported, from the native
Rust lane, exactly as it did before you added a plugin.

Give the rule something to find. Add this as `src/TaskFeed.tsrx`:

```tsrx
type Task = { id: string; label: string; done: boolean };

export function TaskFeed({ tasks }: { tasks: Task[] }) @{
  const rows = tasks.map((task) => <li>{task.label}</li>);

  <ul class="feed">{rows}</ul>;
}
```

<!-- terminal-demo:custom-plugins-tsrx-map -->

That is your own JavaScript rule, reporting a problem in a `.tsrx` file, at the
column of the `<li>` you wrote. Line 4, column 36 is where that `<li>` really
is in the file above.

One command over a directory holding both file types does both halves at once:

<!-- terminal-demo:custom-plugins-mixed-directory -->

## The same plugin in your editor

Nothing extra to configure: open `src/TaskFeed.tsrx` with the official OXC
extension installed and your rule is a squiggle, at the same line and column
`oxlint` just reported, beside the built-in Rust ones.

The language server does the same thing the command line does. It projects the
buffer you are editing, hands that projection to a small Node.js host, and runs
the published Oxlint binary over it, then maps the results back onto your bytes.
Three practical consequences:

- **It re-lints on open, change, and save**, so the extra parse happens per lint
  rather than once. The Node.js host is started once per workspace, and only if
  your config declares `jsPlugins`.
- **It announces itself once**, in the server's output log, with the same
  `jsPluginsOnTsrx` key the command line names. Nothing appears in your editor
  UI.
- **A broken plugin does not take your other diagnostics with it.** If the lane
  cannot start or one of your rules throws, the built-in rules still publish and
  a `js-plugins-unavailable` warning carries the reason. If one of your reports
  had no position in the source you wrote, it is dropped and a
  `js-plugins-unmapped` warning says how many, so an empty Problems panel is
  never how you find out.

[Editor integration](/integrations/editor#your-own-javascript-rules-in-the-editor)
has the rest, including the one activation step the official extension needs.

## How it runs, and what it costs

`.tsrx` files are linted by a native Rust process, and that process has no
Node.js runtime to run your module in. What it does have is a *projection*: one
legal TSX rendering of your `.tsrx` source, which it already builds to run OXC's
built-in rules, plus a byte-for-byte map from positions in that projection back
to positions in what you wrote.

So `oxlint` writes each projection into a throwaway directory, runs the
published Oxlint binary over it with your `.oxlintrc.json`, and sends the
diagnostics back through that map. Your severities, rule options, `extends`,
and `overrides` are resolved by Oxlint itself, from your own config, so a rule
behaves the same on `.tsrx` as it does anywhere else.

The cost is one extra parse per `.tsrx` file, and it is never silent. Every run
that does it writes one line to stderr, ahead of the report, which is the
`oxlint (oxc-tsrx):` line in both runs above. `--silent` suppresses it along
with everything else. A `--format=json` report carries the same fact as data,
under `oxcTsrx.jsPluginProjection`, as
`{ "files": N, "extraParses": N, "unmapped": N }`. `unmapped` is how many of
your plugin diagnostics could not be placed in the source you wrote; the next
section explains when that happens.

If the installed Oxlint is outside the range this route was built against
(`>=1.74.0 <2.0.0`), the command refuses and exits 1 rather than running with
your rules quietly switched off.

## What your rule sees on `.tsrx`

Your rule is handed the projection, not your authored TSRX tree. Four
consequences, in the order they tend to bite:

**TSRX control syntax is already compiled away.** `@if`, `@for`, `@switch`, and
`@try` do not reach your rule as `JSXIfExpression` and friends; they arrive as
the ordinary `if`, `for`, and `switch` statements they project to. A rule keyed
on `JSXForExpression` will never fire on this route. If that is the rule you
need, use [the ESLint route](#when-your-rule-must-see-authored-tsrx-nodes-eslint),
which parses your file directly.

**`context.filename` is the projection's path, not yours.** It points inside the
throwaway directory and ends in `.tsrx.tsx`: a `src/View.tsrx` in your project
is `<temporary directory>/src/View.tsrx.tsx` to your rule. The path relative to
your working directory is preserved, so a rule that tests for `src/` still
works, but one that compares against an absolute project path, or that expects
the extension to be `.tsrx`, does not. The diagnostic itself is still reported
against `src/View.tsrx`, which is what you and your editor see. This holds in
the editor as well, with a different throwaway directory per session.

**A diagnostic that lands on projected-only text is dropped, and counted.** The
projection inserts markers and wrappers that correspond to nothing you typed. If
a rule reports on one of those, there is no authored position to point at, so
the diagnostic is discarded rather than reported at an invented location.

That drop is never silent. A run that discards any of your diagnostics writes a
second `oxlint (oxc-tsrx):` line to stderr saying how many, and the same number
is in `oxcTsrx.jsPluginProjection.unmapped` in a `--format=json` report. In the
editor it arrives as one `js-plugins-unmapped` warning on the file. So a rule
that fires on `.tsx` and reports nothing on `.tsrx` is something you are told
about rather than something you have to suspect.

A report on the whole `Program` is not one of these, even though its span
covers the projection, markers and all. It is mapped to your authored file from
its first token to the end, which is the same place the same rule lands on an
ordinary `.tsx` file. Whatever sits above that first token — a comment, a blank
line, `// @ts-nocheck` — makes no difference. The same holds for any report that
runs to the end of the file across text the projection rewrote.

What is still dropped is a report whose span sits partly on a marker and partly
on code you wrote and stops short of the end of the file, which happens when a
rule reports on a node the projection rewrote rather than on one of your own
tokens. If that is the rule you need,
[the ESLint route](#when-your-rule-must-see-authored-tsrx-nodes-eslint) parses
your file directly.

**An `overrides` glob written for `.tsrx` is matched for you, in your own config
only.** The projection is named `View.tsrx.tsx`, which `**/*.tsrx` does not
match, so `oxlint` also emits each of your `overrides[].files` and
`excludeFiles` globs with `.tsx` appended. A config reached through `extends`
does not get that rewrite yet, so a `.tsrx`-targeted override in a shared config
will not apply on this route. Put those overrides in the config that names
`jsPlugins`.

## Turning the extra parse off

If you would rather not pay the second parse, say so in `settings`:

```json
{
  "jsPlugins": ["./oxlint-demo-plugin.mjs"],
  "rules": {
    "tsrx-demo/require-keyed-map": "error"
  },
  "settings": {
    "oxcTsrx": {
      "jsPluginsOnTsrx": false
    }
  }
}
```

Your plugins keep running on ordinary files. On `.tsrx` the command now refuses
out loud rather than dropping your rule and reporting success:

<!-- terminal-demo:custom-plugins-tsrx-opt-out -->

The same setting turns the editor's lane off. There, the refusal arrives as one
`lint-unavailable` diagnostic on the file, carrying the same text, so an empty
Problems panel is never how you find out.

The [configuration guide](/integrations/configuration#jsplugins-and-the-two-lanes)
has the full support matrix for what the native `.tsrx` lane accepts.

## When your rule must see authored TSRX nodes: ESLint

Everything above hands your rule the projection, where `@if` and `@for` have
already become `if` and `for`. If your rule is *about* that syntax, you need the
authored tree, and for that there is still one route: ESLint's public parser
slot.

This is an escape hatch, not the recommended default. ESLint is a second linter
to install and configure, it is not part of `oxc-tsrx`, and it does not reuse
any of the native `.tsrx` work above.

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

The error is gone and the warning you asked for stays. Both rules read node
types that only exist in the authored tree, which is the one thing this route
still buys you over running the same rule inside `oxlint`.

## What the ESLint route does not do

It is **AST-only**. Rules that read the tree work. Two things do not, and it is
better to hit them here than halfway through writing a rule.

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
| The `oxlint` `oxc-tsrx` installs, on `.tsrx` | The native Rust TSRX parser, then the TSX projection | The same Oxlint JS plugin, plus native Rust rules | Shipping; one extra parse per file, disclosed on stderr |
| Your editor, on `.tsrx` | The same projection, from the in-memory buffer | The same Oxlint JS plugin, plus native Rust rules | Shipping; one extra parse per lint, disclosed in the server log |
| Upstream `oxlint`, on `.tsrx` | none | none | Released upstream Oxlint cannot parse `.tsrx` at all |
| ESLint (its own process) | A `parseForESLint` adapter you copy | A normal ESLint plugin | Works for AST-only rules that need authored TSRX nodes; proven by an ESLint 10 test |
| A Vite plugin (dev/build process) | The repo's TSRX parser service | An ordinary Vite plugin calling `this.warn` | Works, but only as a source-local example in this repo |
| Native `oxc-tsrx-lint`, called directly | The native Rust TSRX projection | Native Rust rules only | Shipping; it is Rust with no Node.js runtime and refuses `jsPlugins`. Run `oxlint` instead |
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
Oxlint's `jsPlugins` in its `lint` block, and those plugins reach both halves of
the project the same way they do from a plain `.oxlintrc.json`.

The runnable version of everything above lives in
`examples/custom-js-plugins`. Its tests use the real parser, ESLint 10.7.0,
Vite 8.1.5, and `@tsrx/vite-plugin-react` 0.0.72. Oxlint in this repository is
pinned and tested at 1.74.0; public releases may have moved past that.

## Status and what is coming

Running your rule on `.tsrx` works today, on the command line and in the editor,
from published packages only, using the projection route described above. It
does not depend on any unmerged upstream change. It was last proved from a
clean project built out of `npm pack` tarballs: the user's rule reported at the
authored positions from `oxlint`, the language server published the same rule at
the same positions, and the built-in Rust rules kept reporting in both.

What is still upstream is running a JavaScript rule against the *authored* TSRX
tree inside Oxlint, which is what would make `JSXIfExpression` and
`JSXForExpression` visible to an Oxlint plugin. OXC PR
[#24262](https://github.com/oxc-project/oxc/pull/24262) adds
`overrides[].languageOptions.parser` routing for Oxlint's JS-plugin host. As of
2026-07-26 it is still a Draft, and it is a local source build, not something
you can install. The wider language-plugin idea ([discussion
#21936](https://github.com/oxc-project/oxc/discussions/21936)) is still a
discussion. Until one of them lands, a rule about TSRX control syntax itself
belongs on [the ESLint
route](#when-your-rule-must-see-authored-tsrx-nodes-eslint).

`examples/vscode-lints/README.md` has an editor demo of that draft: the official
OXC VS Code extension, pointed at a workspace-local launcher, showing
`tsrx-demo(no-tsrx-if)` on an authored `@if` block. It proves you do not need a
second VS Code extension; it does not make the upstream draft released.

Last audited: 2026-07-27.
