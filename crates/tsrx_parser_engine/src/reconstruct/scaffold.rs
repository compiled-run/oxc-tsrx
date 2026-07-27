use tsrx_tape_schema::{FlatTape, RecordIndex};

use crate::TsrxParseError;

use super::access::{object_field, require_type, scalar_field};

pub(super) fn require_scaffold_callee(
    tape: &FlatTape,
    call: RecordIndex,
    prefix: &str,
    tag: &str,
    ordinal: usize,
) -> Result<(), TsrxParseError> {
    let callee = object_field(tape, call, "callee")?;
    require_type(tape, callee, r#""Identifier""#)?;
    if scaffold_tag_matches(scalar_field(tape, callee, "name")?, prefix, tag, ordinal) {
        Ok(())
    } else {
        Err(TsrxParseError::Unsupported("unknown annotated header helper"))
    }
}

pub(super) fn scaffold_tag_matches(
    encoded: &str,
    prefix: &str,
    tag: &str,
    expected_index: usize,
) -> bool {
    scaffold_tag_index(encoded, prefix, tag) == Some(expected_index)
}

pub(super) fn scaffold_tag_index(encoded: &str, prefix: &str, tag: &str) -> Option<usize> {
    encoded
        .strip_prefix('"')?
        .strip_suffix('"')?
        .strip_prefix(prefix)?
        .strip_prefix(tag)?
        .strip_suffix('_')?
        .parse()
        .ok()
}

pub(super) fn require_dynamic_identifier(
    tape: &FlatTape,
    object: RecordIndex,
    prefix: &str,
    kind: char,
    index: usize,
    suffix: bool,
) -> Result<(), TsrxParseError> {
    require_type(tape, object, r#""Identifier""#)
        .or_else(|_| require_type(tape, object, r#""JSXIdentifier""#))?;
    if dynamic_scaffold_index(scalar_field(tape, object, "name")?, prefix, kind, suffix)
        == Some(index)
    {
        Ok(())
    } else {
        Err(TsrxParseError::Unsupported("dynamic scaffold identifier does not match owner"))
    }
}

pub(super) fn dynamic_scaffold_index(
    encoded: &str,
    prefix: &str,
    kind: char,
    suffix: bool,
) -> Option<usize> {
    let name = encoded.strip_prefix('"')?.strip_suffix('"')?;
    let digits = name.strip_prefix(prefix)?.strip_prefix(kind)?;
    let digits = if suffix { digits.strip_suffix('_')? } else { digits };
    (!digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| digits.parse().ok())
        .flatten()
}

pub(super) fn scaffold_name_matches(
    encoded: &str,
    prefix: &str,
    marker: char,
    expected_index: usize,
) -> bool {
    let Some(name) = encoded.strip_prefix('"').and_then(|value| value.strip_suffix('"')) else {
        return false;
    };
    let Some(suffix) = name.strip_prefix(prefix).and_then(|value| value.strip_prefix(marker))
    else {
        return false;
    };
    suffix.strip_suffix('_').and_then(|value| value.parse::<usize>().ok()) == Some(expected_index)
}
