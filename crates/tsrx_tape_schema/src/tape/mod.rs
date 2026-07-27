use std::{error::Error, fmt};

use crate::{RecordIndex, SCHEMA_VERSION, StringRange};

const FIX_SCALAR_TAG: u32 = u32::MAX - 4;
const MISSING_TAG: u32 = u32::MAX - 3;
const OBJECT_TAG: u32 = u32::MAX - 2;
const LIST_TAG: u32 = u32::MAX - 1;
const INLINE_U32_TAG: u32 = u32::MAX;
const FIX_SCALAR_LENGTH: u32 = 4;

/// The storage lane selected by one compact tape value.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ValueKind {
    Missing = 0,
    Scalar = 1,
    Object = 2,
    List = 3,
}

/// Fixed-width reference to a scalar, object, or list table entry.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ValueRef {
    index: u32,
    length_or_tag: u32,
}

impl ValueRef {
    pub const MISSING: Self = Self { index: u32::MAX, length_or_tag: MISSING_TAG };

    /// Creates a scalar reference, optionally tagged for JavaScript special-value repair.
    ///
    /// # Panics
    ///
    /// Panics if `range.length` overlaps the reserved tag space or a repair scalar is not the
    /// fixed JSON `null` spelling.
    #[must_use]
    pub const fn scalar(range: StringRange, needs_fix: bool) -> Self {
        assert!(range.length < FIX_SCALAR_TAG);
        assert!(!needs_fix || range.length == FIX_SCALAR_LENGTH);
        Self {
            index: range.start,
            length_or_tag: if needs_fix { FIX_SCALAR_TAG } else { range.length },
        }
    }

    /// Stores one exact JSON unsigned integer without formatting scalar text first.
    #[must_use]
    pub const fn inline_u32(value: u32) -> Self {
        Self { index: value, length_or_tag: INLINE_U32_TAG }
    }

    #[must_use]
    pub const fn object(index: RecordIndex) -> Self {
        Self { index: index.into_raw(), length_or_tag: OBJECT_TAG }
    }

    #[must_use]
    pub const fn list(index: RecordIndex) -> Self {
        Self { index: index.into_raw(), length_or_tag: LIST_TAG }
    }

    #[must_use]
    pub const fn kind(self) -> ValueKind {
        match self.length_or_tag {
            MISSING_TAG => ValueKind::Missing,
            OBJECT_TAG => ValueKind::Object,
            LIST_TAG => ValueKind::List,
            _ => ValueKind::Scalar,
        }
    }

    #[must_use]
    pub const fn as_scalar(self) -> Option<StringRange> {
        match self.length_or_tag {
            MISSING_TAG | OBJECT_TAG | LIST_TAG | INLINE_U32_TAG => None,
            FIX_SCALAR_TAG => Some(StringRange::new(self.index, FIX_SCALAR_LENGTH)),
            length => Some(StringRange::new(self.index, length)),
        }
    }

    /// Returns the exact unsigned integer stored in the inline scalar lane.
    #[must_use]
    pub const fn as_inline_u32(self) -> Option<u32> {
        if self.length_or_tag == INLINE_U32_TAG { Some(self.index) } else { None }
    }

    #[must_use]
    pub const fn as_object(self) -> Option<RecordIndex> {
        if self.length_or_tag == OBJECT_TAG { Some(RecordIndex::new(self.index)) } else { None }
    }

    #[must_use]
    pub const fn as_list(self) -> Option<RecordIndex> {
        if self.length_or_tag == LIST_TAG { Some(RecordIndex::new(self.index)) } else { None }
    }

    #[must_use]
    pub const fn needs_fix(self) -> bool {
        self.length_or_tag == FIX_SCALAR_TAG
    }
}

impl Default for ValueRef {
    fn default() -> Self {
        Self::MISSING
    }
}

/// One serialized object and the head of its field chain.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ObjectRecord {
    pub first_field: RecordIndex,
    pub field_count: u32,
}

/// One serialized object field. Fields retain source order through `next`.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FieldRecord {
    pub key: StringRange,
    pub value: ValueRef,
    pub next: RecordIndex,
}

/// One serialized sequence and the head of its value chain.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ListRecord {
    pub first_value: RecordIndex,
    pub length: u32,
}

/// One serialized sequence entry. Entries retain source order through `next`.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ListValueRecord {
    pub value: ValueRef,
    pub next: RecordIndex,
}

/// One value to insert after an already indexed member of a tape list.
///
/// Batching these operations lets the tape validate every `after` owner in one flat pass instead
/// of rescanning the same list once per insertion.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListValueInsertion {
    pub list: RecordIndex,
    pub after: RecordIndex,
    pub value: ValueRef,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TapeBuildError {
    CapacityOverflow,
    InvalidRecordIndex,
}

impl fmt::Display for TapeBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CapacityOverflow => formatter.write_str("TSRX tape exceeds its 32-bit limit"),
            Self::InvalidRecordIndex => formatter.write_str("TSRX tape contains an invalid index"),
        }
    }
}

impl Error for TapeBuildError {}

/// Owned, revision-neutral flat `ESTree` tape.
#[derive(Debug, Default)]
pub struct FlatTape {
    schema_version: u16,
    root: ValueRef,
    objects: Vec<ObjectRecord>,
    fields: Vec<FieldRecord>,
    lists: Vec<ListRecord>,
    values: Vec<ListValueRecord>,
    keys: String,
    scalars: String,
    transfer_has_fix: bool,
    transfer_key_bytes_upper: usize,
    transfer_inline_u32_count_upper: usize,
    transfer_keys_require_escape: bool,
}

impl FlatTape {
    /// Reserves record tables for a serializer with bounded source-derived estimates.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] when a requested table cannot be allocated.
    pub fn reserve_records(
        &mut self,
        objects: usize,
        fields: usize,
        lists: usize,
        values: usize,
    ) -> Result<(), TapeBuildError> {
        self.objects.try_reserve_exact(objects).map_err(|_| TapeBuildError::CapacityOverflow)?;
        self.fields.try_reserve_exact(fields).map_err(|_| TapeBuildError::CapacityOverflow)?;
        self.lists.try_reserve_exact(lists).map_err(|_| TapeBuildError::CapacityOverflow)?;
        self.values.try_reserve_exact(values).map_err(|_| TapeBuildError::CapacityOverflow)
    }

    #[must_use]
    pub const fn schema_version(&self) -> u16 {
        self.schema_version
    }

    #[must_use]
    pub const fn root(&self) -> ValueRef {
        self.root
    }

    /// Number of objects in the flat table.
    #[must_use]
    pub fn object_count(&self) -> usize {
        self.objects.len()
    }

    /// Number of lists in the flat table.
    #[must_use]
    pub fn list_count(&self) -> usize {
        self.lists.len()
    }

    /// Number of object fields in the flat table.
    #[must_use]
    pub fn field_count(&self) -> usize {
        self.fields.len()
    }

    /// Number of list entries in the flat table.
    #[must_use]
    pub fn list_value_count(&self) -> usize {
        self.values.len()
    }

    pub fn set_root(&mut self, root: ValueRef) {
        self.schema_version = SCHEMA_VERSION;
        self.transfer_has_fix |= root.needs_fix();
        self.transfer_inline_u32_count_upper = self
            .transfer_inline_u32_count_upper
            .saturating_add(usize::from(root.as_inline_u32().is_some()));
        self.root = root;
    }

    #[must_use]
    pub fn fields(&self, object: RecordIndex) -> FieldIter<'_> {
        let first = self
            .objects
            .get(index_usize(object))
            .map_or(RecordIndex::NONE, |record| record.first_field);
        FieldIter { tape: self, next: first }
    }

    /// Iterates fields together with their stable table indices.
    #[must_use]
    pub fn fields_indexed(&self, object: RecordIndex) -> IndexedFieldIter<'_> {
        let first = self
            .objects
            .get(index_usize(object))
            .map_or(RecordIndex::NONE, |record| record.first_field);
        IndexedFieldIter { tape: self, next: first }
    }

    #[must_use]
    pub fn values(&self, list: RecordIndex) -> ValueIter<'_> {
        let first = self
            .lists
            .get(index_usize(list))
            .map_or(RecordIndex::NONE, |record| record.first_value);
        ValueIter { tape: self, next: first }
    }

    /// Returns the stored list length in constant time.
    #[must_use]
    pub fn list_length(&self, list: RecordIndex) -> Option<u32> {
        self.lists.get(index_usize(list)).map(|record| record.length)
    }

    /// Iterates list values together with their stable table indices.
    #[must_use]
    pub fn values_indexed(&self, list: RecordIndex) -> IndexedValueIter<'_> {
        let first = self
            .lists
            .get(index_usize(list))
            .map_or(RecordIndex::NONE, |record| record.first_value);
        IndexedValueIter { tape: self, next: first }
    }

    /// Returns the first entry in a list in constant time.
    ///
    /// A valid empty list returns [`Some`] containing [`RecordIndex::NONE`].
    #[must_use]
    pub fn list_first_value(&self, list: RecordIndex) -> Option<RecordIndex> {
        self.lists.get(index_usize(list)).map(|record| record.first_value)
    }

    /// Returns the value stored by one list entry in constant time.
    #[must_use]
    pub fn list_value(&self, entry: RecordIndex) -> Option<ValueRef> {
        self.values.get(index_usize(entry)).map(|record| record.value)
    }

    /// Returns the successor stored by one list entry in constant time.
    ///
    /// A valid final entry returns [`Some`] containing [`RecordIndex::NONE`].
    #[must_use]
    pub fn list_value_next(&self, entry: RecordIndex) -> Option<RecordIndex> {
        self.values.get(index_usize(entry)).map(|record| record.next)
    }

    #[expect(
        clippy::inline_always,
        reason = "a single-expression tape lookup that must compile down to the field read it wraps"
    )]
    #[inline(always)]
    pub(crate) fn object_record(&self, object: RecordIndex) -> Option<ObjectRecord> {
        self.objects.get(index_usize(object)).copied()
    }

    #[expect(
        clippy::inline_always,
        reason = "a single-expression tape lookup that must compile down to the field read it wraps"
    )]
    #[inline(always)]
    pub(crate) fn field_record(&self, field: RecordIndex) -> Option<FieldRecord> {
        self.fields.get(index_usize(field)).copied()
    }

    #[expect(
        clippy::inline_always,
        reason = "a single-expression tape lookup that must compile down to the field read it wraps"
    )]
    #[inline(always)]
    pub(crate) fn list_record(&self, list: RecordIndex) -> Option<ListRecord> {
        self.lists.get(index_usize(list)).copied()
    }

    #[expect(
        clippy::inline_always,
        reason = "a single-expression tape lookup that must compile down to the field read it wraps"
    )]
    #[inline(always)]
    pub(crate) fn list_value_record(&self, value: RecordIndex) -> Option<ListValueRecord> {
        self.values.get(index_usize(value)).copied()
    }

    pub(crate) fn take_object_record_for_transfer(
        &mut self,
        object: RecordIndex,
    ) -> Result<ObjectRecord, TapeBuildError> {
        const VISITED: ObjectRecord =
            ObjectRecord { first_field: RecordIndex::NONE, field_count: u32::MAX };
        let record =
            self.objects.get_mut(index_usize(object)).ok_or(TapeBuildError::InvalidRecordIndex)?;
        if *record == VISITED {
            return Err(TapeBuildError::InvalidRecordIndex);
        }
        Ok(std::mem::replace(record, VISITED))
    }

    pub(crate) fn take_field_record_for_transfer(
        &mut self,
        field: RecordIndex,
    ) -> Result<FieldRecord, TapeBuildError> {
        const VISITED: FieldRecord = FieldRecord {
            key: StringRange::new(0, 0),
            value: ValueRef::MISSING,
            next: RecordIndex::NONE,
        };
        let record =
            self.fields.get_mut(index_usize(field)).ok_or(TapeBuildError::InvalidRecordIndex)?;
        if *record == VISITED {
            return Err(TapeBuildError::InvalidRecordIndex);
        }
        Ok(std::mem::replace(record, VISITED))
    }

    pub(crate) fn take_list_record_for_transfer(
        &mut self,
        list: RecordIndex,
    ) -> Result<ListRecord, TapeBuildError> {
        const VISITED: ListRecord = ListRecord { first_value: RecordIndex::NONE, length: u32::MAX };
        let record =
            self.lists.get_mut(index_usize(list)).ok_or(TapeBuildError::InvalidRecordIndex)?;
        if *record == VISITED {
            return Err(TapeBuildError::InvalidRecordIndex);
        }
        Ok(std::mem::replace(record, VISITED))
    }

    pub(crate) fn take_list_value_record_for_transfer(
        &mut self,
        value: RecordIndex,
    ) -> Result<ListValueRecord, TapeBuildError> {
        const VISITED: ListValueRecord =
            ListValueRecord { value: ValueRef::MISSING, next: RecordIndex::NONE };
        let record =
            self.values.get_mut(index_usize(value)).ok_or(TapeBuildError::InvalidRecordIndex)?;
        if *record == VISITED {
            return Err(TapeBuildError::InvalidRecordIndex);
        }
        Ok(std::mem::replace(record, VISITED))
    }

    #[expect(
        clippy::inline_always,
        reason = "a single-expression tape lookup that must compile down to the field read it wraps"
    )]
    #[inline(always)]
    pub(crate) fn checked_key(&self, field: FieldRecord) -> Option<&str> {
        slice_range(&self.keys, field.key)
    }

    #[expect(
        clippy::inline_always,
        reason = "a single-expression tape lookup that must compile down to the field read it wraps"
    )]
    #[inline(always)]
    pub(crate) fn checked_key_range(&self, key: StringRange) -> Option<&str> {
        slice_range(&self.keys, key)
    }

    pub(crate) fn transfer_field_summary(
        &self,
    ) -> Result<(usize, usize, bool, bool), TapeBuildError> {
        let mut key_bytes = 0_usize;
        let mut inline_u32_bytes = 0_usize;
        let mut needs_fix = false;
        for field in &self.fields {
            key_bytes = key_bytes
                .checked_add(
                    self.checked_key(*field).ok_or(TapeBuildError::InvalidRecordIndex)?.len(),
                )
                .ok_or(TapeBuildError::CapacityOverflow)?;
            needs_fix |= field.value.needs_fix();
            if field.value.as_inline_u32().is_some() {
                inline_u32_bytes =
                    inline_u32_bytes.checked_add(10).ok_or(TapeBuildError::CapacityOverflow)?;
            }
        }
        for value in &self.values {
            if value.value.as_inline_u32().is_some() {
                inline_u32_bytes =
                    inline_u32_bytes.checked_add(10).ok_or(TapeBuildError::CapacityOverflow)?;
            }
        }
        if self.root.as_inline_u32().is_some() {
            inline_u32_bytes =
                inline_u32_bytes.checked_add(10).ok_or(TapeBuildError::CapacityOverflow)?;
        }
        let keys_are_json_safe =
            !self.keys.bytes().any(|byte| byte == b'"' || byte == b'\\' || byte <= 0x1f);
        Ok((key_bytes, inline_u32_bytes, needs_fix, keys_are_json_safe))
    }

    pub(crate) fn retained_transfer_layout(&self) -> Result<(usize, bool, bool), TapeBuildError> {
        let key_bytes = if self.transfer_keys_require_escape {
            self.transfer_key_bytes_upper.checked_mul(6).ok_or(TapeBuildError::CapacityOverflow)?
        } else {
            self.transfer_key_bytes_upper
        };
        let inline_u32_bytes = self
            .transfer_inline_u32_count_upper
            .checked_mul(10)
            .ok_or(TapeBuildError::CapacityOverflow)?;
        let punctuation = self
            .field_count()
            .checked_mul(4)
            .and_then(|value| value.checked_add(self.list_value_count().saturating_mul(2)))
            .and_then(|value| value.checked_add(self.object_count().saturating_mul(2)))
            .and_then(|value| value.checked_add(self.list_count().saturating_mul(2)))
            .ok_or(TapeBuildError::CapacityOverflow)?;
        let capacity = self
            .scalar_storage()
            .len()
            .checked_add(key_bytes)
            .and_then(|value| value.checked_add(inline_u32_bytes))
            .and_then(|value| value.checked_add(punctuation))
            .and_then(|value| value.checked_add(43))
            .ok_or(TapeBuildError::CapacityOverflow)?;
        Ok((capacity, self.transfer_has_fix, !self.transfer_keys_require_escape))
    }

    #[must_use]
    #[expect(
        clippy::inline_always,
        reason = "a single-expression tape lookup that must compile down to the field read it wraps"
    )]
    #[inline(always)]
    pub fn key(&self, field: &FieldRecord) -> &str {
        self.checked_key(*field).unwrap_or("")
    }

    #[must_use]
    #[expect(
        clippy::inline_always,
        reason = "a single-expression tape lookup that must compile down to the field read it wraps"
    )]
    #[inline(always)]
    pub fn scalar(&self, value: ValueRef) -> Option<&str> {
        slice_range(&self.scalars, value.as_scalar()?)
    }

    /// Reads an exact unsigned integer from either inline storage or legacy scalar text.
    #[must_use]
    #[expect(
        clippy::inline_always,
        reason = "a single-expression tape lookup that must compile down to the field read it wraps"
    )]
    #[inline(always)]
    pub fn scalar_u32(&self, value: ValueRef) -> Option<u32> {
        value.as_inline_u32().or_else(|| self.scalar(value)?.parse().ok())
    }

    #[must_use]
    pub fn scalar_storage(&self) -> &str {
        &self.scalars
    }

    pub fn set_scalar_storage(&mut self, scalars: String) {
        self.scalars = scalars;
    }

    /// Reserves packed scalar bytes for a bounded batch of in-place tape rewrites.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] when the requested storage exceeds the
    /// 32-bit tape limit or cannot be allocated.
    pub fn reserve_scalar_storage(&mut self, additional: usize) -> Result<(), TapeBuildError> {
        self.scalars
            .len()
            .checked_add(additional)
            .and_then(|length| u32::try_from(length).ok())
            .ok_or(TapeBuildError::CapacityOverflow)?;
        self.scalars.try_reserve(additional).map_err(|_| TapeBuildError::CapacityOverflow)
    }

    /// Appends one field name to packed key storage.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] if the 32-bit tape limit is exceeded.
    pub fn push_key(&mut self, key: &str) -> Result<StringRange, TapeBuildError> {
        self.transfer_keys_require_escape |=
            key.bytes().any(|byte| byte == b'"' || byte == b'\\' || byte <= 0x1f);
        push_string(&mut self.keys, key)
    }

    /// Appends one already-encoded scalar to packed scalar storage.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] if the 32-bit tape limit is exceeded.
    pub fn push_scalar(&mut self, scalar: &str) -> Result<ValueRef, TapeBuildError> {
        push_string(&mut self.scalars, scalar).map(|range| ValueRef::scalar(range, false))
    }

    /// Appends one string as a JSON-encoded scalar without allocating an intermediate `String`.
    ///
    /// The tape stores JSON scalar spellings rather than decoded values. This method writes the
    /// opening/closing quotes and the exact JSON escapes directly into packed scalar storage.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] if the encoded scalar exceeds the tape's
    /// 32-bit storage limit.
    pub fn push_json_string_scalar(&mut self, value: &str) -> Result<ValueRef, TapeBuildError> {
        let encoded_length = value.bytes().try_fold(2_usize, |length, byte| {
            let width = match byte {
                b'"' | b'\\' | b'\x08' | b'\x0c' | b'\n' | b'\r' | b'\t' => 2,
                0x00..=0x1f => 6,
                _ => 1,
            };
            length.checked_add(width)
        });
        let encoded_length = encoded_length.ok_or(TapeBuildError::CapacityOverflow)?;
        let start =
            u32::try_from(self.scalars.len()).map_err(|_| TapeBuildError::CapacityOverflow)?;
        let length = u32::try_from(encoded_length).map_err(|_| TapeBuildError::CapacityOverflow)?;
        start.checked_add(length).ok_or(TapeBuildError::CapacityOverflow)?;

        self.scalars.reserve(encoded_length);
        self.scalars.push('"');
        let mut copied = 0_usize;
        for (index, byte) in value.bytes().enumerate() {
            let escape = match byte {
                b'"' => Some('"'),
                b'\\' => Some('\\'),
                b'\x08' => Some('b'),
                b'\x0c' => Some('f'),
                b'\n' => Some('n'),
                b'\r' => Some('r'),
                b'\t' => Some('t'),
                0x00..=0x1f => Some('\0'),
                _ => None,
            };
            let Some(escape) = escape else {
                continue;
            };
            let unchanged = value.get(copied..index).ok_or(TapeBuildError::InvalidRecordIndex)?;
            self.scalars.push_str(unchanged);
            self.scalars.push('\\');
            if escape == '\0' {
                const HEX: &[u8; 16] = b"0123456789abcdef";
                self.scalars.push('u');
                self.scalars.push('0');
                self.scalars.push('0');
                self.scalars.push(char::from(HEX[usize::from(byte >> 4)]));
                self.scalars.push(char::from(HEX[usize::from(byte & 0x0f)]));
            } else {
                self.scalars.push(escape);
            }
            copied = index + 1;
        }
        let unchanged = value.get(copied..).ok_or(TapeBuildError::InvalidRecordIndex)?;
        self.scalars.push_str(unchanged);
        self.scalars.push('"');
        debug_assert_eq!(self.scalars.len() - start as usize, encoded_length);
        Ok(ValueRef::scalar(StringRange::new(start, length), false))
    }

    /// Appends exact JavaScript UTF-16 code units as one valid JSON string scalar.
    ///
    /// Valid surrogate pairs are encoded as their Unicode scalar. Unpaired high or low units are
    /// emitted as lowercase `\\uXXXX` escapes, so the flat tape remains valid UTF-8 while a later
    /// JavaScript materializer can reproduce the original code units exactly. No intermediate
    /// `String` is allocated.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] if the encoded scalar exceeds the tape's
    /// 32-bit storage limit.
    pub fn push_json_utf16_scalar(&mut self, value: &[u16]) -> Result<ValueRef, TapeBuildError> {
        const HEX: &[u8; 16] = b"0123456789abcdef";

        let mut encoded_length = 2_usize;
        let mut index = 0_usize;
        while index < value.len() {
            let unit = value[index];
            let (width, consumed) = if (0xd800..=0xdbff).contains(&unit)
                && value.get(index + 1).is_some_and(|next| (0xdc00..=0xdfff).contains(next))
            {
                (4, 2)
            } else if (0xd800..=0xdfff).contains(&unit) || unit <= 0x1f {
                (6, 1)
            } else if unit == u16::from(b'"') || unit == u16::from(b'\\') {
                (2, 1)
            } else {
                (bmp_scalar(unit)?.len_utf8(), 1)
            };
            encoded_length =
                encoded_length.checked_add(width).ok_or(TapeBuildError::CapacityOverflow)?;
            index += consumed;
        }
        let start =
            u32::try_from(self.scalars.len()).map_err(|_| TapeBuildError::CapacityOverflow)?;
        let length = u32::try_from(encoded_length).map_err(|_| TapeBuildError::CapacityOverflow)?;
        start.checked_add(length).ok_or(TapeBuildError::CapacityOverflow)?;

        self.scalars.reserve(encoded_length);
        self.scalars.push('"');
        let mut index = 0_usize;
        while index < value.len() {
            let unit = value[index];
            if (0xd800..=0xdbff).contains(&unit)
                && value.get(index + 1).is_some_and(|next| (0xdc00..=0xdfff).contains(next))
            {
                let high = u32::from(unit - 0xd800);
                let low = u32::from(value[index + 1] - 0xdc00);
                self.scalars.push(
                    char::from_u32(0x1_0000 + (high << 10) + low)
                        .ok_or(TapeBuildError::InvalidRecordIndex)?,
                );
                index += 2;
                continue;
            }
            match unit {
                value if (0xd800..=0xdfff).contains(&value) || value <= 0x1f => {
                    self.scalars.push('\\');
                    self.scalars.push('u');
                    self.scalars.push(char::from(HEX[usize::from((value >> 12) & 0x0f)]));
                    self.scalars.push(char::from(HEX[usize::from((value >> 8) & 0x0f)]));
                    self.scalars.push(char::from(HEX[usize::from((value >> 4) & 0x0f)]));
                    self.scalars.push(char::from(HEX[usize::from(value & 0x0f)]));
                }
                value if value == u16::from(b'"') => self.scalars.push_str("\\\""),
                value if value == u16::from(b'\\') => self.scalars.push_str("\\\\"),
                value => self.scalars.push(bmp_scalar(value)?),
            }
            index += 1;
        }
        self.scalars.push('"');
        debug_assert_eq!(self.scalars.len() - start as usize, encoded_length);
        Ok(ValueRef::scalar(StringRange::new(start, length), false))
    }

    /// Stores one exact unsigned integer without allocating or formatting scalar text.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] if the 32-bit tape limit is exceeded.
    pub fn push_u32_scalar(&mut self, scalar: u32) -> Result<ValueRef, TapeBuildError> {
        Ok(ValueRef::inline_u32(scalar))
    }

    /// Appends an object record.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] if the 32-bit tape limit is exceeded.
    pub fn push_object_record(
        &mut self,
        record: ObjectRecord,
    ) -> Result<RecordIndex, TapeBuildError> {
        push_record(&mut self.objects, record)
    }

    /// Appends an object-field record.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] if the 32-bit tape limit is exceeded.
    pub fn push_field_record(
        &mut self,
        record: FieldRecord,
    ) -> Result<RecordIndex, TapeBuildError> {
        self.transfer_has_fix |= record.value.needs_fix();
        self.transfer_key_bytes_upper =
            self.transfer_key_bytes_upper.saturating_add(record.key.length as usize);
        self.transfer_inline_u32_count_upper = self
            .transfer_inline_u32_count_upper
            .saturating_add(usize::from(record.value.as_inline_u32().is_some()));
        push_record(&mut self.fields, record)
    }

    /// Appends a list record.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] if the 32-bit tape limit is exceeded.
    pub fn push_list_record(&mut self, record: ListRecord) -> Result<RecordIndex, TapeBuildError> {
        push_record(&mut self.lists, record)
    }

    /// Appends a list-value record.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] if the 32-bit tape limit is exceeded.
    pub fn push_list_value_record(
        &mut self,
        record: ListValueRecord,
    ) -> Result<RecordIndex, TapeBuildError> {
        self.transfer_has_fix |= record.value.needs_fix();
        self.transfer_inline_u32_count_upper = self
            .transfer_inline_u32_count_upper
            .saturating_add(usize::from(record.value.as_inline_u32().is_some()));
        push_record(&mut self.values, record)
    }

    /// Links an object field to its next field.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::InvalidRecordIndex`] if `field` does not exist.
    pub fn set_field_next(
        &mut self,
        field: RecordIndex,
        next: RecordIndex,
    ) -> Result<(), TapeBuildError> {
        self.fields.get_mut(index_usize(field)).ok_or(TapeBuildError::InvalidRecordIndex)?.next =
            next;
        Ok(())
    }

    /// Links a list entry to its next entry.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::InvalidRecordIndex`] if `value` does not exist.
    pub fn set_list_value_next(
        &mut self,
        value: RecordIndex,
        next: RecordIndex,
    ) -> Result<(), TapeBuildError> {
        self.values.get_mut(index_usize(value)).ok_or(TapeBuildError::InvalidRecordIndex)?.next =
            next;
        Ok(())
    }

    /// Replaces the value referenced by one list entry.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::InvalidRecordIndex`] if `entry` does not exist.
    pub fn set_list_value(
        &mut self,
        entry: RecordIndex,
        value: ValueRef,
    ) -> Result<(), TapeBuildError> {
        self.transfer_has_fix |= value.needs_fix();
        self.transfer_inline_u32_count_upper = self
            .transfer_inline_u32_count_upper
            .saturating_add(usize::from(value.as_inline_u32().is_some()));
        self.values.get_mut(index_usize(entry)).ok_or(TapeBuildError::InvalidRecordIndex)?.value =
            value;
        Ok(())
    }

    #[must_use]
    pub fn field_index(&self, object: RecordIndex, name: &str) -> Option<RecordIndex> {
        let mut next = self.objects.get(index_usize(object))?.first_field;
        while let Some(raw) = next.get() {
            let index = RecordIndex::new(raw);
            let record = self.fields.get(index_usize(index))?;
            if self.key(record) == name {
                return Some(index);
            }
            next = record.next;
        }
        None
    }

    #[must_use]
    pub fn field_value(&self, field: RecordIndex) -> Option<ValueRef> {
        self.fields.get(index_usize(field)).map(|record| record.value)
    }

    /// Replaces the value referenced by an object field.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::InvalidRecordIndex`] if `field` does not exist.
    pub fn set_field_value(
        &mut self,
        field: RecordIndex,
        value: ValueRef,
    ) -> Result<(), TapeBuildError> {
        self.transfer_has_fix |= value.needs_fix();
        self.transfer_inline_u32_count_upper = self
            .transfer_inline_u32_count_upper
            .saturating_add(usize::from(value.as_inline_u32().is_some()));
        self.fields.get_mut(index_usize(field)).ok_or(TapeBuildError::InvalidRecordIndex)?.value =
            value;
        Ok(())
    }

    /// Appends a field to an existing object while preserving field order.
    ///
    /// # Errors
    ///
    /// Returns an error if `object` is invalid or the 32-bit tape limit is exceeded.
    pub fn append_field(
        &mut self,
        object: RecordIndex,
        key: &str,
        value: ValueRef,
    ) -> Result<(), TapeBuildError> {
        let key = self.push_key(key)?;
        let field = self.push_field_record(FieldRecord { key, value, next: RecordIndex::NONE })?;
        let object_index = index_usize(object);
        let (first, count) = self
            .objects
            .get(object_index)
            .map(|record| (record.first_field, record.field_count))
            .ok_or(TapeBuildError::InvalidRecordIndex)?;
        if first.is_none() {
            self.objects[object_index].first_field = field;
        } else {
            let mut last = first;
            loop {
                let next = self
                    .fields
                    .get(index_usize(last))
                    .ok_or(TapeBuildError::InvalidRecordIndex)?
                    .next;
                if next.is_none() {
                    break;
                }
                last = next;
            }
            self.set_field_next(last, field)?;
        }
        self.objects[object_index].field_count =
            count.checked_add(1).ok_or(TapeBuildError::CapacityOverflow)?;
        Ok(())
    }

    /// Detaches every field from an object so it can be rebuilt in place.
    ///
    /// Detached records are reclaimed by [`Self::compact_reachable`]. Reusing the object index
    /// keeps existing parent slots valid while a projected scaffold node is lifted into its
    /// authored shape.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::InvalidRecordIndex`] if `object` does not exist.
    pub fn clear_fields(&mut self, object: RecordIndex) -> Result<(), TapeBuildError> {
        *self.objects.get_mut(index_usize(object)).ok_or(TapeBuildError::InvalidRecordIndex)? =
            ObjectRecord::default();
        Ok(())
    }

    /// Inserts a field immediately before an existing field while preserving field order.
    ///
    /// # Errors
    ///
    /// Returns an error if `object` or `before` is invalid, `before` does not belong to `object`,
    /// or the 32-bit tape limit is exceeded.
    pub fn insert_field_before(
        &mut self,
        object: RecordIndex,
        before: RecordIndex,
        key: &str,
        value: ValueRef,
    ) -> Result<(), TapeBuildError> {
        let object_index = index_usize(object);
        let record = *self.objects.get(object_index).ok_or(TapeBuildError::InvalidRecordIndex)?;
        let mut previous = RecordIndex::NONE;
        let mut current = record.first_field;
        while current != before {
            if current.is_none() {
                return Err(TapeBuildError::InvalidRecordIndex);
            }
            previous = current;
            current = self
                .fields
                .get(index_usize(current))
                .ok_or(TapeBuildError::InvalidRecordIndex)?
                .next;
        }
        self.fields.get(index_usize(before)).ok_or(TapeBuildError::InvalidRecordIndex)?;

        let key = self.push_key(key)?;
        let field = self.push_field_record(FieldRecord { key, value, next: before })?;
        if previous.is_none() {
            self.objects[object_index].first_field = field;
        } else {
            self.set_field_next(previous, field)?;
        }
        self.objects[object_index].field_count =
            record.field_count.checked_add(1).ok_or(TapeBuildError::CapacityOverflow)?;
        Ok(())
    }

    /// Moves an existing object field immediately before another existing field.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::InvalidRecordIndex`] if the object or either field is invalid,
    /// either field does not belong to the object, or both indices identify the same field.
    pub fn move_field_before(
        &mut self,
        object: RecordIndex,
        field: RecordIndex,
        before: RecordIndex,
    ) -> Result<(), TapeBuildError> {
        if field == before {
            return Err(TapeBuildError::InvalidRecordIndex);
        }
        let object_index = index_usize(object);
        let first =
            self.objects.get(object_index).ok_or(TapeBuildError::InvalidRecordIndex)?.first_field;
        let mut previous = RecordIndex::NONE;
        let mut current = first;
        let mut field_previous = None;
        let mut before_seen = false;
        while !current.is_none() {
            if current == field {
                field_previous = Some(previous);
            }
            if current == before {
                before_seen = true;
            }
            previous = current;
            current = self
                .fields
                .get(index_usize(current))
                .ok_or(TapeBuildError::InvalidRecordIndex)?
                .next;
        }
        let field_previous = field_previous.ok_or(TapeBuildError::InvalidRecordIndex)?;
        if !before_seen {
            return Err(TapeBuildError::InvalidRecordIndex);
        }
        if self.fields.get(index_usize(field)).is_some_and(|record| record.next == before) {
            return Ok(());
        }

        let field_next =
            self.fields.get(index_usize(field)).ok_or(TapeBuildError::InvalidRecordIndex)?.next;
        if field_previous.is_none() {
            self.objects[object_index].first_field = field_next;
        } else {
            self.set_field_next(field_previous, field_next)?;
        }

        let mut before_previous = RecordIndex::NONE;
        let mut current = self.objects[object_index].first_field;
        while current != before {
            if current.is_none() {
                return Err(TapeBuildError::InvalidRecordIndex);
            }
            before_previous = current;
            current = self
                .fields
                .get(index_usize(current))
                .ok_or(TapeBuildError::InvalidRecordIndex)?
                .next;
        }
        self.set_field_next(field, before)?;
        if before_previous.is_none() {
            self.objects[object_index].first_field = field;
        } else {
            self.set_field_next(before_previous, field)?;
        }
        Ok(())
    }

    /// Removes and returns one list entry while preserving the remaining order.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::InvalidRecordIndex`] if `list` or `entry` is invalid, or the
    /// entry does not belong to the list.
    pub fn remove_list_value(
        &mut self,
        list: RecordIndex,
        entry: RecordIndex,
    ) -> Result<ValueRef, TapeBuildError> {
        let list_index = index_usize(list);
        let record = *self.lists.get(list_index).ok_or(TapeBuildError::InvalidRecordIndex)?;
        let mut previous = RecordIndex::NONE;
        let mut current = record.first_value;
        while current != entry {
            if current.is_none() {
                return Err(TapeBuildError::InvalidRecordIndex);
            }
            previous = current;
            current = self
                .values
                .get(index_usize(current))
                .ok_or(TapeBuildError::InvalidRecordIndex)?
                .next;
        }
        let removed =
            *self.values.get(index_usize(entry)).ok_or(TapeBuildError::InvalidRecordIndex)?;
        if previous.is_none() {
            self.lists[list_index].first_value = removed.next;
        } else {
            self.set_list_value_next(previous, removed.next)?;
        }
        self.lists[list_index].length =
            record.length.checked_sub(1).ok_or(TapeBuildError::InvalidRecordIndex)?;
        Ok(removed.value)
    }

    /// Removes several list entries in place while preserving list identity and remaining order.
    ///
    /// The complete list table and removal batch are validated before any chain is changed. This
    /// rejects malformed lengths, cycles, shared entries, duplicate removals, and entries owned by
    /// a different list. Removed records remain unreachable until normal tape compaction.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::InvalidRecordIndex`] when a list chain or removal is invalid, or
    /// [`TapeBuildError::CapacityOverflow`] when validation storage cannot be allocated.
    pub fn remove_list_values(
        &mut self,
        removals: &[(RecordIndex, RecordIndex)],
    ) -> Result<(), TapeBuildError> {
        if removals.is_empty() {
            return Ok(());
        }

        let mut removed = Vec::new();
        removed
            .try_reserve_exact(self.values.len())
            .map_err(|_| TapeBuildError::CapacityOverflow)?;
        removed.resize(self.values.len(), 0_u8);
        let mut removal_counts = Vec::new();
        removal_counts
            .try_reserve_exact(self.lists.len())
            .map_err(|_| TapeBuildError::CapacityOverflow)?;
        removal_counts.resize(self.lists.len(), 0_u32);
        for &(list, entry) in removals {
            let list_index = index_usize(list);
            let entry_index = index_usize(entry);
            let count =
                removal_counts.get_mut(list_index).ok_or(TapeBuildError::InvalidRecordIndex)?;
            let flag = removed.get_mut(entry_index).ok_or(TapeBuildError::InvalidRecordIndex)?;
            if *flag != 0 {
                return Err(TapeBuildError::InvalidRecordIndex);
            }
            *flag = 1;
            *count = count.checked_add(1).ok_or(TapeBuildError::CapacityOverflow)?;
        }

        let mut owners = Vec::new();
        owners
            .try_reserve_exact(self.values.len())
            .map_err(|_| TapeBuildError::CapacityOverflow)?;
        owners.resize(self.values.len(), RecordIndex::NONE);
        for (list_index, record) in self.lists.iter().copied().enumerate() {
            let list = RecordIndex::new(
                u32::try_from(list_index).map_err(|_| TapeBuildError::CapacityOverflow)?,
            );
            let mut current = record.first_value;
            for _ in 0..record.length {
                let raw = current.get().ok_or(TapeBuildError::InvalidRecordIndex)?;
                let entry = RecordIndex::new(raw);
                let entry_index = index_usize(entry);
                let item =
                    self.values.get(entry_index).ok_or(TapeBuildError::InvalidRecordIndex)?;
                let owner =
                    owners.get_mut(entry_index).ok_or(TapeBuildError::InvalidRecordIndex)?;
                if !owner.is_none() {
                    return Err(TapeBuildError::InvalidRecordIndex);
                }
                *owner = list;
                current = item.next;
            }
            if !current.is_none() {
                return Err(TapeBuildError::InvalidRecordIndex);
            }
            if removal_counts[list_index] > record.length {
                return Err(TapeBuildError::InvalidRecordIndex);
            }
        }
        for &(list, entry) in removals {
            if owners.get(index_usize(entry)).copied() != Some(list) {
                return Err(TapeBuildError::InvalidRecordIndex);
            }
        }

        for (list_index, remove_count) in removal_counts.into_iter().enumerate() {
            if remove_count == 0 {
                continue;
            }
            let record = self.lists[list_index];
            let mut current = record.first_value;
            let mut first = RecordIndex::NONE;
            let mut previous = RecordIndex::NONE;
            let mut kept = 0_u32;
            for _ in 0..record.length {
                let entry = current;
                let entry_index = index_usize(entry);
                let next = self.values[entry_index].next;
                if removed[entry_index] == 0 {
                    if first.is_none() {
                        first = entry;
                    }
                    if !previous.is_none() {
                        self.values[index_usize(previous)].next = entry;
                    }
                    previous = entry;
                    kept += 1;
                }
                current = next;
            }
            if !previous.is_none() {
                self.values[index_usize(previous)].next = RecordIndex::NONE;
            }
            self.lists[list_index] = ListRecord { first_value: first, length: kept };
        }
        Ok(())
    }

    /// Inserts a list entry immediately after an existing entry while preserving order.
    ///
    /// # Errors
    ///
    /// Returns an error if `list` or `after` is invalid, `after` does not belong to `list`, or the
    /// 32-bit tape limit is exceeded.
    pub fn insert_list_value_after(
        &mut self,
        list: RecordIndex,
        after: RecordIndex,
        value: ValueRef,
    ) -> Result<RecordIndex, TapeBuildError> {
        let list_index = index_usize(list);
        let record = *self.lists.get(list_index).ok_or(TapeBuildError::InvalidRecordIndex)?;
        let mut current = record.first_value;
        while current != after {
            if current.is_none() {
                return Err(TapeBuildError::InvalidRecordIndex);
            }
            current = self
                .values
                .get(index_usize(current))
                .ok_or(TapeBuildError::InvalidRecordIndex)?
                .next;
        }
        let next =
            self.values.get(index_usize(after)).ok_or(TapeBuildError::InvalidRecordIndex)?.next;
        let inserted = self.push_list_value_record(ListValueRecord { value, next })?;
        self.set_list_value_next(after, inserted)?;
        self.lists[list_index].length =
            record.length.checked_add(1).ok_or(TapeBuildError::CapacityOverflow)?;
        Ok(inserted)
    }

    /// Inserts several values after existing list entries in one bounded flat-table pass.
    ///
    /// Every `after` entry must be unique and belong to the supplied `list`. The complete batch is
    /// validated, including capacity and list ownership, before any chain is mutated.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid list or entry, a duplicate insertion point, mismatched list
    /// ownership, a malformed existing list chain, or a 32-bit capacity overflow.
    pub fn insert_list_values_after(
        &mut self,
        insertions: &[ListValueInsertion],
    ) -> Result<(), TapeBuildError> {
        if insertions.is_empty() {
            return Ok(());
        }
        self.values
            .len()
            .checked_add(insertions.len())
            .and_then(|length| u32::try_from(length).ok())
            .ok_or(TapeBuildError::CapacityOverflow)?;

        let original_value_count = self.values.len();
        let mut requested = vec![None; original_value_count];
        let mut list_counts = vec![0_u32; self.lists.len()];
        for insertion in insertions {
            let list_index = index_usize(insertion.list);
            let after_index = index_usize(insertion.after);
            self.lists.get(list_index).ok_or(TapeBuildError::InvalidRecordIndex)?;
            self.values.get(after_index).ok_or(TapeBuildError::InvalidRecordIndex)?;
            if requested[after_index].replace(insertion.list).is_some() {
                return Err(TapeBuildError::InvalidRecordIndex);
            }
            list_counts[list_index] =
                list_counts[list_index].checked_add(1).ok_or(TapeBuildError::CapacityOverflow)?;
        }

        let mut owners = vec![RecordIndex::NONE; original_value_count];
        for (raw_list, &insertion_count) in list_counts.iter().enumerate() {
            if insertion_count == 0 {
                continue;
            }
            let list = RecordIndex::new(
                u32::try_from(raw_list).map_err(|_| TapeBuildError::CapacityOverflow)?,
            );
            let record = self.lists[raw_list];
            record.length.checked_add(insertion_count).ok_or(TapeBuildError::CapacityOverflow)?;
            let mut current = record.first_value;
            let mut visited = 0_u32;
            while let Some(raw) = current.get() {
                let index = usize::try_from(raw).map_err(|_| TapeBuildError::InvalidRecordIndex)?;
                let item = self.values.get(index).ok_or(TapeBuildError::InvalidRecordIndex)?;
                if !owners[index].is_none() {
                    return Err(TapeBuildError::InvalidRecordIndex);
                }
                owners[index] = list;
                visited = visited.checked_add(1).ok_or(TapeBuildError::CapacityOverflow)?;
                if visited > record.length {
                    return Err(TapeBuildError::InvalidRecordIndex);
                }
                current = item.next;
            }
            if visited != record.length {
                return Err(TapeBuildError::InvalidRecordIndex);
            }
        }
        for insertion in insertions {
            if owners[index_usize(insertion.after)] != insertion.list {
                return Err(TapeBuildError::InvalidRecordIndex);
            }
        }

        for insertion in insertions {
            let after_index = index_usize(insertion.after);
            let next = self.values[after_index].next;
            let inserted =
                self.push_list_value_record(ListValueRecord { value: insertion.value, next })?;
            self.values[after_index].next = inserted;
        }
        for (list_index, insertion_count) in list_counts.into_iter().enumerate() {
            if insertion_count != 0 {
                self.lists[list_index].length += insertion_count;
            }
        }
        Ok(())
    }

    /// Removes and returns the last entry in a non-empty list.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::InvalidRecordIndex`] for an invalid or empty list.
    pub fn pop_list_value(&mut self, list: RecordIndex) -> Result<ValueRef, TapeBuildError> {
        let list_index = index_usize(list);
        let record = *self.lists.get(list_index).ok_or(TapeBuildError::InvalidRecordIndex)?;
        if record.length == 0 || record.first_value.is_none() {
            return Err(TapeBuildError::InvalidRecordIndex);
        }
        let mut previous = RecordIndex::NONE;
        let mut current = record.first_value;
        loop {
            let item =
                *self.values.get(index_usize(current)).ok_or(TapeBuildError::InvalidRecordIndex)?;
            if item.next.is_none() {
                if previous.is_none() {
                    self.lists[list_index].first_value = RecordIndex::NONE;
                } else {
                    self.set_list_value_next(previous, RecordIndex::NONE)?;
                }
                self.lists[list_index].length -= 1;
                return Ok(item.value);
            }
            previous = current;
            current = item.next;
        }
    }

    /// Removes unreachable records and rewrites reachable scalar ranges into one packed buffer.
    ///
    /// This is a bounded flat-table pass. It does not create an object graph and preserves object
    /// field and list element order.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::InvalidRecordIndex`] if a chain or value references an invalid,
    /// cyclic, shared, or incorrectly sized record sequence. Returns
    /// [`TapeBuildError::CapacityOverflow`] if the compacted tape exceeds its 32-bit limit.
    pub fn compact_reachable(&mut self) -> Result<(), TapeBuildError> {
        let reachable = Reachability::collect(self)?;
        let object_map = build_index_map(&reachable.objects)?;
        let list_map = build_index_map(&reachable.lists)?;
        let mut scalars = String::with_capacity(self.scalars.len());
        let (objects, fields) =
            compact_objects(self, &reachable, &object_map, &list_map, &mut scalars)?;
        let (lists, values) =
            compact_lists(self, &reachable, &object_map, &list_map, &mut scalars)?;
        let root = compact_value(self.root, &object_map, &list_map, &self.scalars, &mut scalars)?;
        self.root = root;
        self.objects = objects;
        self.fields = fields;
        self.lists = lists;
        self.values = values;
        self.scalars = scalars;
        Ok(())
    }
}

fn bmp_scalar(unit: u16) -> Result<char, TapeBuildError> {
    char::from_u32(u32::from(unit)).ok_or(TapeBuildError::InvalidRecordIndex)
}

struct Reachability {
    objects: Vec<bool>,
    fields: Vec<bool>,
    lists: Vec<bool>,
    values: Vec<bool>,
}

impl Reachability {
    fn collect(tape: &FlatTape) -> Result<Self, TapeBuildError> {
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

pub struct FieldIter<'a> {
    tape: &'a FlatTape,
    next: RecordIndex,
}

pub struct IndexedFieldIter<'a> {
    tape: &'a FlatTape,
    next: RecordIndex,
}

impl<'a> Iterator for IndexedFieldIter<'a> {
    type Item = (RecordIndex, &'a FieldRecord);

    fn next(&mut self) -> Option<Self::Item> {
        let raw = self.next.get()?;
        let index = RecordIndex::new(raw);
        let record = self.tape.fields.get(index_usize(index))?;
        self.next = record.next;
        Some((index, record))
    }
}

impl<'a> Iterator for FieldIter<'a> {
    type Item = &'a FieldRecord;

    fn next(&mut self) -> Option<Self::Item> {
        let index = self.next.get()?;
        let record = self.tape.fields.get(index as usize)?;
        self.next = record.next;
        Some(record)
    }
}

pub struct ValueIter<'a> {
    tape: &'a FlatTape,
    next: RecordIndex,
}

pub struct IndexedValueIter<'a> {
    tape: &'a FlatTape,
    next: RecordIndex,
}

impl Iterator for IndexedValueIter<'_> {
    type Item = (RecordIndex, ValueRef);

    fn next(&mut self) -> Option<Self::Item> {
        let raw = self.next.get()?;
        let index = RecordIndex::new(raw);
        let record = self.tape.values.get(index_usize(index))?;
        self.next = record.next;
        Some((index, record.value))
    }
}

impl Iterator for ValueIter<'_> {
    type Item = ValueRef;

    fn next(&mut self) -> Option<Self::Item> {
        let index = self.next.get()?;
        let record = self.tape.values.get(index as usize)?;
        self.next = record.next;
        Some(record.value)
    }
}

fn push_record<T>(records: &mut Vec<T>, record: T) -> Result<RecordIndex, TapeBuildError> {
    let index = u32::try_from(records.len()).map_err(|_| TapeBuildError::CapacityOverflow)?;
    records.push(record);
    Ok(RecordIndex::new(index))
}

fn push_string(storage: &mut String, value: &str) -> Result<StringRange, TapeBuildError> {
    let start = u32::try_from(storage.len()).map_err(|_| TapeBuildError::CapacityOverflow)?;
    let length = u32::try_from(value.len()).map_err(|_| TapeBuildError::CapacityOverflow)?;
    start.checked_add(length).ok_or(TapeBuildError::CapacityOverflow)?;
    storage.push_str(value);
    Ok(StringRange::new(start, length))
}

fn count_reachable(records: &[bool]) -> usize {
    records.iter().filter(|&&reachable| reachable).count()
}

fn build_index_map(reachable: &[bool]) -> Result<Vec<RecordIndex>, TapeBuildError> {
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

fn compact_objects(
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

fn compact_lists(
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

fn compact_value(
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

#[expect(
    clippy::inline_always,
    reason = "a single-expression tape lookup that must compile down to the field read it wraps"
)]
#[inline(always)]
fn slice_range(storage: &str, range: StringRange) -> Option<&str> {
    let start = range.start as usize;
    let end = start.checked_add(range.length as usize)?;
    storage.get(start..end)
}

fn index_usize(index: RecordIndex) -> usize {
    index.into_raw() as usize
}
