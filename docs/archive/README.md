# Archived design records

This directory holds historical design records. They are deliberately not
published to the docs site and are not listed in `site.config.mjs`.

Each file here describes approved design input rather than implemented
behavior. The designs were accepted, but the product either never built them or
moved on afterwards, so reading them as documentation would mislead you. They
are kept for provenance: they record why a decision was made and what evidence
backed it at the time.

- `tsrx-parser-api.md`: the implementation contract for a JavaScript-callable
  TSRX parser API, approved as design input but never built as specified.
- `tsrx-toolchain-docs-handoff.md`: a 2026-07-24 research and docs-audit
  handoff on persistent installation, Vite parser DX, and custom JavaScript
  lint plugins, whose findings hold but whose package names are stale.

Both documents originally linked into `docs/goals/`, which is gitignored
internal project state. Those links were removed and their text kept inline,
so nothing here points at a path that is missing from a fresh clone.
