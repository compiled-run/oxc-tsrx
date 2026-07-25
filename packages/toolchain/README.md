# `oxc-tsrx`

One package for the OXC-shaped TSRX parser, Oxlint-compatible linting,
Oxfmt-compatible formatting, helpers for *authoring* custom JavaScript lint
plugins (via `oxc-tsrx/lint/plugins-dev`), and the native TSRX language
server.

```sh
npm install --save-dev oxc-tsrx
```

```js
import { parseSync } from "oxc-tsrx/parser";
import { defineConfig } from "oxc-tsrx/lint";
import { format } from "oxc-tsrx/format";
```

JavaScript, TypeScript, JSX, and TSX keep the exact official OXC code paths.
Only `.tsrx` files enter TSRX-specific work.

Note on the plugin helpers: they help you write a JS plugin. They are not a
host that executes one against `.tsrx`. The native TSRX CLI and language server
are Rust and run no JavaScript rules. Running a custom JS rule on `.tsrx` today
means either the ESLint AST-only proof or the unmerged upstream Oxlint draft;
see [Custom JavaScript plugins](../../docs/integrations/custom-js-plugins.md).

## Read the status table before you read anything else

This README describes two different things at once: a protocol we are proposing
to OXC, and the compatibility shims that keep TSRX working until that protocol
exists. Every claim below carries one of these labels, and they never blur.

| Label | Meaning | What it covers here |
| --- | --- | --- |
| **released** | shipped by a third party you can install today | the `oxlint` / `oxfmt` / Vite+ / `oxc.oxc-vscode` behavior described under [How a plain install actually reaches released hosts](#how-a-plain-install-actually-reaches-released-hosts) |
| **local reference implementation and proof** | real, executable, and covered by tests in this repository, but only this repository reads it | `oxc-tsrx/provider-resolve`, the `oxc.provider` block in this package, `oxc-tsrx providers`, the `oxc-tsrx-vscode` client, `oxlint --lsp` multiplexing |
| **proposed** | a protocol shape we have written down and want upstream to adopt | the `oxc.provider` protocol itself |
| **submitted** | sent to an upstream repository | **nothing** |
| **accepted upstream** | merged and released by upstream | **nothing** |

Concretely: **no released OXC, Oxlint, Oxfmt, Vite+, or `oxc.oxc-vscode` build
reads `oxc.provider` metadata.** If you read a sentence below that sounds like
"OXC discovers providers", read it as "the reference implementation in this
repository discovers providers, and we would like OXC to do the same".

## Install-only provider discovery

**Status: local reference implementation and proof.**

This is the design the package is built around, and it is what every host we
own already does.

The whole consumer action is installing the package:

```sh
npm install --save-dev oxc-tsrx
```

There is no second step. No activation command, no dependency alias, no root
`overrides` block, no `postinstall` script, no `PATH` entry, and nothing written
into `node_modules` after the install finishes. Delete `node_modules`, run
`npm ci`, and discovery works again for the same reason it worked the first
time: `oxc-tsrx` is still a direct dependency in your `package.json`.

If you have used Vite, the closest familiar shape is a framework preset that
Vite finds because it is in your dependency list, not because you copied a
config snippet. The difference is that nothing here is executed to be
discovered. A host only reads JSON.

### How a host finds the provider

1. Read the project root `package.json`.
2. Take the union of the names in `dependencies`, `devDependencies`, and
   `optionalDependencies`. Direct dependencies only. A transitive dependency is
   never activated, because "a package deep in my tree changed how my source
   files are parsed" is not a surprise anyone wants.
3. For each name, resolve `<name>/package.json` through ordinary Node module
   resolution.
4. Read and `JSON.parse` that file.
5. Keep the packages that declare a top-level `oxc.provider` block, and build an
   extension index from them.

Nothing is imported, required, or spawned to build that index, and nothing is
written. Module resolution and `JSON.parse` are the only operations performed
against a candidate package, so installing a malicious dependency does not get
that dependency executed by discovery.

### Try it

```sh
npx oxc-tsrx providers --json
```

**Status: local reference implementation and proof.** The command writes
nothing. It exits non-zero when a provider claims a reserved core extension or
when two providers claim the same extension.

## The `oxc.provider` contract

**Status: proposed (protocol) plus local reference implementation and proof
(this package's implementation of it).**

This section is written for anyone who wants to ship a language provider, not
just for TSRX. The protocol has no knowledge of TSRX in it. If you maintain a
package for some other file type, this is the whole contract.

Declare one static block in your own `package.json`:

```jsonc
{
  "oxc": {
    "provider": {
      "protocol": 1,
      "id": "tsrx",
      "languages": [
        {
          "id": "tsrx",
          "extensions": [".tsrx"],
          "capabilities": {
            "parse":  { "module": "./parser" },
            "lint":   { "bin": "oxc-tsrx-lint" },
            "format": { "bin": "oxc-tsrx-fmt" },
            "lsp":    { "bin": "oxc-tsrx-lsp" }
          }
        }
      ]
    }
  }
}
```

That block above is this package's real declaration. Yours would carry your own
id, extensions, and targets.

### Rules the protocol enforces

**Point only at what you publish.** `{ "module": "./sub" }` must be an export
subpath of the declaring package, and `{ "bin": "name" }` must be a key of the
declaring package's own `bin` map. There is no `node_modules/.bin` lookup, no
`PATH` lookup, no alias, and no override. A capability that resolves outside the
declaring package's own directory is rejected with a diagnostic.

**Reserved extensions belong to the core toolchain.** A provider that claims
`.js .cjs .mjs .jsx .ts .cts .mts .tsx .json .jsonc .json5 .vue .svelte .astro`
is a hard error, not a warning. This is the rule that structurally keeps
ordinary source files off provider code paths, so a project that installs a
provider does not start paying provider cost for its ordinary files.

**Conflicts fail loudly, they never pick a winner.** Two providers claiming the
same extension is an error. Two providers declaring the same `id` is an error.
Discovery never breaks a tie by install order, hoisting, or package name.

**A capability target must be a leaf executor.** The binary a host runs for
`lint` must lint exactly the files it is handed. It must not perform provider
discovery itself, must not dispatch on file extension, and must not be an entry
point a host resolves by a canonical tool name. That is why this package's
`lint` capability points at `oxc-tsrx-lint` and not at its own `oxlint` wrapper.
If `lint` pointed at `oxlint`, an adopting Oxlint would execute Oxlint, which
would discover the same provider again, and recurse without bound.

### Capability calling convention

**Status: proposed convention, derived from a local reference implementation.**
The protocol above says *which* file implements a capability. It does not say
how a host runs it. This section is that missing half.

Read the label literally: **no host calls `lint` or `format` through discovery
today.** Only `lsp` has hosts. What follows is what a host *would* follow, and
it is written from what the two executors this package already ships actually
do, not from a design we would prefer.

There are two ways a host can run a capability, and the difference is only about
who renders the output:

- **pass-through**, the baseline every host must support. The host hands the
  executor the files and lets the executor's own output reach the user. The host
  reads nothing.
- **collected**, an opt-in a host may use when it renders the whole run itself.
  The host asks for a machine-readable format, pipes the output, and merges it.

Pass-through is the baseline because it is the only mode a host can implement
without owning the rendering of its own results. That is exactly the situation
the Oxlint npm wrapper is in: it calls a native addon that lints, renders, and
prints inside Rust, so the wrapper never holds canonical diagnostics as data and
has nothing to merge provider results into. The locally built Oxlint dispatch
patch at `docs/architecture/patches/oxlint-provider-dispatch.patch` therefore
uses pass-through, and this section describes that same contract rather than a
merge the patch cannot perform.

#### argv

A host resolves the capability to a file, then spawns it as:

```text
<executor> <file> [<file> ...]
```

An executor forwards `process.argv.slice(2)` to its native tool byte for byte.
It adds nothing, removes nothing, and reorders nothing, so what the host passes
is exactly what the tool parses.

The one exception is not an argument at all. This package's native side is a
single multi-call executable that carries the linter, the formatter, and the
language server, so the `format` and `lsp` executors lead with the subcommand
that selects their tool (`fmt` and `lsp`). It is a tool selector, not a rewrite:
every host argument follows it unchanged and in order. Linting is the default
tool and needs no selector.

What a host **must** pass:

- **only the files the index routes to this provider.** The executor is a leaf:
  it never inspects file names and never hands a file back. Give it a `.ts` file
  and it will lint that file, and the same file will then carry diagnostics from
  two engines.
- **explicit file paths.** No directories, no globs. Walking the project is the
  host's job. Both executors fail with exit 2 when given no file at all.
- **absolute paths, unless the host spawns with the project as its working
  directory.** Relative paths are resolved against the child's working
  directory.

What a host **must not** pass:

- **its own rendering, reporting, or configuration flags.** An option the
  provider's tool does not know is a hard error, not an ignored flag, so a host
  that forwards its own command line will break providers at random. In
  pass-through mode a host passes file paths and nothing else. The one exception
  is collected mode below, which is opt-in precisely because it is a flag.
- a `--` separator to the `lint` executor. The current native lint CLI treats it
  as an unknown option.
- files the host has already handled itself. Nothing deduplicates.

#### Output

An executor inherits the stdio it is handed. It does not buffer, reshape, or
annotate.

In **pass-through** mode the host does not read the executor at all. It gives
the child its own stdout and stderr and lets the provider's diagnostics land
directly in front of the user, interleaved with whatever the host printed for
the files it handled itself. Two consequences a host should expect and state in
its own documentation:

- the user sees two report footers, one per engine, because neither engine knows
  about the other; and
- if the user asked the host for a specific output format, the provider half is
  still in the provider tool's default format. Pass-through cannot honor a host
  format flag, because forwarding that flag is exactly what the rule above
  forbids.

In **collected** mode the host spawns the executor with pipes and adds one
format flag, then parses what comes back. This is opt-in for two reasons: it
requires the provider's tool to understand the flag, and it requires the host to
render every result itself, including its own. This package's `lint` executor
reaches a native Oxlint-compatible CLI, so `--format json` works and produces
exactly one JSON document on stdout:

```jsonc
{
  "diagnostics": [
    {
      "filename": "/abs/path/View.tsrx",
      "rule": "no-debugger",
      "code": "eslint(no-debugger)",
      "severity": "error",
      "message": "…",
      "labels": [{ "span": { "offset": 120, "length": 8 }, "message": "…" }]
    }
  ],
  "number_of_files": 1,
  "number_of_rules": 97,
  "oxcTsrx": { }
}
```

A host in collected mode merges by concatenating `diagnostics`, summing
`number_of_files`, and taking the larger `number_of_rules`. Spans are byte
offsets into the authored file; there are no line and column fields, so a host
that renders them resolves positions itself. The `oxcTsrx` key is provider
metadata a host may ignore.

That merge is not hypothetical: the `combine` function in
`dist/lint-cli.js` already performs exactly it today when it combines canonical
Oxlint output with native TSRX output. It can do that because it spawns *both*
halves as child processes and so owns the whole report. It reaches the native
tool through this package's own wrapper rather than through discovery, so treat
it as a worked example of collected mode, not as a host that discovered
anything.

Nothing in this protocol requires a provider to support collected mode. A host
that wants it must fall back to pass-through when the flag is rejected.

For `format`, the shapes are unchanged in both modes, because the formatter's
output is already machine-readable: `--write` prints nothing, `--check` prints
one changed path per line, and `--stdin-filepath` prints the formatted bytes.
Human-readable failure text goes to stderr in every case.

#### Exit codes

The exit code is the part of the contract both modes share, and in pass-through
mode it is the *only* thing the host reads.

| Code | Meaning | What a host should do |
| --- | --- | --- |
| `0` | Ran to completion with nothing that fails the run | Nothing. In collected mode, parse stdout and merge it |
| `1` | Findings. `lint` saw at least one `error` diagnostic, or warnings that the configured `--deny-warnings` / `--max-warnings` policy promotes. `format --check` saw at least one file that differs | Fail the run. In collected mode, also parse and merge stdout |
| `2` | The executor or its tool broke: the native package is missing, argv was rejected, a file could not be read, or the child died from a signal | Fail the run. Do not parse stdout. Surface stderr |
| other | The native tool exited with some other code, passed through unchanged | Treat as breakage, not as findings |

A host that ran several capabilities takes the worst outcome: any non-zero child
fails the whole run.

Three details decide whether a host reports the truth:

- **Exit `0` does not mean "no diagnostics".** Warnings alone exit `0` and their
  text still reached the user. Use the exit code only to decide whether the run
  failed.
- **`1` and `2` are how a host tells "found problems" from "the tool broke".**
  A `2` comes with one line of stderr and no usable stdout. Failures raised by
  the wrapper itself are prefixed with the executor name, as in
  `oxc-tsrx-lint: …`; failures raised by the native tool underneath carry that
  tool's own name.
- **A host with a boolean exit of its own will flatten `2` into its own failure
  code.** The Oxlint dispatch patch does this: the wrapper's own result is
  success or failure, so a capability's `2` becomes Oxlint's `1`. The provider's
  stderr still reaches the user, which is what keeps the breakage visible, but a
  host that wants to preserve `2` has to widen its own exit handling.

The convention is pinned by `tests/packaging/toolchain-package.test.mjs`, which
runs both executors against a stubbed native tool and asserts argv fidelity,
stdout silence on the failure path, and each exit code above.

### Using the resolver

The resolver is published as a general, provider-agnostic module. It contains no
mention of any individual provider, so another host can vendor it unchanged.

```js
import { discoverProviders, resolveCapability } from "oxc-tsrx/provider-resolve";

const index = await discoverProviders({ root: process.cwd() });
const server = resolveCapability(index, "src/View.tsrx", "lsp");
```

`resolveCapability` returns `null` for every extension the index does not own,
which is the fast path ordinary `.ts` and `.js` files take.

### Host obligation: injected `resolve` and injected `readFile` travel together

`discoverProviders` accepts injected `resolve` and `readFile` functions so a
host whose module map is not the plain filesystem can reuse it. If you inject
one, you must inject the other from the same layer.

This matters most under Yarn Plug'n'Play. A PnP install has no `node_modules`
at all. Packages stay zipped inside `.yarn/cache`, and `.pnp.cjs` answers with a
path that points *into* the archive:

```text
<app>/.yarn/cache/oxc-tsrx-npm-0.1.0-<hash>.zip/node_modules/oxc-tsrx/package.json
```

An ordinary `fs.readFile` cannot open that path. It fails with `ENOTDIR`,
because as far as the operating system is concerned `...zip` is a file and not
a directory. So a host that injects `pnp.resolveRequest` as `resolve` but keeps
reading with an ordinary `fs` resolves every manifest correctly and then reads
none of them.

A Plug'n'Play host must therefore supply **both**:

- `resolve`: the PnP API's `resolveRequest(request, issuer)`;
- `readFile`: a reader backed by the same PnP filesystem layer, which in
  practice means running the host under the PnP runtime (`--require .pnp.cjs`,
  or launching through `yarn node`) so that `fs` is patched to see inside the
  zip.

An injected resolver on its own is not enough. This is the generalizable lesson
for anyone adopting the protocol, and it is easy to get wrong because the
resolver half looks like it is working.

**A host that gets this wrong is told so.** When a dependency's `package.json`
resolves and then cannot be read or parsed, discovery records an
`unreadable-manifest` warning that names the package and the manifest path, and
skips only that package. So a Plug'n'Play host reading with an ordinary `fs` gets
one warning per direct dependency, each carrying its `ENOTDIR` against a `.zip`
path, instead of an empty index and silence. The warnings show up in
`oxc-tsrx providers --json`, in the editor client's diagnostics, and in the
package-manager matrix report.

Two details worth knowing before you rely on this:

- it is a warning, not an error. Discovery reads the manifest of every direct
  dependency, and most of them are ordinary libraries, so one unreadable
  manifest must not abort discovery for a project whose providers all resolved
  fine. Nothing is thrown, and the rest of the index is still built;
- a dependency that does not resolve at all stays quiet. That case only means
  the package is not installed, which is ordinary for an uninstalled optional or
  dev dependency, and warning about it would be noise.

The behavior is pinned by `tests/packaging/provider-resolve.test.mjs` and, for a
real Yarn Plug'n'Play install, by `tests/packaging/provider-matrix.test.mjs`.

A second Plug'n'Play caveat: a `bin` capability under PnP resolves to a path
inside the zip, which an ordinary `child_process.spawn` cannot execute. Running
a provider binary under PnP needs either an unplugged copy
(`dependenciesMeta.<pkg>.unplugged`) or a `yarn node` style launcher. Nothing in
this repository claims to spawn one today.

### Package managers exercised

**Status: local reference implementation and proof**, all on darwin-arm64, all
in `tests/packaging/provider-matrix.test.mjs`. Each lane installs from a local
registry, builds the index, deletes the whole install tree, reinstalls frozen
from the untouched manifest and lockfile, and requires a byte-identical index.

| Lane | Install | Frozen reinstall |
| --- | --- | --- |
| npm 11.12.1 | `npm install` | `npm ci` |
| pnpm 10.33.2 | `pnpm install` | `pnpm install --frozen-lockfile` |
| Bun 1.3.14 | `bun install` | `bun install --frozen-lockfile` |
| Yarn 4.9.2, node-modules linker | `yarn install` | `yarn install --immutable` |
| Yarn 4.9.2, Plug'n'Play linker | `yarn install` | `yarn install --immutable` |

The Plug'n'Play lane passes only with a PnP-backed `readFile`, as described
above. Yarn Classic (1.x) and Windows are not covered.

## Which hosts read the index

**Status: local reference implementation and proof. None of these are released
OXC builds.**

| Host | Capability it uses | What it does |
| --- | --- | --- |
| `oxc-tsrx-vscode` (this repository's editor client) | `lsp` | Discovers once per workspace folder, never merges two folders' indexes, and starts one language client per discovered `lsp` capability, lazily, when the first document that provider claims is opened. |
| `oxlint --lsp` from this package | `lsp` | Registers only the discovered extensions, keeps every other document on canonical Oxlint, and starts a provider language server on first claimed document. |
| `oxc-tsrx providers` | none | Reports the index. |

The `parse`, `lint`, and `format` capability targets exist and are correct, but
no host executes them yet. Only `lsp` has hosts. Do not read the four-capability
declaration as four working integrations.

## How a plain install actually reaches released hosts

No released host discovers providers, so none of the mechanisms in this section
are part of the `oxc.provider` protocol, and a new provider written against the
contract above should not copy them.

They are not all the same kind of thing, though:

- The **`oxlint` and `oxfmt` bin names** are how a plain install works today.
  Deleting them would not make the protocol arrive sooner; it would only make
  `npm install oxc-tsrx` do nothing for a released editor. They stay, and they
  arbitrate rather than assume.
- **`npx oxc-tsrx setup`** is a genuine shim. It exists for hosts that resolve a
  package name, which a bin cannot answer, and Vite+ is the one that matters.

### The `oxlint` and `oxfmt` bin names

**Status: released behavior.**

This package declares commands named `oxlint` and `oxfmt`. Your installer links
them into `node_modules/.bin`, and that link is the whole reason a plain
`npm install oxc-tsrx` reaches anything. Released `oxc.oxc-vscode` 1.59.0 probes
`<folder>/node_modules/.bin/oxlint` before it tries anything else, so the link is
what makes `.tsrx` work in the editor with no further step.

Two packages cannot own one command name, and installers do not agree about who
wins. Measured with `oxc-tsrx` and official `oxlint` in the same project: npm 11
links this package's launcher, pnpm 10 links the official one.

So the launcher decides for itself instead of inheriting that race. Before it
does anything, it reads the nearest `package.json` above the working directory:

- If that manifest **directly declares** `oxlint` (or `oxfmt`) in
  `dependencies`, `devDependencies`, or `optionalDependencies`, the project has
  said what it means by that command name. The launcher hands the whole
  invocation to the exact binary the pinned package declares, and changes
  nothing about it: same version, same output, same exit code, same language
  server. Adding `oxc-tsrx` to such a project cannot change what its lint or
  format command does.
- Otherwise the launcher serves TSRX, as `npx oxlint` and `npx oxfmt` do in a
  project that only installed `oxc-tsrx`.

The result is the same either way round, so it no longer matters which package
your installer linked. A transitive official `oxlint` (Vite+ ships one) is not a
direct declaration, so it does not take the command name.

The one case with no safe answer is a manifest that declares the official
package while that package is not installed. The launcher refuses with exit 2
and says so, instead of guessing which linter you meant.

If you pinned official `oxlint` and still want `.tsrx` linted or formatted, run
`npx oxc-tsrx-lint` and `npx oxc-tsrx-fmt`. Pass a `.tsrx` path to the deferring
`oxlint` or `oxfmt` command and it prints that one line for you.

These bins are never capability targets: the provider block points at
`oxc-tsrx-lint` and `oxc-tsrx-fmt` instead, so a discovering host never re-enters
a host wrapper.

#### What "hands the invocation over" means on each platform

**Status: implemented and asserted; observed on macOS and on the Linux and
Windows CI runners, not on every published target.**

Handing the command name back has to produce the pinned tool's own behavior, and
"run this file" is not one operation across the eight platforms this package
publishes for. Two shapes matter:

- The declared binary is a **Node wrapper** — a file whose first line is a
  `node` shebang, which is what official `oxlint` and `oxfmt` both ship. It is
  imported into the launcher's own process, so argv, stdio, exit code, and
  signal handling stay the program's own. The import goes through a `file:` URL
  rather than a path, because a path is not a module specifier: on Windows
  `C:\project\node_modules\oxlint\bin\oxlint` parses `C:` as a URL scheme, and
  on any host a `#` or a space in a directory name does the same damage. A
  byte-order mark in front of the shebang is skipped, so a wrapper authored on
  Windows is still recognized as one.
- The declared binary is **anything else** — a native executable, or a `.cmd` or
  `.bat` launcher. It is spawned with inherited stdio and its status is
  mirrored. A batch launcher is handed to the command interpreter with each
  argument escaped for it, never concatenated onto a shell command line. If it
  cannot start at all, you get one line naming the file, not a stack trace.

`node_modules/.bin` itself differs too: your installer writes a symlink on
POSIX, and generates `oxlint`, `oxlint.cmd`, and `oxlint.ps1` shims on Windows.
Nothing in this package reads those shims — the launcher decides from your
manifest — so the arbitration answer is the same whichever your installer wrote.

### Vite+ needs one extra command, and here is why

**Status: released behavior. This is a real limit, not a rough edge.**

Vite+ 0.2.4 resolves the **package** named `oxlint`
(`join(dirname(dirname(resolve("oxlint"))), "bin", "oxlint")`). A *bin* called
`oxlint` does not satisfy that, and `oxlint` is not a package name this project
can legitimately publish. So a plain install cannot reach `vp lint` or
`vp check`.

What that actually looks like, measured against Vite+ 0.2.4:

- `vp lint` and `vp check` keep working exactly as before on JavaScript,
  TypeScript, JSX, and TSX. Installing `oxc-tsrx` breaks nothing.
- They do not report anything about `.tsrx` files. Vite+ hands them to official
  Oxlint, which does not know the extension, so they are skipped in silence.
- `npx oxlint` and `npx oxfmt` in the same project still handle `.tsrx`,
  because Vite+'s own `oxlint` dependency is transitive.

To get `.tsrx` through `vp`, run `npx oxc-tsrx setup` once after installing.
There is no install-time substitute for it: a lifecycle script that rewrote
another tool's `node_modules` would be worse than an explicit command.

### `npx oxc-tsrx setup` and dependency aliases

Some released tools, including Vite+, still resolve packages by those literal
names. The temporary compatibility facades are activated explicitly after
installing dependencies:

```sh
npx oxc-tsrx setup
```

The command is explicit, idempotent, package-manager-neutral, and reversible
with `npx oxc-tsrx remove`. It never edits `package.json`, never runs from an
install lifecycle script, preserves and restores transitive official packages
already occupying those exact slots, and refuses to replace direct or
unrecognized packages. Because `node_modules` is disposable, it has to be run
again after a clean dependency install.

That rerun requirement is the clearest sign this is a shim rather than the
target design. Under install-only discovery there is nothing to rerun.

Package installation cannot silently rewrite another tool's closed package
resolver or install an editor extension, which is why the shim is an explicit
command rather than a lifecycle script.

### The released editor path

**Status: released behavior. It needs no second command.**

Install the released **OXC** extension (`oxc.oxc-vscode`) and `npm install
oxc-tsrx`, and that is the whole setup. The extension probes
`<folder>/node_modules/.bin/oxlint` first, which your installer already created,
and launches it with `--lsp`. `oxc-tsrx` keeps ordinary JavaScript and
TypeScript on canonical Oxlint and registers `.tsrx` diagnostics, formatting,
and quick fixes from the native TSRX server. No `oxc-tsrx setup`, no companion
extension, no fork.

Two caveats:

- The released OXC extension does not list `.tsrx` as an activation event, so a
  TSRX-only workspace has to open any JavaScript, TypeScript, or JSON file once
  to activate it.
- If your project directly declares official `oxlint`, that command defers to it
  (see [the bin names](#the-oxlint-and-oxfmt-bin-names)), so the editor gets
  official Oxlint and no `.tsrx` support.

This path reaches TSRX through the `oxlint` bin name, not through provider
discovery.

### Implementation packages

`oxc-tsrx` is the only package to depend on. The eight `@oxc-tsrx/native-*`
packages are platform binaries listed in `optionalDependencies`; your package
manager installs exactly one of them and you never name it yourself.

The separate `@oxc-tsrx/runtime`, `@oxc-tsrx/parser`, `oxlint-tsrx`, and
`oxfmt-tsrx` packages that earlier drafts described no longer exist. Everything
they held is inside `oxc-tsrx`, reachable through its subpath exports.
