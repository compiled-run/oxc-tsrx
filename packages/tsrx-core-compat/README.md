# `@oxc-tsrx/tsrx-core-compat`

A small `@tsrx/core`-compatible parser facade for Markless and similar consumers. It delegates
TSRX parsing to `@oxc-tsrx/parser` and provides the event-name helpers used by Markless.

This package does not provide editor recovery. `loose` and `collect` collect diagnostics only
when the native parser can still return a program.
