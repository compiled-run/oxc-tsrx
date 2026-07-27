use tsrx_tape_schema::StringRange;

use crate::TsrxParseError;

pub(super) fn slice(source: &str, start: u32, end: u32) -> Result<&str, TsrxParseError> {
    let start = usize::try_from(start)
        .map_err(|_| TsrxParseError::Unsupported("span start exceeds host usize"))?;
    let end = usize::try_from(end)
        .map_err(|_| TsrxParseError::Unsupported("span end exceeds host usize"))?;
    source.get(start..end).ok_or(TsrxParseError::Unsupported("span is not a source boundary"))
}

pub(super) fn packed_string(source: &str, range: StringRange) -> Option<&str> {
    let start = usize::try_from(range.start).ok()?;
    let length = usize::try_from(range.length).ok()?;
    source.get(start..start.checked_add(length)?)
}
