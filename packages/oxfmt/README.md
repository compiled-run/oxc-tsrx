# `oxfmt-tsrx`

Oxfmt-compatible command and programmatic formatting exports with native
`.tsrx` support. Ordinary JavaScript and TypeScript are delegated to the exact
official Oxfmt package; TSRX is formatted by the Rust-native OXC for TSRX
binary and lifted back only after structural validation.

Use the package directly, or install it under the `oxfmt` alias expected by
Vite+:

```sh
npm install --save-dev oxfmt@npm:oxfmt-tsrx@0.1.0
```

The executable remains `oxfmt`. Raw `<style>` payload bytes are preserved;
embedded CSS formatting is not claimed. A missing or mismatched platform
package fails without silently delegating TSRX to stock Oxfmt.

OXC for TSRX is a community integration and is not an official OXC or VoidZero
distribution.
