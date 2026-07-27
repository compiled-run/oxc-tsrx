//! Authored offsets recorded as nodes are rewritten, and the closing pass that lifts every
//! reachable span out of projected coordinates and back onto the author's source.

use tsrx_syntax::{ByteSpan, ProjectionSegment};
use tsrx_tape_schema::{FlatTape, RecordIndex, ValueRef};

use crate::{
    TsrxParseError,
    lexical::{FinalizationIndex, SpanFields},
    projection::map_endpoint,
};

use super::access::{index_of, scalar_u32};

#[derive(Debug, Clone, Copy)]
pub(crate) struct AuthoredStart {
    pub(super) object: RecordIndex,
    pub(super) start: u32,
    pub(super) end: Option<u32>,
}

pub(super) fn require_authored_object_span(
    tape: &FlatTape,
    object: RecordIndex,
    segments: &[ProjectionSegment],
    authored: tsrx_syntax::ByteSpan,
) -> Result<(), TsrxParseError> {
    let start = scalar_u32(tape, object, "start")?;
    let end = scalar_u32(tape, object, "end")?;
    if map_endpoint(segments, start, true) == Some(authored.start)
        && map_endpoint(segments, end, false) == Some(authored.end)
    {
        Ok(())
    } else {
        Err(TsrxParseError::Unsupported("annotated header value span is synthetic"))
    }
}

pub(super) fn require_object_span_within(
    tape: &FlatTape,
    object: RecordIndex,
    segments: &[ProjectionSegment],
    authored: tsrx_syntax::ByteSpan,
) -> Result<tsrx_syntax::ByteSpan, TsrxParseError> {
    if authored.is_empty() {
        return Err(TsrxParseError::Unsupported("catch binding has no authored header"));
    }
    let start = scalar_u32(tape, object, "start")?;
    let end = scalar_u32(tape, object, "end")?;
    let start = map_endpoint(segments, start, true)
        .ok_or(TsrxParseError::Unsupported("catch binding start is synthetic"))?;
    let end = map_endpoint(segments, end, false)
        .ok_or(TsrxParseError::Unsupported("catch binding end is synthetic"))?;
    if authored.start < start && start < end && end < authored.end {
        Ok(tsrx_syntax::ByteSpan::new(start, end))
    } else {
        Err(TsrxParseError::Unsupported("catch binding lies outside its authored header"))
    }
}

pub(super) fn require_mapped_object_span(
    tape: &FlatTape,
    object: RecordIndex,
    authored: ByteSpan,
    segments: &[ProjectionSegment],
) -> Result<(), TsrxParseError> {
    let start = map_endpoint(segments, scalar_u32(tape, object, "start")?, true);
    let end = map_endpoint(segments, scalar_u32(tape, object, "end")?, false);
    if start != Some(authored.start) || end != Some(authored.end) {
        return Err(TsrxParseError::Unsupported(
            "projected style span differs from authored source",
        ));
    }
    Ok(())
}

pub(super) fn slice_authored(authored: &str, span: ByteSpan) -> Result<&str, TsrxParseError> {
    let start = usize::try_from(span.start)
        .map_err(|_| TsrxParseError::Unsupported("style span exceeds host usize"))?;
    let end = usize::try_from(span.end)
        .map_err(|_| TsrxParseError::Unsupported("style span exceeds host usize"))?;
    authored
        .get(start..end)
        .ok_or(TsrxParseError::Unsupported("style span is not a source boundary"))
}

pub(super) fn record_authored_span(
    starts: &mut Vec<AuthoredStart>,
    object: RecordIndex,
    span: ByteSpan,
) {
    starts.push(AuthoredStart { object, start: span.start, end: Some(span.end) });
}

pub(crate) fn finalize_reachable_spans(
    tape: &mut FlatTape,
    segments: &[ProjectionSegment],
    authored_positions: &[AuthoredStart],
    finalization_index: &FinalizationIndex,
) -> Result<(), TsrxParseError> {
    let mut overrides = vec![None; tape.object_count()];
    for &position in authored_positions {
        let slot = overrides
            .get_mut(index_of(position.object)?)
            .ok_or(TsrxParseError::Unsupported("authored span override outside object table"))?;
        if slot.replace(position).is_some() {
            return Err(TsrxParseError::Unsupported("duplicate authored span override"));
        }
    }
    for (index, mut span_fields) in finalization_index.reachable_span_fields() {
        let authored = overrides[index].take();
        if authored.is_some() && span_fields.start.is_none() {
            let raw = u32::try_from(index).map_err(|_| {
                TsrxParseError::ResourceExhausted("object index exceeds the 32-bit tape limit")
            })?;
            span_fields = object_span_fields(tape, RecordIndex::new(raw));
        }
        finalize_object_span(tape, span_fields, segments, authored)?;
    }
    Ok(())
}

fn finalize_object_span(
    tape: &mut FlatTape,
    fields: SpanFields,
    segments: &[ProjectionSegment],
    authored: Option<AuthoredStart>,
) -> Result<(), TsrxParseError> {
    let start = finalize_span_endpoint(
        tape,
        fields.start,
        segments,
        true,
        authored.map(|position| position.start),
    )?;
    let end = finalize_span_endpoint(
        tape,
        fields.end,
        segments,
        false,
        authored.and_then(|position| position.end),
    )?;
    if let Some(range_field) = fields.range {
        sync_range_values(
            tape,
            range_field,
            start.ok_or(TsrxParseError::Unsupported("ESTree range has no start field"))?,
            end.ok_or(TsrxParseError::Unsupported("ESTree range has no end field"))?,
        )?;
    }
    Ok(())
}

fn finalize_span_endpoint(
    tape: &mut FlatTape,
    field: Option<RecordIndex>,
    segments: &[ProjectionSegment],
    is_start: bool,
    authored: Option<u32>,
) -> Result<Option<ValueRef>, TsrxParseError> {
    let Some(field) = field else {
        if authored.is_some() {
            return Err(TsrxParseError::Unsupported(if is_start {
                "authored node has no start"
            } else {
                "authored node has no end"
            }));
        }
        return Ok(None);
    };
    let authored = if let Some(authored) = authored {
        authored
    } else {
        let projected = tape
            .field_value(field)
            .and_then(|value| tape.scalar_u32(value))
            .ok_or(TsrxParseError::Unsupported("non-numeric ESTree span"))?;
        map_reachable_endpoint(segments, projected, is_start)
            .ok_or(TsrxParseError::Unsupported("reachable synthetic ESTree span"))?
    };
    let value = tape.push_u32_scalar(authored)?;
    tape.set_field_value(field, value)?;
    Ok(Some(value))
}

fn object_span_fields(tape: &FlatTape, object: RecordIndex) -> SpanFields {
    let mut span_fields = SpanFields::default();
    for (field_index, field) in tape.fields_indexed(object) {
        match tape.key(field) {
            "start" => span_fields.start = Some(field_index),
            "end" => span_fields.end = Some(field_index),
            "range" => span_fields.range = Some(field_index),
            _ => {}
        }
    }
    span_fields
}

fn sync_range_values(
    tape: &mut FlatTape,
    range_field: RecordIndex,
    start: ValueRef,
    end: ValueRef,
) -> Result<(), TsrxParseError> {
    let range = tape
        .field_value(range_field)
        .and_then(ValueRef::as_list)
        .ok_or(TsrxParseError::Unsupported("ESTree range is not a list"))?;
    let (start_entry, end_entry) = {
        let mut entries = tape.values_indexed(range);
        let start_entry = entries
            .next()
            .map(|(entry, _)| entry)
            .ok_or(TsrxParseError::Unsupported("ESTree range has no start"))?;
        let end_entry = entries
            .next()
            .map(|(entry, _)| entry)
            .ok_or(TsrxParseError::Unsupported("ESTree range has no end"))?;
        if entries.next().is_some() {
            return Err(TsrxParseError::Unsupported("ESTree range has more than two entries"));
        }
        (start_entry, end_entry)
    };
    tape.set_list_value(start_entry, start)?;
    tape.set_list_value(end_entry, end)?;
    Ok(())
}

fn map_reachable_endpoint(
    segments: &[ProjectionSegment],
    projected: u32,
    is_start: bool,
) -> Option<u32> {
    map_endpoint(segments, projected, is_start).or_else(|| {
        (!is_start).then_some(())?;
        let index = segments.partition_point(|segment| segment.projected.start < projected);
        segments
            .get(index)
            .filter(|segment| segment.projected.start == projected)
            .map(|segment| segment.original_start)
    })
}
