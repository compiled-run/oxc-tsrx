//! The explicit work stack both serializers share, so a deeply nested Program never recurses on
//! the host stack.

use crate::{FlatTape, RecordIndex, TapeBuildError, ValueRef};

use super::buffer::{BoundedString, push_json_string};

#[derive(Clone, Copy)]
pub(super) enum PathSegment<'a> {
    Key(&'a str),
    Index(u32),
}

pub(super) enum Work {
    Value {
        value: ValueRef,
        fix_owner: Option<RecordIndex>,
    },
    Object {
        object: RecordIndex,
        next: RecordIndex,
        remaining: u32,
        first: bool,
        fix_recorded: bool,
    },
    List {
        next: RecordIndex,
        remaining: u32,
        index: u32,
        first: bool,
    },
    PopPath,
}

#[derive(Clone, Copy)]
pub(super) enum ContainerWork {
    Object { next: RecordIndex, remaining: u32, first: bool },
    List { next: RecordIndex, remaining: u32, first: bool },
}

pub(super) fn zero_flags(length: usize) -> Result<Vec<u8>, TapeBuildError> {
    let mut flags = Vec::new();
    flags.try_reserve_exact(length).map_err(|_| TapeBuildError::CapacityOverflow)?;
    flags.resize(length, 0);
    Ok(flags)
}

pub(super) fn write_fix_path(
    fixes: &mut BoundedString,
    path: &[PathSegment<'_>],
    first: &mut bool,
) -> Result<(), TapeBuildError> {
    if *first {
        *first = false;
    } else {
        fixes.push(',')?;
    }
    fixes.push('[')?;
    for (index, segment) in path.iter().copied().enumerate() {
        if index != 0 {
            fixes.push(',')?;
        }
        match segment {
            PathSegment::Key(key) => push_json_string(fixes, key)?,
            PathSegment::Index(index) => fixes.push_u32(index)?,
        }
    }
    fixes.push(']')
}

pub(super) fn transfer_layout(tape: &FlatTape) -> Result<(usize, bool, bool), TapeBuildError> {
    let (key_bytes, inline_u32_bytes, track_paths, keys_are_json_safe) =
        tape.transfer_field_summary()?;
    let punctuation = tape
        .field_count()
        .checked_mul(4)
        .and_then(|value| value.checked_add(tape.list_value_count().saturating_mul(2)))
        .and_then(|value| value.checked_add(tape.object_count().saturating_mul(2)))
        .and_then(|value| value.checked_add(tape.list_count().saturating_mul(2)))
        .ok_or(TapeBuildError::CapacityOverflow)?;
    let capacity = tape
        .scalar_storage()
        .len()
        .checked_add(key_bytes)
        .and_then(|value| value.checked_add(inline_u32_bytes))
        .and_then(|value| value.checked_add(punctuation))
        .and_then(|value| value.checked_add(43))
        .ok_or(TapeBuildError::CapacityOverflow)?;
    Ok((capacity, track_paths, keys_are_json_safe))
}
