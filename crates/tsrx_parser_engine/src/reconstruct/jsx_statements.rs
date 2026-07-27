use super::access::{field_value, has_type, scalar_u32};
use super::edits::append_node_head;
use super::spans::{AuthoredStart, record_authored_span};
use crate::{
    TsrxParseError, projection::map_endpoint, tape_index::ParentIndex, tape_index::ParentSlot,
};
use tsrx_syntax::{ByteSpan, ProjectionSegment};
use tsrx_tape_schema::{FlatTape, ListValueInsertion, ObjectRecord, RecordIndex, ValueRef};

#[expect(
    clippy::too_many_arguments,
    reason = "the reconstruction context is threaded down explicitly; a parameter struct would relocate these fields, not remove them"
)]
pub(super) fn normalize_custom_jsx_statement(
    tape: &mut FlatTape,
    authored: &str,
    element: RecordIndex,
    element_span: ByteSpan,
    segments: &[ProjectionSegment],
    parents: &ParentIndex,
    starts: &mut Vec<AuthoredStart>,
    semicolons: &mut Vec<ListValueInsertion>,
    separated_semicolon_is_jsx_text: bool,
) -> Result<(), TsrxParseError> {
    let mut expression_root = element;
    let mut parenthesized = false;
    let statement = loop {
        let Some(parent) = parents
            .parent_container(ValueRef::object(expression_root))
            .and_then(ValueRef::as_object)
        else {
            return Ok(());
        };
        if has_type(tape, parent, r#""ParenthesizedExpression""#) {
            if field_value(tape, parent, "expression")? != ValueRef::object(expression_root) {
                return Err(TsrxParseError::Unsupported(
                    "custom JSX parenthesis owns another expression",
                ));
            }
            expression_root = parent;
            parenthesized = true;
            continue;
        }
        break parent;
    };
    if !has_type(tape, statement, r#""ExpressionStatement""#) {
        return Ok(());
    }
    if field_value(tape, statement, "expression")? != ValueRef::object(expression_root) {
        return Err(TsrxParseError::Unsupported(
            "custom JSX expression statement owns another expression",
        ));
    }
    if parenthesized {
        let expression_slot = parents.parent_slot(ValueRef::object(expression_root)).ok_or(
            TsrxParseError::Unsupported(
                "parenthesized custom JSX statement has no expression slot",
            ),
        )?;
        ParentIndex::replace(tape, expression_slot, ValueRef::object(element))?;
        return Ok(());
    }
    let element_end = usize::try_from(element_span.end)
        .map_err(|_| TsrxParseError::Unsupported("custom JSX end exceeds host usize"))?;
    let semicolon_start = skip_custom_jsx_statement_trivia(authored, element_end)?;
    let has_semicolon = authored.as_bytes().get(semicolon_start) == Some(&b';');
    let authored_end = if has_semicolon {
        u32::try_from(semicolon_start)
            .ok()
            .and_then(|start| start.checked_add(1))
            .ok_or(TsrxParseError::Unsupported("custom JSX statement span overflow"))?
    } else {
        element_span.end
    };
    if map_endpoint(segments, scalar_u32(tape, statement, "end")?, false)
        .is_some_and(|mapped| mapped != authored_end)
    {
        return Err(TsrxParseError::Unsupported(
            "custom JSX statement has unsupported trailing syntax",
        ));
    }
    let statement_slot = parents
        .parent_slot(ValueRef::object(statement))
        .ok_or(TsrxParseError::Unsupported("custom JSX statement has no parent slot"))?;
    ParentIndex::replace(tape, statement_slot, ValueRef::object(element))?;
    let (list, entry) = custom_jsx_statement_list_anchor(tape, parents, statement, statement_slot)?;
    if has_semicolon {
        let start = u32::try_from(semicolon_start)
            .map_err(|_| TsrxParseError::Unsupported("custom JSX semicolon exceeds 4 GiB"))?;
        let span = ByteSpan::new(start, authored_end);
        let jsx_text = separated_semicolon_is_jsx_text && start != element_span.end;
        let value = build_custom_jsx_semicolon(tape, span, jsx_text, starts)?;
        semicolons.push(ListValueInsertion { list, after: entry, value });
    }
    Ok(())
}

fn skip_custom_jsx_statement_trivia(
    authored: &str,
    mut index: usize,
) -> Result<usize, TsrxParseError> {
    let bytes = authored.as_bytes();
    loop {
        while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
            index += 1;
        }
        if bytes.get(index..index + 2) == Some(b"//") {
            index += 2;
            while bytes.get(index).is_some_and(|byte| !matches!(byte, b'\n' | b'\r')) {
                index += 1;
            }
        } else if bytes.get(index..index + 2) == Some(b"/*") {
            index += 2;
            while bytes.get(index..index + 2) != Some(b"*/") {
                if bytes.get(index).is_none() {
                    return Err(TsrxParseError::Unsupported(
                        "unterminated comment after custom JSX statement",
                    ));
                }
                index += 1;
            }
            index += 2;
        } else {
            return Ok(index);
        }
    }
}

fn custom_jsx_statement_list_anchor(
    tape: &FlatTape,
    parents: &ParentIndex,
    statement: RecordIndex,
    statement_slot: ParentSlot,
) -> Result<(RecordIndex, RecordIndex), TsrxParseError> {
    let mut current = statement;
    let mut slot = statement_slot;
    loop {
        match slot {
            ParentSlot::ListValue(entry) => {
                let list = parents
                    .parent_container(ValueRef::object(current))
                    .and_then(ValueRef::as_list)
                    .ok_or(TsrxParseError::Unsupported(
                        "custom JSX statement has no parent list",
                    ))?;
                return Ok((list, entry));
            }
            ParentSlot::Field(field) => {
                let label = parents
                    .parent_container(ValueRef::object(current))
                    .and_then(ValueRef::as_object)
                    .filter(|owner| has_type(tape, *owner, r#""LabeledStatement""#))
                    .ok_or(TsrxParseError::Unsupported(
                        "custom JSX statement is in an unsupported field",
                    ))?;
                if tape.field_index(label, "body") != Some(field) {
                    return Err(TsrxParseError::Unsupported(
                        "custom JSX statement is not a label body",
                    ));
                }
                current = label;
                slot = parents.parent_slot(ValueRef::object(label)).ok_or(
                    TsrxParseError::Unsupported("labeled custom JSX statement has no parent slot"),
                )?;
            }
        }
    }
}

fn build_custom_jsx_semicolon(
    tape: &mut FlatTape,
    span: ByteSpan,
    jsx_text: bool,
    starts: &mut Vec<AuthoredStart>,
) -> Result<ValueRef, TsrxParseError> {
    if !jsx_text {
        let empty = tape.push_object_record(ObjectRecord::default())?;
        append_node_head(tape, empty, r#""EmptyStatement""#, span)?;
        record_authored_span(starts, empty, span);
        return Ok(ValueRef::object(empty));
    }

    let text = tape.push_object_record(ObjectRecord::default())?;
    append_node_head(tape, text, r#""JSXText""#, span)?;
    let value = tape.push_scalar(r#"";""#)?;
    tape.append_field(text, "value", value)?;
    let raw = tape.push_scalar(r#"";""#)?;
    tape.append_field(text, "raw", raw)?;
    record_authored_span(starts, text, span);

    let statement = tape.push_object_record(ObjectRecord::default())?;
    append_node_head(tape, statement, r#""ExpressionStatement""#, span)?;
    tape.append_field(statement, "expression", ValueRef::object(text))?;
    record_authored_span(starts, statement, span);
    Ok(ValueRef::object(statement))
}
