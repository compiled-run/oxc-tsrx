use tsrx_tape_schema::{FlatTape, RecordIndex, ValueKind, ValueRef};

use crate::TsrxParseError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ParentSlot {
    Field(RecordIndex),
    ListValue(RecordIndex),
}

pub(super) struct ParentIndex {
    object_parents: Vec<Option<ParentSlot>>,
    list_parents: Vec<Option<ParentSlot>>,
    field_owners: Vec<RecordIndex>,
    list_value_owners: Vec<RecordIndex>,
}

impl ParentIndex {
    pub(super) fn build<F>(tape: &FlatTape, mut visit_object: F) -> Result<Self, TsrxParseError>
    where
        F: FnMut(RecordIndex, Option<&str>, Option<u32>) -> Result<(), TsrxParseError>,
    {
        let mut index = Self {
            object_parents: vec![None; tape.object_count()],
            list_parents: vec![None; tape.list_count()],
            field_owners: vec![RecordIndex::NONE; tape.field_count()],
            list_value_owners: vec![RecordIndex::NONE; tape.list_value_count()],
        };
        for raw in 0..tape.object_count() {
            let object = record_index(raw)?;
            let mut kind = None;
            let mut start = None;
            let mut needs_outline = true;
            for (field, record) in tape.fields_indexed(object) {
                if needs_outline {
                    match tape.key(record) {
                        "type" => kind = tape.scalar(record.value),
                        "start" => start = tape.scalar_u32(record.value),
                        _ => {}
                    }
                    needs_outline = kind.is_none() || start.is_none();
                }
                let field_index = index_of(field)?;
                if !index.field_owners[field_index].is_none() {
                    return Err(TsrxParseError::Unsupported("shared object field record"));
                }
                index.field_owners[field_index] = object;
                index.record_parent(record.value, ParentSlot::Field(field))?;
            }
            visit_object(object, kind, start)?;
        }
        for raw in 0..tape.list_count() {
            let list = record_index(raw)?;
            for (entry, value) in tape.values_indexed(list) {
                let entry_index = index_of(entry)?;
                if !index.list_value_owners[entry_index].is_none() {
                    return Err(TsrxParseError::Unsupported("shared list-value record"));
                }
                index.list_value_owners[entry_index] = list;
                index.record_parent(value, ParentSlot::ListValue(entry))?;
            }
        }
        Ok(index)
    }

    fn record_parent(&mut self, value: ValueRef, parent: ParentSlot) -> Result<(), TsrxParseError> {
        let target = match value.kind() {
            ValueKind::Object => {
                let object = value
                    .as_object()
                    .ok_or(TsrxParseError::Unsupported("invalid object value"))?;
                self.object_parents.get_mut(index_of(object)?)
            }
            ValueKind::List => {
                let list = value
                    .as_list()
                    .ok_or(TsrxParseError::Unsupported("invalid list value"))?;
                self.list_parents.get_mut(index_of(list)?)
            }
            ValueKind::Missing | ValueKind::Scalar => return Ok(()),
        }
        .ok_or(TsrxParseError::Unsupported("parent index outside tape"))?;
        if target.replace(parent).is_some() {
            return Err(TsrxParseError::Unsupported(
                "shared object or list in projected AST",
            ));
        }
        Ok(())
    }

    pub(super) fn parent_slot(&self, value: ValueRef) -> Option<ParentSlot> {
        match value.kind() {
            ValueKind::Object => self
                .object_parents
                .get(index_of(value.as_object()?).ok()?)
                .copied()
                .flatten(),
            ValueKind::List => self
                .list_parents
                .get(index_of(value.as_list()?).ok()?)
                .copied()
                .flatten(),
            ValueKind::Missing | ValueKind::Scalar => None,
        }
    }

    pub(super) fn parent_container(&self, value: ValueRef) -> Option<ValueRef> {
        match self.parent_slot(value)? {
            ParentSlot::Field(field) => self
                .field_owners
                .get(index_of(field).ok()?)
                .copied()
                .filter(|owner| !owner.is_none())
                .map(ValueRef::object),
            ParentSlot::ListValue(entry) => self
                .list_value_owners
                .get(index_of(entry).ok()?)
                .copied()
                .filter(|owner| !owner.is_none())
                .map(ValueRef::list),
        }
    }

    pub(super) fn replace(
        tape: &mut FlatTape,
        slot: ParentSlot,
        value: ValueRef,
    ) -> Result<(), TsrxParseError> {
        match slot {
            ParentSlot::Field(field) => tape.set_field_value(field, value)?,
            ParentSlot::ListValue(entry) => tape.set_list_value(entry, value)?,
        }
        Ok(())
    }
}

fn record_index(index: usize) -> Result<RecordIndex, TsrxParseError> {
    u32::try_from(index)
        .map(RecordIndex::new)
        .map_err(|_| TsrxParseError::Unsupported("tape index above 4 GiB"))
}

fn index_of(index: RecordIndex) -> Result<usize, TsrxParseError> {
    let raw = index
        .get()
        .ok_or(TsrxParseError::Unsupported("missing tape index"))?;
    usize::try_from(raw).map_err(|_| TsrxParseError::Unsupported("tape index exceeds host usize"))
}
