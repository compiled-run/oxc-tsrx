use tsrx_tape_schema::{FlatTape, RecordIndex, TapeSpan, ValueKind, ValueRef};

use crate::TsrxParseError;

use super::{
    observer::{RepairCopyLane, Utf16WorkObserver},
    pua_markers::apply_pua_markers,
};

pub(super) fn object_type(tape: &FlatTape, object: RecordIndex) -> Option<&str> {
    let field = tape.field_index(object, "type")?;
    tape.scalar(tape.field_value(field)?)
}

pub(super) fn object_span(
    tape: &FlatTape,
    object: RecordIndex,
) -> Result<TapeSpan, TsrxParseError> {
    Ok(TapeSpan::new(
        scalar_u32_field(tape, object, "start")?,
        scalar_u32_field(tape, object, "end")?,
    ))
}

fn scalar_u32_field(
    tape: &FlatTape,
    object: RecordIndex,
    name: &str,
) -> Result<u32, TsrxParseError> {
    let field = tape
        .field_index(object, name)
        .ok_or_else(|| TsrxParseError::Adapter(format!("object is missing `{name}` coordinate")))?;
    tape.scalar_u32(
        tape.field_value(field)
            .ok_or_else(|| TsrxParseError::Adapter("invalid coordinate field".to_string()))?,
    )
    .ok_or_else(|| TsrxParseError::Adapter("coordinate is not u32".to_string()))
}

pub(super) fn object_field(
    tape: &FlatTape,
    object: RecordIndex,
    name: &str,
) -> Option<RecordIndex> {
    tape.field_index(object, name)
        .and_then(|field| tape.field_value(field))
        .and_then(ValueRef::as_object)
}

pub(super) fn required_object_field(
    tape: &FlatTape,
    object: RecordIndex,
    name: &str,
) -> Result<RecordIndex, TsrxParseError> {
    object_field(tape, object, name)
        .ok_or_else(|| TsrxParseError::Adapter(format!("`{name}` is not an object")))
}

pub(super) fn style_payload_span(
    tape: &FlatTape,
    style: RecordIndex,
) -> Result<Option<TapeSpan>, TsrxParseError> {
    let opening = required_object_field(tape, style, "openingElement")?;
    let Some(closing) = object_field(tape, style, "closingElement") else {
        return Ok(None);
    };
    let start = scalar_u32_field(tape, opening, "end")?;
    let end = scalar_u32_field(tape, closing, "start")?;
    if start > end {
        return Err(TsrxParseError::Adapter(
            "style payload has inverted child boundaries".to_string(),
        ));
    }
    Ok(Some(TapeSpan::new(start, end)))
}

pub(super) fn replace_json_field<W: Utf16WorkObserver>(
    tape: &mut FlatTape,
    object: RecordIndex,
    name: &str,
    value: &[u16],
    lane: RepairCopyLane,
    observer: &mut W,
) -> Result<(), TsrxParseError> {
    let field = tape
        .field_index(object, name)
        .ok_or_else(|| TsrxParseError::Adapter(format!("missing `{name}` scalar field")))?;
    if tape.field_value(field).is_none_or(|value| value.kind() != ValueKind::Scalar) {
        return Err(TsrxParseError::Adapter(format!("`{name}` is not a scalar field")));
    }
    let restored_units = value.len();
    let value = tape.push_json_utf16_scalar(value)?;
    tape.set_field_value(field, value)?;
    observer.record_copy(lane, restored_units);
    Ok(())
}

pub(super) fn patch_json_field<W: Utf16WorkObserver>(
    tape: &mut FlatTape,
    object: RecordIndex,
    name: &str,
    markers: &[Option<u16>],
    allow_null: bool,
    lane: RepairCopyLane,
    observer: &mut W,
) -> Result<(), TsrxParseError> {
    let field = tape
        .field_index(object, name)
        .ok_or_else(|| TsrxParseError::Adapter(format!("missing `{name}` scalar field")))?;
    let value = tape
        .field_value(field)
        .and_then(|value| tape.scalar(value))
        .ok_or_else(|| TsrxParseError::Adapter(format!("`{name}` is not a scalar field")))?
        .to_owned();
    if allow_null && value == "null" {
        return Ok(());
    }
    let mut units = decode_json_string(&value)?;
    apply_pua_markers(&mut units, markers)?;
    let value = tape.push_json_utf16_scalar(&units)?;
    tape.set_field_value(field, value)?;
    observer.record_copy(lane, units.len());
    Ok(())
}

fn decode_json_string(value: &str) -> Result<Vec<u16>, TsrxParseError> {
    let inner = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .ok_or_else(|| TsrxParseError::Adapter("OXC scalar is not a JSON string".to_string()))?;
    let mut output = Vec::with_capacity(inner.len());
    let mut characters = inner.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            let mut encoded = [0_u16; 2];
            output.extend(character.encode_utf16(&mut encoded).iter().copied());
            continue;
        }
        let escaped = characters.next().ok_or_else(|| {
            TsrxParseError::Adapter("OXC JSON scalar ends in backslash".to_string())
        })?;
        match escaped {
            '"' => output.push(u16::from(b'"')),
            '\\' => output.push(u16::from(b'\\')),
            '/' => output.push(u16::from(b'/')),
            'b' => output.push(0x08),
            'f' => output.push(0x0c),
            'n' => output.push(0x0a),
            'r' => output.push(0x0d),
            't' => output.push(0x09),
            'u' => {
                let mut scalar = 0_u16;
                for _ in 0..4 {
                    let digit = characters.next().and_then(|value| value.to_digit(16)).ok_or_else(
                        || TsrxParseError::Adapter("invalid OXC JSON Unicode escape".to_string()),
                    )?;
                    scalar = scalar
                        .checked_mul(16)
                        .and_then(|value| value.checked_add(u16::try_from(digit).ok()?))
                        .ok_or_else(|| {
                            TsrxParseError::Adapter("OXC JSON Unicode escape overflow".to_string())
                        })?;
                }
                output.push(scalar);
            }
            _ => {
                return Err(TsrxParseError::Adapter("invalid OXC JSON string escape".to_string()));
            }
        }
    }
    Ok(output)
}

pub(super) fn record_index(value: usize) -> Result<RecordIndex, TsrxParseError> {
    u32::try_from(value)
        .map(RecordIndex::new)
        .map_err(|_| TsrxParseError::Unsupported("record index exceeds u32"))
}
