# T003 architecture correction: no OXC fork

Date: 2026-07-15

## Outcome

T003 is superseded. The imported native OXC prototype is not the product
architecture and must not remain in this repository.

The owner clarified a hard invariant: **OXC for TSRX must not fork, vendor, or
carry a patch queue for OXC.** Official `oxlint`, `oxfmt`, and Vite+ releases
must remain independently upgradable dependencies. Integration may use only
documented public package, CLI, configuration, LSP, and Vite boundaries, with
capability detection and a tested version matrix.

This corrects the earlier T001/T002 recommendation. Those receipts remain as
historical evidence, not as an approved architecture.

## Why the prototype was rejected

The native prototype imported the complete OXC source tree and added TSRX
syntax directly to OXC AST/parser crates. Even if distributed as a small patch,
that is a downstream fork.

The prototype also produced direct maintenance evidence against that model:

- adding a public AST variant broke exhaustive matches in unrelated OXC codegen
  and React compiler code;
- adding an AST field changed generated builder APIs and broke unrelated OXC
  consumers;
- routine OXC code generation touched a broad generated surface;
- correctness depended on the exact upstream AST and parser layout.

Those failures are not incidental. They demonstrate that OXC releases could
break the integration even when TSRX behavior itself had not changed.

## Current upstream boundary

As of this review:

- Oxlint documents custom file formats and parsers as unsupported. Its open JS
  Plugins Milestone 3 first needs to research custom template syntax, and the
  open `languageOptions.parser` issue describes the missing parser seam.
- Oxfmt documents Prettier/custom plugins as unsupported and `.tsrx` is absent
  from both its native and bundled-language lists.
- Vite+ resolves its installed `oxlint` and `oxfmt` packages for `vp lint`,
  `vp fmt`, and `vp check`; normal Vite plugins are not a parser hook for those
  static-analysis commands.

Therefore stock OXC binaries cannot honestly be described as natively parsing
TSRX today. The present-tense product must be a non-fork companion/adapter, and
true stock-binary support requires upstream extension hooks or upstream-owned
TSRX support.

## Non-fork direction to validate

1. Keep official `oxlint`, `oxfmt`, and Vite+ as normal unmodified dependencies.
2. Use the public Yuku TSRX parser/code generator as the high-performance TSRX
   frontend candidate. It already parses TSRX, prints TSRX, emits source maps,
   uses a packed native representation, and is independent of OXC internals.
3. Batch-generate a virtual TSX mirror for official Oxlint, invoke one official
   Oxlint process with machine-readable output, and remap diagnostics. Apply a
   fix only when a conservative mapping proof shows the generated edit maps to
   one unchanged original TSRX span.
4. Use a native TSRX formatter path based on the TSRX frontend/printer while
   sharing the supported Oxfmt/Vite+ configuration subset. Delegate ordinary
   JS/TS/JSX/TSX files directly to official Oxfmt with no extra parse or copy.
   Do not claim the TSRX branch is Oxfmt's native formatter until upstream owns
   that hook.
5. Provide an independent TSRX LSP/editor extension. It owns `.tsrx` document
   selection and composes with the official OXC extension rather than forking
   it.
6. Test minimum, current, and next/canary compatible OXC package versions. Use
   capability probes instead of version-specific internal imports, fail closed
   on unsafe fixes, and pass unknown upstream CLI options through unchanged.
7. Prepare focused upstream proposals for a custom Oxlint language/parser seam,
   an Oxfmt language adapter seam, and Vite+ custom check providers. Switch to
   those capabilities automatically when official releases expose them.

## Required proof before implementation expands

- No OXC/Vite+ source or patch is present in the repository.
- A black-box compatibility spike proves the selected public boundaries against
  at least two independently installed official Oxlint/Oxfmt versions.
- The spike measures parse/codegen, virtual-mirror, Oxlint batch, formatter,
  startup, and steady-state costs separately on the retained Markless corpus.
- Config/path/source-map behavior and safe-fix rejection are tested before any
  claim of compatibility.
- Documentation names every stock-binary limitation directly.

No external repository was modified during the rejected prototype or this
correction.
