# Control-flow fixtures

`branch` is derived from Markless
`demos/codegen-size/corpus/09-conditional.tsrx`; `rows` is derived from
`03-keyed-flow.tsrx` and the indexed/empty Markless fixtures; `nested` follows
the nested Markless browser fixtures. Names and literals are reduced while the
authoritative syntax shape is retained. `async-expression` is a Ripple parser
oracle form not currently present in Markless and deliberately retains
`@for await` even though the incumbent Prettier plugin drops `await`.
`switch` reduces Ripple's pinned switch parser/runtime cases, including a
default clause before a later case. `try` reduces Ripple's pinned
`@try`/`@pending`/`@catch` oracle and retains both typed error and reset
bindings. The authoritative Ripple revision for both is
`03a98fd2a230ab5853808a44ff024568d68142fb`.
`expressions` retains Ripple's assignment, argument, expression-statement,
return, and nested-control placements for both newly supported families.
`dynamic-style` combines Ripple's parser-native `<{expression}>` tags with a
component-scoped raw `<style>` block. Its compact CSS golden is deliberately
preserved byte-for-byte while canonical Oxfmt lays out the surrounding
TSRX/JSX; CSS formatting and validation are not claimed. `dynamic-style-lint`
proves that authored tag/body expressions remain affine for Oxlint while the
CSS payload stays outside the JavaScript AST.

External repositories are read-only provenance sources. These copies are owned
test data inside OXC for TSRX.
