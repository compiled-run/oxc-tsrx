use crate::{ListRange, RecordIndex, StringRange, TapeBuildError};

pub(super) fn table_has_room<T>(records: &[T]) -> Result<(), TapeBuildError> {
    checked_record_index(records.len()).map(|_| ())
}

pub(super) fn push_record<T>(
    records: &mut Vec<T>,
    record: T,
) -> Result<RecordIndex, TapeBuildError> {
    let index = checked_record_index(records.len())?;
    records.push(record);
    Ok(index)
}

pub(super) fn append_records<T, I>(
    records: &mut Vec<T>,
    values: I,
) -> Result<ListRange, TapeBuildError>
where
    I: IntoIterator<Item = T>,
{
    let start = records.len();
    for value in values {
        if let Err(error) = push_record(records, value) {
            records.truncate(start);
            return Err(error);
        }
    }
    list_range(start, records.len() - start)
}

pub(super) fn begin_direct_range<T>(records: &[T]) -> Result<u32, TapeBuildError> {
    checked_range_cursor(records.len())
}

pub(super) fn finish_direct_range<T>(
    records: &[T],
    start: u32,
    expected_length: u32,
) -> Result<ListRange, TapeBuildError> {
    checked_direct_range(start, expected_length, checked_range_cursor(records.len())?)
}

pub(super) fn checked_direct_range(
    start: u32,
    expected_length: u32,
    actual_end: u32,
) -> Result<ListRange, TapeBuildError> {
    let expected_end =
        start.checked_add(expected_length).ok_or(TapeBuildError::CapacityOverflow)?;
    if actual_end != expected_end {
        return Err(TapeBuildError::InvalidRecordIndex);
    }
    Ok(ListRange::new(start, expected_length))
}

pub(super) fn checked_record_index(length: usize) -> Result<RecordIndex, TapeBuildError> {
    let index = checked_range_cursor(length)?;
    if index == RecordIndex::NONE.into_raw() {
        return Err(TapeBuildError::CapacityOverflow);
    }
    Ok(RecordIndex::new(index))
}

pub(super) fn checked_range_cursor(length: usize) -> Result<u32, TapeBuildError> {
    u32::try_from(length).map_err(|_| TapeBuildError::CapacityOverflow)
}

pub(super) fn list_range(start: usize, length: usize) -> Result<ListRange, TapeBuildError> {
    let start = u32::try_from(start).map_err(|_| TapeBuildError::CapacityOverflow)?;
    let length = u32::try_from(length).map_err(|_| TapeBuildError::CapacityOverflow)?;
    start.checked_add(length).ok_or(TapeBuildError::CapacityOverflow)?;
    Ok(ListRange::new(start, length))
}

pub(super) fn string_range(start: usize, length: usize) -> Result<StringRange, TapeBuildError> {
    let start = u32::try_from(start).map_err(|_| TapeBuildError::CapacityOverflow)?;
    let length = u32::try_from(length).map_err(|_| TapeBuildError::CapacityOverflow)?;
    start.checked_add(length).ok_or(TapeBuildError::CapacityOverflow)?;
    Ok(StringRange::new(start, length))
}

pub(super) fn slice_range(storage: &str, range: StringRange) -> Option<&str> {
    let start = usize::try_from(range.start).ok()?;
    let length = usize::try_from(range.length).ok()?;
    storage.get(start..start.checked_add(length)?)
}

pub(super) fn range_slice<T>(records: &[T], range: ListRange) -> Option<&[T]> {
    let start = usize::try_from(range.start).ok()?;
    let length = usize::try_from(range.length).ok()?;
    records.get(start..start.checked_add(length)?)
}

pub(super) fn index_usize(index: RecordIndex) -> usize {
    index.into_raw() as usize
}
