//! The `oxc-tsrx-lsp` language server. Selected by `argv[0]` or the `lsp`
//! subcommand.

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::ExitCode,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use oxc_adapter::{
    JsPluginFreeLintConfig,
    editor::{
        EditorActionKind, EditorCodeAction, EditorCodeActionRequest, EditorDiagnostic,
        EditorDocument, EditorDocumentEdit, EditorRange, EditorSeverity, EditorTextEdit,
        EditorTool, EditorToolFactory, EditorWorkspace, run_editor_server,
    },
    lint_config_without_js_plugins,
};
use serde_json::{Value, json};
use tsrx_format::FormatSession;
use tsrx_lint::{ConfigRuleFilter, LintSession};

pub fn run_cli(arguments: &[String]) -> ExitCode {
    if arguments
        .iter()
        .any(|argument| matches!(argument.as_str(), "-V" | "--version"))
    {
        println!(
            "oxc-tsrx-lsp {} (OXC {})",
            env!("CARGO_PKG_VERSION"),
            oxc_adapter::OXC_REVISION
        );
        return ExitCode::SUCCESS;
    }
    if arguments
        .iter()
        .any(|argument| matches!(argument.as_str(), "-h" | "--help"))
    {
        println!(
            "OXC for TSRX language server\n\nUsage: oxc-tsrx-lsp\n       oxc-tsrx-lsp --version"
        );
        return ExitCode::SUCCESS;
    }
    if let Some(argument) = arguments
        .iter()
        .find(|argument| argument.as_str() != "--stdio")
    {
        eprintln!("oxc-tsrx-lsp: unsupported option: {argument}");
        return ExitCode::from(2);
    }
    match run_editor_server(
        "OXC for TSRX",
        env!("CARGO_PKG_VERSION"),
        Arc::new(TsrxEditorFactory),
    ) {
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
        let format = FormatSession::new(&root, format_config.as_deref())?;
        // A lint session that cannot be built is a state the user has to be able to see.
        // Returning it as an error here loses it: the transport has nowhere to put a
        // workspace-construction failure, so the editor shows an empty file with no
        // diagnostics, no message, and nothing in the log. Keep the tool, remember why
        // linting is unavailable, and report it on every `.tsrx` file that is opened.
        match build_lint_sessions(&root, lint_config.as_deref(), type_aware, type_check) {
            Ok(sessions) => Ok(Box::new(TsrxEditorTool {
                lint: Some(sessions.lint),
                actions: Some(sessions.actions),
                format,
                unavailable: None,
                _staged_config: sessions.staged_config,
            })),
            Err(error) => {
                // Also on stderr, which clients surface as the server's output log.
                eprintln!("oxc-tsrx-lsp: TSRX linting is unavailable: {error}");
                Ok(Box::new(TsrxEditorTool {
                    lint: None,
                    actions: None,
                    format,
                    unavailable: Some(error),
                    _staged_config: None,
                }))
            }
        }
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
    let section = if key == "lintConfigPath" {
        "lint"
    } else {
        "format"
    };
    let path =
        option_string(options, key, section, "configPath").filter(|path| !path.is_empty())?;
    let path = PathBuf::from(path);
    Some(if path.is_absolute() {
        path
    } else {
        root.join(path)
    })
}

fn option_string<'a>(
    options: &'a Value,
    flat: &str,
    section: &str,
    nested: &str,
) -> Option<&'a str> {
    options.get(flat).and_then(Value::as_str).or_else(|| {
        options
            .get(section)
            .and_then(|value| value.get(nested))
            .and_then(Value::as_str)
    })
}

/// A stripped Oxlint configuration written to a throwaway directory, removed with the
/// workspace tool that owns it.
struct StagedConfig {
    directory: PathBuf,
    path: PathBuf,
    base: PathBuf,
}

impl StagedConfig {
    fn write(stripped: &JsPluginFreeLintConfig) -> Result<Self, String> {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let directory = env::temp_dir().join(format!(
            "oxc-tsrx-lsp-config-{}-{nanos}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir_all(&directory).map_err(|error| {
            format!(
                "unable to stage a JS-plugin-free Oxlint config in {}: {error}",
                directory.display()
            )
        })?;
        let path = directory.join(".oxlintrc.json");
        fs::write(&path, &stripped.json).map_err(|error| {
            format!(
                "unable to stage a JS-plugin-free Oxlint config at {}: {error}",
                path.display()
            )
        })?;
        Ok(Self {
            directory,
            path,
            base: stripped.base.clone(),
        })
    }
}

impl Drop for StagedConfig {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

struct LintSessions {
    lint: LintSession,
    actions: LintSession,
    staged_config: Option<StagedConfig>,
}

/// Build the diagnostics and quick-fix sessions this workspace lints with.
///
/// A project's `jsPlugins` are hosted by the `oxlint` command OXC for TSRX installs,
/// over each `.tsrx` file's TSX projection. The native engine refuses a configuration
/// that still declares them, so the command line strips them before handing the config
/// down. This is the same strip on the editor path: without it, adding one JavaScript
/// plugin to `.oxlintrc.json` takes away every diagnostic on every `.tsrx` file,
/// including the native Rust ones that have nothing to do with plugins.
///
/// The stripped copy is written to a temporary directory and handed over with the
/// directory the original was authored in as its config base, so relative `extends`,
/// `overrides` globs and `ignorePatterns` still resolve exactly where they did.
fn build_lint_sessions(
    root: &Path,
    config: Option<&Path>,
    type_aware: bool,
    type_check: bool,
) -> Result<LintSessions, String> {
    let filters = Vec::<ConfigRuleFilter>::new();
    let staged_config = match lint_config_without_js_plugins(root, config)? {
        Some(stripped) => Some(StagedConfig::write(&stripped)?),
        None => None,
    };
    let (config_path, config_base) = match &staged_config {
        Some(staged) => (Some(staged.path.as_path()), Some(staged.base.as_path())),
        None => (config, None),
    };
    Ok(LintSessions {
        lint: build_lint_session(
            root,
            config_path,
            config_base,
            &filters,
            false,
            type_aware,
            type_check,
        )?,
        actions: build_lint_session(
            root,
            config_path,
            config_base,
            &filters,
            true,
            type_aware,
            type_check,
        )?,
        staged_config,
    })
}

fn build_lint_session(
    root: &Path,
    config: Option<&Path>,
    config_base: Option<&Path>,
    filters: &[ConfigRuleFilter],
    fix: bool,
    type_aware: bool,
    type_check: bool,
) -> Result<LintSession, String> {
    if type_aware {
        LintSession::new_type_aware_with_config_base(
            root,
            config,
            config_base,
            filters,
            fix,
            type_check,
        )
    } else {
        LintSession::new_with_config_base(root, config, config_base, filters, fix)
    }
}

struct TsrxEditorTool {
    lint: Option<LintSession>,
    actions: Option<LintSession>,
    format: FormatSession,
    /// Why linting is unavailable in this workspace, when it is.
    unavailable: Option<String>,
    /// Held so the staged configuration outlives the sessions compiled from it and is
    /// removed with them.
    _staged_config: Option<StagedConfig>,
}

impl TsrxEditorTool {
    fn source<'a>(document: &'a EditorDocument<'_>) -> Result<(PathBuf, &'a str), String> {
        let path = document
            .path
            .ok_or_else(|| format!("editor URI is not a file: {}", document.uri))?;
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
        let Some(lint) = self.lint.as_ref() else {
            // Silence here is the worst answer available: the file looks clean, the
            // native rules that were working a moment ago are gone, and nothing says
            // why. Publish the reason as this file's own diagnostic instead.
            return Ok(vec![unavailable_diagnostic(
                source,
                self.unavailable.as_deref().unwrap_or(
                    "TSRX linting is unavailable for this workspace and no reason was recorded",
                ),
            )]);
        };
        if lint.should_ignore(&path) {
            return Ok(Vec::new());
        }
        let output = match lint.lint_text(&path, source) {
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
            && !request
                .only
                .iter()
                .any(|kind| kind == "quickfix" || "quickfix".starts_with(kind))
        {
            return Ok(Vec::new());
        }
        let (path, source) = Self::source(&request.document)?;
        // The refusal is already published as a diagnostic; a quick fix cannot repair a
        // configuration, so this stays quiet rather than reporting it a second time.
        let Some(actions) = self.actions.as_ref() else {
            return Ok(Vec::new());
        };
        if actions.should_ignore(&path) {
            return Ok(Vec::new());
        }
        Ok(actions
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

/// The reason TSRX linting is unavailable, as this file's own diagnostic.
///
/// It is anchored at the first character so an editor has something to underline and
/// the message reaches the Problems panel, the hover, and the client's own log.
fn unavailable_diagnostic(source: &str, reason: &str) -> EditorDiagnostic {
    let end = source.chars().next().map_or(0, char::len_utf8);
    EditorDiagnostic {
        range: EditorRange::new(0, u32::try_from(end).unwrap_or(0)),
        severity: EditorSeverity::Error,
        code: Some("lint-unavailable".to_string()),
        source: Some("oxc-tsrx".to_string()),
        message: format!("OXC for TSRX cannot lint this file: {reason}"),
        related: Vec::new(),
        data: Some(json!({ "rule": "lint-unavailable" })),
    }
}

fn parse_error_diagnostic(source: &str, message: String) -> EditorDiagnostic {
    let offset = error_byte_offset(&message)
        .filter(|offset| *offset <= source.len() && source.is_char_boundary(*offset))
        .unwrap_or(0);
    let end = source[offset..]
        .chars()
        .next()
        .map_or(offset, |character| offset + character.len_utf8());
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
    let digits = message[start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>();
    (!digits.is_empty()).then(|| digits.parse().ok()).flatten()
}

fn ranges_overlap(left: EditorRange, right: EditorRange) -> bool {
    if left.start == left.end {
        return right.start <= left.start && left.start <= right.end;
    }
    left.start < right.end && right.start < left.end
}
