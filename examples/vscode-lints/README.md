# VS Code lint demo

This workspace intentionally contains five native Oxlint findings in
`LintDemo.tsrx`: `no-var`, `no-unused-vars`, `no-console`, `eqeqeq`, and
`no-debugger`. It also runs the real JavaScript plugin in
`../custom-js-plugins/demo-lint-plugin.mjs`; its `tsrx-demo/no-tsrx-if` rule
visits the authored `JSXIfExpression` and warns on the `@if` block at line 9.

To see the custom JavaScript rule with only the official OXC extension:

1. Open `examples/vscode-lints` as the VS Code workspace.
2. Install or enable `oxc.oxc-vscode`.
3. Open `oxlint-custom-parser.json` once to activate the official extension,
   then open `LintDemo.tsrx`.

Hover the yellow squiggle under `@if` to see
`tsrx-demo(no-tsrx-if): Demo rule: prefer a declarative component over this
TSRX @if block.`

Be precise about who does what here. The official OXC extension is only the
**client**: it displays diagnostics and starts a language server. It does not
run your JavaScript rule. It launches the workspace-local
`oxlint-custom-parser-lsp.mjs`, which forwards to a local build of the
custom-parser Oxlint **draft** and dynamically registers `.tsrx` document
synchronization and pull diagnostics. The draft Oxlint is what parses `.tsrx`
(through the `parseForESLint` adapter) and runs the `tsrx-demo/no-tsrx-if`
rule.

The OXC-for-TSRX companion is not installed in the retained custom-plugin
editor test. It remains optional for the five native diagnostics, TSRX
formatting, and the validated `no-var` quick fix. To see those additional
features from the repository root, run **TSRX: lint demo** with F5.

This checkout points the launcher at
`target/oxlint-custom-parser/cli.js`, a local build of the upstream
custom-parser draft. Released Oxlint does not accept this parser configuration
yet. The Markless extension remains recommended for TSRX syntax highlighting
and language features.
