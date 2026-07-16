# Editor fixtures

`markless-arm-try-events.tsrx` is an exact retained copy of the MIT-licensed
Markless fixture at
`packages/vitest-browser/browser/fixtures/arm-try-events.tsrx`, pinned during
T025 at Markless commit `b7f834b878847767c14fcac9544e7a7da13a1e17` and
SHA-256 `d2c4df5fe7aa471ab4762ff0879d6af09b26545e4733d7e1ff393c25e9a0203c`.
The Extension Host walkthrough still reads the external source, copies it only
to a disposable workspace, and fingerprints the external worktree before and
after. This retained copy makes editor performance gates reproducible from a
clean OXC for TSRX checkout without requiring a sibling Markless checkout.
