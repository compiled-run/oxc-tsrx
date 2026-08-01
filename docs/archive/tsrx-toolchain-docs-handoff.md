# Fable handoff: persistent installation, Vite parser DX, and custom JavaScript lint plugins

Prepared 2026-07-24 for a later documentation rewrite. This is a research and
docs-audit artifact; the persistent setup design described here is proven in
disposable consumers but is **not implemented in the product yet**.

## Correction, 2026-07-25: package names below are stale

Two refactors landed after this document was written, and it has deliberately
not been rewritten. Its research findings still hold. Its package names do not.

- Four first-party wrapper packages were folded into `oxc-tsrx`:
  `@oxc-tsrx/runtime`, `@oxc-tsrx/parser`, `oxlint-tsrx`, and `oxfmt-tsrx`.
  They will never be published. The parser is now the `oxc-tsrx/parser` export,
  and in-repo callers import `packages/toolchain/dist/parser.js`.
- The published set is nine names: `oxc-tsrx` plus the eight
  `@oxc-tsrx/native-*` platform packages.
- The three native executables were merged into one, `oxc-tsrx`, which selects
  the linter, formatter, or language server from `argv[0]` or a leading
  `lint`/`fmt`/`lsp` subcommand.

So do not copy any alias manifest from this document into a real project. Every
`npm:oxlint-tsrx@…`-style specifier below names a package that does not exist
and will fail to install. The lane results those manifests produced are still
valid evidence about how each package manager resolves aliases and overrides;
only the names are wrong.

## What Fable should accomplish

Rewrite the public docs so a reader can answer, without inspecting source:

1. What does installing `oxc-tsrx` provide today?
2. Why does the current implementation require `setup` after every clean
   install, and what will the approved persistent implementation change?
3. How does authored TSRX parsing compose with an existing Vite framework
   plugin?
4. Which custom JavaScript lint paths are real Vite plugins, ESLint proofs,
   released Oxlint features, upstream Oxlint drafts, or native Rust features?
5. Which claims are released, locally proven, draft-only, or planned?

Do not collapse those into one “Vite toolchain” story. Build/dev transforms,
parser-aware Vite consumers, Vite+ lint/format commands, ESLint custom rules,
Oxlint JS plugins, and editor LSP routing are separate execution hosts.

## Status vocabulary to use everywhere

Use these labels consistently:

| Label | Meaning |
| --- | --- |
| **Released upstream** | Available in an official published Vite, Vite+, OXC, Oxlint, Oxfmt, or editor release |
| **Current repository behavior** | Implemented in this working tree; may still be unpublished |
| **Retained local proof** | Covered by a checked test or disposable consumer, but not necessarily a public API |
| **Upstream draft** | Open PR/discussion behavior; never present it as released |
| **Approved design** | Research-selected future implementation; not current behavior |
| **Unproven** | Plausible but not exercised in the retained matrix |

Avoid using “works now” without naming the host and one of those statuses.

## Extra status labels for the provider protocol

The provider protocol work (`docs/goals/oxc-tsrx-provider-discovery`) needs a
finer split than the table above, because “upstream draft” is not accurate for
it. Nothing has been sent upstream at all. Use these five labels, and never
merge two of them into one sentence:

| Label | Meaning | What it currently covers |
| --- | --- | --- |
| **Released behavior** | shipped by a third party you can install today | `oxlint`, `oxfmt`, Vite+ tool selection by literal package name, and `oxc.oxc-vscode` launching a project-local `oxlint --lsp` |
| **Local reference implementation and proof** | real, executable, covered by tests here, read by nothing outside this repository | `oxc-tsrx/provider-resolve`, the `oxc.provider` block in `packages/toolchain/package.json`, `oxc-tsrx providers`, the `oxc-tsrx-vscode` client, `oxlint --lsp` multiplexing |
| **Proposed protocol** | a shape written down for upstream to adopt | the `oxc.provider` protocol and the boundary shape in `notes/T004-adoption-decision.md` |
| **Submitted** | sent to an upstream repository | **nothing** |
| **Accepted upstream** | merged and released by upstream | **nothing** |

The single sentence every provider page must be able to survive:

> No released OXC, Oxlint, Oxfmt, Vite+, or `oxc.oxc-vscode` build reads
> `oxc.provider` metadata.

Never write “OXC discovers providers”. Write “the reference implementation in
this repository discovers providers, and the protocol proposes that OXC do the
same”. Do not use the words adopted, shipped upstream, or accepted for any part
of this protocol.

# Install-only provider discovery

**Status: local reference implementation and proof, plus a proposed protocol.**

This is the primary DX the docs should lead with for `oxc-tsrx`, and it is the
thing the compatibility sections below exist to be replaced by.

## The one-line story

Installing `oxc-tsrx` is the whole consumer action:

```sh
npm install --save-dev oxc-tsrx
```

There is no second step. No activation command, no dependency alias, no root
`overrides` block, no lifecycle script, no `PATH` entry, and nothing written
into `node_modules` after the install finishes. Deleting `node_modules` and
running a frozen reinstall re-establishes discovery for the same reason it
worked the first time: `oxc-tsrx` is still a direct dependency.

For a Vite reader, the closest familiar shape is a preset that Vite finds
because it is in the dependency list, not because a config snippet was pasted.
The difference worth spelling out is that nothing is executed to be discovered.
A host resolves `<name>/package.json` for each direct dependency and runs
`JSON.parse` on it. Nothing is imported, required, or spawned.

## What the docs must say about scope

Say all four of these together or none of them:

1. The index is built from **direct** dependencies only, so a transitive
   dependency never silently changes how source files are handled.
2. Reserved core extensions (`.js .cjs .mjs .jsx .ts .cts .mts .tsx .json
   .jsonc .json5 .vue .svelte .astro`) are a hard error for a provider to claim,
   which is what keeps ordinary files off provider code paths.
3. Two providers claiming one extension, or one id, is an error. Discovery never
   breaks a tie by install order, hoisting, or package name.
4. `parse`, `lint`, and `format` capability targets exist and are correct, but
   **only `lsp` has hosts today**. Do not present the four-capability
   declaration as four working integrations.

## The `oxc.provider` contract is general infrastructure

`packages/toolchain/README.md` now documents the contract for *any* would-be
provider, not as an `oxc-tsrx` feature. Keep it that way. The resolver module
carries no provider-specific literal at all, and
`tests/packaging/provider-resolve.test.mjs` asserts that with a case-insensitive
`/tsrx/` check against both `provider-resolve.js` and `provider-resolve.d.ts`.

Two rules are load-bearing and are easy to drop when condensing:

- A provider may only point at what it publishes. `{ "module": "./sub" }` must
  be an export subpath of the declaring package, and `{ "bin": "name" }` must be
  a key of its own `bin` map. There is no `node_modules/.bin` lookup and no
  `PATH` lookup.
- A capability target must be a **leaf executor**: it lints or formats exactly
  the files it is handed, performs no discovery, and dispatches on no file
  extension. This is why the `lint` capability points at `oxc-tsrx-lint` rather
  than at this package's `oxlint` wrapper. Pointing it at `oxlint` would make an
  adopting Oxlint execute Oxlint, rediscover the same provider, and recurse
  without bound.
- The protocol says which file implements a capability; the **calling
  convention** (argv, output shape, exit codes) is the separate half, written in
  `packages/toolchain/README.md` under "Capability calling convention" and
  pinned by `tests/packaging/toolchain-package.test.mjs`. It is derived from
  what the shipped executors do, and it is labeled as what a host *would*
  follow. No host calls `lint` or `format` through discovery today.

## Package-manager coverage, and how it differs from the alias matrix

Do not merge this table with the persistent-alias table further down. They are
different experiments with different subjects.

| Matrix | Subject | Managers |
| --- | --- | --- |
| `tests/packaging/provider-matrix.test.mjs` | `oxc.provider` discovery from a direct dependency | npm 11.12.1, pnpm 10.33.2, Bun 1.3.14, Yarn 4.9.2 node-modules linker, Yarn 4.9.2 Plug'n'Play linker |
| `docs/goals/persistent-toolchain-dx-research/notes/T003-persistence-matrix.mjs` | root aliases and overrides for the `oxlint`/`oxfmt` compatibility names | npm, pnpm, Yarn **Classic** 1.22.22, Bun hoisted, Bun isolated |

So “Yarn Berry and Plug'n'Play are unproven” is still correct for the alias
work and is **no longer** correct for provider discovery. Attribute the claim to
one matrix by name every time.

Both matrices are darwin-arm64 only. Windows is covered by neither.

## Plug'n'Play needs the filesystem, not only the resolver

This is the finding a docs rewrite is most likely to lose, so it gets its own
paragraph on any page that mentions injected resolution.

A Plug'n'Play install has no `node_modules`. Packages stay zipped inside
`.yarn/cache`, and `.pnp.cjs` answers with a path *into* the archive. An
ordinary `fs.readFile` cannot open that path; it fails with `ENOTDIR`, because
to the operating system a `.zip` is a file and not a directory.

A Plug'n'Play host must therefore inject **both** halves:

- `resolve`: the PnP API's `resolveRequest(request, issuer)`;
- `readFile`: a reader backed by the same PnP filesystem layer, which in
  practice means running the host under the PnP runtime (`--require .pnp.cjs`,
  or a `yarn node` launcher) so `fs` is patched to see inside the zip.

An injected resolver alone is not enough, and this is easy to get wrong because
the resolver half looks like it is working. Released `oxc.oxc-vscode` 1.59.0
already carries a `.pnp.cjs` `resolveRequest` branch and reads with an ordinary
`fs`, so a host that adopted the protocol by reusing that branch alone would
find nothing for a Plug'n'Play user.

Two limits to keep beside that:

- **Fixed 2026-07-24, and safe to document.** A dependency manifest that resolves
  and then cannot be read or parsed now produces an `unreadable-manifest`
  warning naming the package and the manifest path, so the Plug'n'Play failure
  above is loud instead of silent. Two things must be said with it, or the page
  will mislead. It is a **warning**, not an error: discovery reads every direct
  dependency's manifest, most of them are ordinary libraries, and one unreadable
  manifest must not abort discovery for a project whose providers all resolved
  fine. And a dependency that does not resolve at all still stays **quiet**,
  because that only means it is not installed. The behavior is pinned by
  `tests/packaging/provider-resolve.test.mjs` and, against a real Yarn
  Plug'n'Play install, by `tests/packaging/provider-matrix.test.mjs`.
- A `bin` capability under Plug'n'Play resolves to a path inside the zip, which
  an ordinary `child_process.spawn` cannot execute. It needs either an unplugged
  copy (`dependenciesMeta.<pkg>.unplugged`) or a `yarn node` style launcher.
  Nothing here claims to spawn one.

## Hosts that read the index

All **local reference implementation and proof**. None is a released OXC build.

| Host | Capability | Behavior worth documenting |
| --- | --- | --- |
| `oxc-tsrx-vscode` | `lsp` | discovers once per workspace folder, never merges two folders' indexes, starts one client per discovered `lsp` capability, lazily on the first claimed document |
| `oxlint --lsp` from `oxc-tsrx` | `lsp` | registers only discovered extensions, keeps every other document on canonical Oxlint |
| `oxc-tsrx providers` | none | reports the index; writes nothing; exits non-zero on a fatal protocol violation |

The editor proof runs through a pure decision module and the packaging matrix,
not a real VS Code session. Do not cite `npm run test:editor:vscode` or
`tests/editor/official-oxc-toolchain-run.mjs` as evidence for it.

# Compatibility surfaces, explicitly not the target design

Everything under this heading, including the whole persistent-install section
that follows, is a shim that exists because no released host discovers
providers. Every page that documents any of it must say so in the same
paragraph, not in a footnote.

The three surfaces are:

1. **`npx oxc-tsrx setup`** and the dependency aliases and overrides it writes.
2. **The `oxlint` and `oxfmt` bin names** this package declares. Released Vite+
   selects tools by those literal package and binary names
   (`join(dirname(dirname(resolve("oxlint"))), "bin", "oxlint")`), so owning the
   names is currently the only way to be selected. Relying on bin names is
   exactly the mechanism install-only discovery replaces: it depends on install
   layout and name ownership rather than declared capability, and two tools
   cannot own one name. These bins are never capability targets.
3. **The released `oxc.oxc-vscode` path**, which reaches TSRX by launching the
   project-local `oxlint --lsp`, not by discovering a provider.

All three remain until an upstream host discovers providers. Do not describe
them as the recommended design, and do not lead any install page with them.

The rerun-after-clean-install requirement in the next section is the clearest
signal that setup is a shim: under install-only discovery there is nothing to
rerun.

# Persistent install (compatibility surface)

## Current behavior — accurate today

The current `npx oxc-tsrx setup`:

- requires `oxc-tsrx` as a direct dependency;
- writes compatibility packages directly under project-root
  `node_modules/{oxc-parser,oxlint,oxfmt}`;
- replaces qualified transitive official packages only after preserving them
  under `node_modules/.oxc-tsrx-compat/originals`;
- refuses direct or unowned collisions;
- is idempotent and reversible; and
- deliberately does not edit `package.json` or a lockfile.

Because `node_modules` is disposable, a clean install deletes that selection.
The current docs telling users to rerun setup are therefore factually accurate
for the current code. They are not a good permanent DX, and they must be
rewritten when the approved design is implemented.

The implementation is in `packages/toolchain/dist/compat.js`. The facade
matrix in `tests/packaging/toolchain-compat.test.mjs` proves mutation after
npm, pnpm, and Bun installs. The Vite+ command matrix in
`tests/packaging/vite-plus-matrix.test.mjs` runs npm only and explicitly calls
setup after install.

## Why `oxc-tsrx` cannot silently persist this by itself

Dependency replacement policy belongs to the consumer root:

- npm ignores `overrides` declared by installed dependencies and supports
  replacing another package via `npm:`.
  <https://docs.npmjs.com/cli/v11/configuring-npm/package-json/#overrides>
- pnpm accepts root-only overrides and package aliases.
  <https://pnpm.io/settings#overrides> and <https://pnpm.io/aliases>
- Yarn resolutions are root-only.
  <https://yarnpkg.com/configuration/manifest#resolutions>
- Bun supports npm overrides and aliases, but intentionally does not run an
  installed dependency's lifecycle scripts unless it is trusted.
  <https://bun.com/docs/pm/cli/install>

Therefore a published package cannot put a magic field in its own manifest
that hijacks Vite+'s root dependency graph. The choices are:

- one explicit command that writes project-owned package-manager policy once;
- a hidden lifecycle mutation, which is unsafe and non-portable; or
- a new upstream provider hook.

The approved design chooses the first.

## Approved no-rerun design — not implemented yet

Change `npx oxc-tsrx setup` from a `node_modules` mutator into a one-time,
package-manager-aware policy migration:

1. Find the workspace root and package manager.
2. Inspect direct dependencies, existing overrides/resolutions, lockfile, and
   literal OXC package selections.
3. Preview exact changes.
4. Refuse conflicting or unowned policy.
5. Persist direct aliases for `oxc-parser`, `oxlint`, and `oxfmt`.
6. Persist manager-native transitive replacement policy so nested consumers
   cannot select a nearer stock package.
7. Run the chosen package manager once to update the lockfile.
8. Retain ownership metadata so `status` and `remove` are exact.
9. Never rewrite `node_modules` directly.

The public workflow remains simple:

```sh
npm install --save-dev oxc-tsrx
npx oxc-tsrx setup
```

The difference is that setup runs once per project policy, not after every
install. Every later normal or frozen install reproduces the selection from
the manifest and lockfile.

## Proven package-manager shape

The retained experiment is:

- `docs/goals/persistent-toolchain-dx-research/notes/T003-persistence-matrix.mjs`
- `docs/goals/persistent-toolchain-dx-research/notes/T003-persistence-matrix.md`

It installed untouched local package tarballs, deleted each disposable
consumer's `node_modules`, and performed a frozen reinstall without setup.

| Lane | Result | Frozen reinstall | Lockfile | Setup reruns |
| --- | --- | --- | --- | ---: |
| npm 11.12.1 | Pass | `npm ci` | unchanged | 0 |
| pnpm 10.33.2 | Pass | `pnpm install --frozen-lockfile` | unchanged | 0 |
| Yarn Classic 1.22.22 | Pass | `yarn install --frozen-lockfile` | unchanged | 0 |
| Bun 1.3.14 hoisted | Pass | frozen hoisted install | unchanged | 0 |
| Bun 1.3.14 isolated | Pass | frozen isolated install | unchanged | 0 |

Every lane proved root aliases, resolution from Vite+'s own package context,
and Vite+ 0.2.4's exact project-first binary calculation all reached the TSRX
implementation.

The tested npm shape is:

```json
{
  "devDependencies": {
    "oxc-tsrx": "0.1.0",
    "oxc-parser": "npm:@oxc-tsrx/parser@0.1.0",
    "oxlint": "npm:oxlint-tsrx@0.1.0",
    "oxfmt": "npm:oxfmt-tsrx@0.1.0"
  },
  "overrides": {
    "oxc-parser": "$oxc-parser",
    "oxlint": "$oxlint",
    "oxfmt": "$oxfmt"
  }
}
```

pnpm uses the same direct aliases plus root `pnpm-workspace.yaml` overrides.
Yarn Classic uses direct aliases plus root `resolutions`. Bun uses direct
aliases plus root `overrides`.

This implementation detail does not mean users independently choose four
products. `oxc-tsrx setup` owns the compatibility entries. For an even
cleaner manifest, first make the `oxc-tsrx` root export satisfy the parser,
linter, and formatter root APIs, then point every alias at
`npm:oxc-tsrx@…`. The current proof targets the existing capability packages
because their root APIs/binaries already match.

Do not claim Yarn Berry or Plug'n'Play **for the alias work**. Only Yarn
Classic's `node_modules` linker was exercised here. Vite+ currently computes
filesystem package roots and binary paths, so PnP needs a separate
qualification.

Provider discovery is a different subject with a different matrix, and it does
cover Yarn Berry 4.9.2 on both linkers. Name the matrix whenever either claim is
made, or a reader will read one result as the other.

## Why Vite+ needs package selection rather than a Vite alias

Installed `vite-plus@0.2.4` resolves tools with Node:

```js
require.resolve(path, { paths: [process.cwd(), import.meta.dirname] })
```

It computes:

```js
join(dirname(dirname(resolve("oxlint"))), "bin", "oxlint")
join(dirname(dirname(resolve("oxfmt"))), "bin", "oxfmt")
```

That happens at the Vite+ CLI boundary. A Vite `resolve.alias` applies inside
Vite's module resolution pipeline and cannot select the executables used by
`vp lint` or `vp fmt`.

Vite+'s documented `lint` and `fmt` blocks configure the selected tools; they
do not expose a package or binary provider field:

- <https://viteplus.dev/guide/lint>
- <https://viteplus.dev/config/lint>
- <https://viteplus.dev/config/fmt>

The ideal upstream end state is a typed Vite+/OXC provider hook. That would
remove literal-name compatibility aliases, but it is not required to remove
the every-install rerun now.

# Vite parser DX

## There are two independent Vite concerns

### Framework compilation

`oxc-tsrx` does not compile application modules. A framework plugin such as
`@tsrx/vite-plugin-react` still owns:

- TSRX-to-JavaScript compilation;
- runtime semantics;
- CSS extraction;
- source maps; and
- HMR.

That is true under both Vite and Vite+. Vite+ reuses the Vite plugin pipeline
for `vp dev` and `vp build`.

### Parser-aware sibling plugins

The repository has a source-local composition proof in
`examples/custom-js-plugins/tsrx-parser-service.mjs`. It is not a parser that
Vite automatically exposes.

Current lifecycle:

1. `withTsrxParser(frameworkPlugin, createPlugins, options)` creates a Vite
   preset array.
2. The first plugin is `@oxc-tsrx/vite-parser-service` with
   `enforce: "pre"` and transform `order: "pre"`.
3. For a raw, non-virtual `.tsrx` id, its transform parses authored source and
   caches by clean id plus exact source.
4. The transform returns `null`; it does not modify code.
5. Parser-aware sibling plugins receive the service API through a closure.
6. Calling `parser.parse(id, source)` reuses the cached result.
7. The framework plugin then transforms TSRX.
8. Rolldown parses the generated JavaScript, not the authored TSRX AST.
9. `handleHotUpdate` invalidates the file cache; `closeBundle` clears it.

Vite's official API supports the transform ordering/filter approach and
states that `moduleParsed` does not run during dev:
<https://vite.dev/guide/api-plugin>.

The retained test `tests/plugins/parser-integrations.test.mjs` proves:

- one authored parse;
- a consumer observes `JSXIfExpression`; and
- the final bundle contains generated JavaScript, not TSRX control syntax.

## Important current publish gap

The docs currently make the example sound closer to a published integration
than it is:

- `tsrx-parser-service.mjs` imports
  `../../packages/parser/index.js`, not `oxc-tsrx/parser`;
- `tsrx-eslint-parser.mjs` does the same;
- `withTsrxParser` and `tsrxParserService` are not exported by `oxc-tsrx`;
- the consumer receives the API through a closure created by the helper, not
  from a standard Vite parser lifecycle; and
- no published package subpath currently provides this helper.

Accurate wording today: “the repository contains a complete source-local Vite
composition proof.” Do not say “install `oxc-tsrx` and import the Vite parser
service” until a real export exists and a packed consumer test proves it.

If the helper is published later, a clean API could be:

```js
import { withTsrxParser } from "oxc-tsrx/vite";
```

That is a suggested API, not current behavior.

# Custom JavaScript lint plugins

## Host map

| Path | Host process | Parser source | Rule API | Reuses Vite cache? | Status |
| --- | --- | --- | --- | --- | --- |
| `vite-demo-lint.mjs` | Vite Node process | Vite parser-service API | Ad hoc AST walk + `this.warn` | Yes | Retained local proof |
| ESLint adapter test | ESLint Node process | `parseForESLint` adapter | Real ESLint-shaped JS plugin | No | Retained AST-only proof |
| Released Oxlint ordinary JS/TS | Oxlint Node-enabled JS-plugin host | Native OXC parser | Oxlint JS plugins | No | Released for supported OXC file types |
| TSRX Oxlint draft | Local Node-enabled draft Oxlint | TSRX `parseForESLint` adapter | Oxlint JS plugins plus draft custom parser | No | Upstream draft + retained editor proof |
| Native `oxc-tsrx-lsp` | Rust process | Native TSRX/OXC projection | Native Rust rules | No | Current repository behavior |
| `oxc-tsrx/lint/plugins-dev` | Import-only helper surface | None | JS-plugin authoring helpers | n/a | Current export; not a host |

## Vite warning demo

`examples/custom-js-plugins/vite-demo-lint.mjs` is a Vite plugin. It walks the
authored AST and calls `this.warn`. This proves a custom Vite check can consume
the shared parser result.

It does **not** prove that an ESLint plugin can be dropped into Vite, that
Oxlint runs inside Vite, or that Vite+ `lint.jsPlugins` uses the Vite parser
cache.

## ESLint AST-only proof

`examples/custom-js-plugins/tsrx-eslint-parser.mjs`:

- implements `parseForESLint`;
- supplies authored ranges and locations;
- adds comments;
- derives visitor keys for custom TSRX nodes;
- exposes a small `services` object; and
- deliberately returns `program.tokens = []`.

`examples/custom-js-plugins/demo-lint-plugin.mjs` contains real
ESLint-shaped rules visiting `JSXIfExpression` and `JSXForExpression`.
`tests/plugins/parser-integrations.test.mjs` runs ESLint 10 and proves the
authored `@if` diagnostic.

This is AST-only because the public parser v1 result does not expose the
authored OXC token stream and the adapter has no complete framework scope
contract. Token-dependent `SourceCode` rules and framework binding semantics
must not be claimed.

## Released Oxlint and Vite+ JS plugins

Released Oxlint supports JavaScript plugins for file types its parser already
accepts. Its current released documentation still lists custom parsers and
custom file formats as unsupported:
<https://oxc.rs/docs/guide/usage/linter/js-plugins.html>.

Vite+ exposes Oxlint JS-plugin configuration in its `lint` block. That
provides the ordinary Oxlint JS-plugin host; it does not provide a TSRX parser.

The current `oxc-tsrx` Vite+ bridge extracts serializable `lint`/`fmt`
configuration and sends JSON to a separate native TSRX process. It rejects
`jsPlugins`/functions rather than silently dropping them. Consequently:

- ordinary JS/TS may use released Oxlint JS plugins through Vite+;
- `.tsrx` in the native lane cannot run those JS plugins;
- adding `jsPlugins` does not cause the Vite parser service to be reused; and
- `oxc-tsrx/lint/plugins-dev` helps author a plugin but does not execute it.

## Oxlint custom-parser draft

As of 2026-07-24, OXC PR
[#24262](https://github.com/oxc-project/oxc/pull/24262) remains a draft with
nine commits. It adds explicit
`overrides[].languageOptions.parser` routing for the Node-enabled Oxlint
JS-plugin host.

The docs' old “shortest AST-only route” description is stale. The current PR
description now includes:

- `parseForESLint` / `parse` routing;
- SourceCode and token-store behavior;
- parser services and scope-manager integration;
- fixes and disable directives;
- editor/LSP routing;
- native-rule coverage through an offset-preserving shadow source; and
- explicit per-glob opt-in.

It still leaves important work out:

- typechecking/type-aware framework files;
- faithful framework virtual source and mappings;
- parse/load caching;
- generated typed walkers;
- full module-graph participation; and
- first-class language identity.

The broader language-plugin proposal remains a discussion, not a release:
<https://github.com/oxc-project/oxc/discussions/21936>.

## Editor custom-rule proof

The repository's custom-rule editor demo:

1. uses only the official OXC VS Code extension as the client;
2. points it at a workspace-local launcher;
3. the launcher dynamically registers `.tsrx` document sync and pull
   diagnostics;
4. it forwards LSP traffic to
   `target/oxlint-custom-parser/cli.js`, a local build of the draft;
5. draft Oxlint invokes the TSRX ESLint parser adapter; and
6. the JS rule reports `tsrx-demo/no-tsrx-if`.

The custom JavaScript rule runs in draft Oxlint. It does not run in:

- the official extension client;
- native `oxc-tsrx-lsp`;
- ESLint during the editor session; or
- the Vite parser service.

This proof correctly shows no companion VS Code extension is required. It
does not make the upstream draft released.

# Stale documentation inventory

The inventory uses three severities:

- **Stale now** — contradicts current source or upstream status.
- **Misleading/incomplete now** — technically adjacent to truth but hides a
  boundary readers need.
- **Pending approved implementation** — accurate for today's code, but must
  change in the same patch that implements persistent setup.

## Installation and package selection

| File / section | Severity | What is wrong or will change | Rewrite direction |
| --- | --- | --- | --- |
| `README.md` — Install | Pending approved implementation | Says setup never edits `package.json` and must rerun after clean installs | After implementation, say setup writes owned aliases/overrides and lockfile once; later installs need no setup |
| `README.md` — Vite and Vite+ | Misleading/incomplete now + pending | Describes facades but does not explain Vite+'s exact root-first Node resolver or npm-only Vite+ command proof | Separate current resolver facts, current npm proof, and future persistent manager matrix |
| `docs/guide/getting-started.md` — opening/install | Stale/ambiguous now | Says the project “ships as one public package” before the approval-gated registry launch has occurred | Use “prepared as one public package” or put the publication gate directly beside the install command until publication is verified |
| `docs/guide/getting-started.md` — “Using Vite+?” | Pending | Repeats temporary slots and rerun requirement | Replace with one-time policy migration; keep “not implemented” until code lands |
| `docs/integrations/vite-plus.md` — Install | Pending | Entire setup flow is tied to disposable facades | Document one-time manifest/lockfile policy, preview, collision behavior, status/remove |
| `docs/integrations/vite-plus.md` — Architecture | Pending | Says setup provides physical facades under current node_modules layout | Explain direct aliases + transitive replacement; preserve literal resolver rationale |
| `docs/integrations/vite-plus.md` — Consumer shape | Pending | Shows only two manifest entries because current setup mutates install output | Keep user-facing two-command DX, but disclose setup-owned compatibility entries and manager-specific storage |
| `packages/toolchain/README.md` — compatibility paragraph | Corrected 2026-07-24 | Now leads with install-only provider discovery, carries the five-state status table, and files setup, the `oxlint`/`oxfmt` bin names, and the released extension path under “Compatibility surfaces, not the target design” | Keep the rerun sentence accurate until persistent setup lands; do not promote any compatibility surface back above the discovery section |
| `README.md`, `docs/guide/getting-started.md`, `docs/integrations/vite-plus.md` — install order | Misleading now | Lead with `setup` and alias grammar, which are compatibility surfaces, before any mention of install-only discovery | Lead with “install `oxc-tsrx`, that is the whole action”, then a clearly labeled compatibility section for released Vite+ and the released extension |
| Any page describing the provider protocol | Risk | Easy to write “OXC discovers providers” | Use the five provider status labels; nothing is submitted or accepted upstream |
| `packages/oxlint/README.md` — install block | Corrected 2026-07-24 | Now leads with the plain install and install-only discovery, and files `setup` plus the `oxlint` command name under an explicit compatibility-surface heading | Keep the compatibility framing; do not reintroduce the alias-migration direction |
| `packages/oxfmt/README.md` — install block | Corrected 2026-07-24 | Same, for the `oxfmt` command name | Same |
| `docs/releasing/v0.1.0.md` — Included / Known boundaries | Pending | Release contract explicitly promises disposable facades and reruns | Update release notes only if implementation is part of 0.1.0; otherwise move design to next release |
| `docs/releasing/README.md` — required checks / post-publication smoke | Misleading/incomplete now + pending | Vite+ commands are npm-only; no persistent-policy matrix is named | Add persistent npm/pnpm/Yarn/Bun resolver/reinstall gate and keep end-to-end Vite+ command scope exact |
| `docs/releasing/upgrades.md` — official tool upgrades | Pending | Does not require alias/override grammar and lockfile migration to survive tool version bumps | Add resolver/policy matrix and ownership/remove compatibility |
| `docs/releasing/launch-runbook.md` — install/smoke steps | Pending review | Any copied install instructions must match the final one-time setup | Search generated commands and launch copy when implementation lands |
| `docs/releasing/external-prerequisites.md` | Pending review | External publish/install checks may assume only facade mutation | Ensure registry proof includes policy write plus frozen reinstall |
| `docs/releasing/v0.1.0-launch.json` — social/install claims | Pending review | “one package” remains a user-facing truth, but launch validation must not hide setup-owned aliases | Keep marketing concise; make technical docs disclose project policy |
| `docs/reference/limitations.md` — Packaging and ecosystem | Corrected 2026-07-24 | The rerun is still described as real and temporary, and the superseded alias/override persistence design is now named as superseded rather than as the roadmap | Keep the rerun sentence until `setup` is deleted; never restore alias persistence as the end state |
| `docs/integrations/editor.md` — how the extension reaches TSRX | Corrected 2026-07-24 | The opening no longer says the extension "discovers" the local `oxlint` command; it selects it by literal name, and a new section separates name selection from provider discovery | Keep the two mechanisms separate on every edit; the pointer-to-a-general-host limit must survive condensing |
| `docs/integrations/editor.md` and `packages/vscode/README.md` — install language | Ambiguous now | The official extension is released, but the `oxc-tsrx` registry set is only locally packaged; nearby proof text eventually explains this | Put the registry/publication qualifier beside the initial install instruction |
| `docs/architecture/upstreaming-to-oxc.md` — compatibility surfaces | Pending | Names package-name facades as the surface to retire | Rename to package-name compatibility policy/aliases after implementation |
| `docs/dist/**` | Generated | Mirrors stale source pages | Never hand-edit; regenerate after source docs are corrected |

Important: do not “fix” the current rerun text ahead of code. Until persistent
setup is implemented, removing it would make the docs false.

## Vite parser service

| File / section | Severity | What is wrong | Rewrite direction |
| --- | --- | --- | --- |
| `docs/integrations/custom-js-plugins.md` — “Add the parser beside an existing Vite plugin” | Misleading/incomplete now | Calls the example complete without disclosing repo-relative parser import and missing public helper export | Label source-local proof; explain closure-injected API and publish gap |
| `docs/integrations/custom-js-plugins.md` — preset order | Incomplete now | Correct order, but readers may infer a Vite-provided parser lifecycle | State the project creates a custom plugin `api`; Vite does not hand it out |
| `docs/integrations/vite-plus.md` — Optional parser-aware Vite plugins | Misleading/incomplete now | Describes retained example alongside installed package DX, which suggests it is public | State it is source-local and unrelated to Vite+ `lint` configuration |
| `examples/custom-js-plugins/README.md` — Vite | Stale now | Prose says `@oxc-tsrx/parser`, while code imports `../../packages/parser/index.js` | Describe actual import; change prose only after example uses public package |
| `README.md` — Vite and Vite+ | Incomplete now | Build/dev proof and parser-service proof can read as one integration | Split framework compilation from optional authored-AST consumers |
| `docs/guide/introduction.md` — “Not a Vite plugin” | Ambiguous now | Correct for compilation, but sounds incompatible with the parser-service example | Say no required compiler plugin; optional helper can expose authored AST |
| `docs/guide/getting-started.md` — next steps | Incomplete now | “Wire commands into Vite+” does not distinguish build plugin from lint/format package selection | Link separately to build plugin ownership and command integration |

## Custom JavaScript lint plugins and editor

| File / section | Severity | What is wrong | Rewrite direction |
| --- | --- | --- | --- |
| `docs/integrations/custom-js-plugins.md` — status table | Stale now | Treats the current draft mainly as an AST-only route and mixes official extension client status with host status | Use the host map above; update draft features but keep it unmerged |
| `docs/integrations/custom-js-plugins.md` — “What Oxlint needs” | Stale now | PR #24262 description predates SourceCode, fixes, LSP routing, shadow native rules, and disable directives | Summarize current nine-commit draft scope and remaining limits |
| `docs/integrations/custom-js-plugins.md` — “accepted by upstream draft” | Incomplete now | True locally, but does not make clear that the configured binary is a local source build | Name `target/oxlint-custom-parser/cli.js` and source-only status |
| `examples/custom-js-plugins/README.md` — ESLint adapter | Stale now | Says adapter adapts `@oxc-tsrx/parser`; source imports repo-relative parser | Correct the current source claim |
| `examples/custom-js-plugins/README.md` — Oxlint draft | Stale now | Draft description omits its expanded behavior and still frames production destination too narrowly | Refresh from PR, preserve released/draft split |
| `examples/vscode-lints/README.md` | Incomplete now | Mostly accurate, but “official extension launches” can make the extension sound like the JS host | Explicitly say official extension is the client; draft Oxlint hosts parser/rule |
| `docs/integrations/editor.md` — custom JS experiment | Incomplete now | Does not explicitly contrast draft host with native multiplexer path | Add two editor stacks: native current path vs Node-enabled draft custom-rule path |
| `docs/reference/limitations.md` — JavaScript lint plugins | Stale/ambiguous now | “Token APIs remain” is true for local parser v1, but no longer describes the draft, which forces token/range/location options and provides SourceCode | Attribute each limitation to local adapter, released host, or draft |
| `docs/integrations/configuration.md` — Not yet supported | Ambiguous now | “JavaScript plugins unsupported” can be read globally | Say native TSRX CLI/LSP and serialized TSRX Vite+ lane reject them; ordinary released Oxlint lane supports JS plugins |
| `docs/architecture/upstreaming-to-oxc.md` — upstream activity | Stale now | Correctly says draft/unmerged, but its consequence summary predates shadow-source native rules and LSP work | Refresh capabilities; retain gaps for formatter, type-aware, caching, and language identity |
| `packages/toolchain/README.md` — “custom JavaScript lint-plugin development” | Incomplete now | Can sound like execution support | Say it exports authoring helpers; execution for TSRX remains ESLint proof or Oxlint draft |
| `README.md` — package feature list | Incomplete now | “custom JavaScript lint-plugin development helpers” is accurate but easy to confuse with a host | Add “authoring helpers, not a native plugin host” where integration is explained |

## Proof scope and version wording

| File / section | Severity | What is wrong | Rewrite direction |
| --- | --- | --- | --- |
| `docs/reference/limitations.md` — test scope | Incomplete now | Says Vite/Vite+ matrices pass but not that package-manager facade proof and npm Vite+ command proof are separate | State exact manager/command axes |
| `docs/integrations/vite-plus.md` — Proven compatibility | Incomplete now | Clean physical lanes are npm; readers can infer broader manager coverage from other tests | Name npm explicitly; add persistent resolver matrix separately |
| `docs/acceptance/matrix.md` — Vite+ row | Incomplete now | “Clean physical consumers” is true but package manager is omitted | Add npm to the lane description |
| `docs/releasing/README.md` — Vite+ required checks | Incomplete now | Same omission | Name npm or extend command matrix before claiming more |
| `README.md` — “minimum/current” Vite+ language | Maintenance risk | Version labels become stale quickly | Say “tested minimum 0.1.24 and pinned current 0.2.4” rather than timeless “current” |
| `docs/releasing/upgrades.md` — pinned versions | Maintenance risk | Correct pins, but “current recommended” needs an audit date | Add tested/pinned wording and date |
| `docs/integrations/custom-js-plugins.md` — “Oxlint 1.74” and audit footer | Maintenance risk | Repository pin is 1.74.0, while public releases may move; footer already predates this audit | Say “tested/pinned 1.74.0” and separately cite current released limitation docs |
| `examples/custom-js-plugins/README.md` — “Oxlint 1.74” | Maintenance risk | Same | Same |

The exact performance numbers and pinned test versions are not stale merely
because newer packages exist. Preserve them as dated retained evidence; avoid
calling them the universal latest.

# Files reviewed with no required correction from this research

Do not create churn in these areas unless another audit finds an issue:

- `docs/guide/parsing.md` and `packages/parser/README.md` — public parser API
  descriptions remain separate from the source-local Vite helper caveat.
- `docs/guide/tsrx-syntax.md` — syntax contract is unrelated.
- `docs/guide/formatting.md` and embedded CSS architecture — unrelated.
- `packages/runtime/README.md` — implementation-package role is accurate.
- `packages/vscode/README.md` — legacy client status is separate; preserve the
  warning not to run two clients.
- benchmark result pages — preserve frozen evidence and timestamps.
- legal and license inventories — do not alter for a docs-only rewrite.
- prior `docs/goals/**` receipts — historical evidence, not public docs to
  rewrite.

# Recommended documentation architecture

## 1. Getting started

Lead with:

- install one public package;
- direct `oxlint`, `oxfmt`, and parser examples;
- Vite+ users run one setup migration once after implementation;
- explicit current publication status.

Keep package-manager-specific generated snippets behind tabs or an expandable
section. Do not dump all alias grammar into the quick start.

## 2. Vite and Vite+

Split the page into:

1. **Build/dev:** framework plugin owns TSRX compilation.
2. **Lint/format commands:** Vite+ selects literal packages; persistent setup
   writes project policy.
3. **Optional authored AST service:** a separate source-local or published
   Vite helper.
4. **Configuration bridge:** serializable lint/fmt values only.
5. **Proof scope:** npm command matrix versus package-manager resolver matrix.

## 3. Custom JavaScript plugins

Start with the host table, then give four short sections:

1. Vite warnings/checks.
2. ESLint AST-only adapter.
3. Released Oxlint ordinary JS-plugin support.
4. Draft Oxlint TSRX custom-parser/editor path.

Put the native Rust LSP in a contrasting callout so readers understand why it
cannot execute Node plugins.

## 4. Editor

Show two distinct stacks:

```text
Official OXC extension
  -> project oxlint --lsp multiplexer
     -> canonical Oxlint for JS/TS
     -> native oxc-tsrx-lsp for TSRX
```

and:

```text
Official OXC extension
  -> source-local custom-parser launcher
     -> draft Node-enabled Oxlint
        -> TSRX parseForESLint adapter
        -> JavaScript plugin rule
```

The first is the current native product path. The second is the source-only
upstream-draft proof.

# Rewrite order

1. Implement persistent setup before changing current setup instructions.
2. Update canonical technical pages:
   - `docs/integrations/vite-plus.md`
   - `docs/integrations/custom-js-plugins.md`
   - `docs/integrations/configuration.md`
   - `docs/integrations/editor.md`
3. Update quick-start surfaces:
   - `README.md`
   - `docs/guide/getting-started.md`
   - `docs/guide/introduction.md`
4. Update package READMEs.
5. Update limitations, architecture, acceptance, release, and upgrade docs.
6. Run every docs/source claim test.
7. Regenerate `docs/dist/**` from sources; never patch generated pages.
8. Search for old language:

```sh
rg -n \
  "Run it again|after a clean dependency install|temporary compatibility|facades|AST-only route|Oxlint 1\\.74|current recommended|jsPlugins" \
  README.md docs packages examples \
  --glob '*.md' --glob '!docs/dist/**'
```

# Fable acceptance checklist

- [ ] Every `oxc-tsrx` install page leads with install-only provider discovery,
      and installing the package is described as the whole consumer action.
- [ ] `oxc-tsrx setup`, the `oxlint`/`oxfmt` bin names, and dependency aliases
      are labeled compatibility-only and not the target design, on every page
      that mentions them.
- [ ] The `oxc.provider` contract is presented as general infrastructure other
      providers can adopt, not as an `oxc-tsrx` feature.
- [ ] No page says the protocol is adopted, shipped upstream, or accepted, and
      the submitted and accepted-upstream rows stay empty.
- [ ] Released behavior, local reference implementation and proof, proposed
      protocol, submitted, and accepted upstream are five separate labels, never
      blurred into “supported”.
- [ ] Provider-discovery package-manager coverage and alias-persistence coverage
      are attributed to their own matrices by name.
- [ ] Any page describing injected resolution states that a Plug'n'Play host
      must supply both `resolve` and a PnP-backed `readFile`.
- [ ] Any page describing the unreadable-manifest diagnostic calls it a warning,
      not an error, and says that a dependency which does not resolve at all
      stays quiet.
- [ ] No current page says persistent setup is implemented before code lands.
- [ ] After code lands, no page tells users to rerun setup after clean installs.
- [ ] Package-manager policy is described as project-owned, collision-safe,
      lockfile-backed, and reversible.
- [ ] npm, pnpm, Yarn Classic, and Bun claims match the retained matrix.
- [ ] Yarn Berry/PnP remains explicitly unproven.
- [ ] Vite framework compilation and parser-aware sibling plugins are separate.
- [ ] The source-local Vite helper is not presented as a published export.
- [ ] The repo-relative parser imports are acknowledged or changed before docs
      claim public imports.
- [ ] Vite warning checks are not called Oxlint JS plugins.
- [ ] ESLint proof, released Oxlint, Oxlint draft, and native LSP are separate.
- [ ] `oxc-tsrx/lint/plugins-dev` is described as authoring helpers, not a host.
- [ ] PR #24262 is still labeled Draft and its current expanded scope is accurate.
- [ ] Official OXC extension is described as a client, not the JS-rule runtime.
- [ ] Vite+ end-to-end command proof is not generalized beyond npm.
- [ ] Frozen performance/version evidence is labeled pinned and dated.
- [ ] `docs/dist/**` is regenerated rather than hand-edited.

# Primary upstream references

- Vite plugin API and lifecycle:
  <https://vite.dev/guide/api-plugin>
- Vite+ lint and config:
  <https://viteplus.dev/guide/lint>,
  <https://viteplus.dev/config/lint>,
  <https://viteplus.dev/config/fmt>
- Vite+ source repository:
  <https://github.com/voidzero-dev/vite-plus>
- npm overrides:
  <https://docs.npmjs.com/cli/v11/configuring-npm/package-json/#overrides>
- pnpm overrides and aliases:
  <https://pnpm.io/settings#overrides>,
  <https://pnpm.io/aliases>
- Yarn resolutions and Classic aliases:
  <https://yarnpkg.com/configuration/manifest#resolutions>,
  <https://classic.yarnpkg.com/lang/en/docs/cli/add/>
- Bun install, aliases, overrides, lifecycle policy, and linkers:
  <https://bun.com/docs/pm/cli/install>
- Released Oxlint JS-plugin limits:
  <https://oxc.rs/docs/guide/usage/linter/js-plugins.html>
- Draft Oxlint custom parser:
  <https://github.com/oxc-project/oxc/pull/24262>
- Oxlint custom-parser issue:
  <https://github.com/oxc-project/oxc/issues/19918>
- OXC language-plugin proposal:
  <https://github.com/oxc-project/oxc/discussions/21936>
