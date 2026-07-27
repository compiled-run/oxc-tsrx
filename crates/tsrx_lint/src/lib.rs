//! Native lint orchestration and identity-only TSRX fix mapping.

use std::{
    cmp::Reverse,
    collections::{HashMap, HashSet},
    ffi::OsString,
    fmt, fs,
    path::{Path, PathBuf},
    time::Instant,
};

use oxc_adapter::{
    DynamicTagContract, EngineDiagnostic, LintEngine, LintEngineOptions, LintRequest, LintResult,
    OXC_REVISION, RuleFilter, RuleSeverity, SourceKind, TypeBatchFile,
};
use serde::Serialize;
use tsrx_syntax::{
    MappedProjection, ProjectionError, TypeProjection, project_for_lint, project_for_types, scan,
};

#[derive(Debug, Clone)]
pub struct Options {
    pub rules: Vec<String>,
    pub fix: bool,
}
pub use oxc_adapter::{RuleFilter as ConfigRuleFilter, RuleSeverity as ConfigRuleSeverity};

/// One compiled configuration reused across every file in a lint command/editor batch.
pub struct LintSession {
    engine: LintEngine,
    fix: bool,
}

impl LintSession {
    /// Discover or explicitly load one JSON/JSONC Oxlint configuration.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid or unsupported configuration before any source is parsed.
    pub fn new(
        cwd: &Path,
        config_path: Option<&Path>,
        filters: &[RuleFilter],
        fix: bool,
    ) -> Result<Self, String> {
        Self::new_with_config_base(cwd, config_path, None, filters, fix)
    }

    /// Load a materialized config while preserving the directory in which it was authored.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid base or any invalid/unsupported lint configuration.
    pub fn new_with_config_base(
        cwd: &Path,
        config_path: Option<&Path>,
        config_base: Option<&Path>,
        filters: &[RuleFilter],
        fix: bool,
    ) -> Result<Self, String> {
        Self::new_with_capabilities(cwd, config_path, config_base, filters, fix, false, false)
    }

    /// Build a session with the explicitly opted-in TypeScript-Go lane.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid configuration. Missing tsgolint is reported on the first lint
    /// operation so session construction remains side-effect free.
    pub fn new_type_aware_with_config_base(
        cwd: &Path,
        config_path: Option<&Path>,
        config_base: Option<&Path>,
        filters: &[RuleFilter],
        fix: bool,
        type_check: bool,
    ) -> Result<Self, String> {
        Self::new_with_capabilities(cwd, config_path, config_base, filters, fix, true, type_check)
    }

    /// Build a session from an in-memory JSON Oxlint configuration without
    /// reading the filesystem (used by the WebAssembly playground).
    ///
    /// # Errors
    ///
    /// Returns an error for invalid or unsupported configuration.
    pub fn new_with_config_source(
        cwd: &Path,
        config_source: Option<&str>,
        filters: &[RuleFilter],
        fix: bool,
    ) -> Result<Self, String> {
        let engine = LintEngine::new_from_config_source(cwd, config_source, filters, fix)?;
        Ok(Self { engine, fix })
    }

    fn new_with_capabilities(
        cwd: &Path,
        config_path: Option<&Path>,
        config_base: Option<&Path>,
        filters: &[RuleFilter],
        fix: bool,
        type_aware: bool,
        type_check: bool,
    ) -> Result<Self, String> {
        let options =
            LintEngineOptions { cwd, config_path, config_base, filters, collect_fixes: fix };
        let engine = if type_aware {
            LintEngine::new_type_aware(&options, type_check)?
        } else {
            LintEngine::new(&options)?
        };
        Ok(Self { engine, fix })
    }

    #[must_use]
    pub fn should_ignore(&self, path: &Path) -> bool {
        self.engine.should_ignore(path)
    }

    #[must_use]
    pub fn deny_warnings(&self) -> bool {
        self.engine.deny_warnings()
    }

    #[must_use]
    pub fn max_warnings(&self) -> Option<usize> {
        self.engine.max_warnings()
    }

    /// Lint one filesystem source with this compiled configuration.
    ///
    /// A TSRX file that cannot be scanned or projected is reported as that file's own error
    /// diagnostic rather than as a command failure, so a syntax error reads like every other
    /// diagnostic and never discards the rest of a batch. Read, parser, semantic, and lint
    /// failures remain errors.
    ///
    /// # Errors
    ///
    /// Returns an error without writing for read, parser, semantic, or lint failures.
    pub fn lint_file(&self, path: &Path) -> Result<Output, String> {
        let source = fs::read_to_string(path)
            .map_err(|error| format!("Unable to read {}: {error}", path.display()))?;
        lint_loaded_file(self, path, &source, true)
    }

    /// Lint a filesystem batch with one shared TypeScript-Go project process when opted in.
    ///
    /// One unprojectable TSRX file contributes its own error diagnostic and the batch continues,
    /// so a single typo cannot blank every other file's report.
    ///
    /// # Errors
    ///
    /// Returns before writing if any source, OXC pass, or type-aware batch fails.
    pub fn lint_files(&self, paths: &[PathBuf]) -> Result<Vec<Output>, String> {
        if !self.engine.type_aware_enabled() {
            return paths.iter().map(|path| self.lint_file(path)).collect();
        }
        // Each path keeps its slot so a file that fails projection stays in argument order
        // alongside the files that reached the shared type-aware batch.
        let mut ordered = Vec::with_capacity(paths.len());
        ordered.resize_with(paths.len(), || None);
        let mut pending = Vec::with_capacity(paths.len());
        for (slot, path) in paths.iter().enumerate() {
            let source = fs::read_to_string(path)
                .map_err(|error| format!("Unable to read {}: {error}", path.display()))?;
            match run_syntax_lint(self, path, &source) {
                Ok((prepared, syntax)) => pending.push(PendingBatchFile {
                    slot,
                    path: path.clone(),
                    source,
                    prepared,
                    syntax,
                }),
                Err(PrepareError::Projection(error)) => {
                    ordered[slot] = Some(projection_failure_output(self, path, &error));
                }
                Err(PrepareError::Other(message)) => return Err(message),
            }
        }
        if pending.is_empty() {
            return Ok(ordered.into_iter().flatten().collect());
        }
        let virtual_paths = pending
            .iter()
            .map(|file| {
                if file.prepared.is_tsrx {
                    virtual_type_path(&file.path)
                } else {
                    file.path.clone()
                }
            })
            .collect::<Vec<_>>();
        let batch_files = pending
            .iter()
            .zip(&virtual_paths)
            .map(|(file, virtual_path)| TypeBatchFile {
                authored_path: &file.path,
                virtual_path,
                projected_source: file
                    .prepared
                    .type_projection
                    .as_ref()
                    .map_or(file.source.as_str(), TypeProjection::source),
                disable_directives: file.syntax.disable_directives.as_ref(),
            })
            .collect::<Vec<_>>();
        let type_result = self.engine.lint_type_batch(&batch_files, self.fix)?;
        let mut by_path = HashMap::<PathBuf, Vec<EngineDiagnostic>>::new();
        let mut global = Vec::new();
        for result in type_result.diagnostics {
            if let Some(path) = result.virtual_path {
                by_path.entry(path).or_default().push(result.diagnostic);
            } else {
                global.push(result.diagnostic);
            }
        }
        for (index, (file, virtual_path)) in pending.into_iter().zip(virtual_paths).enumerate() {
            let mut diagnostics = by_path.remove(&virtual_path).unwrap_or_default();
            if index == 0 {
                diagnostics.append(&mut global);
            }
            let slot = file.slot;
            ordered[slot] = Some(finish_lint(
                self,
                &file.path,
                &file.source,
                file.prepared,
                file.syntax,
                diagnostics,
                true,
                if index == 0 { type_result.elapsed_ns } else { 0 },
                if index == 0 { type_result.process_count } else { 0 },
            )?);
        }
        Ok(ordered.into_iter().flatten().collect())
    }

    /// Lint caller-owned source with this compiled configuration and no filesystem writes.
    ///
    /// Unlike [`LintSession::lint_file`], a projection failure stays an error here. The editor
    /// boundary in `oxc_tsrx_cli::lsp` renders it as its own LSP diagnostic and must keep
    /// receiving it as an error.
    ///
    /// # Errors
    ///
    /// Returns an error when this session was created with fixes enabled or linting fails.
    pub fn lint_text(&self, path: &Path, source: &str) -> Result<Output, String> {
        if self.fix {
            return Err("LintSession::lint_text cannot apply filesystem fixes".to_string());
        }
        lint_loaded_source(self, path, source, false).map_err(|error| error.to_string())
    }

    /// Collect safe, mapped, validation-passed edits for an in-memory editor document.
    ///
    /// This method never writes the document. The session must have been constructed with
    /// `fix = true` so canonical OXC and tsgolint retain their fix payloads. Every returned edit
    /// belongs to one authored affine range and has survived the same TSRX validation reparse as
    /// a filesystem fix.
    ///
    /// # Errors
    ///
    /// Returns an error when fix collection was not enabled or linting cannot complete.
    pub fn code_actions(&self, path: &Path, source: &str) -> Result<Vec<EditorFix>, String> {
        if !self.fix {
            return Err("LintSession::code_actions requires a fix-enabled session".to_string());
        }
        let (prepared, syntax) =
            run_syntax_lint(self, path, source).map_err(|error| error.to_string())?;
        let (type_diagnostics, _, _) = run_type_lint(self, path, source, &prepared, &syntax)?;
        let mut translated =
            translate_diagnostics(syntax.diagnostics, prepared.projection.as_ref());
        let mut type_translated =
            translate_type_diagnostics(type_diagnostics, prepared.type_projection.as_ref());
        translated.diagnostics.append(&mut type_translated.diagnostics);
        Ok(collect_editor_fixes(self, path, source, &prepared, &translated.diagnostics))
    }

    /// Combine file results without recompiling configuration or hiding per-file work.
    #[must_use]
    pub fn aggregate(&self, outputs: Vec<Output>) -> Output {
        aggregate_outputs(self, outputs)
    }
}

#[derive(Debug, Serialize)]
pub struct SpanOutput {
    pub offset: u32,
    pub length: u32,
}

#[derive(Debug, Serialize)]
pub struct LabelOutput {
    pub span: SpanOutput,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DiagnosticOutput {
    pub filename: String,
    pub rule: String,
    pub code: String,
    pub severity: String,
    pub message: String,
    pub labels: Vec<LabelOutput>,
}

/// One safe authored-source edit exposed to an editor without writing the file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorFix {
    pub title: String,
    pub rule: String,
    pub offset: u32,
    pub length: u32,
    pub replacement: String,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimingOutput {
    pub config_ns: u64,
    pub scan_ns: u64,
    pub projection_ns: u64,
    pub parse_ns: u64,
    pub semantic_ns: u64,
    pub lint_ns: u64,
    pub type_aware_ns: u64,
}

#[derive(Debug, Default, Serialize)]
pub struct FileCounts {
    pub tsrx: u32,
    pub standard: u32,
}

#[derive(Debug, Default, Serialize)]
pub struct FixOutput {
    pub applied: u32,
    pub rejected: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    pub native: bool,
    pub engine: &'static str,
    pub oxc_revision: &'static str,
    pub mode: &'static str,
    pub config_loads: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_path: Option<String>,
    pub parse_count: u32,
    pub reparse_count: u32,
    pub files: FileCounts,
    pub timings: TimingOutput,
    pub projection_bytes: usize,
    pub diagnostics_suppressed: u32,
    pub fixes: FixOutput,
    pub type_aware: bool,
    pub type_check: bool,
    pub type_aware_files: u32,
    pub type_aware_processes: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct Output {
    pub diagnostics: Vec<DiagnosticOutput>,
    pub number_of_files: u32,
    pub number_of_rules: usize,
    #[serde(rename = "oxcTsrx")]
    pub metadata: Metadata,
}

struct PreparedSource {
    projection: Option<MappedProjection>,
    type_projection: Option<TypeProjection>,
    source_kind: SourceKind,
    is_tsrx: bool,
    timings: TimingOutput,
}

struct PendingBatchFile {
    slot: usize,
    path: PathBuf,
    source: String,
    prepared: PreparedSource,
    syntax: LintResult,
}

/// A per-file failure that has not yet been decided to be a diagnostic or a command failure.
///
/// `Projection` is the one kind the CLI lint lane turns into a diagnostic: it is a syntax error in
/// the user's own TSRX file, positioned by [`ProjectionError::byte_offset`]. Everything else is a
/// genuine tool failure. `Display` reproduces each message unchanged, so a caller that maps this
/// straight back to a `String` keeps the exact text it had before.
#[derive(Debug)]
enum PrepareError {
    Projection(ProjectionError),
    Other(String),
}

impl fmt::Display for PrepareError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Projection(error) => error.fmt(formatter),
            Self::Other(message) => formatter.write_str(message),
        }
    }
}

impl From<ProjectionError> for PrepareError {
    fn from(error: ProjectionError) -> Self {
        Self::Projection(error)
    }
}

impl From<String> for PrepareError {
    fn from(message: String) -> Self {
        Self::Other(message)
    }
}

impl PreparedSource {
    fn parse_source<'a>(&'a self, original: &'a str) -> &'a str {
        self.projection.as_ref().map_or(original, MappedProjection::source)
    }

    fn projection_bytes(&self) -> usize {
        self.projection.as_ref().map_or(0, |projection| projection.source().len())
    }
}

#[derive(Default)]
struct TranslatedDiagnostics {
    diagnostics: Vec<EngineDiagnostic>,
    suppressed: u32,
    rejected_fixes: u32,
}

#[derive(Default)]
struct AppliedFixes {
    applied: u32,
    rejected: u32,
    reparse_count: u32,
    rules: Vec<String>,
}

/// Lints one explicit JavaScript, TypeScript, TSX, or TSRX file with canonical OXC.
///
/// # Errors
///
/// Returns an error without writing when the file cannot be read, projected, parsed, analyzed,
/// linted, fix-validated, or written.
pub fn lint_file(path: &Path, options: &Options) -> Result<Output, String> {
    let session = legacy_session(path, options)?;
    session.lint_file(path)
}

/// Lints source already owned by a caller without filesystem I/O.
///
/// This is the native library/editor boundary and the benchmarkable hot path. Fixes are rejected
/// here because an in-memory caller must choose how to apply edits; [`lint_file`] remains the
/// write-capable boundary for the current native CLI.
///
/// # Errors
///
/// Returns an error when fixes are requested or the source cannot be projected, parsed, analyzed,
/// or linted.
pub fn lint_text(path: &Path, source: &str, options: &Options) -> Result<Output, String> {
    if options.fix {
        return Err("lint_text does not write or apply fixes; use lint_file".to_string());
    }
    let session = legacy_session(path, options)?;
    lint_loaded_source(&session, path, source, false).map_err(|error| error.to_string())
}

fn legacy_session(path: &Path, options: &Options) -> Result<LintSession, String> {
    let filters = options
        .rules
        .iter()
        .map(|name| RuleFilter { severity: RuleSeverity::Deny, name: name.clone() })
        .collect::<Vec<_>>();
    LintSession::new(path.parent().unwrap_or_else(|| Path::new(".")), None, &filters, options.fix)
}

fn aggregate_outputs(session: &LintSession, outputs: Vec<Output>) -> Output {
    let mode = if outputs.len() == 1 { outputs[0].metadata.mode } else { "batch" };
    let mut diagnostics = Vec::new();
    let mut number_of_files = 0_u32;
    let mut parse_count = 0_u32;
    let mut reparse_count = 0_u32;
    let mut file_counts = FileCounts::default();
    let mut timings =
        TimingOutput { config_ns: session.engine.config_load_ns(), ..TimingOutput::default() };
    let mut projection_bytes = 0_usize;
    let mut diagnostics_suppressed = 0_u32;
    let mut fix_counts = FixOutput { applied: 0, rejected: 0 };
    let mut type_aware_files = 0_u32;
    let mut type_aware_processes = 0_u32;
    for output in outputs {
        number_of_files = number_of_files.saturating_add(output.number_of_files);
        parse_count = parse_count.saturating_add(output.metadata.parse_count);
        reparse_count = reparse_count.saturating_add(output.metadata.reparse_count);
        file_counts.tsrx = file_counts.tsrx.saturating_add(output.metadata.files.tsrx);
        file_counts.standard = file_counts.standard.saturating_add(output.metadata.files.standard);
        timings.scan_ns = timings.scan_ns.saturating_add(output.metadata.timings.scan_ns);
        timings.projection_ns =
            timings.projection_ns.saturating_add(output.metadata.timings.projection_ns);
        timings.parse_ns = timings.parse_ns.saturating_add(output.metadata.timings.parse_ns);
        timings.semantic_ns =
            timings.semantic_ns.saturating_add(output.metadata.timings.semantic_ns);
        timings.lint_ns = timings.lint_ns.saturating_add(output.metadata.timings.lint_ns);
        timings.type_aware_ns =
            timings.type_aware_ns.saturating_add(output.metadata.timings.type_aware_ns);
        projection_bytes = projection_bytes.saturating_add(output.metadata.projection_bytes);
        diagnostics_suppressed =
            diagnostics_suppressed.saturating_add(output.metadata.diagnostics_suppressed);
        fix_counts.applied = fix_counts.applied.saturating_add(output.metadata.fixes.applied);
        fix_counts.rejected = fix_counts.rejected.saturating_add(output.metadata.fixes.rejected);
        type_aware_files = type_aware_files.saturating_add(output.metadata.type_aware_files);
        type_aware_processes =
            type_aware_processes.saturating_add(output.metadata.type_aware_processes);
        diagnostics.extend(output.diagnostics);
    }
    Output {
        diagnostics,
        number_of_files,
        number_of_rules: session.engine.number_of_rules(),
        metadata: Metadata {
            native: true,
            engine: "oxc_linter",
            oxc_revision: OXC_REVISION,
            mode,
            config_loads: session.engine.config_loads(),
            config_path: session
                .engine
                .config_path()
                .map(|path| path.to_string_lossy().into_owned()),
            parse_count,
            reparse_count,
            files: file_counts,
            timings,
            projection_bytes,
            diagnostics_suppressed,
            fixes: fix_counts,
            type_aware: session.engine.type_aware_enabled(),
            type_check: session.engine.type_check_enabled(),
            type_aware_files,
            type_aware_processes,
        },
    }
}

/// Lint one loaded file, reporting an unprojectable TSRX source as that file's own diagnostic.
///
/// This is the filesystem CLI boundary. A syntax error in the user's source is their defect, not
/// the linter's, so it is answered with a positioned error diagnostic and the caller's exit code
/// falls out of the usual error count.
fn lint_loaded_file(
    session: &LintSession,
    path: &Path,
    source: &str,
    allow_writes: bool,
) -> Result<Output, String> {
    match lint_loaded_source(session, path, source, allow_writes) {
        Ok(output) => Ok(output),
        Err(PrepareError::Projection(error)) => {
            Ok(projection_failure_output(session, path, &error))
        }
        Err(PrepareError::Other(message)) => Err(message),
    }
}

/// Build the one-diagnostic report that stands in for a TSRX file the scanner could not project.
///
/// The diagnostic carries the filename and, for the four offset-bearing failures, the authored
/// byte offset in `labels[0].span.offset`, which is the same shape every other diagnostic uses.
/// It carries no rule and no code because there is no rule to disable; the failure is the source
/// itself. The nine positionless failures get no label rather than a fabricated offset 0.
fn projection_failure_output(
    session: &LintSession,
    path: &Path,
    error: &ProjectionError,
) -> Output {
    let labels = error
        .byte_offset()
        .map(|offset| LabelOutput { span: SpanOutput { offset, length: 0 }, message: None })
        .into_iter()
        .collect();
    Output {
        diagnostics: vec![DiagnosticOutput {
            filename: path.to_string_lossy().into_owned(),
            rule: String::new(),
            code: String::new(),
            severity: "error".to_string(),
            message: error.to_string(),
            labels,
        }],
        number_of_files: 1,
        number_of_rules: session.engine.number_of_rules(),
        metadata: Metadata {
            native: true,
            engine: "oxc_linter",
            oxc_revision: OXC_REVISION,
            mode: "mapped_projection",
            config_loads: 0,
            config_path: None,
            parse_count: 0,
            reparse_count: 0,
            files: FileCounts { tsrx: 1, standard: 0 },
            timings: TimingOutput::default(),
            projection_bytes: 0,
            diagnostics_suppressed: 0,
            fixes: FixOutput::default(),
            type_aware: session.engine.type_aware_enabled(),
            type_check: session.engine.type_check_enabled(),
            type_aware_files: 0,
            type_aware_processes: 0,
        },
    }
}

fn lint_loaded_source(
    session: &LintSession,
    path: &Path,
    source: &str,
    allow_writes: bool,
) -> Result<Output, PrepareError> {
    let (prepared, syntax) = run_syntax_lint(session, path, source)?;
    let (diagnostics, type_aware_ns, type_aware_processes) =
        run_type_lint(session, path, source, &prepared, &syntax)?;
    Ok(finish_lint(
        session,
        path,
        source,
        prepared,
        syntax,
        diagnostics,
        allow_writes,
        type_aware_ns,
        type_aware_processes,
    )?)
}

fn run_type_lint(
    session: &LintSession,
    path: &Path,
    source: &str,
    prepared: &PreparedSource,
    syntax: &LintResult,
) -> Result<(Vec<EngineDiagnostic>, u64, u32), String> {
    if !session.engine.type_aware_enabled() {
        return Ok((Vec::new(), 0, 0));
    }
    let virtual_path = if prepared.is_tsrx { virtual_type_path(path) } else { path.to_path_buf() };
    let projected_source = prepared.type_projection.as_ref().map_or(source, TypeProjection::source);
    let batch = session.engine.lint_type_batch(
        &[TypeBatchFile {
            authored_path: path,
            virtual_path: &virtual_path,
            projected_source,
            disable_directives: syntax.disable_directives.as_ref(),
        }],
        session.fix,
    )?;
    let diagnostics = batch
        .diagnostics
        .into_iter()
        .filter(|diagnostic| {
            diagnostic.virtual_path.as_ref().is_none_or(|candidate| candidate == &virtual_path)
        })
        .map(|diagnostic| diagnostic.diagnostic)
        .collect();
    Ok((diagnostics, batch.elapsed_ns, batch.process_count))
}

fn run_syntax_lint(
    session: &LintSession,
    path: &Path,
    source: &str,
) -> Result<(PreparedSource, LintResult), PrepareError> {
    let prepared = prepare_source(path, source, session.engine.type_aware_enabled())?;
    let parse_source = prepared.parse_source(source);
    let syntax = session.engine.lint(&LintRequest {
        path,
        original_source: source,
        parse_source,
        source_kind: prepared.source_kind,
        rules: &[],
        collect_fixes: session.fix,
        dynamic_tags: prepared.projection.as_ref().and_then(|projection| {
            projection.dynamic_contract().map(|(prefix, count, original_offsets)| {
                DynamicTagContract { prefix, count, original_offsets }
            })
        }),
    })?;
    Ok((prepared, syntax))
}

#[allow(clippy::too_many_arguments)]
fn finish_lint(
    session: &LintSession,
    path: &Path,
    source: &str,
    mut prepared: PreparedSource,
    syntax: LintResult,
    type_diagnostics: Vec<EngineDiagnostic>,
    allow_writes: bool,
    type_aware_ns: u64,
    type_aware_processes: u32,
) -> Result<Output, String> {
    prepared.timings.parse_ns = syntax.timings.parse_ns;
    prepared.timings.semantic_ns = syntax.timings.semantic_ns;
    prepared.timings.lint_ns = syntax.timings.lint_ns;
    prepared.timings.type_aware_ns = type_aware_ns;
    let mut translated = translate_diagnostics(syntax.diagnostics, prepared.projection.as_ref());
    let mut type_translated =
        translate_type_diagnostics(type_diagnostics, prepared.type_projection.as_ref());
    translated.diagnostics.append(&mut type_translated.diagnostics);
    translated.suppressed = translated.suppressed.saturating_add(type_translated.suppressed);
    translated.rejected_fixes =
        translated.rejected_fixes.saturating_add(type_translated.rejected_fixes);
    let fixes = if session.fix && allow_writes {
        apply_safe_fixes(
            session,
            path,
            source,
            &prepared,
            &translated.diagnostics,
            translated.rejected_fixes,
        )?
    } else {
        AppliedFixes { rejected: translated.rejected_fixes, ..AppliedFixes::default() }
    };
    let diagnostics = map_diagnostics(path, translated.diagnostics, &fixes.rules);

    let projection_bytes = prepared.projection_bytes();
    Ok(Output {
        diagnostics,
        number_of_files: 1,
        number_of_rules: session.engine.number_of_rules(),
        metadata: Metadata {
            native: true,
            engine: "oxc_linter",
            oxc_revision: OXC_REVISION,
            mode: if prepared.is_tsrx { "mapped_projection" } else { "direct" },
            config_loads: 0,
            config_path: None,
            parse_count: syntax.parse_count,
            reparse_count: fixes.reparse_count,
            files: FileCounts {
                tsrx: u32::from(prepared.is_tsrx),
                standard: u32::from(!prepared.is_tsrx),
            },
            timings: prepared.timings,
            projection_bytes,
            diagnostics_suppressed: translated.suppressed,
            fixes: FixOutput { applied: fixes.applied, rejected: fixes.rejected },
            type_aware: session.engine.type_aware_enabled(),
            type_check: session.engine.type_check_enabled(),
            type_aware_files: u32::from(session.engine.type_aware_enabled()),
            type_aware_processes,
        },
    })
}

fn prepare_source(
    path: &Path,
    source: &str,
    type_aware: bool,
) -> Result<PreparedSource, PrepareError> {
    let is_tsrx = path.extension().is_some_and(|extension| extension == "tsrx");
    let mut timings = TimingOutput::default();
    if !is_tsrx {
        return Ok(PreparedSource {
            projection: None,
            type_projection: None,
            source_kind: source_kind(path)?,
            is_tsrx,
            timings,
        });
    }
    let started = Instant::now();
    let overlay = scan(source)?;
    timings.scan_ns = elapsed_ns(started);
    let started = Instant::now();
    let projection = project_for_lint(source, &overlay)?;
    let type_projection = type_aware.then(|| project_for_types(source, &overlay)).transpose()?;
    timings.projection_ns = elapsed_ns(started);
    Ok(PreparedSource {
        projection: Some(projection),
        type_projection,
        source_kind: SourceKind::TypeScriptReact,
        is_tsrx,
        timings,
    })
}

fn virtual_type_path(path: &Path) -> PathBuf {
    let mut path = OsString::from(path.as_os_str());
    path.push(".tsx");
    PathBuf::from(path)
}

fn apply_safe_fixes(
    session: &LintSession,
    path: &Path,
    source: &str,
    prepared: &PreparedSource,
    diagnostics: &[EngineDiagnostic],
    rejected_fixes: u32,
) -> Result<AppliedFixes, String> {
    let mut result = AppliedFixes { rejected: rejected_fixes, ..AppliedFixes::default() };
    let mut edits = Vec::new();
    for diagnostic in diagnostics {
        for fix in &diagnostic.fixes {
            let range = fix.offset..fix.offset.saturating_add(fix.length);
            if !fix.safe || range.end as usize > source.len() {
                result.rejected += 1;
                continue;
            }
            edits.push((range, fix.replacement.clone(), diagnostic.rule.clone()));
        }
    }
    edits.sort_unstable_by_key(|edit| Reverse(edit.0.start));
    let mut fixed = source.to_string();
    let mut previous_start = u32::MAX;
    for (range, replacement, rule) in edits {
        if range.end > previous_start {
            result.rejected += 1;
            continue;
        }
        fixed.replace_range(range.start as usize..range.end as usize, &replacement);
        previous_start = range.start;
        result.applied += 1;
        result.rules.extend(rule);
    }
    if result.applied > 0 {
        validate_fixed(session, path, &fixed, prepared.is_tsrx, prepared.source_kind)?;
        result.reparse_count = 1;
        fs::write(path, fixed)
            .map_err(|error| format!("Unable to write {}: {error}", path.display()))?;
    }
    Ok(result)
}

fn collect_editor_fixes(
    session: &LintSession,
    path: &Path,
    source: &str,
    prepared: &PreparedSource,
    diagnostics: &[EngineDiagnostic],
) -> Vec<EditorFix> {
    let mut fixes = Vec::new();
    let mut seen = HashSet::new();
    for diagnostic in diagnostics {
        let rule = diagnostic.rule.clone().unwrap_or_else(|| diagnostic_code(diagnostic));
        for fix in &diagnostic.fixes {
            let end = fix.offset.saturating_add(fix.length);
            if !fix.safe || end as usize > source.len() {
                continue;
            }
            let identity = (fix.offset, fix.length, fix.replacement.clone());
            if !seen.insert(identity) {
                continue;
            }
            let mut candidate = source.to_string();
            candidate.replace_range(fix.offset as usize..end as usize, &fix.replacement);
            if validate_fixed(session, path, &candidate, prepared.is_tsrx, prepared.source_kind)
                .is_err()
            {
                continue;
            }
            fixes.push(EditorFix {
                title: format!("Fix {rule}"),
                rule: rule.clone(),
                offset: fix.offset,
                length: fix.length,
                replacement: fix.replacement.clone(),
            });
        }
    }
    fixes
}

fn translate_diagnostics(
    diagnostics: Vec<EngineDiagnostic>,
    projection: Option<&MappedProjection>,
) -> TranslatedDiagnostics {
    let Some(projection) = projection else {
        return TranslatedDiagnostics { diagnostics, ..TranslatedDiagnostics::default() };
    };
    let mut translated = TranslatedDiagnostics::default();
    for mut diagnostic in diagnostics {
        if diagnostic.labels.is_empty() {
            translated.suppressed += 1;
            translated.rejected_fixes += u32::try_from(diagnostic.fixes.len()).unwrap_or(u32::MAX);
            continue;
        }
        let mut labels = Vec::with_capacity(diagnostic.labels.len());
        let mut labels_are_authored = true;
        for mut label in diagnostic.labels {
            let range = label.offset..label.offset.saturating_add(label.length);
            let Some(mapped) = projection.map_range(range) else {
                labels_are_authored = false;
                break;
            };
            label.offset = mapped.start;
            label.length = mapped.end - mapped.start;
            labels.push(label);
        }
        if !labels_are_authored {
            translated.suppressed += 1;
            translated.rejected_fixes += u32::try_from(diagnostic.fixes.len()).unwrap_or(u32::MAX);
            continue;
        }
        diagnostic.labels = labels;
        if diagnostic.rule.as_deref() == Some("require-yield")
            && diagnostic.labels.iter().any(|label| {
                projection.is_synthetic_generator_range(
                    label.offset..label.offset.saturating_add(label.length),
                )
            })
        {
            translated.suppressed = translated.suppressed.saturating_add(1);
            translated.rejected_fixes = translated
                .rejected_fixes
                .saturating_add(u32::try_from(diagnostic.fixes.len()).unwrap_or(u32::MAX));
            continue;
        }
        diagnostic.fixes = diagnostic
            .fixes
            .into_iter()
            .filter_map(|mut fix| {
                let range = fix.offset..fix.offset.saturating_add(fix.length);
                let Some(mapped) = projection.map_fix_range(range) else {
                    translated.rejected_fixes += 1;
                    return None;
                };
                fix.offset = mapped.start;
                fix.length = mapped.end - mapped.start;
                Some(fix)
            })
            .collect();
        translated.diagnostics.push(diagnostic);
    }
    translated
}

fn translate_type_diagnostics(
    diagnostics: Vec<EngineDiagnostic>,
    projection: Option<&TypeProjection>,
) -> TranslatedDiagnostics {
    let Some(projection) = projection else {
        return TranslatedDiagnostics { diagnostics, ..TranslatedDiagnostics::default() };
    };
    let mut translated = TranslatedDiagnostics::default();
    for mut diagnostic in diagnostics {
        if diagnostic.labels.is_empty() {
            translated.suppressed = translated.suppressed.saturating_add(1);
            translated.rejected_fixes = translated
                .rejected_fixes
                .saturating_add(u32::try_from(diagnostic.fixes.len()).unwrap_or(u32::MAX));
            continue;
        }
        let mut labels = Vec::with_capacity(diagnostic.labels.len());
        for mut label in diagnostic.labels {
            let range = label.offset..label.offset.saturating_add(label.length);
            let Some(mapped) = projection.map_range(range) else {
                labels.clear();
                break;
            };
            label.offset = mapped.start;
            label.length = mapped.end - mapped.start;
            labels.push(label);
        }
        if labels.is_empty() {
            translated.suppressed = translated.suppressed.saturating_add(1);
            translated.rejected_fixes = translated
                .rejected_fixes
                .saturating_add(u32::try_from(diagnostic.fixes.len()).unwrap_or(u32::MAX));
            continue;
        }
        diagnostic.labels = labels;
        diagnostic.fixes = diagnostic
            .fixes
            .into_iter()
            .filter_map(|mut fix| {
                let range = fix.offset..fix.offset.saturating_add(fix.length);
                let Some(mapped) = projection.map_fix_range(range) else {
                    translated.rejected_fixes = translated.rejected_fixes.saturating_add(1);
                    return None;
                };
                fix.offset = mapped.start;
                fix.length = mapped.end - mapped.start;
                Some(fix)
            })
            .collect();
        translated.diagnostics.push(diagnostic);
    }
    translated
}

fn map_diagnostics(
    path: &Path,
    diagnostics: Vec<EngineDiagnostic>,
    applied_rules: &[String],
) -> Vec<DiagnosticOutput> {
    let filename = path.to_string_lossy().into_owned();
    diagnostics
        .into_iter()
        .filter(|diagnostic| {
            !diagnostic
                .rule
                .as_ref()
                .is_some_and(|rule| applied_rules.iter().any(|applied| applied == rule))
        })
        .map(|diagnostic| DiagnosticOutput {
            filename: filename.clone(),
            rule: diagnostic.rule.clone().unwrap_or_else(|| "parse-error".to_string()),
            code: diagnostic_code(&diagnostic),
            severity: diagnostic.severity,
            message: diagnostic.message,
            labels: diagnostic
                .labels
                .into_iter()
                .map(|label| LabelOutput {
                    span: SpanOutput { offset: label.offset, length: label.length },
                    message: label.message,
                })
                .collect(),
        })
        .collect()
}

fn diagnostic_code(diagnostic: &EngineDiagnostic) -> String {
    if !diagnostic.code.is_empty() {
        return diagnostic.code.clone();
    }
    match (&diagnostic.plugin, &diagnostic.rule) {
        (Some(plugin), Some(rule)) => format!("{plugin}({rule})"),
        _ => "oxc".to_string(),
    }
}

fn validate_fixed(
    session: &LintSession,
    path: &Path,
    fixed: &str,
    is_tsrx: bool,
    source_kind: SourceKind,
) -> Result<(), String> {
    let projection = if is_tsrx {
        let overlay = scan(fixed).map_err(|error| error.to_string())?;
        Some(project_for_lint(fixed, &overlay).map_err(|error| error.to_string())?)
    } else {
        None
    };
    let parse_source = projection.as_ref().map_or(fixed, MappedProjection::source);
    session.engine.lint(&LintRequest {
        path,
        original_source: fixed,
        parse_source,
        source_kind,
        rules: &[],
        collect_fixes: session.fix,
        dynamic_tags: projection.as_ref().and_then(|projection| {
            projection.dynamic_contract().map(|(prefix, count, original_offsets)| {
                DynamicTagContract { prefix, count, original_offsets }
            })
        }),
    })?;
    Ok(())
}

fn source_kind(path: &Path) -> Result<SourceKind, String> {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("js" | "mjs" | "cjs") => Ok(SourceKind::JavaScript),
        Some("jsx") => Ok(SourceKind::JavaScriptReact),
        Some("ts" | "mts" | "cts") => Ok(SourceKind::TypeScript),
        Some("tsx") => Ok(SourceKind::TypeScriptReact),
        extension => Err(format!("Unsupported source extension: {extension:?}")),
    }
}

fn elapsed_ns(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use oxc_adapter::{RuleFilter, RuleSeverity};
    use tsrx_syntax::{project_for_lint, scan};

    use super::LintSession;

    #[test]
    fn fix_mapping_is_identity_only() {
        let source = "function View() @{ var value = 1; }";
        let overlay = scan(source).unwrap();
        let projection = project_for_lint(source, &overlay).unwrap();
        let projected_var = u32::try_from(projection.source().find("var").unwrap()).unwrap();
        let original_var = u32::try_from(source.find("var").unwrap()).unwrap();
        assert_eq!(
            projection.map_range(projected_var..projected_var + 3),
            Some(original_var..original_var + 3)
        );
        let marker = u32::try_from(projection.source().find("/*").unwrap()).unwrap();
        assert!(projection.map_range(marker..marker + 1).is_none());
    }

    #[test]
    fn editor_actions_are_identity_mapped_validated_and_do_not_write() {
        let source = "function View() @{ var value = 1; <p>{value}</p>; }";
        let path = Path::new("editor-action.tsrx");
        let session = LintSession::new(
            Path::new("."),
            None,
            &[RuleFilter { severity: RuleSeverity::Deny, name: "no-var".to_string() }],
            true,
        )
        .unwrap();
        let actions = session.code_actions(path, source).unwrap();
        assert_eq!(actions.len(), 1);
        let action = &actions[0];
        assert_eq!(action.rule, "no-var");
        let mut fixed = source.to_string();
        fixed.replace_range(
            action.offset as usize..(action.offset + action.length) as usize,
            &action.replacement,
        );
        assert!(!fixed.contains("var value"));
        assert!(fixed.contains("let value") || fixed.contains("const value"));
        assert!(!path.exists());
    }

    #[test]
    fn in_memory_config_applies_without_a_config_file() {
        let source = "export function View() @{ console.log('browser'); <p>ok</p>; }";
        let path = Path::new("browser-demo.tsrx");
        let session = LintSession::new_with_config_source(
            Path::new("/demo"),
            Some(r#"{ "rules": { "no-console": "error" } }"#),
            &[],
            false,
        )
        .unwrap();
        let output = session.lint_text(path, source).unwrap();
        assert!(output.diagnostics.iter().any(|diagnostic| diagnostic.rule == "no-console"));
        assert!(!path.exists());
    }

    #[test]
    fn one_unprojectable_file_keeps_the_rest_of_the_batch_reporting() {
        let directory = std::env::temp_dir().join("oxc-tsrx-lint-batch-continues");
        std::fs::create_dir_all(&directory).expect("temp directory");
        let good_source = "export function Good() @{\n  var legacy = 1;\n  <div>hi</div>\n}\n";
        let broken_source =
            "export function Broken() @{\n  let x = 1;\n  <main>\n    <h1>hi</h1>\n}\n";
        let good = directory.join("Good.tsrx");
        let broken = directory.join("Broken.tsrx");
        std::fs::write(&good, good_source).expect("write");
        std::fs::write(&broken, broken_source).expect("write");

        let session = LintSession::new(
            &directory,
            None,
            // A warning, so the one error in the aggregate below can only be the syntax error.
            &[RuleFilter { severity: RuleSeverity::Warn, name: "no-var".to_string() }],
            false,
        )
        .unwrap();
        let outputs = session
            .lint_files(&[good.clone(), broken.clone()])
            .expect("an unprojectable file must not fail the batch");

        assert_eq!(outputs.len(), 2);
        // The good file still reports, which is the whole point: before this, the first failing
        // file short-circuited the collect and discarded every other file's diagnostics.
        assert!(
            outputs[0]
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.rule == "no-var" && diagnostic.severity == "warning"),
            "{:?}",
            outputs[0].diagnostics
        );

        let failure = &outputs[1].diagnostics;
        assert_eq!(failure.len(), 1);
        assert_eq!(failure[0].filename, broken.to_string_lossy());
        assert_eq!(failure[0].severity, "error");
        assert!(failure[0].message.contains("unterminated"), "{failure:?}");
        // No rule and no code: there is nothing to disable, and the default renderer omits the
        // code slot for a diagnostic that carries none, matching canonical Oxlint's parse errors.
        assert_eq!(failure[0].rule, "");
        assert_eq!(failure[0].code, "");
        assert_eq!(
            failure[0].labels[0].span.offset as usize,
            broken_source.find("<main>").expect("fixture")
        );

        // The aggregate the CLI exits on now counts the syntax error as one error.
        let aggregated = session.aggregate(outputs);
        assert_eq!(
            aggregated
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.severity == "error")
                .count(),
            1
        );
        std::fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn lint_text_still_fails_a_projection_error_for_the_editor() {
        let session = LintSession::new(Path::new("."), None, &[], false).unwrap();
        let error = session
            .lint_text(Path::new("Broken.tsrx"), "export function Broken() @{\n  <main>\n}\n")
            .expect_err("the LSP boundary must keep receiving projection failures as errors");
        assert!(error.contains("unterminated"), "{error}");
    }

    #[test]
    fn in_memory_config_rejects_invalid_json() {
        let error =
            LintSession::new_with_config_source(Path::new("/demo"), Some("{ not-json"), &[], false)
                .err()
                .expect("invalid JSON must fail before linting");
        assert!(!error.is_empty());
    }
}
