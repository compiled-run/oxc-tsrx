use crate::{RecordIndex, StringRange, TapeBuildError};

pub(super) fn push_record<T>(
    records: &mut Vec<T>,
    record: T,
) -> Result<RecordIndex, TapeBuildError> {
    let index = u32::try_from(records.len()).map_err(|_| TapeBuildError::CapacityOverflow)?;
    records.push(record);
    Ok(RecordIndex::new(index))
}

pub(super) fn push_string(
    storage: &mut String,
    value: &str,
) -> Result<StringRange, TapeBuildError> {
    let start = u32::try_from(storage.len()).map_err(|_| TapeBuildError::CapacityOverflow)?;
    let length = u32::try_from(value.len()).map_err(|_| TapeBuildError::CapacityOverflow)?;
    start.checked_add(length).ok_or(TapeBuildError::CapacityOverflow)?;
    storage.push_str(value);
    Ok(StringRange::new(start, length))
}

#[expect(
    clippy::inline_always,
    reason = "a single-expression tape lookup that must compile down to the field read it wraps"
)]
#[inline(always)]
pub(super) fn slice_range(storage: &str, range: StringRange) -> Option<&str> {
    let start = range.start as usize;
    let end = start.checked_add(range.length as usize)?;
    storage.get(start..end)
}

pub(super) fn index_usize(index: RecordIndex) -> usize {
    index.into_raw() as usize
}
