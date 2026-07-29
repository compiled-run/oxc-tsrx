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

This is a compatibility boundary, not a shortcut. At the pinned OXC revision,
OXC's CSS formatter is an unpublished workspace crate that pulls a registry copy
of a crate the formatter itself takes from git. OXC resolves that duplicate with
a `[patch.crates-io]` entry, and applying one downstream would break this
project's no-patch, no-fork contract, so this waits on upstream.

The exact evidence hashes, pinned source links, shipping invariants, and
requalification conditions live in `docs/architecture/css-boundary.json`. The gate
fails if CSS crates or a Cargo patch enter the product graph, if formatter
source gains a subprocess path, if the constants stop reporting zero hidden
CSS work, or if a real raw payload changes.

We re-evaluate only after official OXC exposes a consumable same-allocator CSS
path without downstream patching. That candidate must then retain source
fidelity, fail-closed behavior, convergence, and the established native
performance and memory budgets on the Markless corpus.
