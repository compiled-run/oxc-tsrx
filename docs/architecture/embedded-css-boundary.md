---
title: Embedded CSS boundary
description: Why TSRX style payloads deliberately stay byte-exact, in-process, and parser-free today.
---

# Embedded CSS boundary

The shipping decision is **KEEP RAW**. Oxfmt formats the surrounding TSRX and
JSX through the pinned native adapter, while every byte between a lowercase
`<style>` and `</style>` is restored exactly. There is no CSS parse, no CSS
format time, and no formatter subprocess. The format result reports
`embedded_parse_count = 0` and `embedded_format_ns = 0`.

<!-- diagram:embedded-css-boundary -->

This is a compatibility boundary, not a shortcut. At canonical OXC revision
`8e0ed2ebb96137fb1611cdbd5742d5cb46037d40`, `oxc_formatter_css` 0.59.0 is an
unpublished workspace crate. It consumes registry `oxc-css-parser` 0.0.7,
which in turn requests the registry copy of `oxc_allocator`; the formatter
uses the workspace/git copy. OXC's own workspace resolves the duplicate type
graph with a `[patch.crates-io] oxc_allocator = { path = ... }` entry. Applying
that downstream would violate this project's no-patch/no-fork upgrade
contract.

The exact evidence hashes, pinned source links, shipping invariants, and
requalification conditions live in `docs/architecture/css-boundary.json`. The gate
fails if CSS crates or a Cargo patch enter the product graph, if formatter
source gains a subprocess path, if the constants stop reporting zero hidden
CSS work, or if a real raw payload changes.

We re-evaluate only after canonical OXC exposes a consumable same-allocator CSS
path without downstream patching. That candidate must then retain source
fidelity, fail-closed behavior, convergence, and the established native
performance and memory budgets on the Markless corpus.
