//! The `oxc-tsrx-lsp` language server. Selected by `argv[0]` or the `lsp`
//! subcommand.

use std::{env, path::PathBuf, process::ExitCode, sync::Arc};

use oxc_adapter::{
    LintError as EngineLintError,
    editor::{
        EditorActionKind, EditorCodeAction, EditorCodeActionRequest, EditorDiagnostic,
        EditorDocument, EditorDocumentEdit, EditorRange, EditorSeverity, EditorTextEdit,
        EditorTool, EditorToolFactory, EditorWorkspace, run_editor_server,
    },
};
use serde_json::{Value, json};
use tsrx_format::FormatSession;
use tsrx_lint::{ConfigRuleFilter, LintError, LintSession};

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
) -> Result<LintSession, LintError> {
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
            Err(error) => return Ok(vec![parse_error_diagnostic(source, &error)]),
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

fn parse_error_diagnostic(source: &str, error: &LintError) -> EditorDiagnostic {
    // Two of the ten variants position themselves in the authored source, and both hand the
    // offset over as a number. The remaining eight describe a whole-file or tool failure, so the
    // diagnostic covers the first character. This match is written out rather than wildcarded so
    // a future positioned variant fails to compile here instead of silently losing its offset.
    let positioned = match error {
        LintError::Projection(error) => error.byte_offset(),
        LintError::Syntax(EngineLintError::DynamicTags(error)) => error.byte_offset(),
        LintError::UnreadableSource { .. }
        | LintError::UnwritableSource { .. }
        | LintError::TextLintWithFixes
        | LintError::CodeActionsWithoutFixes
        | LintError::SourceKind(_)
        | LintError::Config(_)
        | LintError::Syntax(_)
        | LintError::TypeAware(_) => None,
    };
    // An offset is only usable if it still addresses this document: the editor can hand over a
    // buffer that has moved on since the error was produced. `is_char_boundary` is false past the
    // end, so it rejects a stale offset and a mid-character one in the same call.
    let offset = positioned
        .and_then(|offset| usize::try_from(offset).ok())
        .filter(|offset| source.is_char_boundary(*offset))
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
        message: error.to_string(),
        related: Vec::new(),
        data: None,
    }
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use oxc_adapter::{DynamicTagError, editor::EditorRange};
    use tsrx_lint::{LintError, LintSession};

    use super::{EngineLintError, parse_error_diagnostic};

    /// The range `parse_error_diagnostic` produced while it read the offset out of the rendered
    /// `Display` text: the last `byte ` anywhere in the whole message, then its digits.
    ///
    /// This is the retired implementation, kept only so the typed accessors can be held against
    /// it. It takes the last match of the marker, which is exactly the position the reverse `str`
    /// search it replaced returned.
    fn scraped_range(source: &str, message: &str) -> EditorRange {
        let marker = "byte ";
        let offset = message
            .rmatch_indices(marker)
            .next()
            .map(|(index, _)| index + marker.len())
            .map(|start| {
                message[start..].chars().take_while(char::is_ascii_digit).collect::<String>()
            })
            .filter(|digits| !digits.is_empty())
            .and_then(|digits| digits.parse::<usize>().ok())
            .filter(|offset| *offset <= source.len() && source.is_char_boundary(*offset))
            .unwrap_or(0);
        let end = source[offset..]
            .chars()
            .next()
            .map_or(offset, |character| offset + character.len_utf8());
        EditorRange::new(u32::try_from(offset).unwrap_or(0), u32::try_from(end).unwrap_or(0))
    }

    fn lint_failure(source: &str) -> LintError {
        LintSession::new_with_config_source(Path::new("/demo"), Some("{}"), &[], false)
            .expect("an in-memory config compiles without reading the filesystem")
            .lint_text(Path::new("View.tsrx"), source)
            .expect_err("the fixture must fail before it produces diagnostics")
    }

    #[test]
    fn the_typed_offset_reproduces_the_display_scrape_it_replaced() {
        // Both positioned variants, each reached through a real lint of a real authored source
        // rather than by hand: an unterminated element fails in projection, and a call expression
        // in a dynamic tag survives projection and fails against the parsed AST. The multi-byte
        // identifier in the first fixture shifts the offset off a code-unit count.
        let unterminated =
            "export function Broken() @{\n  let \u{3c0} = 1;\n  <main>\n    <h1>hi</h1>\n}\n";
        let dynamic_tag = "export function View() @{ <{tag()}>hi</{tag()}> }";
        for (source, reaches_the_syntax_lane) in [(unterminated, false), (dynamic_tag, true)] {
            let error = lint_failure(source);
            // Each fixture must exercise a different arm, or one of the two would go untested.
            assert_eq!(
                matches!(error, LintError::Syntax(EngineLintError::DynamicTags(_))),
                reaches_the_syntax_lane,
                "{source}: {error:?}"
            );
            assert!(
                matches!(
                    error,
                    LintError::Projection(_) | LintError::Syntax(EngineLintError::DynamicTags(_))
                ),
                "{source}: {error:?}"
            );
            let message = error.to_string();
            assert!(message.contains("byte "), "{source}: {message}");
            let diagnostic = parse_error_diagnostic(source, &error);
            assert_eq!(diagnostic.range, scraped_range(source, &message), "{source}: {message}");
            assert_ne!(diagnostic.range.start, 0, "{source}: {message}");
            assert_eq!(diagnostic.message, message);
        }
    }

    #[test]
    fn an_unaddressable_or_positionless_failure_still_lands_on_the_first_character() {
        // A stale offset past the end of the editor's buffer and a variant that never carries one
        // both fall back to the first character, which is what the scrape did too.
        let source = "short";
        let stale =
            LintError::Syntax(EngineLintError::DynamicTags(DynamicTagError::AuthoredGrammar {
                index: 0,
                offset: 4096,
            }));
        for error in [stale, LintError::TextLintWithFixes] {
            let diagnostic = parse_error_diagnostic(source, &error);
            assert_eq!(diagnostic.range, scraped_range(source, &error.to_string()), "{error:?}");
            assert_eq!(diagnostic.range, EditorRange::new(0, 1), "{error:?}");
        }
    }
}
