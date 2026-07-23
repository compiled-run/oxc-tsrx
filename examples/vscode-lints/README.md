# VS Code lint demo

This workspace intentionally contains five real Oxlint findings in
`LintDemo.tsrx`: `no-var`, `no-unused-vars`, `no-console`, `eqeqeq`, and
`no-debugger`.

From the repository root in Visual Studio Code:

1. Open **Run and Debug**.
2. Select **TSRX: lint demo**.
3. Press **F5**.

The launch target builds `oxc-tsrx-lsp` and the local extension, opens this
folder in an Extension Development Host, and opens `LintDemo.tsrx`. Hover the
squiggles to see their authored TSRX ranges. `no-var` also has a validated
quick fix.

The Markless extension is recommended for syntax highlighting and language
features. The OXC extension under development owns only linting, formatting,
and safe fixes for `.tsrx`.
