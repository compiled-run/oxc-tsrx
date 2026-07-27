//! The private binary graph format: a fixed header, interned keys, and fixed-width records the
//! receiving side can address without parsing.

use crate::{
    FlatTape, RecordIndex, SCHEMA_VERSION, StringRange, TapeBuildError, ValueKind, ValueRef,
};

use super::binary_records::{
    BINARY_INLINE_U32_TAG, BINARY_LIST_TAG, BINARY_OBJECT_TAG, BINARY_SCALAR_TAG, BinaryField,
    BinaryList, BinaryObject, BinaryPathNode, BinaryPathSegment, BinaryPending, BinaryValue,
    InternSlot, intern_hash, intern_slots,
};
use super::buffer::{BoundedString, push_json_string};
use super::common_keys::{BINARY_COMMON_KEY_FLAG, common_key, common_key_id};
use super::{
    PROGRAM_BINARY_TRANSFER_MAGIC, PROGRAM_BINARY_TRANSFER_VERSION, PROGRAM_TRANSFER_MAX_BYTES,
    ProgramBinaryTransfer,
};

const PROGRAM_BINARY_HEADER_WORDS: usize = 12;
const BINARY_UNUSED_RANGE: u32 = u32::MAX;

pub(super) struct BinaryProgramSerializer {
    tape: FlatTape,
    objects: Vec<BinaryObject>,
    fields: Vec<BinaryField>,
    lists: Vec<BinaryList>,
    values: Vec<BinaryValue>,
    keys: Vec<StringRange>,
    scalars: Vec<StringRange>,
    key_slots: Vec<InternSlot>,
    key_mask: usize,
    key_upper: usize,
    scalar_slots: Vec<InternSlot>,
    scalar_mask: usize,
    track_paths: bool,
    paths: Vec<BinaryPathNode>,
    fixes: Vec<Option<u32>>,
    pending: Vec<BinaryPending>,
}

impl BinaryProgramSerializer {
    pub(super) fn new(tape: FlatTape) -> Result<Self, TapeBuildError> {
        let object_count = tape.object_count();
        let list_count = tape.list_count();
        let field_count = tape.field_count();
        let value_count = tape.list_value_count();
        let container_count =
            object_count.checked_add(list_count).ok_or(TapeBuildError::CapacityOverflow)?;
        let scalar_upper = field_count
            .checked_add(value_count)
            .and_then(|value| value.checked_add(1))
            .ok_or(TapeBuildError::CapacityOverflow)?;
        let track_paths = tape.retained_transfer_layout()?.1;

        let mut objects = Vec::new();
        objects.try_reserve_exact(object_count).map_err(|_| TapeBuildError::CapacityOverflow)?;
        objects.resize(
            object_count,
            BinaryObject { field_start: BINARY_UNUSED_RANGE, field_count: 0 },
        );
        let mut fields = Vec::new();
        fields.try_reserve_exact(field_count).map_err(|_| TapeBuildError::CapacityOverflow)?;
        let mut lists = Vec::new();
        lists.try_reserve_exact(list_count).map_err(|_| TapeBuildError::CapacityOverflow)?;
        lists.resize(list_count, BinaryList { value_start: BINARY_UNUSED_RANGE, value_count: 0 });
        let mut values = Vec::new();
        values.try_reserve_exact(value_count).map_err(|_| TapeBuildError::CapacityOverflow)?;
        let keys = Vec::new();
        let mut scalars = Vec::new();
        scalars.try_reserve_exact(scalar_upper).map_err(|_| TapeBuildError::CapacityOverflow)?;
        let (key_slots, key_mask) = (Vec::new(), 0);
        let (scalar_slots, scalar_mask) = intern_slots(scalar_upper)?;
        let mut paths = Vec::new();
        if track_paths {
            paths
                .try_reserve_exact(container_count)
                .map_err(|_| TapeBuildError::CapacityOverflow)?;
        }
        let mut fixes = Vec::new();
        if track_paths {
            fixes.try_reserve_exact(object_count).map_err(|_| TapeBuildError::CapacityOverflow)?;
        }
        let mut pending = Vec::new();
        pending.try_reserve_exact(container_count).map_err(|_| TapeBuildError::CapacityOverflow)?;

        Ok(Self {
            tape,
            objects,
            fields,
            lists,
            values,
            keys,
            scalars,
            key_slots,
            key_mask,
            key_upper: field_count,
            scalar_slots,
            scalar_mask,
            track_paths,
            paths,
            fixes,
            pending,
        })
    }

    fn key_id(&mut self, range: StringRange) -> Result<u32, TapeBuildError> {
        let key = self.tape.checked_key_range(range).ok_or(TapeBuildError::InvalidRecordIndex)?;
        // Engine-origin ESTree field names are schema keys, never authored object-property names.
        // Reject the one setter-bearing JavaScript key before the trusted decoder sees it.
        if key == "__proto__" {
            return Err(TapeBuildError::InvalidRecordIndex);
        }
        if let Some(id) = common_key_id(key) {
            return Ok(BINARY_COMMON_KEY_FLAG | id);
        }
        if self.key_slots.is_empty() {
            let (slots, mask) = intern_slots(self.key_upper)?;
            self.key_slots = slots;
            self.key_mask = mask;
        }
        let hash = intern_hash(key);
        let mut slot_index = usize::try_from(hash).unwrap_or(0) & self.key_mask;
        loop {
            let slot = self.key_slots[slot_index];
            if slot.id == u32::MAX {
                let id =
                    u32::try_from(self.keys.len()).map_err(|_| TapeBuildError::CapacityOverflow)?;
                self.keys.try_reserve(1).map_err(|_| TapeBuildError::CapacityOverflow)?;
                self.keys.push(range);
                self.key_slots[slot_index] = InternSlot { hash, id };
                return Ok(id);
            }
            if slot.hash == hash {
                let existing = self
                    .keys
                    .get(usize::try_from(slot.id).map_err(|_| TapeBuildError::InvalidRecordIndex)?)
                    .copied()
                    .ok_or(TapeBuildError::InvalidRecordIndex)?;
                if self.tape.checked_key_range(existing) == Some(key) {
                    return Ok(slot.id);
                }
            }
            slot_index = (slot_index + 1) & self.key_mask;
        }
    }

    fn scalar_id(&mut self, value: ValueRef) -> Result<u32, TapeBuildError> {
        let range = value.as_scalar().ok_or(TapeBuildError::InvalidRecordIndex)?;
        let scalar = self.tape.scalar(value).ok_or(TapeBuildError::InvalidRecordIndex)?;
        if value.needs_fix() && scalar != "null" {
            return Err(TapeBuildError::InvalidRecordIndex);
        }
        if scalar.len() > 48 {
            let id =
                u32::try_from(self.scalars.len()).map_err(|_| TapeBuildError::CapacityOverflow)?;
            self.scalars.push(range);
            return Ok(id);
        }
        let hash = intern_hash(scalar);
        let mut slot_index = usize::try_from(hash).unwrap_or(0) & self.scalar_mask;
        loop {
            let slot = self.scalar_slots[slot_index];
            if slot.id == u32::MAX {
                let id = u32::try_from(self.scalars.len())
                    .map_err(|_| TapeBuildError::CapacityOverflow)?;
                self.scalars.push(range);
                self.scalar_slots[slot_index] = InternSlot { hash, id };
                return Ok(id);
            }
            if slot.hash == hash {
                let existing = self
                    .scalars
                    .get(usize::try_from(slot.id).map_err(|_| TapeBuildError::InvalidRecordIndex)?)
                    .copied()
                    .ok_or(TapeBuildError::InvalidRecordIndex)?;
                if self.tape.scalar(ValueRef::scalar(existing, false)) == Some(scalar) {
                    return Ok(slot.id);
                }
            }
            slot_index = (slot_index + 1) & self.scalar_mask;
        }
    }

    fn path_id(
        &mut self,
        parent: Option<u32>,
        segment: BinaryPathSegment,
    ) -> Result<u32, TapeBuildError> {
        let id = u32::try_from(self.paths.len()).map_err(|_| TapeBuildError::CapacityOverflow)?;
        self.paths.push(BinaryPathNode { parent, segment });
        Ok(id)
    }

    fn object_value(
        &mut self,
        source: RecordIndex,
        path: Option<(Option<u32>, BinaryPathSegment)>,
    ) -> Result<BinaryValue, TapeBuildError> {
        let source_index = usize::try_from(source.get().ok_or(TapeBuildError::InvalidRecordIndex)?)
            .map_err(|_| TapeBuildError::InvalidRecordIndex)?;
        self.objects.get(source_index).ok_or(TapeBuildError::InvalidRecordIndex)?;
        let wire = source.into_raw();
        let path = path.map(|(parent, segment)| self.path_id(parent, segment)).transpose()?;
        self.pending.push(BinaryPending::Object { source, wire, path });
        BinaryValue::new(BINARY_OBJECT_TAG, wire)
    }

    fn list_value(
        &mut self,
        source: RecordIndex,
        path: Option<(Option<u32>, BinaryPathSegment)>,
    ) -> Result<BinaryValue, TapeBuildError> {
        let source_index = usize::try_from(source.get().ok_or(TapeBuildError::InvalidRecordIndex)?)
            .map_err(|_| TapeBuildError::InvalidRecordIndex)?;
        self.lists.get(source_index).ok_or(TapeBuildError::InvalidRecordIndex)?;
        let wire = source.into_raw();
        let path = path.map(|(parent, segment)| self.path_id(parent, segment)).transpose()?;
        self.pending.push(BinaryPending::List { source, wire, path });
        BinaryValue::new(BINARY_LIST_TAG, wire)
    }

    fn encode_value(
        &mut self,
        value: ValueRef,
        path: Option<(Option<u32>, BinaryPathSegment)>,
    ) -> Result<BinaryValue, TapeBuildError> {
        match value.kind() {
            ValueKind::Missing => Err(TapeBuildError::InvalidRecordIndex),
            ValueKind::Scalar => value.as_inline_u32().map_or_else(
                || {
                    let index = self.scalar_id(value)?;
                    BinaryValue::new(BINARY_SCALAR_TAG, index)
                },
                |index| BinaryValue::new(BINARY_INLINE_U32_TAG, index),
            ),
            ValueKind::Object => self
                .object_value(value.as_object().ok_or(TapeBuildError::InvalidRecordIndex)?, path),
            ValueKind::List => {
                self.list_value(value.as_list().ok_or(TapeBuildError::InvalidRecordIndex)?, path)
            }
        }
    }

    fn encode_object(
        &mut self,
        source: RecordIndex,
        wire: u32,
        path: Option<u32>,
    ) -> Result<(), TapeBuildError> {
        let record = self.tape.take_object_record_for_transfer(source)?;
        let field_start =
            u32::try_from(self.fields.len()).map_err(|_| TapeBuildError::CapacityOverflow)?;
        let mut next = record.first_field;
        let mut fix_recorded = false;
        for _ in 0..record.field_count {
            let field_index =
                next.get().map(RecordIndex::new).ok_or(TapeBuildError::InvalidRecordIndex)?;
            let field = self.tape.take_field_record_for_transfer(field_index)?;
            let key = self.key_id(field.key)?;
            if field.value.needs_fix() {
                if !matches!(field.value.kind(), ValueKind::Scalar)
                    || self.tape.scalar(field.value) != Some("null")
                {
                    return Err(TapeBuildError::InvalidRecordIndex);
                }
                if !fix_recorded {
                    self.fixes.push(path);
                    fix_recorded = true;
                }
            }
            let child_path = self.track_paths.then_some((path, BinaryPathSegment::Key(key)));
            let value = self.encode_value(field.value, child_path)?;
            self.fields.push(BinaryField { key, value });
            next = field.next;
        }
        if !next.is_none() {
            return Err(TapeBuildError::InvalidRecordIndex);
        }
        let slot = self
            .objects
            .get_mut(usize::try_from(wire).map_err(|_| TapeBuildError::InvalidRecordIndex)?)
            .ok_or(TapeBuildError::InvalidRecordIndex)?;
        *slot = BinaryObject { field_start, field_count: record.field_count };
        Ok(())
    }

    fn encode_list(
        &mut self,
        source: RecordIndex,
        wire: u32,
        path: Option<u32>,
    ) -> Result<(), TapeBuildError> {
        let record = self.tape.take_list_record_for_transfer(source)?;
        let value_start =
            u32::try_from(self.values.len()).map_err(|_| TapeBuildError::CapacityOverflow)?;
        let mut next = record.first_value;
        for index in 0..record.length {
            let value_index =
                next.get().map(RecordIndex::new).ok_or(TapeBuildError::InvalidRecordIndex)?;
            let item = self.tape.take_list_value_record_for_transfer(value_index)?;
            if item.value.needs_fix() {
                return Err(TapeBuildError::InvalidRecordIndex);
            }
            let child_path = self.track_paths.then_some((path, BinaryPathSegment::Index(index)));
            let value = self.encode_value(item.value, child_path)?;
            self.values.push(value);
            next = item.next;
        }
        if !next.is_none() {
            return Err(TapeBuildError::InvalidRecordIndex);
        }
        let slot = self
            .lists
            .get_mut(usize::try_from(wire).map_err(|_| TapeBuildError::InvalidRecordIndex)?)
            .ok_or(TapeBuildError::InvalidRecordIndex)?;
        *slot = BinaryList { value_start, value_count: record.length };
        Ok(())
    }

    fn metadata(&self) -> Result<String, TapeBuildError> {
        let mut output = BoundedString::with_capacity(
            self.tape
                .scalar_storage()
                .len()
                .checked_add(64)
                .ok_or(TapeBuildError::CapacityOverflow)?,
        )?;
        output.push_str("[[")?;
        for (index, range) in self.keys.iter().copied().enumerate() {
            if index != 0 {
                output.push(',')?;
            }
            push_json_string(
                &mut output,
                self.tape.checked_key_range(range).ok_or(TapeBuildError::InvalidRecordIndex)?,
            )?;
        }
        output.push_str("],[")?;
        for (index, range) in self.scalars.iter().copied().enumerate() {
            if index != 0 {
                output.push(',')?;
            }
            output.push_str(
                self.tape
                    .scalar(ValueRef::scalar(range, false))
                    .ok_or(TapeBuildError::InvalidRecordIndex)?,
            )?;
        }
        output.push_str("],[")?;
        let mut scratch = Vec::new();
        scratch.try_reserve(self.paths.len()).map_err(|_| TapeBuildError::CapacityOverflow)?;
        for (fix_index, mut tail) in self.fixes.iter().copied().enumerate() {
            if fix_index != 0 {
                output.push(',')?;
            }
            scratch.clear();
            while let Some(path_index) = tail {
                let node = self
                    .paths
                    .get(
                        usize::try_from(path_index)
                            .map_err(|_| TapeBuildError::InvalidRecordIndex)?,
                    )
                    .ok_or(TapeBuildError::InvalidRecordIndex)?;
                scratch.push(node.segment);
                tail = node.parent;
            }
            output.push('[')?;
            for (segment_index, segment) in scratch.iter().rev().copied().enumerate() {
                if segment_index != 0 {
                    output.push(',')?;
                }
                match segment {
                    BinaryPathSegment::Key(key) => {
                        if key & BINARY_COMMON_KEY_FLAG != 0 {
                            push_json_string(
                                &mut output,
                                common_key(key & !BINARY_COMMON_KEY_FLAG)
                                    .ok_or(TapeBuildError::InvalidRecordIndex)?,
                            )?;
                            continue;
                        }
                        let range = self
                            .keys
                            .get(
                                usize::try_from(key)
                                    .map_err(|_| TapeBuildError::InvalidRecordIndex)?,
                            )
                            .copied()
                            .ok_or(TapeBuildError::InvalidRecordIndex)?;
                        push_json_string(
                            &mut output,
                            self.tape
                                .checked_key_range(range)
                                .ok_or(TapeBuildError::InvalidRecordIndex)?,
                        )?;
                    }
                    BinaryPathSegment::Index(index) => output.push_u32(index)?,
                }
            }
            output.push(']')?;
        }
        output.push_str("]]")?;
        Ok(output.into_string())
    }

    fn words(&self, root: BinaryValue) -> Result<Vec<u32>, TapeBuildError> {
        let word_count = PROGRAM_BINARY_HEADER_WORDS
            .checked_add(self.objects.len().checked_mul(2).ok_or(TapeBuildError::CapacityOverflow)?)
            .and_then(|value| value.checked_add(self.fields.len().checked_mul(2)?))
            .and_then(|value| value.checked_add(self.lists.len().checked_mul(2)?))
            .and_then(|value| value.checked_add(self.values.len()))
            .ok_or(TapeBuildError::CapacityOverflow)?;
        let mut words = Vec::new();
        words.try_reserve_exact(word_count).map_err(|_| TapeBuildError::CapacityOverflow)?;
        words.extend_from_slice(&[
            PROGRAM_BINARY_TRANSFER_MAGIC,
            PROGRAM_BINARY_TRANSFER_VERSION,
            u32::try_from(self.objects.len()).map_err(|_| TapeBuildError::CapacityOverflow)?,
            u32::try_from(self.fields.len()).map_err(|_| TapeBuildError::CapacityOverflow)?,
            u32::try_from(self.lists.len()).map_err(|_| TapeBuildError::CapacityOverflow)?,
            u32::try_from(self.values.len()).map_err(|_| TapeBuildError::CapacityOverflow)?,
            root.tag(),
            root.index(),
            u32::try_from(self.keys.len()).map_err(|_| TapeBuildError::CapacityOverflow)?,
            u32::try_from(self.scalars.len()).map_err(|_| TapeBuildError::CapacityOverflow)?,
            u32::try_from(self.fixes.len()).map_err(|_| TapeBuildError::CapacityOverflow)?,
            0,
        ]);
        for object in &self.objects {
            words.extend_from_slice(&[object.field_start, object.field_count]);
        }
        for field in &self.fields {
            words.extend_from_slice(&[field.key, field.value.0]);
        }
        for list in &self.lists {
            words.extend_from_slice(&[list.value_start, list.value_count]);
        }
        for value in &self.values {
            words.push(value.0);
        }
        debug_assert_eq!(words.len(), word_count);
        Ok(words)
    }

    pub(super) fn run(mut self) -> Result<ProgramBinaryTransfer, TapeBuildError> {
        if self.tape.schema_version() != SCHEMA_VERSION || self.tape.root().needs_fix() {
            return Err(TapeBuildError::InvalidRecordIndex);
        }
        let root = self.encode_value(self.tape.root(), None)?;
        if root.tag() != BINARY_OBJECT_TAG {
            return Err(TapeBuildError::InvalidRecordIndex);
        }
        let mut cursor = 0_usize;
        while let Some(pending) = self.pending.get(cursor).copied() {
            cursor = cursor.checked_add(1).ok_or(TapeBuildError::CapacityOverflow)?;
            match pending {
                BinaryPending::Object { source, wire, path } => {
                    self.encode_object(source, wire, path)?;
                }
                BinaryPending::List { source, wire, path } => {
                    self.encode_list(source, wire, path)?;
                }
            }
        }
        let metadata = self.metadata()?;
        let words = self.words(root)?;
        let byte_count = words
            .len()
            .checked_mul(size_of::<u32>())
            .and_then(|value| value.checked_add(metadata.len()))
            .ok_or(TapeBuildError::CapacityOverflow)?;
        if byte_count > PROGRAM_TRANSFER_MAX_BYTES {
            return Err(TapeBuildError::CapacityOverflow);
        }
        Ok(ProgramBinaryTransfer { metadata, words })
    }
}
