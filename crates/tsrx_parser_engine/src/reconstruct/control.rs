//! What all four control constructs share: preparing block bodies, proving a projected wrapper
//! call is generated scaffolding, and re-seating the rewritten node in its authored statement,
//! expression, or JSX-child slot.

use tsrx_syntax::{ByteSpan, ControlContext};
use tsrx_tape_schema::{FlatTape, RecordIndex, ValueRef};

use crate::{
    TsrxParseError,
    tape_index::{ParentIndex, ParentSlot},
};

use super::{
    access::{
        exact_one_value, exact_two_values, field_value, has_type, is_jsx_child_type, list_field,
        object_field, require_type, scalar_field,
    },
    edits::{append_empty_metadata, create_expression_statement, order_span_fields_before},
    scaffold::scaffold_name_matches,
    spans::AuthoredStart,
};

pub(super) fn prepare_control_block(
    tape: &mut FlatTape,
    block: RecordIndex,
    body_lists: &mut Vec<RecordIndex>,
) -> Result<(), TsrxParseError> {
    require_type(tape, block, r#""BlockStatement""#)?;
    let body = list_field(tape, block, "body")?;
    order_span_fields_before(tape, block, "body")?;
    append_empty_metadata(tape, block)?;
    body_lists.push(body);
    Ok(())
}

pub(super) fn normalize_control_body_lists(
    tape: &mut FlatTape,
    bodies: &[RecordIndex],
) -> Result<(), TsrxParseError> {
    let mut replacements = Vec::new();
    for &body in bodies {
        prepare_body_list(tape, body, &mut replacements)?;
    }
    Ok(())
}

fn prepare_body_list(
    tape: &mut FlatTape,
    body: RecordIndex,
    replacements: &mut Vec<(RecordIndex, ValueRef)>,
) -> Result<(), TsrxParseError> {
    let scratch_start = replacements.len();
    for (entry, value) in tape.values_indexed(body) {
        let Some(statement) = value.as_object() else {
            continue;
        };
        if !has_type(tape, statement, r#""ExpressionStatement""#) {
            continue;
        }
        let expression = field_value(tape, statement, "expression")?;
        let Some(expression_object) = expression.as_object() else {
            continue;
        };
        if is_jsx_child_type(tape, expression_object) {
            replacements.push((entry, expression));
        }
    }
    for &(entry, value) in &replacements[scratch_start..] {
        tape.set_list_value(entry, value)?;
    }
    replacements.truncate(scratch_start);
    Ok(())
}

pub(super) fn find_wrapper_call(
    tape: &FlatTape,
    parents: &ParentIndex,
    control: RecordIndex,
    prefix: &str,
    node_index: usize,
    trailing: Option<RecordIndex>,
) -> Result<RecordIndex, TsrxParseError> {
    let mut ancestor = ValueRef::object(control);
    let max_steps = tape.object_count().saturating_add(tape.list_count());
    for _ in 0..max_steps {
        ancestor = parents
            .parent_container(ancestor)
            .ok_or(TsrxParseError::Unsupported("control wrapper chain ended early"))?;
        let Some(object) = ancestor.as_object() else {
            continue;
        };
        if has_type(tape, object, r#""CallExpression""#) {
            validate_wrapper_call(tape, object, control, prefix, node_index, trailing)?;
            return Ok(object);
        }
    }
    Err(TsrxParseError::Unsupported("control wrapper chain is cyclic or missing"))
}

fn validate_wrapper_call(
    tape: &FlatTape,
    call: RecordIndex,
    control: RecordIndex,
    prefix: &str,
    node_index: usize,
    trailing: Option<RecordIndex>,
) -> Result<(), TsrxParseError> {
    let callee = object_field(tape, call, "callee")?;
    require_type(tape, callee, r#""Identifier""#)?;
    if !scaffold_name_matches(scalar_field(tape, callee, "name")?, prefix, 'W', node_index) {
        return Err(TsrxParseError::Unsupported("unknown control wrapper callee"));
    }
    let (manifest, end_marker) = exact_two_values(tape, list_field(tape, call, "arguments")?)?;
    let object = manifest
        .as_object()
        .ok_or(TsrxParseError::Unsupported("wrapper manifest is not an object"))?;
    require_type(tape, object, r#""ObjectExpression""#)?;
    let property = exact_one_value(tape, list_field(tape, object, "properties")?)?
        .as_object()
        .ok_or(TsrxParseError::Unsupported("wrapper manifest property missing"))?;
    require_type(tape, property, r#""Property""#)?;
    let key = object_field(tape, property, "key")?;
    require_type(tape, key, r#""Identifier""#)?;
    if !scaffold_name_matches(scalar_field(tape, key, "name")?, prefix, 'M', node_index) {
        return Err(TsrxParseError::Unsupported("unknown wrapper method key"));
    }
    let function = object_field(tape, property, "value")?;
    require_type(tape, function, r#""FunctionExpression""#)?;
    if scalar_field(tape, function, "generator")? != "true"
        || scalar_field(tape, function, "async")? != "true"
    {
        return Err(TsrxParseError::Unsupported("control wrapper is not an async generator"));
    }
    let function_body = object_field(tape, function, "body")?;
    let body_list = list_field(tape, function_body, "body")?;
    let mut body = tape.values(body_list);
    if body.next() != Some(ValueRef::object(control))
        || trailing.is_some_and(|object| body.next() != Some(ValueRef::object(object)))
        || trailing.is_none() && body.next().is_some()
        || body.next().is_some()
    {
        return Err(TsrxParseError::Unsupported("control wrapper has unexpected statements"));
    }
    let end =
        end_marker.as_object().ok_or(TsrxParseError::Unsupported("wrapper end marker missing"))?;
    require_type(tape, end, r#""Identifier""#)?;
    if !scaffold_name_matches(scalar_field(tape, end, "name")?, prefix, 'E', node_index) {
        return Err(TsrxParseError::Unsupported("unknown wrapper end marker"));
    }
    Ok(())
}

pub(super) fn place_control(
    tape: &mut FlatTape,
    parents: &ParentIndex,
    control: RecordIndex,
    context: ControlContext,
    wrapper: Option<RecordIndex>,
    authored: ByteSpan,
    starts: &mut Vec<AuthoredStart>,
) -> Result<(), TsrxParseError> {
    let value = ValueRef::object(control);
    match context {
        ControlContext::Statement => {
            if wrapper.is_some() {
                return Err(TsrxParseError::Unsupported(
                    "statement control unexpectedly has a wrapper",
                ));
            }
            let slot = parents
                .parent_slot(value)
                .ok_or(TsrxParseError::Unsupported("statement control has no parent slot"))?;
            if !matches!(slot, ParentSlot::ListValue(_)) {
                return Err(TsrxParseError::Unsupported(
                    "statement control is not in a statement list",
                ));
            }
            let statement = create_expression_statement(tape, control)?;
            ParentIndex::replace(tape, slot, ValueRef::object(statement))?;
            starts.push(AuthoredStart { object: statement, start: authored.start, end: None });
        }
        ControlContext::Expression => {
            let wrapper =
                wrapper.ok_or(TsrxParseError::Unsupported("expression control has no wrapper"))?;
            let slot = parents
                .parent_slot(ValueRef::object(wrapper))
                .ok_or(TsrxParseError::Unsupported("expression wrapper has no parent"))?;
            record_labeled_control_statement(tape, parents, wrapper, authored, starts)?;
            ParentIndex::replace(tape, slot, value)?;
        }
        ControlContext::JsxChild => {
            let wrapper =
                wrapper.ok_or(TsrxParseError::Unsupported("JSX-child control has no wrapper"))?;
            let container = parents
                .parent_container(ValueRef::object(wrapper))
                .and_then(ValueRef::as_object)
                .ok_or(TsrxParseError::Unsupported(
                    "JSX-child wrapper has no expression container",
                ))?;
            require_type(tape, container, r#""JSXExpressionContainer""#)?;
            if field_value(tape, container, "expression")? != ValueRef::object(wrapper) {
                return Err(TsrxParseError::Unsupported(
                    "wrapper is not the JSX container expression",
                ));
            }
            let slot = parents
                .parent_slot(ValueRef::object(container))
                .ok_or(TsrxParseError::Unsupported("JSX expression container has no child slot"))?;
            if !matches!(slot, ParentSlot::ListValue(_)) {
                return Err(TsrxParseError::Unsupported("JSX expression container is not a child"));
            }
            ParentIndex::replace(tape, slot, value)?;
        }
    }
    Ok(())
}

fn record_labeled_control_statement(
    tape: &FlatTape,
    parents: &ParentIndex,
    wrapper: RecordIndex,
    authored: ByteSpan,
    starts: &mut Vec<AuthoredStart>,
) -> Result<(), TsrxParseError> {
    let Some(statement) = parents
        .parent_container(ValueRef::object(wrapper))
        .and_then(ValueRef::as_object)
        .filter(|statement| has_type(tape, *statement, r#""ExpressionStatement""#))
    else {
        return Ok(());
    };
    if field_value(tape, statement, "expression")? != ValueRef::object(wrapper) {
        return Err(TsrxParseError::Unsupported(
            "control wrapper statement has an unexpected expression",
        ));
    }
    let Some(label) = parents
        .parent_container(ValueRef::object(statement))
        .and_then(ValueRef::as_object)
        .filter(|label| has_type(tape, *label, r#""LabeledStatement""#))
    else {
        return Ok(());
    };
    if field_value(tape, label, "body")? != ValueRef::object(statement) {
        return Err(TsrxParseError::Unsupported("labeled control wrapper is not the label body"));
    }
    starts.push(AuthoredStart {
        object: statement,
        start: authored.start,
        end: Some(authored.end),
    });
    Ok(())
}
