# Platform and ABI policy

OXC for TSRX ships one generated npm package per supported native ABI. The
runtime selects exactly one package from `process.platform`, `process.arch`, and
Linux libc. It validates package version, protocol version, OXC revision,
target identity, executable declaration, and executable mode before launching.
There is no download-on-install script and no fallback to a similar-looking
ABI.

## Release matrix

| npm package suffix | Rust target | Build runner | npm selector | VSIX target |
| --- | --- | --- | --- | --- |
| `darwin-arm64` | `aarch64-apple-darwin` | `macos-14` arm64 | `darwin` / `arm64` | `darwin-arm64` |
| `darwin-x64` | `x86_64-apple-darwin` | `macos-15-intel` | `darwin` / `x64` | `darwin-x64` |
| `linux-arm64-gnu` | `aarch64-unknown-linux-gnu` | `ubuntu-22.04-arm` | `linux` / `arm64` / `glibc` | `linux-arm64` |
| `linux-x64-gnu` | `x86_64-unknown-linux-gnu` | `ubuntu-22.04` | `linux` / `x64` / `glibc` | `linux-x64` |
| `linux-arm64-musl` | `aarch64-unknown-linux-musl` | `ubuntu-22.04-arm` | `linux` / `arm64` / `musl` | `alpine-arm64` |
| `linux-x64-musl` | `x86_64-unknown-linux-musl` | `ubuntu-22.04` | `linux` / `x64` / `musl` | `alpine-x64` |
| `win32-arm64-msvc` | `aarch64-pc-windows-msvc` | `windows-11-arm` | `win32` / `arm64` | `win32-arm64` |
| `win32-x64-msvc` | `x86_64-pc-windows-msvc` | `windows-2025` | `win32` / `x64` | `win32-x64` |

The two Arm GitHub labels are currently public-preview runner infrastructure.
That does not weaken artifact checks: those jobs are required in the release
matrix, run on the matching architecture, and a missing runner/build blocks the
candidate. A release may not omit an advertised optional dependency.

## Compatibility floors

- Native releases are built with Rust 1.95.0 and the committed `Cargo.lock`.
  Rust is a build input, not a user runtime dependency.
- GNU/Linux builds use Ubuntu 22.04. The workflow records the highest referenced
  `GLIBC_*` symbol and rejects anything newer than `GLIBC_2.35`.
- musl builds use Rust's musl targets and must be statically linked. They are a
  distinct npm package, never a fallback selected for glibc hosts or vice versa.
- macOS arm64 sets `MACOSX_DEPLOYMENT_TARGET=11.0`; macOS x64 sets `10.15`.
- Windows artifacts use the MSVC ABI on matching x64 or arm64 Windows runners.
- Release compilation uses Rust's target baseline. `target-cpu=native` and
  runner-specific CPU features are forbidden in distributed artifacts.
- The JavaScript boundary supports Node `^20.19.0 || >=22.12.0`. CI exercises
  20.19.0, 22.12.0, and a maintained 24.x LTS release.
- The editor manifest declares VS Code `^1.95.0`.

Changing any floor is a compatibility decision, not routine CI maintenance. It
requires a documented rationale, clean installs on the old and new boundary
where possible, and a semver decision before release.

## Artifact contents

Each `@oxc-tsrx/native-*` tarball contains:

- `oxc-tsrx`;
- `oxc-tsrx-fmt`;
- `oxc-tsrx-lsp`;
- a machine-readable checksum/target/OXC manifest; and
- the project, OXC, and locked dependency legal material.

Each platform VSIX embeds only `oxc-tsrx-lsp` plus its manifest, checksum, and
legal material. It must not embed the lint CLI, format CLI, `node_modules`, a
second platform binary, a Cargo checkout, or an OXC source tree.

The platform-independent packages contain only their declared JavaScript
runtime surface and documentation. They have exact same-version dependencies
on the runtime/native set, no postinstall script, and `preferUnplugged` on the
native packages so package managers do not try to execute binaries from an
archive.

## Verification rules

The release workflow builds on a matching OS and CPU. The packager executes all
three binaries on a matching native target and checks their exact version/OXC
identity. A musl target is also executed on its same-architecture Linux host
after static-link verification. Every output receives SHA-256 metadata, and the
assembled candidate receives a deterministic sorted checksum list.

Before packing, the JavaScript packager reads each executable header directly
and requires a 64-bit Mach-O, ELF, or PE image matching the declared OS and
x64/arm64 architecture. This does not execute a shell `file` heuristic and
works for cross artifacts. Linux libc is a separate ABI property: the release
workflow inspects every GNU binary's highest `GLIBC_*` reference and requires
every musl binary to be static, then executes all three on the matching
same-architecture runner.

Cross-platform packaging is release evidence; it is not full runtime evidence
for every operating-system version. Before changing support claims, retain a
real clean-install transcript for that ABI. A report produced through a binary
override, `NODE_PATH`, source-tree `target/release`, or a mismatched native
package is invalid.

Unsupported OS, CPU, or libc combinations must fail with an actionable error.
They must never delegate `.tsrx` to stock Oxlint/Oxfmt or silently skip files.
