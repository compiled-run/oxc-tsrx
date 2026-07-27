use oxc_adapter::parser::{
    AuthoredGrammarFailure, RejectionModuleNames, parse_failed_tsrx_metadata,
    render_diagnostic_codeframes,
};
use tsrx_syntax::ProjectionError;
use tsrx_tape_schema::{
    CommentTable, DiagnosticPhase, DiagnosticSeverity, DiagnosticTable, TapeSpan,
};

use crate::{
    TsrxParseError, TsrxParseResult,
    pipeline::{rejection_metadata, require_one_oxc_parse},
};

pub(super) fn adapter_grammar_result(
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

pub(super) fn projection_grammar_result(
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

pub(super) fn authored_grammar_result(
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

pub(super) fn grammar_result_with_rejection_module_names(
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

pub(super) fn grammar_result(
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
