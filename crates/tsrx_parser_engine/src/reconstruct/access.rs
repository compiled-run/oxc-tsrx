use crate::TsrxParseError;
use tsrx_tape_schema::{FlatTape, RecordIndex, ValueRef};

pub(super) fn unwrap_parenthesized_expression(
    tape: &FlatTape,
    mut expression: RecordIndex,
) -> Result<RecordIndex, TsrxParseError> {
    let mut remaining = tape.object_count();
    while has_type(tape, expression, r#""ParenthesizedExpression""#) {
        if remaining == 0 {
            return Err(TsrxParseError::Unsupported("cyclic parenthesized dynamic expression"));
        }
        expression = object_field(tape, expression, "expression")?;
        remaining -= 1;
    }
    Ok(expression)
}

pub(super) fn is_jsx_child_type(tape: &FlatTape, object: RecordIndex) -> bool {
    object_type(tape, object).is_some_and(|kind| {
        matches!(
            kind,
            r#""JSXElement""#
                | r#""JSXCodeBlock""#
                | r#""JSXStyleElement""#
                | r#""JSXFragment""#
                | r#""JSXIfExpression""#
                | r#""JSXForExpression""#
                | r#""JSXSwitchExpression""#
                | r#""JSXTryExpression""#
        )
    })
}

pub(super) fn index_of_overlay(index: u32) -> Result<usize, TsrxParseError> {
    usize::try_from(index)
        .map_err(|_| TsrxParseError::Unsupported("overlay index exceeds host usize"))
}

pub(super) fn exact_one_value(
    tape: &FlatTape,
    list: RecordIndex,
) -> Result<ValueRef, TsrxParseError> {
    let mut values = tape.values(list);
    let value = values
        .next()
        .ok_or(TsrxParseError::Unsupported("scaffold list has an unexpected length"))?;
    if values.next().is_some() {
        return Err(TsrxParseError::Unsupported("scaffold list has an unexpected length"));
    }
    Ok(value)
}

pub(super) fn exact_two_values(
    tape: &FlatTape,
    list: RecordIndex,
) -> Result<(ValueRef, ValueRef), TsrxParseError> {
    let mut values = tape.values(list);
    let first = values
        .next()
        .ok_or(TsrxParseError::Unsupported("scaffold list has an unexpected length"))?;
    let second = values
        .next()
        .ok_or(TsrxParseError::Unsupported("scaffold list has an unexpected length"))?;
    if values.next().is_some() {
        return Err(TsrxParseError::Unsupported("scaffold list has an unexpected length"));
    }
    Ok((first, second))
}

pub(super) fn field_value(
    tape: &FlatTape,
    object: RecordIndex,
    name: &str,
) -> Result<ValueRef, TsrxParseError> {
    tape.field_index(object, name)
        .and_then(|field| tape.field_value(field))
        .ok_or(TsrxParseError::Unsupported("missing required ESTree field"))
}

pub(super) fn scalar_field<'a>(
    tape: &'a FlatTape,
    object: RecordIndex,
    name: &str,
) -> Result<&'a str, TsrxParseError> {
    tape.scalar(field_value(tape, object, name)?)
        .ok_or(TsrxParseError::Unsupported("required ESTree field is not scalar"))
}

pub(super) fn scalar_u32(
    tape: &FlatTape,
    object: RecordIndex,
    name: &str,
) -> Result<u32, TsrxParseError> {
    tape.scalar_u32(field_value(tape, object, name)?)
        .ok_or(TsrxParseError::Unsupported("required ESTree field is not u32"))
}

pub(super) fn object_field(
    tape: &FlatTape,
    object: RecordIndex,
    name: &str,
) -> Result<RecordIndex, TsrxParseError> {
    field_value(tape, object, name)?
        .as_object()
        .ok_or(TsrxParseError::Unsupported("required ESTree field is not an object"))
}

pub(super) fn list_field(
    tape: &FlatTape,
    object: RecordIndex,
    name: &str,
) -> Result<RecordIndex, TsrxParseError> {
    field_value(tape, object, name)?
        .as_list()
        .ok_or(TsrxParseError::Unsupported("required ESTree field is not a list"))
}

pub(super) fn object_type(tape: &FlatTape, object: RecordIndex) -> Option<&str> {
    tape.field_index(object, "type")
        .and_then(|field| tape.field_value(field))
        .and_then(|value| tape.scalar(value))
}

pub(super) fn has_type(tape: &FlatTape, object: RecordIndex, expected: &str) -> bool {
    object_type(tape, object) == Some(expected)
}

pub(super) fn require_type(
    tape: &FlatTape,
    object: RecordIndex,
    expected: &'static str,
) -> Result<(), TsrxParseError> {
    if has_type(tape, object, expected) {
        Ok(())
    } else {
        Err(TsrxParseError::Unsupported("unexpected ESTree node type"))
    }
}

pub(super) fn index_of(index: RecordIndex) -> Result<usize, TsrxParseError> {
    let raw = index.get().ok_or(TsrxParseError::Unsupported("missing tape index"))?;
    usize::try_from(raw).map_err(|_| TsrxParseError::Unsupported("tape index exceeds host usize"))
}
