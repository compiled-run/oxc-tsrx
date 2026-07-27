use crate::{RecordIndex, TapeBuildError};

use super::FlatTape;
use super::bounds::{index_usize, push_record, push_string, slice_range};
use super::record::{FieldRecord, ListRecord, ListValueRecord, ObjectRecord};
use super::value::{ValueKind, ValueRef};

pub(super) struct Reachability {
    pub(super) objects: Vec<bool>,
    pub(super) fields: Vec<bool>,
    pub(super) lists: Vec<bool>,
    pub(super) values: Vec<bool>,
}

impl Reachability {
    pub(super) fn collect(tape: &FlatTape) -> Result<Self, TapeBuildError> {
        let mut reachable = Self {
            objects: vec![false; tape.objects.len()],
            fields: vec![false; tape.fields.len()],
            lists: vec![false; tape.lists.len()],
            values: vec![false; tape.values.len()],
        };
        let mut pending = vec![tape.root];
        while let Some(value) = pending.pop() {
            match value.kind() {
                ValueKind::Missing | ValueKind::Scalar => {}
                ValueKind::Object => reachable.visit_object(tape, value, &mut pending)?,
                ValueKind::List => reachable.visit_list(tape, value, &mut pending)?,
            }
        }
        Ok(reachable)
    }

    fn visit_object(
        &mut self,
        tape: &FlatTape,
        value: ValueRef,
        pending: &mut Vec<ValueRef>,
    ) -> Result<(), TapeBuildError> {
        let object = value.as_object().ok_or(TapeBuildError::InvalidRecordIndex)?;
        let object_index = index_usize(object);
        let record = *tape.objects.get(object_index).ok_or(TapeBuildError::InvalidRecordIndex)?;
        if self.objects[object_index] {
            return Ok(());
        }
        self.objects[object_index] = true;
        let mut next = record.first_field;
        let mut count = 0_u32;
        while let Some(raw) = next.get() {
            let field_index =
                usize::try_from(raw).map_err(|_| TapeBuildError::InvalidRecordIndex)?;
            let field = *tape.fields.get(field_index).ok_or(TapeBuildError::InvalidRecordIndex)?;
            if self.fields[field_index] {
                return Err(TapeBuildError::InvalidRecordIndex);
            }
            self.fields[field_index] = true;
            pending.push(field.value);
            count = count.checked_add(1).ok_or(TapeBuildError::CapacityOverflow)?;
            next = field.next;
        }
        (count == record.field_count).then_some(()).ok_or(TapeBuildError::InvalidRecordIndex)
    }

    fn visit_list(
        &mut self,
        tape: &FlatTape,
        value: ValueRef,
        pending: &mut Vec<ValueRef>,
    ) -> Result<(), TapeBuildError> {
        let list = value.as_list().ok_or(TapeBuildError::InvalidRecordIndex)?;
        let list_index = index_usize(list);
        let record = *tape.lists.get(list_index).ok_or(TapeBuildError::InvalidRecordIndex)?;
        if self.lists[list_index] {
            return Ok(());
        }
        self.lists[list_index] = true;
        let mut next = record.first_value;
        let mut count = 0_u32;
        while let Some(raw) = next.get() {
            let value_index =
                usize::try_from(raw).map_err(|_| TapeBuildError::InvalidRecordIndex)?;
            let item = *tape.values.get(value_index).ok_or(TapeBuildError::InvalidRecordIndex)?;
            if self.values[value_index] {
                return Err(TapeBuildError::InvalidRecordIndex);
            }
            self.values[value_index] = true;
            pending.push(item.value);
            count = count.checked_add(1).ok_or(TapeBuildError::CapacityOverflow)?;
            next = item.next;
        }
        (count == record.length).then_some(()).ok_or(TapeBuildError::InvalidRecordIndex)
    }
}

pub(super) fn count_reachable(records: &[bool]) -> usize {
    records.iter().filter(|&&reachable| reachable).count()
}

pub(super) fn build_index_map(reachable: &[bool]) -> Result<Vec<RecordIndex>, TapeBuildError> {
    let mut map = vec![RecordIndex::NONE; reachable.len()];
    let mut next = 0_u32;
    for (index, is_reachable) in reachable.iter().copied().enumerate() {
        if is_reachable {
            map[index] = RecordIndex::new(next);
            next = next.checked_add(1).ok_or(TapeBuildError::CapacityOverflow)?;
        }
    }
    Ok(map)
}

pub(super) fn compact_objects(
    tape: &FlatTape,
    reachable: &Reachability,
    object_map: &[RecordIndex],
    list_map: &[RecordIndex],
    scalars: &mut String,
) -> Result<(Vec<ObjectRecord>, Vec<FieldRecord>), TapeBuildError> {
    let mut objects = vec![ObjectRecord::default(); count_reachable(&reachable.objects)];
    let mut fields = Vec::with_capacity(count_reachable(&reachable.fields));
    for (old_index, is_reachable) in reachable.objects.iter().copied().enumerate() {
        if !is_reachable {
            continue;
        }
        let old = tape.objects[old_index];
        let mut next = old.first_field;
        let mut first = RecordIndex::NONE;
        let mut previous = RecordIndex::NONE;
        let mut count = 0_u32;
        while let Some(raw) = next.get() {
            let field_index =
                usize::try_from(raw).map_err(|_| TapeBuildError::InvalidRecordIndex)?;
            let field = tape.fields[field_index];
            let value = compact_value(field.value, object_map, list_map, &tape.scalars, scalars)?;
            let new_field = push_record(
                &mut fields,
                FieldRecord { key: field.key, value, next: RecordIndex::NONE },
            )?;
            if first.is_none() {
                first = new_field;
            } else {
                fields[index_usize(previous)].next = new_field;
            }
            previous = new_field;
            count += 1;
            next = field.next;
        }
        objects[index_usize(object_map[old_index])] =
            ObjectRecord { first_field: first, field_count: count };
    }
    Ok((objects, fields))
}

pub(super) fn compact_lists(
    tape: &FlatTape,
    reachable: &Reachability,
    object_map: &[RecordIndex],
    list_map: &[RecordIndex],
    scalars: &mut String,
) -> Result<(Vec<ListRecord>, Vec<ListValueRecord>), TapeBuildError> {
    let mut lists = vec![ListRecord::default(); count_reachable(&reachable.lists)];
    let mut values = Vec::with_capacity(count_reachable(&reachable.values));
    for (old_index, is_reachable) in reachable.lists.iter().copied().enumerate() {
        if !is_reachable {
            continue;
        }
        let old = tape.lists[old_index];
        let mut next = old.first_value;
        let mut first = RecordIndex::NONE;
        let mut previous = RecordIndex::NONE;
        let mut count = 0_u32;
        while let Some(raw) = next.get() {
            let value_index =
                usize::try_from(raw).map_err(|_| TapeBuildError::InvalidRecordIndex)?;
            let item = tape.values[value_index];
            let value = compact_value(item.value, object_map, list_map, &tape.scalars, scalars)?;
            let new_value =
                push_record(&mut values, ListValueRecord { value, next: RecordIndex::NONE })?;
            if first.is_none() {
                first = new_value;
            } else {
                values[index_usize(previous)].next = new_value;
            }
            previous = new_value;
            count += 1;
            next = item.next;
        }
        lists[index_usize(list_map[old_index])] = ListRecord { first_value: first, length: count };
    }
    Ok((lists, values))
}

pub(super) fn compact_value(
    value: ValueRef,
    object_map: &[RecordIndex],
    list_map: &[RecordIndex],
    old_scalars: &str,
    new_scalars: &mut String,
) -> Result<ValueRef, TapeBuildError> {
    match value.kind() {
        ValueKind::Missing => Ok(ValueRef::MISSING),
        ValueKind::Scalar => {
            if let Some(value) = value.as_inline_u32() {
                return Ok(ValueRef::inline_u32(value));
            }
            let scalar = slice_range(
                old_scalars,
                value.as_scalar().ok_or(TapeBuildError::InvalidRecordIndex)?,
            )
            .ok_or(TapeBuildError::InvalidRecordIndex)?;
            let range = push_string(new_scalars, scalar)?;
            Ok(ValueRef::scalar(range, value.needs_fix()))
        }
        ValueKind::Object => {
            let old = value.as_object().ok_or(TapeBuildError::InvalidRecordIndex)?;
            let mapped = object_map
                .get(index_usize(old))
                .copied()
                .filter(|index| !index.is_none())
                .ok_or(TapeBuildError::InvalidRecordIndex)?;
            Ok(ValueRef::object(mapped))
        }
        ValueKind::List => {
            let old = value.as_list().ok_or(TapeBuildError::InvalidRecordIndex)?;
            let mapped = list_map
                .get(index_usize(old))
                .copied()
                .filter(|index| !index.is_none())
                .ok_or(TapeBuildError::InvalidRecordIndex)?;
            Ok(ValueRef::list(mapped))
        }
    }
}
