//! The `oxc-tsrx-lsp` language server. Selected by `argv[0]` or the `lsp`
//! subcommand.

use std::{env, path::PathBuf, process::ExitCode, sync::Arc};

use oxc_adapter::editor::{
    EditorActionKind, EditorCodeAction, EditorCodeActionRequest, EditorDiagnostic, EditorDocument,
    EditorDocumentEdit, EditorRange, EditorSeverity, EditorTextEdit, EditorTool, EditorToolFactory,
    EditorWorkspace, run_editor_server,
};
use serde_json::{Value, json};
use tsrx_format::FormatSession;
use tsrx_lint::{ConfigRuleFilter, LintSession};

#[expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "oxc-tsrx-lsp's version banner and errors are the CLI's contract"
)]
pub fn run_cli(arguments: &[String]) -> ExitCode {
    if arguments.iter().any(|argument| matches!(argument.as_str(), "-V" | "--version")) {
        println!("oxc-tsrx-lsp {} (OXC {})", env!("CARGO_PKG_VERSION"), oxc_adapter::OXC_REVISION);
        return ExitCode::SUCCESS;
    }
    if arguments.iter().any(|argument| matches!(argument.as_str(), "-h" | "--help")) {
        println!(
            "OXC for TSRX language server\n\nUsage: oxc-tsrx-lsp\n       oxc-tsrx-lsp --version"
        );
        return ExitCode::SUCCESS;
    }
    if let Some(argument) = arguments.iter().find(|argument| argument.as_str() != "--stdio") {
        eprintln!("oxc-tsrx-lsp: unsupported option: {argument}");
        return ExitCode::from(2);
    }
    match run_editor_server("OXC for TSRX", env!("CARGO_PKG_VERSION"), Arc::new(TsrxEditorFactory))
    {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("oxc-tsrx-lsp: {error}");
            ExitCode::from(2)
        }
    }
}

struct TsrxEditorFactory;

impl EditorToolFactory for TsrxEditorFactory {
    fn create(
        &self,
        workspace: &EditorWorkspace,
        options: &Value,
    ) -> Result<Box<dyn EditorTool>, String> {
        let root = workspace
            .root_path
            .clone()
            .or_else(|| env::current_dir().ok())
            .ok_or("editor workspace has no filesystem root")?;
        let lint_config = option_path(&root, options, "lintConfigPath");
        let format_config = option_path(&root, options, "formatConfigPath");
        let type_check = option_bool(options, "typeCheck");
        let type_aware = option_bool(options, "typeAware") || type_check;
        let filters = Vec::<ConfigRuleFilter>::new();
        let lint = build_lint_session(
            &root,
            lint_config.as_deref(),
            &filters,
            false,
            type_aware,
            type_check,
        )?;
        let actions = build_lint_session(
            &root,
            lint_config.as_deref(),
            &filters,
            true,
            type_aware,
            type_check,
        )?;
        let format = FormatSession::new(&root, format_config.as_deref())?;
        Ok(Box::new(TsrxEditorTool { lint, actions, format }))
    }

    fn watcher_patterns(&self, _workspace: &EditorWorkspace, options: &Value) -> Vec<String> {
        let mut patterns = vec![
            "**/.oxlintrc.json".to_string(),
            "**/.oxlintrc.jsonc".to_string(),
            "**/.oxfmtrc.json".to_string(),
            "**/.oxfmtrc.jsonc".to_string(),
        ];
        for (key, section) in [("lintConfigPath", "lint"), ("formatConfigPath", "format")] {
            if let Some(path) = option_string(options, key, section, "configPath")
                && !path.is_empty()
            {
                patterns.push(path.to_string());
            }
        }
        patterns
    }
}

fn option_bool(options: &Value, key: &str) -> bool {
    options.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn option_path(root: &std::path::Path, options: &Value, key: &str) -> Option<PathBuf> {
    let section = if key == "lintConfigPath" { "lint" } else { "format" };
    let path =
        option_string(options, key, section, "configPath").filter(|path| !path.is_empty())?;
    let path = PathBuf::from(path);
    Some(if path.is_absolute() { path } else { root.join(path) })
}

fn option_string<'a>(
    options: &'a Value,
    flat: &str,
    section: &str,
    nested: &str,
) -> Option<&'a str> {
    options.get(flat).and_then(Value::as_str).or_else(|| {
        options.get(section).and_then(|value| value.get(nested)).and_then(Value::as_str)
    })
}

fn build_lint_session(
    root: &std::path::Path,
    config: Option<&std::path::Path>,
    filters: &[ConfigRuleFilter],
    fix: bool,
    type_aware: bool,
    type_check: bool,
) -> Result<LintSession, String> {
    if type_aware {
        LintSession::new_type_aware_with_config_base(root, config, None, filters, fix, type_check)
    } else {
        LintSession::new(root, config, filters, fix)
    }
}

struct TsrxEditorTool {
    lint: LintSession,
    actions: LintSession,
    format: FormatSession,
}

impl TsrxEditorTool {
    fn source<'a>(document: &'a EditorDocument<'_>) -> Result<(PathBuf, &'a str), String> {
        let path =
            document.path.ok_or_else(|| format!("editor URI is not a file: {}", document.uri))?;
        if path.extension().is_none_or(|extension| extension != "tsrx") {
            return Err(format!("editor document is not TSRX: {}", path.display()));
        }
        let source = document
            .source
            .ok_or_else(|| format!("editor document has no in-memory source: {}", document.uri))?;
        Ok((path.to_path_buf(), source))
    }
}

impl EditorTool for TsrxEditorTool {
    fn diagnostics(&self, document: &EditorDocument<'_>) -> Result<Vec<EditorDiagnostic>, String> {
        let (path, source) = Self::source(document)?;
        if self.lint.should_ignore(&path) {
            return Ok(Vec::new());
        }
        let output = match self.lint.lint_text(&path, source) {
            Ok(output) => output,
            Err(error) => return Ok(vec![parse_error_diagnostic(source, error)]),
        };
        Ok(output
            .diagnostics
            .into_iter()
            .filter_map(|diagnostic| {
                let primary = diagnostic.labels.first()?;
                let start = primary.span.offset;
                let end = start.saturating_add(primary.span.length);
                Some(EditorDiagnostic {
                    range: EditorRange::new(start, end),
                    severity: if diagnostic.severity == "error" {
                        EditorSeverity::Error
                    } else {
                        EditorSeverity::Warning
                    },
                    code: Some(diagnostic.rule.clone()),
                    source: Some("oxlint-tsrx".to_string()),
                    message: diagnostic.message,
                    related: Vec::new(),
                    data: Some(json!({ "rule": diagnostic.rule, "code": diagnostic.code })),
                })
            })
            .collect())
    }

    fn format(&self, document: &EditorDocument<'_>) -> Result<Vec<EditorTextEdit>, String> {
        let (path, source) = Self::source(document)?;
        if self.format.should_ignore(&path) {
            return Ok(Vec::new());
        }
        let output = self.format.format_text(&path, source)?;
        if !output.changed {
            return Ok(Vec::new());
        }
        Ok(vec![EditorTextEdit {
            range: EditorRange::new(
                0,
                u32::try_from(source.len()).map_err(|_| "editor source is too large")?,
            ),
            new_text: output.code,
        }])
    }

    fn code_actions(
        &self,
        request: &EditorCodeActionRequest<'_>,
    ) -> Result<Vec<EditorCodeAction>, String> {
        if !request.only.is_empty()
            && !request.only.iter().any(|kind| kind == "quickfix" || "quickfix".starts_with(kind))
        {
            return Ok(Vec::new());
        }
        let (path, source) = Self::source(&request.document)?;
        if self.actions.should_ignore(&path) {
            return Ok(Vec::new());
        }
        Ok(self
            .actions
            .code_actions(&path, source)?
            .into_iter()
            .filter(|fix| {
                ranges_overlap(
                    request.range,
                    EditorRange::new(fix.offset, fix.offset.saturating_add(fix.length)),
                )
            })
            .map(|fix| EditorCodeAction {
                title: fix.title,
                kind: EditorActionKind::QuickFix,
                is_preferred: true,
                edits: vec![EditorDocumentEdit {
                    uri: request.document.uri.to_string(),
                    edits: vec![EditorTextEdit {
                        range: EditorRange::new(fix.offset, fix.offset.saturating_add(fix.length)),
                        new_text: fix.replacement,
                    }],
                }],
                data: Some(json!({ "rule": fix.rule })),
            })
            .collect())
    }
}

fn parse_error_diagnostic(source: &str, message: String) -> EditorDiagnostic {
    let offset = error_byte_offset(&message)
        .filter(|offset| *offset <= source.len() && source.is_char_boundary(*offset))
        .unwrap_or(0);
    let end =
        source[offset..].chars().next().map_or(offset, |character| offset + character.len_utf8());
    EditorDiagnostic {
        range: EditorRange::new(
            u32::try_from(offset).unwrap_or(0),
            u32::try_from(end).unwrap_or(0),
        ),
        severity: EditorSeverity::Error,
        code: Some("parse-error".to_string()),
        source: Some("oxc-tsrx".to_string()),
        message,
        related: Vec::new(),
        data: None,
    }
}

fn error_byte_offset(message: &str) -> Option<usize> {
    let marker = "byte ";
    let start = message.rfind(marker)? + marker.len();
    let digits = message[start..].chars().take_while(char::is_ascii_digit).collect::<String>();
    (!digits.is_empty()).then(|| digits.parse().ok()).flatten()
}

#[expect(
    clippy::suspicious_operation_groupings,
    reason = "an empty `left` overlaps when its single position lies inside `right`, so both comparisons are against `left.start` on purpose"
)]
fn ranges_overlap(left: EditorRange, right: EditorRange) -> bool {
    if left.start == left.end {
        return right.start <= left.start && left.start <= right.end;
    }
    left.start < right.end && right.start < left.end
}
