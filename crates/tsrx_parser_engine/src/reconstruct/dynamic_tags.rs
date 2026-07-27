use super::access::{
    exact_one_value, field_value, has_type, list_field, object_field, require_type, scalar_field,
    scalar_u32, unwrap_parenthesized_expression,
};
use super::edits::{append_empty_metadata, append_node_head, require_empty_metadata};
use super::jsx_statements::normalize_custom_jsx_statement;
use super::scaffold::{dynamic_scaffold_index, require_dynamic_identifier};
use super::spans::{AuthoredStart, record_authored_span};
use crate::{TsrxParseError, projection::map_endpoint, tape_index::ParentIndex};
use tsrx_syntax::{ByteSpan, OverlayView, ProjectionSegment};
use tsrx_tape_schema::{FlatTape, ListValueInsertion, RecordIndex, ValueKind, ValueRef};

#[derive(Debug, Clone, Copy, Default)]
struct DynamicTokenSpans {
    opening: Option<ByteSpan>,
    closing: Option<ByteSpan>,
}

pub(super) fn reconstruct_dynamic_tags(
    tape: &mut FlatTape,
    authored: &str,
    overlay: OverlayView<'_>,
    segments: &[ProjectionSegment],
    prefix: &str,
    parents: &ParentIndex,
    starts: &mut Vec<AuthoredStart>,
) -> Result<(), TsrxParseError> {
    if overlay.dynamic_tags.is_empty() {
        return Ok(());
    }
    let spans = collect_dynamic_token_spans(overlay)?;
    let openings = collect_dynamic_openings(tape, overlay.dynamic_tags.len(), prefix)?;
    let mut semicolons = Vec::new();
    for index in (0..overlay.dynamic_tags.len()).rev() {
        let opening = openings[index]
            .ok_or(TsrxParseError::Unsupported("projected dynamic opening is missing"))?;
        reconstruct_dynamic_tag(
            tape,
            authored,
            overlay.dynamic_tags[index],
            spans[index],
            segments,
            prefix,
            index,
            opening,
            parents,
            starts,
            &mut semicolons,
        )?;
    }
    tape.insert_list_values_after(&semicolons)?;
    Ok(())
}

fn collect_dynamic_token_spans(
    overlay: OverlayView<'_>,
) -> Result<Vec<DynamicTokenSpans>, TsrxParseError> {
    overlay
        .dynamic_tags
        .iter()
        .map(|tag| {
            if tag.opening.is_empty() || tag.self_closing != tag.closing.is_empty() {
                return Err(TsrxParseError::Unsupported("incomplete dynamic projection span"));
            }
            Ok(DynamicTokenSpans {
                opening: Some(tag.opening),
                closing: (!tag.self_closing).then_some(tag.closing),
            })
        })
        .collect()
}

fn collect_dynamic_openings(
    tape: &FlatTape,
    count: usize,
    prefix: &str,
) -> Result<Vec<Option<RecordIndex>>, TsrxParseError> {
    let mut openings = vec![None; count];
    for raw in 0..tape.object_count() {
        let raw = u32::try_from(raw)
            .map_err(|_| TsrxParseError::Unsupported("object table above 4 GiB"))?;
        let opening = RecordIndex::new(raw);
        if !has_type(tape, opening, r#""JSXOpeningElement""#) {
            continue;
        }
        let Some(name) = tape
            .field_index(opening, "name")
            .and_then(|field| tape.field_value(field))
            .and_then(ValueRef::as_object)
            .filter(|name| has_type(tape, *name, r#""JSXIdentifier""#))
        else {
            continue;
        };
        let Some(index) = tape
            .field_index(name, "name")
            .and_then(|field| tape.field_value(field))
            .and_then(|value| tape.scalar(value))
            .and_then(|name| dynamic_scaffold_index(name, prefix, 'D', false))
        else {
            continue;
        };
        let slot = openings
            .get_mut(index)
            .ok_or(TsrxParseError::Unsupported("unknown dynamic opening ordinal"))?;
        if slot.replace(opening).is_some() {
            return Err(TsrxParseError::Unsupported("duplicate dynamic opening ordinal"));
        }
    }
    Ok(openings)
}

#[expect(
    clippy::too_many_arguments,
    reason = "the reconstruction context is threaded down explicitly; a parameter struct would relocate these fields, not remove them"
)]
#[expect(
    clippy::too_many_lines,
    reason = "one flat match over every dynamic-tag shape the projection can carry"
)]
fn reconstruct_dynamic_tag(
    tape: &mut FlatTape,
    authored: &str,
    tag: tsrx_syntax::OverlayDynamicTag,
    spans: DynamicTokenSpans,
    segments: &[ProjectionSegment],
    prefix: &str,
    index: usize,
    opening: RecordIndex,
    parents: &ParentIndex,
    starts: &mut Vec<AuthoredStart>,
    semicolons: &mut Vec<ListValueInsertion>,
) -> Result<(), TsrxParseError> {
    let opening_span =
        spans.opening.ok_or(TsrxParseError::Unsupported("dynamic tag has no opening token"))?;
    let closing_span = spans.closing;
    if tag.self_closing != closing_span.is_none() {
        return Err(TsrxParseError::Unsupported("dynamic closing topology disagrees with overlay"));
    }
    let element = parents
        .parent_container(ValueRef::object(opening))
        .and_then(ValueRef::as_object)
        .ok_or(TsrxParseError::Unsupported("dynamic opening has no JSX element parent"))?;
    require_type(tape, element, r#""JSXElement""#)?;
    if field_value(tape, element, "openingElement")? != ValueRef::object(opening) {
        return Err(TsrxParseError::Unsupported("dynamic opening is not owned by its JSX element"));
    }
    append_empty_metadata(tape, element)?;
    let metadata = object_field(tape, element, "metadata")?;
    require_empty_metadata(tape, metadata)?;
    let children = list_field(tape, element, "children")?;
    let attributes = list_field(tape, opening, "attributes")?;
    let projected_name = object_field(tape, opening, "name")?;
    require_dynamic_identifier(tape, projected_name, prefix, 'D', index, false)?;
    let projected_self_closing = scalar_field(tape, opening, "selfClosing")?;
    if projected_self_closing != if tag.self_closing { "true" } else { "false" } {
        return Err(TsrxParseError::Unsupported(
            "projected dynamic self-closing flag disagrees with overlay",
        ));
    }
    let opening_end = map_endpoint(segments, scalar_u32(tape, opening, "end")?, false)
        .ok_or(TsrxParseError::Unsupported("dynamic opening end is outside authored source"))?;
    if opening_end <= opening_span.end {
        return Err(TsrxParseError::Unsupported(
            "dynamic opening element does not include its terminator",
        ));
    }

    let (attribute_expression_container, opening_expression, first_attribute, end_attribute) =
        dynamic_opening_expression(tape, attributes, tag.expression, segments, prefix, index)?;
    let opening_expression = unwrap_parenthesized_expression(tape, opening_expression)?;
    require_expression_within(tape, opening_expression, tag.expression, segments)?;

    let closing_value = field_value(tape, element, "closingElement")?;
    let closing = if let Some(closing_span) = closing_span {
        let closing = closing_value
            .as_object()
            .ok_or(TsrxParseError::Unsupported("paired dynamic element has no closing element"))?;
        require_type(tape, closing, r#""JSXClosingElement""#)?;
        let projected_closing_name = object_field(tape, closing, "name")?;
        require_dynamic_identifier(tape, projected_closing_name, prefix, 'D', index, false)?;
        let (container, expression) = dynamic_closing_expression(
            tape,
            children,
            tag.closing_expression,
            segments,
            prefix,
            index,
        )?;
        if expression == opening_expression || container == attribute_expression_container {
            return Err(TsrxParseError::Unsupported(
                "dynamic opening and closing expressions share projected identity",
            ));
        }
        let removed = tape.pop_list_value(children)?;
        if removed != ValueRef::object(container) {
            return Err(TsrxParseError::Unsupported(
                "dynamic closing helper is not the final child",
            ));
        }
        rebuild_dynamic_name(
            tape,
            container,
            ByteSpan::new(
                closing_span.start.saturating_add(2),
                tag.closing_expression.end.saturating_add(1),
            ),
            expression,
            starts,
        )?;
        rebuild_dynamic_closing(tape, closing, closing_span, container, starts)?;
        Some(closing)
    } else {
        if tape.scalar(closing_value) != Some("null") {
            return Err(TsrxParseError::Unsupported(
                "self-closing dynamic element has a closing object",
            ));
        }
        None
    };

    let removed_first = tape.remove_list_value(attributes, first_attribute)?;
    let removed_end = tape.remove_list_value(attributes, end_attribute)?;
    if removed_first.kind() != ValueKind::Object || removed_end.kind() != ValueKind::Object {
        return Err(TsrxParseError::Unsupported("dynamic attributes are not object entries"));
    }
    rebuild_dynamic_name(
        tape,
        attribute_expression_container,
        ByteSpan::new(opening_span.start.saturating_add(1), opening_span.end),
        opening_expression,
        starts,
    )?;
    rebuild_dynamic_opening(
        tape,
        opening,
        ByteSpan::new(opening_span.start, opening_end),
        attributes,
        attribute_expression_container,
        tag.self_closing,
        starts,
    )?;
    let element_end = closing_span.map_or(opening_end, |span| span.end);
    rebuild_dynamic_element(
        tape,
        element,
        ByteSpan::new(opening_span.start, element_end),
        metadata,
        children,
        opening,
        closing,
        starts,
    )?;
    normalize_custom_jsx_statement(
        tape,
        authored,
        element,
        ByteSpan::new(opening_span.start, element_end),
        segments,
        parents,
        starts,
        semicolons,
        true,
    )?;
    Ok(())
}

fn dynamic_opening_expression(
    tape: &FlatTape,
    attributes: RecordIndex,
    authored: ByteSpan,
    segments: &[ProjectionSegment],
    prefix: &str,
    index: usize,
) -> Result<(RecordIndex, RecordIndex, RecordIndex, RecordIndex), TsrxParseError> {
    let first_entry = tape
        .list_first_value(attributes)
        .filter(|entry| !entry.is_none())
        .ok_or(TsrxParseError::Unsupported("dynamic opening has no expression attribute"))?;
    let end_entry = tape
        .list_value_next(first_entry)
        .filter(|entry| !entry.is_none())
        .ok_or(TsrxParseError::Unsupported("dynamic opening has no end sentinel attribute"))?;
    let expression_attribute = tape
        .list_value(first_entry)
        .and_then(ValueRef::as_object)
        .ok_or(TsrxParseError::Unsupported("dynamic expression attribute is not an object"))?;
    let end_attribute = tape
        .list_value(end_entry)
        .and_then(ValueRef::as_object)
        .ok_or(TsrxParseError::Unsupported("dynamic end attribute is not an object"))?;
    require_type(tape, expression_attribute, r#""JSXAttribute""#)?;
    require_type(tape, end_attribute, r#""JSXAttribute""#)?;
    let expression_name = object_field(tape, expression_attribute, "name")?;
    let end_name = object_field(tape, end_attribute, "name")?;
    require_dynamic_identifier(tape, expression_name, prefix, 'A', index, true)?;
    require_dynamic_identifier(tape, end_name, prefix, 'Z', index, true)?;

    let container = object_field(tape, expression_attribute, "value")?;
    require_type(tape, container, r#""JSXExpressionContainer""#)?;
    let expression = object_field(tape, container, "expression")?;
    require_expression_within(
        tape,
        unwrap_parenthesized_expression(tape, expression)?,
        authored,
        segments,
    )?;

    let end_container = object_field(tape, end_attribute, "value")?;
    require_type(tape, end_container, r#""JSXExpressionContainer""#)?;
    let sentinel = object_field(tape, end_container, "expression")?;
    require_type(tape, sentinel, r#""Literal""#)?;
    if tape.scalar(field_value(tape, sentinel, "value")?) != Some("null") {
        return Err(TsrxParseError::Unsupported("dynamic end sentinel is not null"));
    }
    Ok((container, expression, first_entry, end_entry))
}

fn dynamic_closing_expression(
    tape: &FlatTape,
    children: RecordIndex,
    authored: ByteSpan,
    segments: &[ProjectionSegment],
    prefix: &str,
    index: usize,
) -> Result<(RecordIndex, RecordIndex), TsrxParseError> {
    let helper = tape
        .values(children)
        .last()
        .and_then(ValueRef::as_object)
        .ok_or(TsrxParseError::Unsupported("dynamic closing helper child is missing"))?;
    require_type(tape, helper, r#""JSXExpressionContainer""#)?;
    let call = object_field(tape, helper, "expression")?;
    require_type(tape, call, r#""CallExpression""#)?;
    let callee = object_field(tape, call, "callee")?;
    require_dynamic_identifier(tape, callee, prefix, 'C', index, true)?;
    if tape.field_index(call, "optional").is_some_and(|field| {
        tape.field_value(field).and_then(|value| tape.scalar(value)) != Some("false")
    }) {
        return Err(TsrxParseError::Unsupported("dynamic closing helper call is optional"));
    }
    let grouped = exact_one_value(tape, list_field(tape, call, "arguments")?)?.as_object().ok_or(
        TsrxParseError::Unsupported("dynamic closing helper argument is not an expression"),
    )?;
    let expression = unwrap_parenthesized_expression(tape, grouped)?;
    require_expression_within(tape, expression, authored, segments)?;
    Ok((helper, expression))
}

fn require_expression_within(
    tape: &FlatTape,
    expression: RecordIndex,
    authored: ByteSpan,
    segments: &[ProjectionSegment],
) -> Result<(), TsrxParseError> {
    let start = map_endpoint(segments, scalar_u32(tape, expression, "start")?, true).ok_or(
        TsrxParseError::Unsupported("dynamic expression start is outside authored source"),
    )?;
    let end = map_endpoint(segments, scalar_u32(tape, expression, "end")?, false)
        .ok_or(TsrxParseError::Unsupported("dynamic expression end is outside authored source"))?;
    if authored.start <= start && start < end && end <= authored.end {
        Ok(())
    } else {
        Err(TsrxParseError::Unsupported("dynamic expression lies outside authored name"))
    }
}

fn rebuild_dynamic_name(
    tape: &mut FlatTape,
    name: RecordIndex,
    span: ByteSpan,
    expression: RecordIndex,
    starts: &mut Vec<AuthoredStart>,
) -> Result<(), TsrxParseError> {
    tape.clear_fields(name)?;
    append_node_head(tape, name, r#""JSXExpressionContainer""#, span)?;
    tape.append_field(name, "expression", ValueRef::object(expression))?;
    let dynamic = tape.push_scalar("true")?;
    tape.append_field(name, "isDynamic", dynamic)?;
    record_authored_span(starts, name, span);
    Ok(())
}

fn rebuild_dynamic_opening(
    tape: &mut FlatTape,
    opening: RecordIndex,
    span: ByteSpan,
    attributes: RecordIndex,
    name: RecordIndex,
    self_closing: bool,
    starts: &mut Vec<AuthoredStart>,
) -> Result<(), TsrxParseError> {
    tape.clear_fields(opening)?;
    append_node_head(tape, opening, r#""JSXOpeningElement""#, span)?;
    tape.append_field(opening, "attributes", ValueRef::list(attributes))?;
    tape.append_field(opening, "name", ValueRef::object(name))?;
    let dynamic = tape.push_scalar("true")?;
    tape.append_field(opening, "isDynamic", dynamic)?;
    let self_closing = tape.push_scalar(if self_closing { "true" } else { "false" })?;
    tape.append_field(opening, "selfClosing", self_closing)?;
    record_authored_span(starts, opening, span);
    Ok(())
}

fn rebuild_dynamic_closing(
    tape: &mut FlatTape,
    closing: RecordIndex,
    span: ByteSpan,
    name: RecordIndex,
    starts: &mut Vec<AuthoredStart>,
) -> Result<(), TsrxParseError> {
    tape.clear_fields(closing)?;
    append_node_head(tape, closing, r#""JSXClosingElement""#, span)?;
    tape.append_field(closing, "name", ValueRef::object(name))?;
    let dynamic = tape.push_scalar("true")?;
    tape.append_field(closing, "isDynamic", dynamic)?;
    record_authored_span(starts, closing, span);
    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    reason = "the reconstruction context is threaded down explicitly; a parameter struct would relocate these fields, not remove them"
)]
fn rebuild_dynamic_element(
    tape: &mut FlatTape,
    element: RecordIndex,
    span: ByteSpan,
    metadata: RecordIndex,
    children: RecordIndex,
    opening: RecordIndex,
    closing: Option<RecordIndex>,
    starts: &mut Vec<AuthoredStart>,
) -> Result<(), TsrxParseError> {
    tape.clear_fields(element)?;
    append_node_head(tape, element, r#""JSXElement""#, span)?;
    tape.append_field(element, "metadata", ValueRef::object(metadata))?;
    tape.append_field(element, "children", ValueRef::list(children))?;
    tape.append_field(element, "openingElement", ValueRef::object(opening))?;
    let closing = if let Some(closing) = closing {
        ValueRef::object(closing)
    } else {
        tape.push_scalar("null")?
    };
    tape.append_field(element, "closingElement", closing)?;
    let dynamic = tape.push_scalar("true")?;
    tape.append_field(element, "isDynamic", dynamic)?;
    record_authored_span(starts, element, span);
    Ok(())
}
