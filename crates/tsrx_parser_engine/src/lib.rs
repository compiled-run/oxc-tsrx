//! Canonical authored-TSRX reconstruction over allocator-contained OXC tapes.

mod lexical;
mod projection;
mod reconstruct;
mod results;
mod source_bridge;
mod tape_index;
mod utf16_result;

#[cfg(feature = "stage4-observer")]
use std::mem::size_of;
use std::{error::Error, fmt};

use oxc_adapter::{
    DynamicTagContract,
    parser::{
        AuthoredGrammarFailure, ProjectedParseError, ProjectedParseRequest, ProjectedParseResult,
        RejectionMetadata, RejectionModuleNames, parse_failed_tsrx_metadata,
        parse_to_projected_tape, parse_to_projected_tape_program_only,
        render_diagnostic_codeframes,
    },
};
use reconstruct::{finalize_reachable_spans, reconstruct_projected};
use results::{reconstruct_diagnostics, reconstruct_module};
use source_bridge::PreparedSource;
use tsrx_syntax::{
    OpaqueSurrogateContext, Overlay, OverlayView, ProjectionError, ProjectionView,
    project_for_parser, scan_for_parser,
};
use tsrx_tape_schema::{
    CommentTable, Completeness, CoordinateDomain, DiagnosticPhase, DiagnosticSeverity,
    DiagnosticTable, FlatTape, ModuleTable, ParseCompleteness, TapeBuildError, TapeSpan,
};
#[cfg(feature = "stage4-observer")]
use tsrx_tape_schema::{
    FieldRecord, ListRecord, ListValueRecord, ObjectRecord, RecordIndex, ValueRef,
};
#[cfg(test)]
use utf16_result::Utf16Work;
use utf16_result::{
    NoopUtf16WorkObserver, Utf16WorkObserver, finalize_utf16_result, forbidden_module_name_span,
    forbidden_rejection_module_name_span,
};

#[derive(Debug, Clone, Copy)]
struct Utf16Rejection {
    span: TapeSpan,
    message: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub struct TsrxParseRequest<'a> {
    pub source: &'a str,
}

/// Binding-neutral request over the exact JavaScript UTF-16 source units.
///
/// Unlike Rust `str`, this can represent the unpaired surrogate units accepted by JavaScript
/// strings in opaque lexical contexts. The returned result never borrows this storage.
#[derive(Debug, Clone, Copy)]
pub struct TsrxUtf16ParseRequest<'a> {
    pub source: &'a [u16],
}

#[derive(Debug, Clone, Copy)]
pub struct TsrxParseOptions<'a> {
    pub filename: &'a str,
    pub source_type: Option<&'a str>,
    pub include_ts_fields: bool,
    pub ranges: bool,
    pub preserve_parens: Option<bool>,
    pub show_semantic_errors: bool,
}

impl Default for TsrxParseOptions<'static> {
    fn default() -> Self {
        Self {
            filename: "input.tsrx",
            source_type: None,
            include_ts_fields: false,
            ranges: false,
            preserve_parens: None,
            show_semantic_errors: false,
        }
    }
}

/// Route-owned work observed by the nonshipping Stage 4 qualification sibling.
///
/// The feature that exposes this type is disabled in production. Each field is accumulated at the
/// operation that owns the work; the Node-API sibling only publishes the resulting totals.
#[cfg(feature = "stage4-observer")]
#[doc(hidden)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Stage4WorkCounters {
    pub scans: usize,
    pub copied_bytes: usize,
    pub projection_bytes: usize,
    pub map_bytes: usize,
    pub surrogate_bytes: usize,
    pub tape_bytes: usize,
}

#[cfg(feature = "stage4-observer")]
impl Utf16WorkObserver for Stage4WorkCounters {
    fn record_scan(&mut self) {
        self.scans = self.scans.saturating_add(1);
    }

    fn record_bridge(&mut self, source: &PreparedSource<'_>) {
        let work = source.work();
        self.copied_bytes = self.copied_bytes.saturating_add(work.utf8_bytes);
        self.map_bytes = self.map_bytes.saturating_add(work.boundary_bytes);
        self.surrogate_bytes = self.surrogate_bytes.saturating_add(work.fixup_bytes);
    }

    fn record_projection(&mut self, projected_bytes: usize, map_bytes: usize) {
        self.projection_bytes = self.projection_bytes.saturating_add(projected_bytes);
        self.map_bytes = self.map_bytes.saturating_add(map_bytes);
    }

    fn record_tape(&mut self, tape: &FlatTape) {
        self.tape_bytes = self.tape_bytes.saturating_add(logical_tape_bytes(tape));
    }

    fn record_copy(&mut self, _lane: utf16_result::RepairCopyLane, utf16_units: usize) {
        self.copied_bytes =
            self.copied_bytes.saturating_add(utf16_units.saturating_mul(size_of::<u16>()));
    }
}

#[cfg(feature = "stage4-observer")]
fn logical_tape_bytes(tape: &FlatTape) -> usize {
    let key_bytes = (0..tape.object_count()).fold(0_usize, |total, index| {
        let Ok(index) = u32::try_from(index) else {
            return usize::MAX;
        };
        tape.fields(RecordIndex::new(index))
            .fold(total, |total, field| total.saturating_add(tape.key(field).len()))
    });
    size_of::<u16>()
        .saturating_add(size_of::<ValueRef>())
        .saturating_add(tape.object_count().saturating_mul(size_of::<ObjectRecord>()))
        .saturating_add(tape.field_count().saturating_mul(size_of::<FieldRecord>()))
        .saturating_add(tape.list_count().saturating_mul(size_of::<ListRecord>()))
        .saturating_add(tape.list_value_count().saturating_mul(size_of::<ListValueRecord>()))
        .saturating_add(key_bytes)
        .saturating_add(tape.scalar_storage().len())
}

#[derive(Debug)]
pub struct TsrxParseResult {
    pub status: ParseCompleteness,
    pub coordinate_domain: CoordinateDomain,
    pub completeness: Completeness,
    pub program: Option<FlatTape>,
    pub module: Option<ModuleTable>,
    pub comments: CommentTable,
    pub errors: DiagnosticTable,
    pub suppressed_diagnostics: u32,
    needs_compaction: bool,
    rejection_module_names: RejectionModuleNames,
}

impl TsrxParseResult {
    /// Returns the complete Program for production callers that already require parse success.
    ///
    /// # Panics
    ///
    /// Panics when called on a failed or future recovered result without a Program.
    #[must_use]
    pub fn program(&self) -> &FlatTape {
        self.program.as_ref().expect("complete TSRX result must contain a Program")
    }

    fn complete(
        program: FlatTape,
        module: Option<ModuleTable>,
        comments: CommentTable,
        errors: DiagnosticTable,
        suppressed_diagnostics: u32,
        needs_compaction: bool,
        rejection_module_names: RejectionModuleNames,
    ) -> Self {
        let mut completeness = Completeness::COMPLETE.with(Completeness::HAS_PROGRAM);
        if module.is_some() {
            completeness = completeness.with(Completeness::HAS_MODULE);
        }
        if !comments.is_empty() {
            completeness = completeness.with(Completeness::HAS_COMMENTS);
        }
        if !errors.is_empty() {
            completeness = completeness.with(Completeness::HAS_ERRORS);
        }
        Self {
            status: ParseCompleteness::Complete,
            coordinate_domain: CoordinateDomain::AuthoredUtf8Bytes,
            completeness,
            program: Some(program),
            module,
            comments,
            errors,
            suppressed_diagnostics,
            needs_compaction,
            rejection_module_names,
        }
    }

    fn failed(
        comments: CommentTable,
        errors: DiagnosticTable,
        suppressed_diagnostics: u32,
    ) -> Result<Self, TsrxParseError> {
        if errors.is_empty() {
            return Err(TsrxParseError::Unsupported(
                "failed TSRX result has no authored diagnostic",
            ));
        }
        let mut completeness = Completeness::HAS_ERRORS;
        if !comments.is_empty() {
            completeness = completeness.with(Completeness::HAS_COMMENTS);
        }
        Ok(Self {
            status: ParseCompleteness::Failed,
            coordinate_domain: CoordinateDomain::AuthoredUtf8Bytes,
            completeness,
            program: None,
            module: None,
            comments,
            errors,
            suppressed_diagnostics,
            needs_compaction: false,
            rejection_module_names: RejectionModuleNames::default(),
        })
    }

    fn failed_with_rejection_module_names(
        comments: CommentTable,
        errors: DiagnosticTable,
        suppressed_diagnostics: u32,
        rejection_module_names: RejectionModuleNames,
    ) -> Result<Self, TsrxParseError> {
        let mut result = Self::failed(comments, errors, suppressed_diagnostics)?;
        result.rejection_module_names = rejection_module_names;
        Ok(result)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum TsrxParseError {
    AuthoredGrammar(String),
    Unsupported(&'static str),
    ResourceExhausted(&'static str),
    Projection(String),
    Adapter(String),
    Tape(TapeBuildError),
}

impl fmt::Display for TsrxParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AuthoredGrammar(message) => write!(formatter, "invalid TSRX grammar: {message}"),
            Self::Unsupported(shape) => write!(formatter, "unsupported TSRX parser shape: {shape}"),
            Self::ResourceExhausted(message) => formatter.write_str(message),
            Self::Projection(error) => write!(formatter, "TSRX projection failed: {error}"),
            Self::Adapter(error) => write!(formatter, "OXC adapter failed: {error}"),
            Self::Tape(error) => error.fmt(formatter),
        }
    }
}

impl Error for TsrxParseError {}

impl TsrxParseError {
    #[must_use]
    pub const fn is_resource_exhausted(&self) -> bool {
        matches!(self, Self::ResourceExhausted(_) | Self::Tape(TapeBuildError::CapacityOverflow))
    }
}

impl From<TapeBuildError> for TsrxParseError {
    fn from(error: TapeBuildError) -> Self {
        match error {
            TapeBuildError::CapacityOverflow => {
                Self::ResourceExhausted("TSRX tape exceeds its 32-bit limit")
            }
            TapeBuildError::InvalidRecordIndex => Self::Tape(error),
        }
    }
}

impl From<ProjectedParseError> for TsrxParseError {
    fn from(error: ProjectedParseError) -> Self {
        match error {
            ProjectedParseError::Tape(error) => error.into(),
            ProjectedParseError::Invariant(message) => {
                Self::Adapter(format!("projected OXC invariant failed: {message}"))
            }
        }
    }
}

impl From<ProjectionError> for TsrxParseError {
    fn from(error: ProjectionError) -> Self {
        match error {
            ProjectionError::SourceTooLarge => {
                Self::ResourceExhausted("TSRX source exceeds the 4 GiB span limit")
            }
            ProjectionError::MarkerSpaceExhausted => {
                Self::ResourceExhausted("TSRX marker namespace is exhausted")
            }
            error => Self::Projection(error.to_string()),
        }
    }
}

/// Parses the implemented canonical TSRX grammar through one pinned public-OXC arena parse.
///
/// Unsupported syntax fails closed. Broader authored constructs are added as independently
/// verified reconstruction slices without changing the ordinary JS/TS/TSX OXC route.
///
/// # Errors
///
/// Malformed, unsupported, or OXC-rejected authored grammar is returned as a structured
/// [`ParseCompleteness::Failed`] result with null `program` and `module` records. Returns
/// [`TsrxParseError`] only for operational failures such as unsupported coordinate domains,
/// capacity exhaustion, or an internal projection/reconstruction invariant.
pub fn parse_tsrx(request: &TsrxParseRequest<'_>) -> Result<TsrxParseResult, TsrxParseError> {
    parse_tsrx_with_options(request, TsrxParseOptions::default())
}

/// Parses canonical TSRX with the bounded options needed by the eventual OXC-compatible binding.
///
/// # Errors
///
/// Malformed, unsupported, or OXC-rejected authored grammar is returned as a structured
/// [`ParseCompleteness::Failed`] result with null `program` and `module` records. Requested
/// semantic diagnostics are likewise returned as structured result data. Returns an error only
/// for operational failures such as unsupported coordinate domains, capacity exhaustion, or an
/// internal projection/reconstruction invariant.
pub fn parse_tsrx_with_options(
    request: &TsrxParseRequest<'_>,
    options: TsrxParseOptions<'_>,
) -> Result<TsrxParseResult, TsrxParseError> {
    let source = request.source;
    if !source.is_ascii() {
        return Err(TsrxParseError::Unsupported("non-ASCII source"));
    }

    let mut observer = NoopUtf16WorkObserver;
    parse_tsrx_utf8_source(source, options, false, false, true, &mut observer)
}

/// Parses ASCII TSRX for a consumer that serializes only the reachable Program tree.
#[doc(hidden)]
pub fn parse_tsrx_with_options_for_transfer(
    request: &TsrxParseRequest<'_>,
    options: TsrxParseOptions<'_>,
) -> Result<TsrxParseResult, TsrxParseError> {
    if !request.source.is_ascii() {
        return Err(TsrxParseError::Unsupported("non-ASCII source"));
    }
    let mut observer = NoopUtf16WorkObserver;
    parse_tsrx_utf8_source(request.source, options, true, false, true, &mut observer)
}

/// Parses ASCII TSRX for the private Program-and-diagnostics compatibility transport.
#[doc(hidden)]
pub fn parse_tsrx_with_options_for_compat_transfer(
    request: &TsrxParseRequest<'_>,
    options: TsrxParseOptions<'_>,
) -> Result<TsrxParseResult, TsrxParseError> {
    if !request.source.is_ascii() {
        return Err(TsrxParseError::Unsupported("non-ASCII source"));
    }
    let mut observer = NoopUtf16WorkObserver;
    parse_tsrx_utf8_source(request.source, options, true, false, false, &mut observer)
}

/// Parses ASCII canonical TSRX while returning real route-owned Stage 4 work totals.
#[cfg(feature = "stage4-observer")]
#[doc(hidden)]
pub fn parse_tsrx_with_options_observed(
    request: &TsrxParseRequest<'_>,
    options: TsrxParseOptions<'_>,
) -> Result<(TsrxParseResult, Stage4WorkCounters), TsrxParseError> {
    if !request.source.is_ascii() {
        return Err(TsrxParseError::Unsupported("non-ASCII source"));
    }
    let mut work = Stage4WorkCounters::default();
    let result = parse_tsrx_utf8_source(request.source, options, false, false, true, &mut work)?;
    Ok((result, work))
}

/// Parses ASCII TSRX for reachable-tree transfer while returning Stage 4 work totals.
#[cfg(feature = "stage4-observer")]
#[doc(hidden)]
pub fn parse_tsrx_with_options_for_transfer_observed(
    request: &TsrxParseRequest<'_>,
    options: TsrxParseOptions<'_>,
) -> Result<(TsrxParseResult, Stage4WorkCounters), TsrxParseError> {
    if !request.source.is_ascii() {
        return Err(TsrxParseError::Unsupported("non-ASCII source"));
    }
    let mut work = Stage4WorkCounters::default();
    let result = parse_tsrx_utf8_source(request.source, options, true, false, true, &mut work)?;
    Ok((result, work))
}

/// Parses ASCII TSRX for the private compatibility transport while returning Stage 4 totals.
#[cfg(feature = "stage4-observer")]
#[doc(hidden)]
pub fn parse_tsrx_with_options_for_compat_transfer_observed(
    request: &TsrxParseRequest<'_>,
    options: TsrxParseOptions<'_>,
) -> Result<(TsrxParseResult, Stage4WorkCounters), TsrxParseError> {
    if !request.source.is_ascii() {
        return Err(TsrxParseError::Unsupported("non-ASCII source"));
    }
    let mut work = Stage4WorkCounters::default();
    let result = parse_tsrx_utf8_source(request.source, options, true, false, false, &mut work)?;
    Ok((result, work))
}

/// Parses exact JavaScript UTF-16 source units through the same single public-OXC parse.
///
/// # Errors
///
/// Returns an operational error only when the lossless bridge or existing canonical parser
/// invariants fail. Authored syntax failures remain structured failed parse results.
pub fn parse_tsrx_utf16(
    request: &TsrxUtf16ParseRequest<'_>,
) -> Result<TsrxParseResult, TsrxParseError> {
    parse_tsrx_utf16_with_options(request, TsrxParseOptions::default())
}

/// Parses exact JavaScript UTF-16 source units with canonical parser options.
///
/// # Errors
///
/// Returns an operational error only when the lossless bridge or existing canonical parser
/// invariants fail. Authored syntax failures remain structured failed parse results.
pub fn parse_tsrx_utf16_with_options(
    request: &TsrxUtf16ParseRequest<'_>,
    options: TsrxParseOptions<'_>,
) -> Result<TsrxParseResult, TsrxParseError> {
    let mut observer = NoopUtf16WorkObserver;
    parse_tsrx_utf16_with_options_and_observer(request, options, false, true, &mut observer)
}

/// Parses exact UTF-16 TSRX for the private Program-and-diagnostics compatibility transport.
#[doc(hidden)]
pub fn parse_tsrx_utf16_with_options_for_compat_transfer(
    request: &TsrxUtf16ParseRequest<'_>,
    options: TsrxParseOptions<'_>,
) -> Result<TsrxParseResult, TsrxParseError> {
    let mut observer = NoopUtf16WorkObserver;
    parse_tsrx_utf16_with_options_and_observer(request, options, true, false, &mut observer)
}

fn parse_tsrx_utf16_with_options_and_observer<W: Utf16WorkObserver>(
    request: &TsrxUtf16ParseRequest<'_>,
    options: TsrxParseOptions<'_>,
    force_defer_compaction: bool,
    retain_module: bool,
    observer: &mut W,
) -> Result<TsrxParseResult, TsrxParseError> {
    let prepared = PreparedSource::new(request.source)?;
    observer.record_bridge(&prepared);
    let retain_rejection_module_names = prepared.has_context(OpaqueSurrogateContext::QuotedString);
    let mut result = parse_tsrx_utf8_source(
        prepared.source(),
        options,
        force_defer_compaction || !prepared.is_identity(),
        retain_rejection_module_names,
        retain_module,
        observer,
    )?;
    if prepared.rejected_fixup().is_some() && result.status == ParseCompleteness::Complete {
        return Err(TsrxParseError::Adapter(
            "active-surrogate poison marker survived a successful OXC parse".to_string(),
        ));
    }
    if let Some(rejection) = utf16_rejection_candidate(&result, &prepared)? {
        if let Some(errors) = earlier_grammar_diagnostic(
            &result.errors,
            rejection.span.start,
            options.filename,
            prepared.source(),
            Some(&prepared),
        )? {
            if result.status != ParseCompleteness::Failed {
                return Err(TsrxParseError::Adapter(
                    "complete parse retained an earlier grammar diagnostic".to_string(),
                ));
            }
            let discarded = result.errors.len().saturating_sub(1);
            result.errors = errors;
            result.suppressed_diagnostics = result
                .suppressed_diagnostics
                .saturating_add(u32::try_from(discarded).unwrap_or(u32::MAX));
        } else {
            result = grammar_result(
                prepared.source(),
                options.filename,
                std::mem::take(&mut result.comments),
                rejection.message,
                Some(rejection.span),
            )?;
        }
    }
    result.rejection_module_names = RejectionModuleNames::default();
    finalize_utf16_result(&mut result, &prepared, observer)?;
    result.coordinate_domain = CoordinateDomain::OriginalUtf16Units;
    Ok(result)
}

/// Parses exact JavaScript UTF-16 while returning real route-owned Stage 4 work totals.
///
/// This entry point exists only in the nonshipping observer build. The production parser uses the
/// monomorphized no-op observer and exposes no observation API.
#[cfg(feature = "stage4-observer")]
#[doc(hidden)]
pub fn parse_tsrx_utf16_with_options_observed(
    request: &TsrxUtf16ParseRequest<'_>,
    options: TsrxParseOptions<'_>,
) -> Result<(TsrxParseResult, Stage4WorkCounters), TsrxParseError> {
    let mut work = Stage4WorkCounters::default();
    let result =
        parse_tsrx_utf16_with_options_and_observer(request, options, false, true, &mut work)?;
    Ok((result, work))
}

/// Parses exact UTF-16 TSRX for the compatibility transport while returning Stage 4 totals.
#[cfg(feature = "stage4-observer")]
#[doc(hidden)]
pub fn parse_tsrx_utf16_with_options_for_compat_transfer_observed(
    request: &TsrxUtf16ParseRequest<'_>,
    options: TsrxParseOptions<'_>,
) -> Result<(TsrxParseResult, Stage4WorkCounters), TsrxParseError> {
    let mut work = Stage4WorkCounters::default();
    let result =
        parse_tsrx_utf16_with_options_and_observer(request, options, true, false, &mut work)?;
    Ok((result, work))
}

#[cfg(test)]
fn parse_tsrx_utf16_with_options_measured(
    request: &TsrxUtf16ParseRequest<'_>,
    options: TsrxParseOptions<'_>,
) -> Result<(TsrxParseResult, Utf16Work), TsrxParseError> {
    let mut work = Utf16Work::default();
    let result =
        parse_tsrx_utf16_with_options_and_observer(request, options, false, true, &mut work)?;
    Ok((result, work))
}

fn utf16_rejection_candidate(
    result: &TsrxParseResult,
    source: &PreparedSource<'_>,
) -> Result<Option<Utf16Rejection>, TsrxParseError> {
    let active = source
        .rejected_fixup()
        .map(|fixup| {
            let end = fixup
                .byte_start
                .checked_add(3)
                .ok_or(TsrxParseError::Unsupported("active-surrogate byte interval overflow"))?;
            Ok::<Utf16Rejection, TsrxParseError>(Utf16Rejection {
                span: TapeSpan::new(fixup.byte_start, end),
                message: "unexpected unpaired UTF-16 surrogate in active syntax",
            })
        })
        .transpose()?;
    let public_module = result
        .module
        .as_ref()
        .map(|module| forbidden_module_name_span(module, source))
        .transpose()?
        .flatten();
    let private_module =
        forbidden_rejection_module_name_span(result.rejection_module_names.spans(), source);
    let module = match (public_module, private_module) {
        (Some(public), Some(private)) => {
            Some(if (private.start, private.end) < (public.start, public.end) {
                private
            } else {
                public
            })
        }
        (Some(span), None) | (None, Some(span)) => Some(span),
        (None, None) => None,
    }
    .map(|span| Utf16Rejection {
        span,
        message: "An export name cannot include a lone surrogate.",
    });
    Ok(match (active, module) {
        (Some(active), Some(module)) => {
            Some(if (module.span.start, module.span.end) < (active.span.start, active.span.end) {
                module
            } else {
                active
            })
        }
        (Some(active), None) => Some(active),
        (None, Some(module)) => Some(module),
        (None, None) => None,
    })
}

fn earlier_grammar_diagnostic(
    table: &DiagnosticTable,
    candidate_start: u32,
    filename: &str,
    source: &str,
    source_bridge: Option<&PreparedSource<'_>>,
) -> Result<Option<DiagnosticTable>, TsrxParseError> {
    let mut selected = None;
    for (index, diagnostic) in table.records().iter().enumerate() {
        if diagnostic.phase != DiagnosticPhase::Grammar {
            continue;
        }
        let labels = table.labels(diagnostic.labels).ok_or_else(|| {
            TsrxParseError::Adapter("grammar diagnostic has an invalid label range".to_string())
        })?;
        let has_primary = labels.iter().any(|label| label.primary);
        let causal =
            labels.iter().filter(|label| !has_primary || label.primary).collect::<Vec<_>>();
        if causal.is_empty() {
            continue;
        }
        let mut causal_start = u32::MAX;
        let mut causal_end = 0_u32;
        let mut wholly_earlier = true;
        for label in causal {
            if label.span.start > label.span.end {
                return Err(TsrxParseError::Adapter(
                    "grammar diagnostic has a reversed causal label".to_string(),
                ));
            }
            causal_start = causal_start.min(label.span.start);
            causal_end = causal_end.max(label.span.end);
            // A poison-caused OXC diagnostic may point at the immediately preceding token.
            // Equality is therefore adjacency, not proof that the failure is independent.
            wholly_earlier &= label.span.end < candidate_start
                || (label.span.end == candidate_start
                    && source_bridge.is_some_and(|bridge| {
                        bridge.is_authored_collision_scalar(label.span.start, label.span.end)
                    }));
        }
        if wholly_earlier
            && selected.is_none_or(|(best_index, best_start, best_end)| {
                (causal_start, causal_end, index) < (best_start, best_end, best_index)
            })
        {
            selected = Some((index, causal_start, causal_end));
        }
    }
    let Some((index, _, _)) = selected else {
        return Ok(None);
    };
    let diagnostic = table.records()[index];
    let labels = table.labels(diagnostic.labels).ok_or_else(|| {
        TsrxParseError::Adapter("selected diagnostic has an invalid label range".to_string())
    })?;
    let mut retained = DiagnosticTable::default();
    let label_start = retained.begin_labels()?;
    for label in labels {
        let message = label
            .message
            .get()
            .map(|range| {
                table.string(range).ok_or_else(|| {
                    TsrxParseError::Adapter(
                        "selected diagnostic label has an invalid message".to_string(),
                    )
                })
            })
            .transpose()?;
        retained.push_labeled(label.span, message, label.primary)?;
    }
    let labels = retained.finish_labels(label_start, diagnostic.labels.length)?;
    let optional = |range: tsrx_tape_schema::OptionalStringRange| {
        range
            .get()
            .map(|range| {
                table.string(range).ok_or_else(|| {
                    TsrxParseError::Adapter(
                        "selected diagnostic has invalid optional text".to_string(),
                    )
                })
            })
            .transpose()
    };
    retained.push_diagnostic(
        diagnostic.phase,
        diagnostic.severity,
        table.string(diagnostic.message).ok_or_else(|| {
            TsrxParseError::Adapter("selected diagnostic has an invalid message".to_string())
        })?,
        labels,
        optional(diagnostic.help)?,
        optional(diagnostic.note)?,
        optional(diagnostic.code_scope)?,
        optional(diagnostic.code_number)?,
        optional(diagnostic.url)?,
        None,
    )?;
    render_diagnostic_codeframes(filename, source, &mut retained).map_err(TsrxParseError::from)?;
    Ok(Some(retained))
}

fn parse_tsrx_utf8_source<W: Utf16WorkObserver>(
    source: &str,
    options: TsrxParseOptions<'_>,
    defer_compaction: bool,
    retain_rejection_module_names: bool,
    retain_module: bool,
    observer: &mut W,
) -> Result<TsrxParseResult, TsrxParseError> {
    observer.record_scan();
    let overlay = match scan_for_parser(source) {
        Ok(overlay) => overlay,
        Err(error) => {
            return projection_grammar_result(
                source,
                options.filename,
                &error,
                retain_rejection_module_names,
            );
        }
    };
    let overlay_view = overlay.view();
    projection::validate_overlay(overlay_view)?;
    if overlay_view.tokens.is_empty()
        && overlay_view.dynamic_tags.is_empty()
        && overlay_view.style_blocks.is_empty()
    {
        return parse_direct(
            source,
            options,
            retain_rejection_module_names,
            retain_module,
            observer,
        );
    }
    parse_projected(
        source,
        options,
        &overlay,
        defer_compaction,
        retain_rejection_module_names,
        retain_module,
        observer,
    )
}

fn parse_direct<W: Utf16WorkObserver>(
    source: &str,
    options: TsrxParseOptions<'_>,
    retain_rejection_module_names: bool,
    retain_module: bool,
    observer: &mut W,
) -> Result<TsrxParseResult, TsrxParseError> {
    let request = ProjectedParseRequest {
        filename: options.filename,
        source,
        source_type: options.source_type,
        include_ts_fields: options.include_ts_fields,
        ranges: options.ranges,
        preserve_parens: options.preserve_parens,
        show_semantic_errors: options.show_semantic_errors,
        rejection_metadata: rejection_metadata(retain_rejection_module_names),
        dynamic_tags: None,
        synthetic_callee_spans: &[],
    };
    let parsed = if retain_module {
        parse_to_projected_tape(request)
    } else {
        parse_to_projected_tape_program_only(request)
    }
    .map_err(TsrxParseError::from)?;
    if let Some(tape) = parsed.program.as_ref() {
        observer.record_tape(tape);
    }
    require_one_oxc_parse(parsed.parse_count)?;
    let mut errors = parsed.errors;
    render_diagnostic_codeframes(options.filename, source, &mut errors)
        .map_err(TsrxParseError::from)?;
    if parsed.syntax_failed {
        return TsrxParseResult::failed_with_rejection_module_names(
            parsed.comments,
            errors,
            0,
            parsed.rejection_module_names,
        );
    }
    let program = parsed.program.ok_or(TsrxParseError::Unsupported("missing direct Program"))?;
    let module = if retain_module {
        Some(parsed.module.ok_or(TsrxParseError::Unsupported("missing direct module record"))?)
    } else {
        None
    };
    Ok(TsrxParseResult::complete(
        program,
        module,
        parsed.comments,
        errors,
        0,
        false,
        parsed.rejection_module_names,
    ))
}

// Keeping the parse, authored-coordinate repair, and fail-closed exits together makes the exact
// one-OXC-parse invariant auditable; splitting this pipeline would obscure its ordered ownership.
#[allow(clippy::too_many_lines)]
fn parse_projected<W: Utf16WorkObserver>(
    source: &str,
    options: TsrxParseOptions<'_>,
    overlay: &Overlay,
    defer_compaction: bool,
    retain_rejection_module_names: bool,
    retain_module: bool,
    observer: &mut W,
) -> Result<TsrxParseResult, TsrxParseError> {
    let overlay_view = overlay.view();
    let projected = project_for_parser(source, overlay).map_err(TsrxParseError::from)?;
    let projection_view = projected.view();
    observer.record_projection(
        projection_view.source.len(),
        std::mem::size_of_val(projection_view.segments),
    );
    if let Some(result) = projected_validation_failure(
        source,
        options.filename,
        projection_view,
        overlay_view,
        retain_rejection_module_names,
    )? {
        return Ok(result);
    }

    let dynamic_contract = projected.dynamic_contract().map(|(prefix, count, original_offsets)| {
        DynamicTagContract { prefix, count, original_offsets }
    });

    let request = ProjectedParseRequest {
        filename: options.filename,
        source: projection_view.source,
        source_type: options.source_type,
        include_ts_fields: options.include_ts_fields,
        ranges: options.ranges,
        preserve_parens: options.preserve_parens,
        show_semantic_errors: options.show_semantic_errors,
        rejection_metadata: rejection_metadata(retain_rejection_module_names),
        dynamic_tags: dynamic_contract,
        synthetic_callee_spans: projected.synthetic_callee_spans(),
    };
    let parsed = if retain_module {
        parse_to_projected_tape(request)
    } else {
        parse_to_projected_tape_program_only(request)
    }
    .map_err(TsrxParseError::from)?;
    if let Some(tape) = parsed.program.as_ref() {
        observer.record_tape(tape);
    }
    let ProjectedParseResult {
        parse_count,
        program,
        module,
        mut rejection_module_names,
        comments: projected_comments,
        errors: projected_errors,
        authored_grammar,
        syntax_failed,
        panicked: _,
    } = parsed;
    require_one_oxc_parse(parse_count)?;
    rejection_module_names.try_map_spans(|span| {
        projection::map_affine_span(projection_view.segments, span).ok_or_else(|| {
            TsrxParseError::Adapter(
                "private rejection module name is outside authored projection".to_string(),
            )
        })
    })?;
    let (prefix, comments) = projection::reconstruct_comments(
        source,
        projection_view.source,
        projection_view.segments,
        projected_comments,
        overlay_view,
        projected.parser_marker_prefix(),
        !syntax_failed,
    )?;
    if let Some(failure) = authored_grammar {
        return adapter_grammar_result(
            source,
            options.filename,
            comments,
            &failure,
            rejection_module_names,
        );
    }
    let (mut errors, suppressed_diagnostics) =
        reconstruct_diagnostics(projected_errors, projection_view.segments)?;
    render_diagnostic_codeframes(options.filename, source, &mut errors)
        .map_err(TsrxParseError::from)?;
    if syntax_failed {
        return TsrxParseResult::failed_with_rejection_module_names(
            comments,
            errors,
            suppressed_diagnostics,
            rejection_module_names,
        );
    }
    let prefix = prefix.ok_or(TsrxParseError::Unsupported("missing marker namespace"))?;
    let tape = program.ok_or(TsrxParseError::Unsupported("missing projected Program"))?;
    let projected_module = if retain_module {
        Some(module.ok_or(TsrxParseError::Unsupported("missing projected module record"))?)
    } else {
        None
    };
    ProjectedCompletion {
        source,
        filename: options.filename,
        overlay: overlay_view,
        projection: projection_view,
        prefix,
        tape,
        projected_module,
        comments,
        errors,
        suppressed_diagnostics,
        defer_compaction,
        rejection_module_names,
    }
    .finish()
}

fn projected_validation_failure(
    source: &str,
    filename: &str,
    projection_view: ProjectionView<'_>,
    overlay_view: OverlayView<'_>,
    retain_rejection_module_names: bool,
) -> Result<Option<TsrxParseResult>, TsrxParseError> {
    match projection::validate_projection(source, projection_view, overlay_view) {
        Ok(()) => Ok(None),
        Err(TsrxParseError::AuthoredGrammar(message)) => {
            let metadata = parse_failed_tsrx_metadata(
                source,
                rejection_metadata(retain_rejection_module_names),
            )
            .map_err(TsrxParseError::from)?;
            require_one_oxc_parse(metadata.parse_count)?;
            grammar_result_with_rejection_module_names(
                source,
                filename,
                metadata.comments,
                &message,
                None,
                metadata.rejection_module_names,
            )
            .map(Some)
        }
        Err(error) => Err(error),
    }
}

struct ProjectedCompletion<'source, 'filename, 'overlay, 'projection> {
    source: &'source str,
    filename: &'filename str,
    overlay: OverlayView<'overlay>,
    projection: ProjectionView<'projection>,
    prefix: &'projection str,
    tape: FlatTape,
    projected_module: Option<ModuleTable>,
    comments: CommentTable,
    errors: DiagnosticTable,
    suppressed_diagnostics: u32,
    defer_compaction: bool,
    rejection_module_names: RejectionModuleNames,
}

impl ProjectedCompletion<'_, '_, '_, '_> {
    fn finish(mut self) -> Result<TsrxParseResult, TsrxParseError> {
        let module = self
            .projected_module
            .map(|projected| reconstruct_module(projected, self.projection.segments))
            .transpose()?
            .map(|(module, _suppressed_module_records)| module);
        let authored_starts = match reconstruct_projected(
            &mut self.tape,
            self.source,
            self.overlay,
            self.projection.segments,
            self.prefix,
        ) {
            Ok(authored_starts) => authored_starts,
            Err(error) => {
                return authored_grammar_result(
                    self.source,
                    self.filename,
                    self.comments,
                    error,
                    self.rejection_module_names,
                );
            }
        };
        let finalization_index = match lexical::validate_authored_contexts(&mut self.tape) {
            Ok(index) => index,
            Err(error) => {
                return authored_grammar_result(
                    self.source,
                    self.filename,
                    self.comments,
                    error,
                    self.rejection_module_names,
                );
            }
        };
        finalize_reachable_spans(
            &mut self.tape,
            self.projection.segments,
            &authored_starts,
            &finalization_index,
        )?;
        if !self.defer_compaction {
            self.tape.compact_reachable()?;
        }
        Ok(TsrxParseResult::complete(
            self.tape,
            module,
            self.comments,
            self.errors,
            self.suppressed_diagnostics,
            self.defer_compaction,
            self.rejection_module_names,
        ))
    }
}

fn adapter_grammar_result(
    source: &str,
    filename: &str,
    comments: CommentTable,
    failure: &AuthoredGrammarFailure,
    rejection_module_names: RejectionModuleNames,
) -> Result<TsrxParseResult, TsrxParseError> {
    let source_len = u32::try_from(source.len()).map_err(|_| {
        TsrxParseError::ResourceExhausted("ASCII source exceeds the 4 GiB span limit")
    })?;
    if failure.offset > source_len {
        return Err(TsrxParseError::Unsupported(
            "authored dynamic-tag diagnostic is outside source",
        ));
    }
    let end = failure.offset.saturating_add(1).min(source_len);
    grammar_result_with_rejection_module_names(
        source,
        filename,
        comments,
        &failure.message,
        Some(TapeSpan::new(failure.offset, end)),
        rejection_module_names,
    )
}

fn projection_grammar_result(
    source: &str,
    filename: &str,
    error: &ProjectionError,
    retain_rejection_module_names: bool,
) -> Result<TsrxParseResult, TsrxParseError> {
    let offset = match error {
        ProjectionError::UnsupportedSyntax { offset, .. }
        | ProjectionError::UnterminatedSyntax { offset, .. }
        | ProjectionError::MalformedSyntax { offset, .. } => *offset,
        other => return Err(TsrxParseError::from(other.clone())),
    };
    let message = error.to_string();
    let source_len = u32::try_from(source.len()).map_err(|_| {
        TsrxParseError::ResourceExhausted("ASCII source exceeds the 4 GiB span limit")
    })?;
    if offset > source_len {
        return Err(TsrxParseError::Unsupported("scanner diagnostic is outside authored source"));
    }
    let end = if offset < source_len { offset + 1 } else { offset };
    let metadata =
        parse_failed_tsrx_metadata(source, rejection_metadata(retain_rejection_module_names))
            .map_err(TsrxParseError::from)?;
    require_one_oxc_parse(metadata.parse_count)?;
    grammar_result_with_rejection_module_names(
        source,
        filename,
        metadata.comments,
        &message,
        Some(TapeSpan::new(offset, end)),
        metadata.rejection_module_names,
    )
}

fn require_one_oxc_parse(parse_count: u32) -> Result<(), TsrxParseError> {
    if parse_count == 1 {
        Ok(())
    } else {
        Err(TsrxParseError::Adapter(format!(
            "TSRX route performed {parse_count} public OXC parses instead of exactly one"
        )))
    }
}

const fn rejection_metadata(retain_module_names: bool) -> RejectionMetadata {
    if retain_module_names { RejectionMetadata::ModuleNames } else { RejectionMetadata::None }
}

fn authored_grammar_result(
    source: &str,
    filename: &str,
    comments: CommentTable,
    error: TsrxParseError,
    rejection_module_names: RejectionModuleNames,
) -> Result<TsrxParseResult, TsrxParseError> {
    match error {
        TsrxParseError::AuthoredGrammar(message) => grammar_result_with_rejection_module_names(
            source,
            filename,
            comments,
            &message,
            None,
            rejection_module_names,
        ),
        error => Err(error),
    }
}

fn grammar_result_with_rejection_module_names(
    source: &str,
    filename: &str,
    comments: CommentTable,
    message: &str,
    span: Option<TapeSpan>,
    rejection_module_names: RejectionModuleNames,
) -> Result<TsrxParseResult, TsrxParseError> {
    let mut result = grammar_result(source, filename, comments, message, span)?;
    result.rejection_module_names = rejection_module_names;
    Ok(result)
}

fn grammar_result(
    source: &str,
    filename: &str,
    comments: CommentTable,
    message: &str,
    span: Option<TapeSpan>,
) -> Result<TsrxParseResult, TsrxParseError> {
    let mut errors = DiagnosticTable::default();
    let labels = match span {
        Some(span) => errors.append_labels([(span, None, true)])?,
        None => errors.append_labels(std::iter::empty())?,
    };
    errors.push_diagnostic(
        DiagnosticPhase::Grammar,
        DiagnosticSeverity::Error,
        message,
        labels,
        None,
        None,
        None,
        None,
        None,
        None,
    )?;
    render_diagnostic_codeframes(filename, source, &mut errors).map_err(TsrxParseError::from)?;
    TsrxParseResult::failed(comments, errors, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operational_capacity_errors_keep_their_resource_classification() {
        let direct = TsrxParseError::from(TapeBuildError::CapacityOverflow);
        assert!(direct.is_resource_exhausted());
        assert_eq!(direct.to_string(), "TSRX tape exceeds its 32-bit limit");

        let projected =
            TsrxParseError::from(ProjectedParseError::Tape(TapeBuildError::CapacityOverflow));
        assert!(projected.is_resource_exhausted());

        let invalid = TsrxParseError::from(TapeBuildError::InvalidRecordIndex);
        assert!(!invalid.is_resource_exhausted());

        let source = TsrxParseError::from(ProjectionError::SourceTooLarge);
        assert!(source.is_resource_exhausted());
    }

    fn grammar_table<const N: usize>(labels: [(TapeSpan, bool); N]) -> DiagnosticTable {
        let mut diagnostics = DiagnosticTable::new();
        let labels = diagnostics
            .append_labels(labels.into_iter().map(|(span, primary)| (span, None, primary)))
            .expect("labels");
        diagnostics
            .push_diagnostic(
                DiagnosticPhase::Grammar,
                DiagnosticSeverity::Error,
                "test grammar diagnostic",
                labels,
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .expect("diagnostic");
        diagnostics
    }

    #[test]
    fn a_secondary_label_before_the_candidate_cannot_override_a_later_primary_cause() {
        let diagnostics =
            grammar_table([(TapeSpan::new(1, 2), false), (TapeSpan::new(7, 8), true)]);

        assert!(
            earlier_grammar_diagnostic(&diagnostics, 5, "input.tsrx", "abcdefghij", None)
                .expect("arbitration")
                .is_none()
        );
    }

    #[test]
    fn a_later_secondary_context_does_not_hide_an_earlier_primary_cause() {
        let diagnostics =
            grammar_table([(TapeSpan::new(1, 2), true), (TapeSpan::new(7, 8), false)]);

        assert!(
            earlier_grammar_diagnostic(&diagnostics, 5, "input.tsrx", "abcdefghij", None)
                .expect("arbitration")
                .is_some()
        );
    }

    #[test]
    fn every_label_is_causal_when_oxc_marks_no_primary_label() {
        let wholly_earlier =
            grammar_table([(TapeSpan::new(1, 2), false), (TapeSpan::new(3, 4), false)]);
        assert!(
            earlier_grammar_diagnostic(&wholly_earlier, 5, "input.tsrx", "abcdefghij", None)
                .expect("earlier arbitration")
                .is_some()
        );

        let crosses_candidate =
            grammar_table([(TapeSpan::new(1, 2), false), (TapeSpan::new(5, 6), false)]);
        assert!(
            earlier_grammar_diagnostic(&crosses_candidate, 5, "input.tsrx", "abcdefghij", None,)
                .expect("crossing arbitration")
                .is_none()
        );
    }

    #[test]
    fn candidate_adjacency_is_allowed_only_for_an_exact_authored_collision_scalar() {
        let mut original = "const x=".encode_utf16().collect::<Vec<_>>();
        original.extend([0xffff, 0xd800]);
        original.extend(";".encode_utf16());
        let prepared = PreparedSource::new(&original).expect("prepared collision source");
        let candidate_start = prepared.rejected_fixup().expect("active rejection").byte_start;

        let exact = grammar_table([(TapeSpan::new(candidate_start - 3, candidate_start), false)]);
        assert!(
            earlier_grammar_diagnostic(
                &exact,
                candidate_start,
                "input.tsrx",
                prepared.source(),
                Some(&prepared),
            )
            .expect("exact adjacency")
            .is_some()
        );

        let overlapping =
            grammar_table([(TapeSpan::new(candidate_start - 4, candidate_start), false)]);
        assert!(
            earlier_grammar_diagnostic(
                &overlapping,
                candidate_start,
                "input.tsrx",
                prepared.source(),
                Some(&prepared),
            )
            .expect("overlapping adjacency")
            .is_none()
        );
    }

    #[test]
    fn public_utf16_results_clear_private_name_spans_and_debug_placeholder_text() {
        let template = r#"export { "a<U>" as x } from "m"; const b<U>=1;"#;
        let mut source = Vec::new();
        let mut remaining = template;
        while let Some(index) = remaining.find("<U>") {
            source.extend(remaining[..index].encode_utf16());
            source.push(0xd800);
            remaining = &remaining[index + 3..];
        }
        source.extend(remaining.encode_utf16());

        let result = parse_tsrx_utf16(&TsrxUtf16ParseRequest { source: &source })
            .expect("structured rejection");
        assert!(result.rejection_module_names.spans().is_empty());
        let debug = format!("{result:?}");
        assert!(!debug.contains('\u{e000}'));
        assert!(!debug.contains('\u{ffff}'));
    }

    #[test]
    fn complete_opaque_utf16_results_compact_unreachable_placeholder_storage() {
        let mut source = "const value=\"".encode_utf16().collect::<Vec<_>>();
        source.push(0xd800);
        source.extend("\";".encode_utf16());

        let result = parse_tsrx_utf16(&TsrxUtf16ParseRequest { source: &source })
            .expect("complete opaque result");
        assert_eq!(result.status, ParseCompleteness::Complete);
        assert!(!result.program().scalar_storage().contains('\u{e000}'));
        let debug = format!("{result:?}").to_ascii_lowercase();
        assert!(!debug.contains("e000"), "private placeholder leaked: {debug}");
    }

    #[test]
    fn measured_utf16_work_is_zero_beyond_the_owned_ascii_bridge() {
        let source = "const value=\"plain ASCII\";".encode_utf16().collect::<Vec<_>>();
        let (result, work) = parse_tsrx_utf16_with_options_measured(
            &TsrxUtf16ParseRequest { source: &source },
            TsrxParseOptions::default(),
        )
        .expect("measured ASCII parse");

        assert_eq!(result.status, ParseCompleteness::Complete);
        assert_eq!(work.bridge_observations, 1);
        assert_eq!(work.bridge.input_units, source.len());
        assert_eq!(work.bridge.utf8_bytes, source.len());
        assert_eq!(work.bridge.boundary_records, 0);
        assert_eq!(work.bridge.fixup_records, 0);
        assert_eq!(work.bridge.opaque_fixup_records, 0);
        assert_eq!(work.bridge.rejection_fixup_records, 0);
        assert_eq!(work.bridge.sanitized_bytes, 0);
        assert_eq!(work.restored_units(), 0);
        assert_eq!(work.restored_bytes(), 0);
        assert_eq!(work.program_compactions, 0);
    }

    #[test]
    fn measured_well_formed_utf16_never_enters_a_repair_lane() {
        let source = "const value=\"é😀\";".encode_utf16().collect::<Vec<_>>();
        let (result, work) = parse_tsrx_utf16_with_options_measured(
            &TsrxUtf16ParseRequest { source: &source },
            TsrxParseOptions::default(),
        )
        .expect("measured well-formed parse");

        assert_eq!(result.status, ParseCompleteness::Complete);
        assert_eq!(work.bridge_observations, 1);
        assert_eq!(work.bridge.boundary_records, 2);
        assert_eq!(work.bridge.fixup_records, 0);
        assert_eq!(work.bridge.opaque_fixup_records, 0);
        assert_eq!(work.bridge.rejection_fixup_records, 0);
        assert_eq!(work.bridge.sanitized_bytes, 0);
        assert_eq!(work.restored_units(), 0);
        assert_eq!(work.program_compactions, 0);
    }

    #[test]
    fn measured_active_rejection_counts_poison_without_value_repair() {
        let mut source = "const value=".encode_utf16().collect::<Vec<_>>();
        source.push(0xd800);
        source.extend(";".encode_utf16());
        let (result, work) = parse_tsrx_utf16_with_options_measured(
            &TsrxUtf16ParseRequest { source: &source },
            TsrxParseOptions::default(),
        )
        .expect("measured active rejection");

        assert_eq!(result.status, ParseCompleteness::Failed);
        assert_eq!(work.bridge_observations, 1);
        assert_eq!(work.bridge.fixup_records, 1);
        assert_eq!(work.bridge.opaque_fixup_records, 0);
        assert_eq!(work.bridge.rejection_fixup_records, 1);
        assert_eq!(work.bridge.sanitized_bytes, 3);
        assert_eq!(work.program_raw_units, 0);
        assert_eq!(work.program_semantic_units, 0);
        assert_eq!(work.module_units, 0);
        assert_eq!(work.comment_units, 0);
        assert!(work.codeframe_units > 0);
        assert_eq!(work.program_compactions, 0);
    }

    #[test]
    fn measured_utf16_work_accounts_for_every_current_repair_emission_lane() {
        let template = concat!(
            "import \"m<U>\";\n",
            "const string=\"s<U>\";\n",
            "const template=`t<U>`;\n",
            "/* c<U> */\n",
            "let duplicate;\n",
            "let duplicate;\n",
        );
        let mut source = Vec::new();
        let mut remaining = template;
        while let Some(index) = remaining.find("<U>") {
            source.extend(remaining[..index].encode_utf16());
            source.push(0xd800);
            remaining = &remaining[index + 3..];
        }
        source.extend(remaining.encode_utf16());

        let (result, work) = parse_tsrx_utf16_with_options_measured(
            &TsrxUtf16ParseRequest { source: &source },
            TsrxParseOptions {
                filename: "Work.tsrx",
                show_semantic_errors: true,
                ..TsrxParseOptions::default()
            },
        )
        .expect("measured all-lane parse");

        assert_eq!(result.status, ParseCompleteness::Complete);
        assert!(!result.errors.is_empty());
        assert_eq!(work.bridge_observations, 1);
        assert_eq!(work.bridge.input_units, source.len());
        assert_eq!(work.bridge.utf8_bytes, source.len() + 8);
        assert_eq!(work.bridge.boundary_records, 4);
        assert_eq!(work.bridge.fixup_records, 4);
        assert_eq!(work.bridge.opaque_fixup_records, 4);
        assert_eq!(work.bridge.rejection_fixup_records, 0);
        assert_eq!(work.bridge.sanitized_bytes, 12);
        assert_eq!(work.program_raw_units, 10);
        assert_eq!(work.program_semantic_units, 6);
        assert_eq!(work.module_units, 2);
        assert_eq!(work.comment_units, 4);
        let expected_codeframe_units = result
            .errors
            .records()
            .iter()
            .filter_map(|diagnostic| result.errors.optional_text(diagnostic.codeframe))
            .map(|codeframe| codeframe.to_utf16().len())
            .sum::<usize>();
        assert!(expected_codeframe_units > 0);
        assert_eq!(work.codeframe_units, expected_codeframe_units);
        assert_eq!(work.restored_units(), 10 + 6 + 2 + 4 + expected_codeframe_units);
        assert_eq!(work.restored_bytes(), work.restored_units() * 2);
        assert_eq!(work.program_compactions, 1);
    }

    fn dense_measured_source(records: usize) -> Vec<u16> {
        let mut source = Vec::new();
        for index in 0..records {
            let record = format!(
                "import \"m{index}<U>\"; const s{index}=\"s<U>\"; const t{index}=`t<U>`; /* c<U> */ let duplicate;\n"
            );
            let mut remaining = record.as_str();
            while let Some(marker) = remaining.find("<U>") {
                source.extend(remaining[..marker].encode_utf16());
                source.push(0xd800);
                remaining = &remaining[marker + 3..];
            }
            source.extend(remaining.encode_utf16());
        }
        source
    }

    fn assert_copy_work_scales_linearly(label: &str, counts: &[usize], units: &[usize]) {
        assert_eq!(counts.len(), units.len(), "{label} sample shape");
        for pair in units.windows(2) {
            assert!(
                pair[1] <= pair[0].saturating_mul(3),
                "{label} doubled superlinearly: {pair:?}"
            );
        }
        let first = units[0].saturating_mul(*counts.last().expect("last count"));
        let last = units.last().copied().expect("last units").saturating_mul(counts[0]);
        assert!(
            last <= first.saturating_mul(2),
            "{label} per-record copy work grew beyond 2x across the retained 8x range"
        );
    }

    #[cfg(debug_assertions)]
    const fn require_release_copy_campaign() {
        panic!("the retained copy campaign must run with --release");
    }

    #[cfg(not(debug_assertions))]
    const fn require_release_copy_campaign() {}

    #[test]
    #[ignore = "run explicitly in release mode for retained repair-copy evidence"]
    fn release_repair_copy_campaign_is_linear_and_lane_complete() {
        require_release_copy_campaign();
        let counts = [16_usize, 32, 64, 128];
        let mut program_raw = Vec::new();
        let mut program_semantic = Vec::new();
        let mut module = Vec::new();
        let mut comment = Vec::new();
        let mut codeframe = Vec::new();
        let mut total = Vec::new();

        for count in counts {
            let source = dense_measured_source(count);
            let (result, work) = parse_tsrx_utf16_with_options_measured(
                &TsrxUtf16ParseRequest { source: &source },
                TsrxParseOptions {
                    filename: "CopyScaling.tsrx",
                    show_semantic_errors: true,
                    ..TsrxParseOptions::default()
                },
            )
            .expect("release copy-work parse");
            assert_eq!(result.status, ParseCompleteness::Complete);
            assert_eq!(work.bridge_observations, 1);
            assert_eq!(work.bridge.fixup_records, count * 4);
            assert_eq!(work.bridge.opaque_fixup_records, count * 4);
            assert_eq!(work.bridge.rejection_fixup_records, 0);
            assert_eq!(work.bridge.sanitized_bytes, count * 12);
            assert_eq!(work.comment_units, count * 4);
            assert_eq!(work.program_compactions, 1);
            assert!(work.program_raw_units > 0);
            assert!(work.program_semantic_units > 0);
            assert!(work.module_units > 0);
            let expected_codeframe_units = result
                .errors
                .records()
                .iter()
                .filter_map(|diagnostic| result.errors.optional_text(diagnostic.codeframe))
                .map(|codeframe| codeframe.to_utf16().len())
                .sum::<usize>();
            assert!(expected_codeframe_units > 0);
            assert_eq!(work.codeframe_units, expected_codeframe_units);
            assert_eq!(
                work.restored_units(),
                work.program_raw_units
                    + work.program_semantic_units
                    + work.module_units
                    + work.comment_units
                    + expected_codeframe_units
            );
            assert_eq!(work.restored_bytes(), work.restored_units() * 2);
            println!(
                "copy records={count} units={} utf8_bytes={} boundaries={} fixups={} substituted_bytes={} program_raw_units={} program_semantic_units={} module_units={} comment_units={} codeframe_units={} restored_units={} restored_bytes={}",
                work.bridge.input_units,
                work.bridge.utf8_bytes,
                work.bridge.boundary_records,
                work.bridge.fixup_records,
                work.bridge.sanitized_bytes,
                work.program_raw_units,
                work.program_semantic_units,
                work.module_units,
                work.comment_units,
                work.codeframe_units,
                work.restored_units(),
                work.restored_bytes(),
            );
            program_raw.push(work.program_raw_units);
            program_semantic.push(work.program_semantic_units);
            module.push(work.module_units);
            comment.push(work.comment_units);
            codeframe.push(work.codeframe_units);
            total.push(work.restored_units());
        }

        assert_copy_work_scales_linearly("program raw", &counts, &program_raw);
        assert_copy_work_scales_linearly("program semantic", &counts, &program_semantic);
        assert_copy_work_scales_linearly("module", &counts, &module);
        assert_copy_work_scales_linearly("comment", &counts, &comment);
        assert_copy_work_scales_linearly("codeframe", &counts, &codeframe);
        assert_copy_work_scales_linearly("total restored", &counts, &total);
    }
}
