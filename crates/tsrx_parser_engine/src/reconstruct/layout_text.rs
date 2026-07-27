use super::access::{has_type, scalar_field};
use super::edits::ListEntryRemoval;
use crate::TsrxParseError;
use tsrx_tape_schema::{FlatTape, RecordIndex, ValueRef};

/// Applies TSRX's JSX significant-whitespace rule over the serialized child lists in one flat
/// tape pass. Inline whitespace remains observable text; indentation-only text containing a line
/// break is scheduled for the shared validated in-place removal batch.
pub(super) fn normalize_template_layout_text(
    tape: &mut FlatTape,
    layout_containers: &[RecordIndex],
    removals: &mut Vec<ListEntryRemoval>,
) -> Result<(), TsrxParseError> {
    let mut value_updates = Vec::new();
    for &object in layout_containers {
        let Some(children) = tape
            .field_index(object, "children")
            .and_then(|field| tape.field_value(field))
            .and_then(ValueRef::as_list)
        else {
            continue;
        };
        for (entry, value) in tape.values_indexed(children) {
            let Some(text) = value.as_object().filter(|text| has_type(tape, *text, r#""JSXText""#))
            else {
                continue;
            };
            let value_field = tape
                .field_index(text, "value")
                .ok_or(TsrxParseError::Unsupported("JSXText has no value field"))?;
            let raw = scalar_field(tape, text, "raw")?;
            let normalized = strip_template_block_comments_json(raw)?;
            let value = normalized.as_deref().unwrap_or(scalar_field(tape, text, "value")?);
            if value == r#""""# || is_layout_only_text_json(value) {
                removals.push(ListEntryRemoval { list: children, entry });
            } else if let Some(value) = normalized {
                value_updates.push((value_field, value));
            }
        }
    }
    for (field, encoded) in value_updates {
        let value = tape.push_scalar(&encoded)?;
        tape.set_field_value(field, value)?;
    }
    Ok(())
}

/// Matches `@tsrx/core`'s template raw-text semantics without another source or AST pass. OXC
/// correctly preserves the authored JSX text in `raw`; TSRX additionally treats `/* ... */` as a
/// template comment anywhere in that run, so only `value` drops those ranges.
fn strip_template_block_comments_json(encoded: &str) -> Result<Option<String>, TsrxParseError> {
    let inner = encoded
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .ok_or(TsrxParseError::Unsupported("JSXText raw field is not a JSON string"))?;
    let Some(first) = inner.find("/*") else {
        return Ok(None);
    };

    let mut output = String::with_capacity(encoded.len());
    output.push('"');
    output.push_str(&inner[..first]);
    let mut cursor = first;
    loop {
        let content_start = cursor + 2;
        let Some(close_offset) = inner[content_start..].find("*/") else {
            cursor = inner.len();
            break;
        };
        cursor = content_start + close_offset + 2;
        let Some(next_offset) = inner[cursor..].find("/*") else {
            break;
        };
        output.push_str(&inner[cursor..cursor + next_offset]);
        cursor += next_offset;
    }
    output.push_str(&inner[cursor..]);
    output.push('"');
    Ok(Some(output))
}

/// Classifies one JSON string scalar without allocating it. TSRX drops indentation-only text and
/// template line comments when the span contains CR/LF layout; a plain inline space remains a
/// real child.
fn is_layout_only_text_json(encoded: &str) -> bool {
    let Some(inner) = encoded.strip_prefix('"').and_then(|value| value.strip_suffix('"')) else {
        return false;
    };
    let mut chars = inner.chars();
    let mut has_newline = false;
    loop {
        let decoded = match next_json_string_character(&mut chars) {
            Ok(Some(character)) => character,
            Ok(None) => return has_newline,
            Err(()) => return false,
        };
        if decoded.is_whitespace() {
            has_newline |= matches!(decoded, '\n' | '\r');
            continue;
        }
        if decoded != '/' || !matches!(next_json_string_character(&mut chars), Ok(Some('/'))) {
            return false;
        }
        loop {
            match next_json_string_character(&mut chars) {
                Ok(Some('\n' | '\r')) => {
                    has_newline = true;
                    break;
                }
                Ok(Some(_)) => {}
                Ok(None) => return has_newline,
                Err(()) => return false,
            }
        }
    }
}

fn next_json_string_character(chars: &mut std::str::Chars<'_>) -> Result<Option<char>, ()> {
    let Some(character) = chars.next() else {
        return Ok(None);
    };
    if character != '\\' {
        return Ok(Some(character));
    }
    let decoded = match chars.next() {
        Some('"') => '"',
        Some('\\') => '\\',
        Some('/') => '/',
        Some('b') => '\u{0008}',
        Some('f') => '\u{000c}',
        Some('n') => '\n',
        Some('r') => '\r',
        Some('t') => '\t',
        Some('u') => {
            let mut value = 0_u32;
            for _ in 0..4 {
                let Some(digit) = chars.next().and_then(|digit| digit.to_digit(16)) else {
                    return Err(());
                };
                value = (value << 4) | digit;
            }
            char::from_u32(value).ok_or(())?
        }
        _ => return Err(()),
    };
    Ok(Some(decoded))
}
