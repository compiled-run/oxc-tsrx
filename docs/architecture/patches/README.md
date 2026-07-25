# Provider discovery adoption patches

These are the full diffs behind the table in
[Upstreaming to OXC](../upstreaming-to-oxc.md#provider-discovery-patches-built-locally).
They live here, tracked in git, so a clean checkout of this repository can
reproduce them. Nothing about them is vendored upstream source: each file is a
diff against a public repository at a pinned revision, and the clones they were
built in are temporary and outside this repository.

Read the status labels literally.

- **built and verified locally** means the patch was applied to a clone at the
  pinned revision, compiled, and exercised by tests on this machine.
- **submitted** would mean sent to an upstream repository. Nothing here is
  submitted.
- **accepted** would mean merged upstream. Nothing here is accepted.
- **released** would mean shipped to users. Nothing here is released.

| File | Target repository | Pinned revision | Size | Status |
| --- | --- | --- | --- | --- |
| `oxlint-provider-dispatch.patch` | [`oxc-project/oxc`](https://github.com/oxc-project/oxc) | [`a065946`](https://github.com/oxc-project/oxc/commit/a065946a8ce95eb3374e08242cd9086ab050314b) on `main`, and [`2d4e8d2`](https://github.com/oxc-project/oxc/commit/2d4e8d20644e0e7446f0a381894b45ea339a0625) at tag `oxlint_v1.74.0` | +1463 / -10 | built, verified locally |
| `oxc-vscode-provider-selector.patch` | [`oxc-project/oxc-vscode`](https://github.com/oxc-project/oxc-vscode) | [`beaffb9`](https://github.com/oxc-project/oxc-vscode/commit/beaffb967b06db53907723cbb61712c0fa9d9dea) (v1.59.0) | +106 / -1 | built, verified locally |

There is no third patch. [`voidzero-dev/vite-plus`](https://github.com/voidzero-dev/vite-plus)
needs zero lines of change, for the reason and with the qualification recorded in
[Upstreaming to OXC](../upstreaming-to-oxc.md#why-vite-needs-nothing-and-what-that-is-worth-today).

## Applying one

Each `.patch` file starts with a short plain-text preamble naming its target
repository and revision. `git apply` skips that preamble and reads only the
diff, so the file works as-is:

```sh
git clone https://github.com/oxc-project/oxc.git
git -C oxc worktree add --detach oxc-1740 oxlint_v1.74.0
git -C oxc-1740 apply --check /path/to/oxlint-provider-dispatch.patch
```

`--check` reports whether it would apply without writing anything. Drop it to
apply for real.

Two revisions are pinned for the Oxlint patch on purpose. It is written against
`main`, and it was built at the `oxlint_v1.74.0` tag because the released NAPI
addon available for the build is 1.74.0 and the built JavaScript half has to pair
with a matching addon. `apps/oxlint/src-js/cli.ts` and
`apps/oxlint/tsdown.config.ts` are byte-identical between the two revisions, so
it is the same patch either way.

## Verifying the files are intact

`git apply --numstat` parses a patch without needing the target files, so it is
a cheap structural check:

```sh
git apply --numstat docs/architecture/patches/oxlint-provider-dispatch.patch
git apply --numstat docs/architecture/patches/oxc-vscode-provider-selector.patch
```

The first prints eight files totalling +1463 / -10, the second two files
totalling +106 / -1.

## What each patch does

`oxlint-provider-dispatch.patch` adds provider dispatch to the Oxlint npm
wrapper, above the in-process NAPI addon, at the one existing call site in
`apps/oxlint/src-js/cli.ts`. The `lint(...)` signature is untouched and no Rust
changes. With no provider installed the wrapper runs today's exact statement.
With one installed it splits the command line, keeps paths Oxlint already owns on
the same call, and runs the capability binary the provider declares for the rest.
In `--lsp` mode it instead composes the canonical language server with each
provider's server behind the one stdio connection the editor opened. It runs
capability binaries in pass-through mode, which is the contract written down in
`packages/toolchain/README.md` under "Capability calling convention".

`oxc-vscode-provider-selector.patch` widens the extension's document selector to
include extensions found in the workspace's provider index, importing the
discovery algorithm from the `oxlint` package the extension already resolves
rather than reimplementing it. An `oxlint` without that export, which is every
release to date, yields no extra extensions and the selector is byte-identical to
today's. It deliberately does not touch `activationEvents` or
`contributes.languages`; that is a separate and more contentious change.
