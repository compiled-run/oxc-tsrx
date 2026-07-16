# `oxlint-tsrx`

Oxlint-compatible command and configuration exports with native `.tsrx`
support. Ordinary JavaScript and TypeScript are delegated to the exact official
Oxlint package; TSRX is routed once to the Rust-native OXC for TSRX binary.

Use the package directly, or install it under the `oxlint` alias expected by
Vite+:

```sh
npm install --save-dev oxlint@npm:oxlint-tsrx@0.1.0
```

The executable remains `oxlint`. It supports mixed ordinary/TSRX batches,
JSON/JSONC Oxlint configuration, safe fixes, and opt-in type-aware rules. A
missing or mismatched platform package fails without silently skipping TSRX.

OXC for TSRX is a community integration and is not an official OXC or VoidZero
distribution.
