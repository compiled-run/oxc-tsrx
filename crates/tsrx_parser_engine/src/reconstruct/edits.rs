//! The small set of in-place tape rewrites a pass is allowed to perform: retype a node, give it
//! a span head, reorder its fields, wrap it in a statement.

use tsrx_syntax::ByteSpan;
use tsrx_tape_schema::{FlatTape, ListRecord, ObjectRecord, RecordIndex, ValueRef};

use crate::TsrxParseError;

use super::access::{field_value, list_field};

#[derive(Debug, Clone, Copy)]
pub(super) struct ListEntryRemoval {
    pub(super) list: RecordIndex,
    pub(super) entry: RecordIndex,
}

pub(super) fn require_empty_metadata(
    tape: &FlatTape,
    metadata: RecordIndex,
) -> Result<(), TsrxParseError> {
    let mut fields = tape.fields(metadata);
    let path = fields.next().ok_or(TsrxParseError::Unsupported("dynamic metadata has no path"))?;
    if tape.key(path) != "path" || fields.next().is_some() {
        return Err(TsrxParseError::Unsupported("dynamic metadata is not canonical"));
    }
    let path = path
        .value
        .as_list()
        .ok_or(TsrxParseError::Unsupported("dynamic metadata path is not a list"))?;
    if tape.values(path).next().is_some() {
        return Err(TsrxParseError::Unsupported("dynamic metadata path is not empty"));
    }
    Ok(())
}

pub(super) fn append_node_head(
    tape: &mut FlatTape,
    object: RecordIndex,
    kind: &str,
    span: ByteSpan,
) -> Result<(), TsrxParseError> {
    let kind = tape.push_scalar(kind)?;
    let start = tape.push_u32_scalar(span.start)?;
    let end = tape.push_u32_scalar(span.end)?;
    tape.append_field(object, "type", kind)?;
    tape.append_field(object, "start", start)?;
    tape.append_field(object, "end", end)?;
    Ok(())
}

pub(super) fn order_span_fields_before(
    tape: &mut FlatTape,
    object: RecordIndex,
    before: &str,
) -> Result<(), TsrxParseError> {
    let start = tape
        .field_index(object, "start")
        .ok_or(TsrxParseError::Unsupported("object has no start field"))?;
    let end = tape
        .field_index(object, "end")
        .ok_or(TsrxParseError::Unsupported("object has no end field"))?;
    let before = tape
        .field_index(object, before)
        .ok_or(TsrxParseError::Unsupported("object has no ordering anchor"))?;
    tape.move_field_before(object, start, before)?;
    tape.move_field_before(object, end, before)?;
    Ok(())
}

pub(super) fn create_expression_statement(
    tape: &mut FlatTape,
    expression: RecordIndex,
) -> Result<RecordIndex, TsrxParseError> {
    let start = field_value(tape, expression, "start")?;
    let end = field_value(tape, expression, "end")?;
    let statement = tape.push_object_record(ObjectRecord::default())?;
    let kind = tape.push_scalar(r#""ExpressionStatement""#)?;
    tape.append_field(statement, "type", kind)?;
    tape.append_field(statement, "start", start)?;
    tape.append_field(statement, "end", end)?;
    if let Some(range) =
        tape.field_index(expression, "range").and_then(|field| tape.field_value(field))
    {
        tape.append_field(statement, "range", range)?;
    }
    tape.append_field(statement, "expression", ValueRef::object(expression))?;
    Ok(statement)
}

pub(super) fn append_empty_metadata(
    tape: &mut FlatTape,
    object: RecordIndex,
) -> Result<(), TsrxParseError> {
    if let Some(field) = tape.field_index(object, "metadata") {
        let metadata = tape
            .field_value(field)
            .and_then(ValueRef::as_object)
            .ok_or(TsrxParseError::Unsupported("metadata is not an object"))?;
        let path = list_field(tape, metadata, "path")?;
        if tape.values(path).next().is_some() {
            return Err(TsrxParseError::Unsupported("metadata path is not empty"));
        }
        return Ok(());
    }
    let path = tape.push_list_record(ListRecord::default())?;
    let metadata = tape.push_object_record(ObjectRecord::default())?;
    tape.append_field(metadata, "path", ValueRef::list(path))?;
    tape.append_field(object, "metadata", ValueRef::object(metadata))?;
    Ok(())
}

pub(super) fn replace_type(
    tape: &mut FlatTape,
    object: RecordIndex,
    kind: &str,
) -> Result<(), TsrxParseError> {
    let field =
        tape.field_index(object, "type").ok_or(TsrxParseError::Unsupported("node has no type"))?;
    let kind = tape.push_scalar(kind)?;
    tape.set_field_value(field, kind)?;
    Ok(())
}
