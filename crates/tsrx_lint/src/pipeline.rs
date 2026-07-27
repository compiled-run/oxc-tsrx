use std::{
    ffi::OsString,
    fmt,
    path::{Path, PathBuf},
    time::Instant,
};

use oxc_adapter::{
    DynamicTagContract, EngineDiagnostic, LintRequest, LintResult, OXC_REVISION, SourceKind,
    TypeBatchFile,
};
use tsrx_syntax::{
    MappedProjection, ProjectionError, TypeProjection, project_for_lint, project_for_types, scan,
};

use crate::{
    fixes::{AppliedFixes, apply_safe_fixes},
    report::{
        FileCounts, FixOutput, Metadata, Output, TimingOutput, map_diagnostics,
        projection_failure_output,
    },
    session::LintSession,
    translate::{translate_diagnostics, translate_type_diagnostics},
};

pub(crate) struct PreparedSource {
    pub(crate) projection: Option<MappedProjection>,
    pub(crate) type_projection: Option<TypeProjection>,
    pub(crate) source_kind: SourceKind,
    pub(crate) is_tsrx: bool,
    timings: TimingOutput,
}

pub(crate) struct PendingBatchFile {
    pub(crate) slot: usize,
    pub(crate) path: PathBuf,
    pub(crate) source: String,
    pub(crate) prepared: PreparedSource,
    pub(crate) syntax: LintResult,
}

/// A per-file failure that has not yet been decided to be a diagnostic or a command failure.
///
/// `Projection` is the one kind the CLI lint lane turns into a diagnostic: it is a syntax error in
/// the user's own TSRX file, positioned by [`ProjectionError::byte_offset`]. Everything else is a
/// genuine tool failure. `Display` reproduces each message unchanged, so a caller that maps this
/// straight back to a `String` keeps the exact text it had before.
#[derive(Debug)]
pub(crate) enum PrepareError {
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

/// Lint one loaded file, reporting an unprojectable TSRX source as that file's own diagnostic.
///
/// This is the filesystem CLI boundary. A syntax error in the user's source is their defect, not
/// the linter's, so it is answered with a positioned error diagnostic and the caller's exit code
/// falls out of the usual error count.
pub(crate) fn lint_loaded_file(
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

pub(crate) fn lint_loaded_source(
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

pub(crate) fn run_type_lint(
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

pub(crate) fn run_syntax_lint(
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

#[expect(
    clippy::too_many_arguments,
    reason = "the run's inputs and timings are threaded in explicitly; a parameter struct would relocate these fields, not remove them"
)]
pub(crate) fn finish_lint(
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

pub(crate) fn virtual_type_path(path: &Path) -> PathBuf {
    let mut path = OsString::from(path.as_os_str());
    path.push(".tsx");
    PathBuf::from(path)
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
