//! The single entry point, and the order the repair lanes run in: values first, then span
//! mapping, then compaction, so no lane rewrites text a later lane still has to locate.

use tsrx_syntax::OpaqueSurrogateContext;
use tsrx_tape_schema::TapeSpan;

use crate::{TsrxParseError, TsrxParseResult, source_bridge::PreparedSource};

use super::{
    codeframe::repair_codeframes,
    comments::repair_comment_values,
    ledger::FixupLedger,
    module_values::repair_module_values,
    observer::Utf16WorkObserver,
    program_values::repair_program_values,
    reachability::{map_program_spans, program_reachable_objects},
};

pub(crate) fn finalize_utf16_result<W: Utf16WorkObserver>(
    result: &mut TsrxParseResult,
    source: &PreparedSource<'_>,
    observer: &mut W,
) -> Result<(), TsrxParseError> {
    if source.is_identity() {
        return Ok(());
    }
    let reachable_objects = result.program.as_ref().map(program_reachable_objects).transpose()?;
    let mut repaired_program_values = false;
    if !source.fixups().is_empty() {
        let mut ledger = FixupLedger::new(source);
        ledger.claim_rejected()?;
        if source.has_program_value_fixups()
            && let Some(program) = result.program.as_mut()
        {
            repair_program_values(
                program,
                source,
                reachable_objects
                    .as_deref()
                    .ok_or(TsrxParseError::Unsupported("missing Program reachability"))?,
                &mut ledger,
                observer,
            )?;
            repaired_program_values = true;
        }
        if source.has_context(OpaqueSurrogateContext::QuotedString)
            && let Some(module) = result.module.as_mut()
        {
            repair_module_values(module, source, observer)?;
        }
        if source.has_context(OpaqueSurrogateContext::Comment) {
            repair_comment_values(&mut result.comments, source, &mut ledger, observer)?;
        }
        repair_codeframes(&mut result.errors, source, observer)?;
        ledger.finish(result.status)?;
    }

    if let Some(program) = result.program.as_mut() {
        map_program_spans(
            program,
            source,
            reachable_objects
                .as_deref()
                .ok_or(TsrxParseError::Unsupported("missing Program reachability"))?,
        )?;
        if result.needs_compaction || repaired_program_values {
            program.compact_reachable()?;
            observer.record_program_compaction();
            result.needs_compaction = false;
        }
    }
    if let Some(module) = result.module.as_mut() {
        module.try_map_spans(|span| map_span(source, span))?;
    }
    result.comments.try_map_spans(|span| map_span(source, span))?;
    result.errors.try_map_spans(|span| map_span(source, span))?;
    Ok(())
}

fn map_span(source: &PreparedSource<'_>, span: TapeSpan) -> Result<TapeSpan, TsrxParseError> {
    let start = source.map_endpoint(span.start).ok_or_else(|| {
        TsrxParseError::Adapter(format!(
            "source span start {} is not an exact UTF-8 boundary",
            span.start
        ))
    })?;
    let end = source.map_endpoint(span.end).ok_or_else(|| {
        TsrxParseError::Adapter(format!(
            "source span end {} is not an exact UTF-8 boundary",
            span.end
        ))
    })?;
    Ok(TapeSpan::new(start, end))
}
