//! Raw-text `<script>` payloads, hidden from OXC and restored as one authored JSX text child.

use tsrx_syntax::{ByteSpan, OverlayView, ProjectionSegment};
use tsrx_tape_schema::{FlatTape, RecordIndex, ValueRef};

use crate::{
    TsrxParseError,
    projection::{map_endpoint, project_authored_end, project_authored_start},
    tape_index::ParentIndex,
};

use super::{
    access::{
        exact_one_value, field_value, has_type, list_field, object_field, scalar_field, scalar_u32,
    },
    edits::append_node_head,
    spans::{AuthoredStart, record_authored_span, require_mapped_object_span, slice_authored},
};

pub(super) fn reconstruct_script_elements(
    tape: &mut FlatTape,
    authored: &str,
    overlay: OverlayView<'_>,
    segments: &[ProjectionSegment],
    opening_elements: &[(u32, RecordIndex)],
    parents: &ParentIndex,
    starts: &mut Vec<AuthoredStart>,
) -> Result<(), TsrxParseError> {
    for script in overlay.script_blocks {
        let (element, opening) =
            find_projected_script(tape, *script, segments, opening_elements, parents)?;
        let opening_span = ByteSpan::new(script.element.start, script.content.start);
        let closing_span = ByteSpan::new(script.content.end, script.element.end);
        require_mapped_object_span(tape, element, script.element, segments)?;
        require_mapped_object_span(tape, opening, opening_span, segments)?;

        let closing = field_value(tape, element, "closingElement")?
            .as_object()
            .ok_or(TsrxParseError::Unsupported("raw script has no closing element"))?;
        if !has_type(tape, closing, r#""JSXClosingElement""#) {
            return Err(TsrxParseError::Unsupported(
                "raw script closing element has an unexpected type",
            ));
        }
        require_mapped_object_span(tape, closing, closing_span, segments)?;

        let children = list_field(tape, element, "children")?;
        let helper = exact_one_value(tape, children)?
            .as_object()
            .ok_or(TsrxParseError::Unsupported("raw script scaffold is not an object"))?;
        validate_payload_scaffold(tape, helper, *script, segments)?;
        rebuild_script_text(tape, authored, helper, script.content, starts)?;

        if tape.field_index(element, "content").is_some() {
            return Err(TsrxParseError::Unsupported(
                "projected script already has a content field",
            ));
        }
        let content = tape.push_json_string_scalar(slice_authored(authored, script.content)?)?;
        tape.append_field(element, "content", content)?;
    }
    Ok(())
}

fn find_projected_script(
    tape: &FlatTape,
    script: tsrx_syntax::ScriptBlock,
    segments: &[ProjectionSegment],
    opening_elements: &[(u32, RecordIndex)],
    parents: &ParentIndex,
) -> Result<(RecordIndex, RecordIndex), TsrxParseError> {
    let mut matched = None;
    let projected_start = project_authored_start(segments, script.element.start)
        .ok_or(TsrxParseError::Unsupported("raw script opening start is unmapped"))?;
    let first = opening_elements.partition_point(|(start, _)| *start < projected_start);
    let last = opening_elements.partition_point(|(start, _)| *start <= projected_start);
    for &(_, opening) in &opening_elements[first..last] {
        if map_endpoint(segments, scalar_u32(tape, opening, "start")?, true)
            != Some(script.element.start)
        {
            continue;
        }
        let name = object_field(tape, opening, "name")?;
        if !has_type(tape, name, r#""JSXIdentifier""#)
            || scalar_field(tape, name, "name")? != r#""script""#
        {
            continue;
        }
        let element = parents
            .parent_container(ValueRef::object(opening))
            .and_then(ValueRef::as_object)
            .ok_or(TsrxParseError::Unsupported("raw script opening has no element parent"))?;
        if field_value(tape, element, "openingElement")? != ValueRef::object(opening) {
            return Err(TsrxParseError::Unsupported(
                "raw script opening is not owned by its element",
            ));
        }
        if matched.replace((element, opening)).is_some() {
            return Err(TsrxParseError::Unsupported("projected raw script opening is duplicated"));
        }
    }
    matched.ok_or(TsrxParseError::Unsupported("projected raw script opening is missing"))
}

fn validate_payload_scaffold(
    tape: &FlatTape,
    helper: RecordIndex,
    script: tsrx_syntax::ScriptBlock,
    segments: &[ProjectionSegment],
) -> Result<(), TsrxParseError> {
    if !has_type(tape, helper, r#""JSXExpressionContainer""#)
        || scalar_u32(tape, helper, "start")?
            != project_authored_end(segments, script.content.start)
                .ok_or(TsrxParseError::Unsupported("script scaffold start is unmapped"))?
        || scalar_u32(tape, helper, "end")?
            != project_authored_start(segments, script.content.end)
                .ok_or(TsrxParseError::Unsupported("script scaffold end is unmapped"))?
    {
        return Err(TsrxParseError::Unsupported("script payload scaffold span is displaced"));
    }
    let sentinel = object_field(tape, helper, "expression")?;
    if !has_type(tape, sentinel, r#""Literal""#)
        || tape.scalar(field_value(tape, sentinel, "value")?) != Some("null")
    {
        return Err(TsrxParseError::Unsupported(
            "script payload scaffold is not the null sentinel",
        ));
    }
    Ok(())
}

fn rebuild_script_text(
    tape: &mut FlatTape,
    authored: &str,
    text: RecordIndex,
    span: ByteSpan,
    starts: &mut Vec<AuthoredStart>,
) -> Result<(), TsrxParseError> {
    tape.clear_fields(text)?;
    append_node_head(tape, text, r#""JSXText""#, span)?;
    let source = slice_authored(authored, span)?;
    let value = tape.push_json_string_scalar(source)?;
    tape.append_field(text, "value", value)?;
    let raw = tape.push_json_string_scalar(source)?;
    tape.append_field(text, "raw", raw)?;
    record_authored_span(starts, text, span);
    Ok(())
}
