# Shipping license inventory

The native npm packages and platform VSIX include this directory alongside
the project `LICENSE` and `THIRD_PARTY_NOTICES.md`.

- `oxc/LICENSE` and `oxc/THIRD-PARTY-LICENSE` are byte-exact copies from the
  canonical OXC revision recorded in `oxc/PROVENANCE.json`.
- `rust-dependencies.json` is the machine-readable dependency closure for the
  three distributed Rust binaries.
- `RUST_DEPENDENCIES.md` is the matching human-readable report.
- `allowed-rust-license-expressions.json` is a fail-closed review policy, not
  legal advice. It explicitly selects a distribution license for every
  dual/multi-license expression (including Apache-2.0 for `self_cell`, not
  GPL-2.0-only). A new expression stops the deterministic compliance gate.

Regenerate the locked Rust reports with `npm run licenses:generate` and verify
them without mutation with `npm run licenses:check`.
