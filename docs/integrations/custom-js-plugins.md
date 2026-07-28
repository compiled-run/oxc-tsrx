---
title: Custom JavaScript plugins
description: Write a custom lint rule with the oxlint that oxc-tsrx installs and run it on .tsrx files, with positions in your authored source.
---

# Custom JavaScript plugins

`npm install oxc-tsrx@latest` puts an `oxlint` command on your PATH, and that command
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

Two ways to read this page. The sections below build the idea up from an empty
directory, one command at a time, and they are the shortest route to
understanding what runs where. If you already have a Vite+ app from `vp create`,
or you want the complete configured project rather than the explanation, skip to
[the whole path on a fresh Vite+
project](#the-whole-path-on-a-fresh-vite-project), which is the same route with
every scaffold-specific step filled in.

## Set up the project

You need Node.js 20.19 or newer, and one install:

<!-- pm-install -->
```sh
npm install oxc-tsrx@latest
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

## The whole path, on a fresh Vite+ project

Everything above assumed a plain project: one install, a file, a config. A real
TSRX app is usually a [Vite+](/integrations/vite-plus) project scaffolded by
`vp create`, and that route has more moving parts. Three of them belong to the
TSRX toolchain rather than to this package, and `setup` tells you so instead of
doing them.

This section is that whole route, in order, with nothing left out. It was run
end to end on a scaffold created for it: Node.js 24.15.0, pnpm 11.17.0,
Vite+ 0.2.6, `oxc-tsrx` 0.1.4, `oxlint-tsgolint` 0.24.0,
`@tsrx/typescript-plugin` 0.3.112, `@tsrx/react` 0.2.50, TypeScript 6.0.3, on
macOS arm64.

### 1. Scaffold, and install this package

You do not have `vp` yet, and there is nothing to install to get it. Run it from
the registry, naming the Vite+ version you want:

<!-- typed-commands -->
```sh
npx --yes -p vite-plus@0.2.6 vp create vite -- my-app --template react-ts
cd my-app
```

**From here on, use the pnpm tab, not the npm one.** `vp create` writes a
`devEngines.packageManager` block into `package.json` naming pnpm, and npm then
refuses to run in that project at all, exiting with `npm error code
EBADDEVENGINES` and `npm error Invalid name "pnpm" does not match "npm"`. That
is npm honouring your own project's declaration, not a failure of anything here.
Every transcript below was captured with the pnpm tab's commands, which is why
`setup` prints `(pnpm)`. Pick whichever manager your scaffold names.

<!-- pm-install -->
```sh
npm install --save-dev oxc-tsrx@latest
npx oxc-tsrx setup
```

### 2. Read what `setup` says it will not do

<!-- terminal-demo:custom-plugins-vp-setup -->

The first four lines are the slots `setup` owns. Three are packages inside
`node_modules` that Vite+ resolves by name; the fourth is one key merged into
your own `.vscode/settings.json`, which is the only file `setup` writes outside
`node_modules`. [The Vite+ page](/integrations/vite-plus#setup-writes-one-file-in-your-tree-and-it-says-so)
has the rules it follows there.

The lines marked `!` are the boundary. `setup` never installs a package, never
edits `package.json`, and never edits a `tsconfig.json`, so TSRX *language*
support is reported and left alone. The next step is you doing those four
things.

### 3. Do the part `setup` will not do

Install the TypeScript plugin that teaches an editor what `.tsrx` is, plus one
framework binding. `@tsrx/react` is the React one; `@tsrx/vue`, `@tsrx/solid`,
`@tsrx/preact`, `@tsrx/ripple`, and `octane` are the others.

<!-- pm-install -->
```sh
npm install --save-dev @tsrx/typescript-plugin @tsrx/react
```

Then declare the plugin **in the tsconfig that owns your source**. In a Vite+
scaffold that is `tsconfig.app.json`, the project whose `include` names `src`.
Add one key to the `compilerOptions` block already there; leave everything else
alone:

```json
{
  "compilerOptions": {
    "plugins": [{ "name": "@tsrx/typescript-plugin" }]
  },
  "include": ["src"]
}
```

Not the root `tsconfig.json`. A `vp create` scaffold writes a solution-style
root, `{ "files": [], "references": [...] }`, which owns no files at all, so a
`plugins` entry there applies to nothing and fails by doing nothing. That is the
single most expensive way to get this wrong, and it is why `setup` names the
referenced project by path rather than saying "your tsconfig".

One expected warning: `vp create` scaffolds TypeScript 6, and
`@tsrx/typescript-plugin` declares `peerDependencies.typescript: ^5.9.3`, so a
stock scaffold is outside the declared range and your package manager will say
so. That is a support statement, not a failure. A stock scaffold on TypeScript
6.0.3 was measured working while this page was written. If the language service
does misbehave, pinning TypeScript into the declared range is the first thing to
try.

### 4. Check the boundary is closed

<!-- terminal-demo:custom-plugins-vp-status -->

Three of the four `!` lines are gone, because you did them. The remaining one is
the TypeScript range from the previous step, and it stays until you pin
TypeScript yourself.

### 5. Write the rule

Save this as `house-rules.mjs`:

```js
// A house rule: an ordinary Oxlint JavaScript plugin. The default export is
// `{ meta, rules }`, and each rule's `create(context)` returns a visitor keyed
// by AST node type. Nothing here is TSRX-specific.

const noInlineStyleObject = {
  meta: {
    type: "suggestion",
    docs: { description: "Prefer a class over an inline style object" },
    messages: { inline: "Inline `style={{ ... }}` object. Use a class instead." },
    schema: [],
  },
  create(context) {
    return {
      JSXAttribute(node) {
        if (node.name?.name !== "style") return;
        if (node.value?.type !== "JSXExpressionContainer") return;
        if (node.value.expression?.type !== "ObjectExpression") return;
        context.report({ node, messageId: "inline" });
      },
    };
  },
};

export default {
  meta: { name: "house-rules", version: "1.0.0" },
  rules: { "no-inline-style-object": noInlineStyleObject },
};
```

Save this as `.oxlintrc.json`:

```json
{
  "jsPlugins": ["./house-rules.mjs"],
  "rules": {
    "house-rules/no-inline-style-object": "warn"
  }
}
```

One **top-level** `jsPlugins`, with no `overrides` block. That is deliberate: a
single top-level declaration serves a mixed batch correctly, and the next step
is the measurement that says so.

Now give the rule one of each file type to find. Save this as
`src/Greeting.tsrx`:

```tsrx
type Guest = { id: string; name: string };

export function Greeting({ guests, ready }: { guests: Guest[]; ready: boolean }) @{
  @if (ready) {
    <ul style={{ padding: 0 }}>
      @for (const guest of guests; key guest.id) {
        <li>{guest.name}</li>;
      }
    </ul>;
  } @else {
    <p>Loading</p>;
  }
}
```

And save this as `src/Panel.tsx`:

```tsx
export function Panel({ label }: { label: string }) {
  return <section style={{ margin: 0 }}>{label}</section>;
}
```

### 6. Run it on both halves at once

The command is a path rather than `npx oxlint`, because in a Vite+ project that
name belongs to Vite+; step 7 is about that. This is the `oxlint` this package
installed, at the same path `setup` just wrote into `.vscode/settings.json`.

<!-- terminal-demo:custom-plugins-vp-cli -->

One command, one plugin, one top-level `jsPlugins`, both file types, and both
diagnostics at the positions in the files you wrote. `src/Greeting.tsrx:5:9` is
the `style` attribute on the `<ul>`, and `src/Panel.tsx:2:19` is the one on the
`<section>`.

**No `overrides` block was needed for that.** An earlier version of this page
suggested scoping `jsPlugins` under `overrides` in a Vite+ project to keep a
mixed batch from being poisoned. Re-measured on the scaffold above, a top-level
declaration handles the mixed batch, so the extra scoping is not part of this
route. Reach for `overrides` when you actually want different rules on the two
halves, not as a workaround.

### 7. In a Vite+ project, the `oxlint` on your PATH is Vite+'s

This is the one command that does not behave the way it does elsewhere in these
docs, which is why step 6 spells out a path instead of typing `npx oxlint`. In a
Vite+ project `node_modules/.bin/oxlint` belongs to Vite+, and running it prints
`This oxlint wrapper is for IDE extension use only (--lsp mode). To lint your
code, run: vp lint` and exits 1 without linting anything.

That is Vite+ being correct about its own project. Your two working commands are
`node_modules/oxc-tsrx/bin/oxlint`, which is this package's own and the same path
`setup` wrote into `.vscode/settings.json`, and `vp lint`, which needs one more
edit; see step 9. [The Vite+ page](/integrations/vite-plus#oxlint-and-oxfmt-on-the-command-line-belong-to-vite-here)
has the shim in full.

### 8. The same rule in your editor

Open `src/Greeting.tsrx` with the official OXC extension installed and the rule
is a squiggle, beside the built-in Rust ones. There is nothing further to
configure: `setup` already pointed the extension at this package's `oxlint`, and
the language server reads the same `.oxlintrc.json` the command line just used.

The position is the same one step 6 printed. That is not a hope; the language
server and `oxlint` are held to it by `pnpm run test:plugins`, which drives one
real user plugin over one real `.tsrx` file through both surfaces and fails if
they disagree about a position.
[Editor integration](/integrations/editor#your-own-javascript-rules-in-the-editor)
covers the editor half, including the one-line extra-parse disclosure the server
writes to its output log and what happens when a plugin fails to load.

### 9. If you also want `vp lint` to run it

`vp lint` does not read `.oxlintrc.json`. Vite+ owns lint configuration in the
`lint` block of `vite.config.ts`, and `vp create` folds any scaffolded
`.oxlintrc.json` into that block on the way out. Measured on the project above:
a plugin declared only in `.oxlintrc.json` produces nothing from `vp lint`, and
a plugin declared only in `vite.config.ts` produces nothing in the editor. If
you want both surfaces, declare it in both places.

The `vite.config.ts` half is two additions to what `vp create` already wrote: a
`{ name, specifier }` entry in `lint.jsPlugins`, and a `lint.rules` entry.
Nothing is deleted, `lint.options` included:

```ts
import { defineConfig, lazyPlugins } from "vite-plus";
import react from "@vitejs/plugin-react";

// https://vite.dev/config/
export default defineConfig({
  staged: {
    "*": "vp check --fix",
  },
  fmt: {},
  lint: {
    plugins: ["react", "typescript", "oxc"],
    rules: {
      "react/rules-of-hooks": "error",
      "react/only-export-components": [
        "warn",
        {
          allowConstantExport: true,
        },
      ],
      "vite-plus/prefer-vite-plus-imports": "error",
      "house-rules/no-inline-style-object": "warn",
    },
    options: {
      typeAware: true,
      typeCheck: true,
    },
    jsPlugins: [
      {
        name: "vite-plus",
        specifier: "vite-plus/oxlint-plugin",
      },
      {
        name: "house-rules",
        specifier: "./house-rules.mjs",
      },
    ],
  },
  plugins: lazyPlugins(() => [react()]),
});
```

`lint.options` is the scaffold's type-aware default, and on Vite+ 0.2.6 it needs
one dependency that the scaffold does not have. Vite+ 0.2.6 depends on
`oxlint-tsgolint` 7.0.2001; this package runs the type-aware lane only against
0.24.0. Without a 0.24.0 in the project, `vp lint` on a batch containing `.tsrx`
stops with `unsupported tsgolint version 7.0.2001` and exits 2, after reporting
the ordinary half. Install the runner and the whole `lint` block works as
written:

<!-- pm-install -->
```sh
npm install --save-dev oxlint-tsgolint@0.24.0
```

That was measured on this scaffold, with `options` untouched: both `.tsrx` and
`.tsx` reported, type-aware included.
[The type-aware template default](/integrations/vite-plus#the-type-aware-template-default)
has all three runs, the resolution rule behind them, and the alternative if you
would rather delete `lint.options` than add a dependency.

Do not confuse that with the refusal below, which is a different thing with a
similar shape. `options.typeAware` in a plain `.oxlintrc.json` reaches the
direct `oxlint` command, which never starts a type process from config alone and
says so:

<!-- terminal-demo:custom-plugins-typeaware -->

A batch with no `.tsrx` file in it is unaffected by either, which is why a stock
scaffold lints fine until you add your first TSRX component.

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

**TSRX control syntax is already compiled away, and reporting on it does not
work.** `@if`, `@for`, `@switch`, and `@try` do not reach your rule as
`JSXIfExpression` and friends; they arrive as the ordinary `if`, `for`, and
`switch` statements they project to. A rule keyed on `JSXForExpression` will
never fire on this route.

Read the next sentence carefully, because the two facts together are easy to get
wrong. Your rule *visits* those projected statements, but a report *on* one of
them is dropped, because its span covers text the projection wrote rather than
text you did. A rule keyed on `IfStatement` therefore fires inside Oxlint and
produces nothing on a file whose `if` came from `@if`:

| rule keyed on | reported | dropped |
| --- | ---: | ---: |
| `JSXElement` | 7 | 0 |
| `IfStatement` | 0 | 1 |
| `SwitchStatement` | 0 | 1 |
| `FunctionDeclaration` | 0 | 1 |
| `Identifier` | 11 | 5 |

Measured on one `.tsrx` file using `@if`, `@switch`, and `@try`, with one rule
per node type.

The drops are counted, not silent: see `unmapped` below. But if your rule needs
to report on TSRX control flow, this route cannot do it. Use
[the ESLint route](#when-your-rule-must-see-authored-tsrx-nodes-eslint), which
parses your file directly.

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
own parser.

**The adapter is not in the `oxc-tsrx` npm package.** It lives in this project's
repository and nowhere else, so installing `oxc-tsrx` does not give it to you.
Download
[`tsrx-eslint-parser.mjs`](https://raw.githubusercontent.com/markless-dev/oxc-tsrx/main/examples/custom-js-plugins/tsrx-eslint-parser.mjs)
and save it in your project under that name. It sits at
`examples/custom-js-plugins/tsrx-eslint-parser.mjs`
[in the repository](https://github.com/markless-dev/oxc-tsrx/blob/main/examples/custom-js-plugins/tsrx-eslint-parser.mjs),
and the link tracks `main` rather than a release, because the file is not
versioned with the package. It is about 120 lines, and most of it is offset
bookkeeping. This is the part that matters:

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

The runnable version of everything above lives in `examples/custom-js-plugins`,
with the fresh-scaffold walkthrough's own files in
`examples/custom-js-plugins/vite-plus`. Its tests use the real parser,
ESLint 10.7.0, Vite 8.1.5, and `@tsrx/vite-plugin-react` 0.0.72. Oxlint in this
repository is pinned and tested at 1.74.0; public releases may have moved past
that.

## Status and what is coming

Running your rule on `.tsrx` works today, on the command line and in the editor,
using the projection route described above. It does not depend on any unmerged
upstream change. It was last proved on a scaffold created by `vp create` and
configured by following the walkthrough on this page and nothing else: the
reader's rule reported at the authored positions from `oxlint`, the language
server published the same rule at the same position, the ordinary `.tsx` half
reported from the same command, and the built-in Rust rules kept reporting
throughout.

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
