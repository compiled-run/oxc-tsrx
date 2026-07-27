use oxc_data_structures::code_buffer::CodeBuffer;
use oxc_estree::{
    CompactFormatter, ConfigFixes, ESTree, ESTreeSpan, Formatter, SequenceSerializer, Serializer,
    StructSerializer,
};
use rustc_hash::{FxBuildHasher, FxHashMap};
use tsrx_tape_schema::{
    FieldRecord, FlatTape, ListRecord, ListValueRecord, ObjectRecord, RecordIndex, StringRange,
    TapeBuildError, ValueRef,
};

const STATIC_KEY_CACHE_SIZE: usize = 256;

#[derive(Clone, Copy)]
struct StaticKeyCacheEntry {
    pointer: usize,
    length: usize,
    range: StringRange,
}

impl StaticKeyCacheEntry {
    const EMPTY: Self = Self { pointer: 0, length: 0, range: StringRange::new(0, 0) };
}

pub(super) struct FlatTapeSerializer {
    tape: FlatTape,
    scalars: CodeBuffer,
    formatter: CompactFormatter,
    keys: FxHashMap<(usize, usize), StringRange>,
    key_cache: [StaticKeyCacheEntry; STATIC_KEY_CACHE_SIZE],
    last_value: Option<ValueRef>,
    pending_fix: bool,
    include_ts_fields: bool,
    ranges: bool,
    error: Option<TapeBuildError>,
}

impl FlatTapeSerializer {
    pub(super) fn new(capacity: usize, include_ts_fields: bool, ranges: bool) -> Self {
        let mut tape = FlatTape::default();
        let error = tape
            .reserve_records(
                capacity.saturating_div(8).saturating_add(8),
                capacity.saturating_div(2).saturating_add(16),
                capacity.saturating_div(32).saturating_add(8),
                capacity.saturating_div(24).saturating_add(8),
            )
            .err();
        Self {
            tape,
            scalars: CodeBuffer::with_capacity(capacity),
            formatter: CompactFormatter::new(),
            keys: FxHashMap::with_capacity_and_hasher(256, FxBuildHasher),
            key_cache: [StaticKeyCacheEntry::EMPTY; STATIC_KEY_CACHE_SIZE],
            last_value: None,
            pending_fix: false,
            include_ts_fields,
            ranges,
            error,
        }
    }

    pub(super) fn finish(mut self) -> Result<FlatTape, TapeBuildError> {
        if let Some(error) = self.error {
            return Err(error);
        }
        let root = self.last_value.take().ok_or(TapeBuildError::InvalidRecordIndex)?;
        self.tape.set_scalar_storage(self.scalars.into_string());
        self.tape.set_root(root);
        Ok(self.tape)
    }

    fn capture<T: ESTree + ?Sized>(&mut self, value: &T) -> ValueRef {
        let scalar_start = self.scalars.len();
        self.last_value = None;
        self.pending_fix = false;
        value.serialize(&mut *self);
        if let Some(value) = self.last_value.take() {
            return value;
        }
        let scalar_end = self.scalars.len();
        let Ok(start) = u32::try_from(scalar_start) else {
            self.fail(TapeBuildError::CapacityOverflow);
            return ValueRef::MISSING;
        };
        let Ok(length) = u32::try_from(scalar_end.saturating_sub(scalar_start)) else {
            self.fail(TapeBuildError::CapacityOverflow);
            return ValueRef::MISSING;
        };
        ValueRef::scalar(StringRange::new(start, length), self.pending_fix)
    }

    fn intern_key(&mut self, key: &'static str) -> StringRange {
        let identity = (key.as_ptr() as usize, key.len());
        let slot =
            ((identity.0 >> 4) ^ (identity.0 >> 13) ^ identity.1) & (STATIC_KEY_CACHE_SIZE - 1);
        let cached = self.key_cache[slot];
        if cached.pointer == identity.0 && cached.length == identity.1 {
            return cached.range;
        }
        let range = if let Some(range) = self.keys.get(&identity).copied() {
            range
        } else {
            match self.tape.push_key(key) {
                Ok(range) => {
                    self.keys.insert(identity, range);
                    range
                }
                Err(error) => {
                    self.fail(error);
                    StringRange::default()
                }
            }
        };
        self.key_cache[slot] =
            StaticKeyCacheEntry { pointer: identity.0, length: identity.1, range };
        range
    }

    fn push_field(&mut self, record: FieldRecord) -> RecordIndex {
        match self.tape.push_field_record(record) {
            Ok(index) => index,
            Err(error) => {
                self.fail(error);
                RecordIndex::NONE
            }
        }
    }

    fn push_object(&mut self, record: ObjectRecord) -> RecordIndex {
        match self.tape.push_object_record(record) {
            Ok(index) => index,
            Err(error) => {
                self.fail(error);
                RecordIndex::NONE
            }
        }
    }

    fn push_list(&mut self, record: ListRecord) -> RecordIndex {
        match self.tape.push_list_record(record) {
            Ok(index) => index,
            Err(error) => {
                self.fail(error);
                RecordIndex::NONE
            }
        }
    }

    fn push_list_value(&mut self, record: ListValueRecord) -> RecordIndex {
        match self.tape.push_list_value_record(record) {
            Ok(index) => index,
            Err(error) => {
                self.fail(error);
                RecordIndex::NONE
            }
        }
    }

    fn link_field(&mut self, previous: RecordIndex, next: RecordIndex) {
        if let Err(error) = self.tape.set_field_next(previous, next) {
            self.fail(error);
        }
    }

    fn link_list_value(&mut self, previous: RecordIndex, next: RecordIndex) {
        if let Err(error) = self.tape.set_list_value_next(previous, next) {
            self.fail(error);
        }
    }

    fn fail(&mut self, error: TapeBuildError) {
        if self.error.is_none() {
            self.error = Some(error);
        }
    }
}

impl<'s> Serializer for &'s mut FlatTapeSerializer {
    type Formatter = CompactFormatter;
    type StructSerializer = TapeStructSerializer<'s>;
    type SequenceSerializer = TapeSequenceSerializer<'s>;

    fn include_ts_fields(&self) -> bool {
        self.include_ts_fields
    }

    fn ranges(&self) -> bool {
        self.ranges
    }

    fn serialize_struct(self) -> Self::StructSerializer {
        TapeStructSerializer::new(self)
    }

    fn serialize_sequence(self) -> Self::SequenceSerializer {
        TapeSequenceSerializer::new(self)
    }

    fn record_fix_path(&mut self) {
        self.pending_fix = true;
    }

    fn buffer_mut(&mut self) -> &mut CodeBuffer {
        &mut self.scalars
    }

    fn buffer_and_formatter_mut(&mut self) -> (&mut CodeBuffer, &mut Self::Formatter) {
        (&mut self.scalars, &mut self.formatter)
    }
}

pub(super) struct TapeStructSerializer<'s> {
    serializer: &'s mut FlatTapeSerializer,
    first: RecordIndex,
    last: RecordIndex,
    count: u32,
}

impl<'s> TapeStructSerializer<'s> {
    fn new(serializer: &'s mut FlatTapeSerializer) -> Self {
        Self { serializer, first: RecordIndex::NONE, last: RecordIndex::NONE, count: 0 }
    }

    fn add_value(&mut self, key: &'static str, value: ValueRef) {
        let key = self.serializer.intern_key(key);
        let field = self.serializer.push_field(FieldRecord { key, value, next: RecordIndex::NONE });
        if self.first.is_none() {
            self.first = field;
        } else {
            self.serializer.link_field(self.last, field);
        }
        self.last = field;
        match self.count.checked_add(1) {
            Some(count) => self.count = count,
            None => self.serializer.fail(TapeBuildError::CapacityOverflow),
        }
    }

    fn add_field<T: ESTree + ?Sized>(&mut self, key: &'static str, value: &T) {
        let value = self.serializer.capture(value);
        self.add_value(key, value);
    }
}

impl StructSerializer for TapeStructSerializer<'_> {
    type Config = ConfigFixes;
    type Formatter = CompactFormatter;

    fn serialize_field<T: ESTree + ?Sized>(&mut self, key: &'static str, value: &T) {
        self.add_field(key, value);
    }

    fn serialize_js_field<T: ESTree + ?Sized>(&mut self, key: &'static str, value: &T) {
        if !self.include_ts_fields() {
            self.add_field(key, value);
        }
    }

    fn serialize_ts_field<T: ESTree + ?Sized>(&mut self, key: &'static str, value: &T) {
        if self.include_ts_fields() {
            self.add_field(key, value);
        }
    }

    fn serialize_span<S: ESTreeSpan>(&mut self, span: S) {
        let [start, end] = span.range();
        let start = ValueRef::inline_u32(start);
        let end = ValueRef::inline_u32(end);
        self.add_value("start", start);
        self.add_value("end", end);
        if self.ranges() {
            let first = self
                .serializer
                .push_list_value(ListValueRecord { value: start, next: RecordIndex::NONE });
            let second = self
                .serializer
                .push_list_value(ListValueRecord { value: end, next: RecordIndex::NONE });
            self.serializer.link_list_value(first, second);
            let range = self.serializer.push_list(ListRecord { first_value: first, length: 2 });
            self.add_value("range", ValueRef::list(range));
        }
    }

    fn end(self) {
        let object = self
            .serializer
            .push_object(ObjectRecord { first_field: self.first, field_count: self.count });
        self.serializer.last_value = Some(ValueRef::object(object));
    }

    fn include_ts_fields(&self) -> bool {
        self.serializer.include_ts_fields
    }

    fn ranges(&self) -> bool {
        self.serializer.ranges
    }
}

pub(super) struct TapeSequenceSerializer<'s> {
    serializer: &'s mut FlatTapeSerializer,
    first: RecordIndex,
    last: RecordIndex,
    count: u32,
}

impl<'s> TapeSequenceSerializer<'s> {
    fn new(serializer: &'s mut FlatTapeSerializer) -> Self {
        Self { serializer, first: RecordIndex::NONE, last: RecordIndex::NONE, count: 0 }
    }
}

impl SequenceSerializer for TapeSequenceSerializer<'_> {
    fn serialize_element<T: ESTree + ?Sized>(&mut self, value: &T) {
        let value = self.serializer.capture(value);
        let item =
            self.serializer.push_list_value(ListValueRecord { value, next: RecordIndex::NONE });
        if self.first.is_none() {
            self.first = item;
        } else {
            self.serializer.link_list_value(self.last, item);
        }
        self.last = item;
        match self.count.checked_add(1) {
            Some(count) => self.count = count,
            None => self.serializer.fail(TapeBuildError::CapacityOverflow),
        }
    }

    fn end(self) {
        let list =
            self.serializer.push_list(ListRecord { first_value: self.first, length: self.count });
        self.serializer.last_value = Some(ValueRef::list(list));
    }
}
