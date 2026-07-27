use crate::{
    FlatTape, RecordIndex, SCHEMA_VERSION, StringRange, TapeBuildError, ValueKind, ValueRef,
};

/// Revision of the installed-package Program transfer envelope.
pub const PROGRAM_TRANSFER_VERSION: u16 = 1;

/// Hard limit for one Program transfer, including its special-value fix paths.
pub const PROGRAM_TRANSFER_MAX_BYTES: usize = 256 * 1024 * 1024;

/// Magic word for the private installed-package Program graph transfer.
pub const PROGRAM_BINARY_TRANSFER_MAGIC: u32 = 0x4252_5354;

/// Revision of the private installed-package Program graph transfer.
pub const PROGRAM_BINARY_TRANSFER_VERSION: u32 = 1;

const PROGRAM_BINARY_HEADER_WORDS: usize = 12;
const BINARY_SCALAR_TAG: u32 = 0;
const BINARY_OBJECT_TAG: u32 = 1;
const BINARY_LIST_TAG: u32 = 2;
const BINARY_INLINE_U32_TAG: u32 = 3;
const BINARY_VALUE_TAG_SHIFT: u32 = 30;
const BINARY_VALUE_INDEX_MASK: u32 = (1 << BINARY_VALUE_TAG_SHIFT) - 1;
const BINARY_UNUSED_RANGE: u32 = u32::MAX;
const BINARY_COMMON_KEY_FLAG: u32 = 1 << 31;

fn common_key_id(key: &str) -> Option<u32> {
    Some(match key {
        "body" => 0,
        "end" => 1,
        "hashbang" => 2,
        "sourceType" => 3,
        "start" => 4,
        "type" => 5,
        "async" => 6,
        "attributes" => 7,
        "declaration" => 8,
        "expression" => 9,
        "generator" => 10,
        "id" => 11,
        "metadata" => 12,
        "name" => 13,
        "params" => 14,
        "path" => 15,
        "render" => 16,
        "children" => 17,
        "closingElement" => 18,
        "openingElement" => 19,
        "selfClosing" => 20,
        "value" => 21,
        "raw" => 22,
        "source" => 23,
        "specifiers" => 24,
        "kind" => 25,
        "local" => 26,
        "phase" => 27,
        "optional" => 28,
        "imported" => 29,
        "declarations" => 30,
        "init" => 31,
        "arguments" => 32,
        "callee" => 33,
        "computed" => 34,
        "key" => 35,
        "operator" => 36,
        "method" => 37,
        "properties" => 38,
        "shorthand" => 39,
        "left" => 40,
        "right" => 41,
        "object" => 42,
        "property" => 43,
        "argument" => 44,
        "statementType" => 45,
        "prefix" => 46,
        "consequent" => 47,
        "test" => 48,
        "alternate" => 49,
        "block" => 50,
        "finalizer" => 51,
        "handler" => 52,
        "param" => 53,
        "pending" => 54,
        "resetParam" => 55,
        "await" => 56,
        "empty" => 57,
        "index" => 58,
        "elements" => 59,
        "typeAnnotation" => 60,
        "accessibility" => 61,
        "declare" => 62,
        "members" => 63,
        _ => return None,
    })
}

fn common_key(id: u32) -> Option<&'static str> {
    const KEYS: [&str; 64] = [
        "body",
        "end",
        "hashbang",
        "sourceType",
        "start",
        "type",
        "async",
        "attributes",
        "declaration",
        "expression",
        "generator",
        "id",
        "metadata",
        "name",
        "params",
        "path",
        "render",
        "children",
        "closingElement",
        "openingElement",
        "selfClosing",
        "value",
        "raw",
        "source",
        "specifiers",
        "kind",
        "local",
        "phase",
        "optional",
        "imported",
        "declarations",
        "init",
        "arguments",
        "callee",
        "computed",
        "key",
        "operator",
        "method",
        "properties",
        "shorthand",
        "left",
        "right",
        "object",
        "property",
        "argument",
        "statementType",
        "prefix",
        "consequent",
        "test",
        "alternate",
        "block",
        "finalizer",
        "handler",
        "param",
        "pending",
        "resetParam",
        "await",
        "empty",
        "index",
        "elements",
        "typeAnnotation",
        "accessibility",
        "declare",
        "members",
    ];
    KEYS.get(usize::try_from(id).ok()?).copied()
}

/// One private installed-package Program graph transfer.
pub struct ProgramBinaryTransfer {
    pub metadata: String,
    pub words: Vec<u32>,
}

struct BoundedString {
    value: String,
}

impl BoundedString {
    fn with_capacity(capacity: usize) -> Result<Self, TapeBuildError> {
        let capacity = capacity.min(PROGRAM_TRANSFER_MAX_BYTES);
        let mut value = String::new();
        value.try_reserve(capacity).map_err(|_| TapeBuildError::CapacityOverflow)?;
        Ok(Self { value })
    }

    fn ensure(&mut self, additional: usize) -> Result<(), TapeBuildError> {
        let length =
            self.value.len().checked_add(additional).ok_or(TapeBuildError::CapacityOverflow)?;
        if length > PROGRAM_TRANSFER_MAX_BYTES {
            return Err(TapeBuildError::CapacityOverflow);
        }
        if length > self.value.capacity() {
            self.value
                .try_reserve(length - self.value.len())
                .map_err(|_| TapeBuildError::CapacityOverflow)?;
        }
        Ok(())
    }

    fn push(&mut self, value: char) -> Result<(), TapeBuildError> {
        self.ensure(value.len_utf8())?;
        self.value.push(value);
        Ok(())
    }

    fn push_str(&mut self, value: &str) -> Result<(), TapeBuildError> {
        self.ensure(value.len())?;
        self.value.push_str(value);
        Ok(())
    }

    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn push_reserved(&mut self, value: char) {
        debug_assert!(self.value.len() + value.len_utf8() <= self.value.capacity());
        self.value.push(value);
    }

    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn push_str_reserved(&mut self, value: &str) {
        debug_assert!(self.value.len() + value.len() <= self.value.capacity());
        self.value.push_str(value);
    }

    fn push_u32(&mut self, value: u32) -> Result<(), TapeBuildError> {
        self.ensure(10)?;
        self.push_u32_digits(value);
        Ok(())
    }

    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn push_u32_reserved(&mut self, value: u32) {
        debug_assert!(self.value.len() + 10 <= self.value.capacity());
        self.push_u32_digits(value);
    }

    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn push_u32_digits(&mut self, mut value: u32) {
        let mut digits = [0_u8; 10];
        let mut start = digits.len();
        loop {
            start -= 1;
            digits[start] = b'0' + (value % 10) as u8;
            value /= 10;
            if value == 0 {
                break;
            }
        }
        let encoded =
            std::str::from_utf8(&digits[start..]).expect("decimal digits are valid UTF-8");
        self.value.push_str(encoded);
    }

    fn into_string(self) -> String {
        self.value
    }
}

#[derive(Clone, Copy)]
enum PathSegment<'a> {
    Key(&'a str),
    Index(u32),
}

enum Work {
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
enum ContainerWork {
    Object { next: RecordIndex, remaining: u32, first: bool },
    List { next: RecordIndex, remaining: u32, first: bool },
}

fn zero_flags(length: usize) -> Result<Vec<u8>, TapeBuildError> {
    let mut flags = Vec::new();
    flags.try_reserve_exact(length).map_err(|_| TapeBuildError::CapacityOverflow)?;
    flags.resize(length, 0);
    Ok(flags)
}

fn push_json_string(output: &mut BoundedString, value: &str) -> Result<(), TapeBuildError> {
    const HEX: &[u8; 16] = b"0123456789abcdef";

    if !value.bytes().any(|byte| byte == b'"' || byte == b'\\' || byte <= 0x1f) {
        output.push('"')?;
        output.push_str(value)?;
        return output.push('"');
    }

    output.push('"')?;
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
        output.push_str(value.get(copied..index).ok_or(TapeBuildError::InvalidRecordIndex)?)?;
        output.push('\\')?;
        if escape == '\0' {
            output.push_str("u00")?;
            output.push(char::from(HEX[usize::from(byte >> 4)]))?;
            output.push(char::from(HEX[usize::from(byte & 0x0f)]))?;
        } else {
            output.push(escape)?;
        }
        copied = index + 1;
    }
    output.push_str(value.get(copied..).ok_or(TapeBuildError::InvalidRecordIndex)?)?;
    output.push('"')
}

fn write_fix_path(
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

fn transfer_layout(tape: &FlatTape) -> Result<(usize, bool, bool), TapeBuildError> {
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

struct ProgramSerializer<'a> {
    tape: &'a FlatTape,
    node: BoundedString,
    fixes: BoundedString,
    first_fix: bool,
    track_paths: bool,
    keys_are_json_safe: bool,
    path: Vec<PathSegment<'a>>,
    work: Vec<Work>,
    objects: Vec<u8>,
    fields: Vec<u8>,
    lists: Vec<u8>,
    values: Vec<u8>,
}

impl<'a> ProgramSerializer<'a> {
    fn new(tape: &'a FlatTape) -> Result<Self, TapeBuildError> {
        let (capacity, track_paths, keys_are_json_safe) = transfer_layout(tape)?;
        let mut work = Vec::new();
        if track_paths {
            work.try_reserve(64).map_err(|_| TapeBuildError::CapacityOverflow)?;
        }
        let mut path = Vec::new();
        if track_paths {
            path.try_reserve(64).map_err(|_| TapeBuildError::CapacityOverflow)?;
        }
        let mut node = BoundedString::with_capacity(capacity)?;
        node.push_str("{\"version\":")?;
        node.push_u32(u32::from(PROGRAM_TRANSFER_VERSION))?;
        node.push_str(",\"node\":")?;
        Ok(Self {
            tape,
            node,
            fixes: BoundedString::with_capacity(if track_paths { 64 } else { 0 })?,
            first_fix: true,
            track_paths,
            keys_are_json_safe,
            path,
            work,
            objects: zero_flags(tape.object_count())?,
            fields: zero_flags(tape.field_count())?,
            lists: zero_flags(tape.list_count())?,
            values: zero_flags(tape.list_value_count())?,
        })
    }

    fn push_work(&mut self, work: Work) -> Result<(), TapeBuildError> {
        self.work.try_reserve(1).map_err(|_| TapeBuildError::CapacityOverflow)?;
        self.work.push(work);
        Ok(())
    }

    fn push_path(&mut self, segment: PathSegment<'a>) -> Result<(), TapeBuildError> {
        self.path.try_reserve(1).map_err(|_| TapeBuildError::CapacityOverflow)?;
        self.path.push(segment);
        Ok(())
    }

    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn mark(flags: &mut [u8], index: RecordIndex) -> Result<(), TapeBuildError> {
        let index = usize::try_from(index.get().ok_or(TapeBuildError::InvalidRecordIndex)?)
            .map_err(|_| TapeBuildError::InvalidRecordIndex)?;
        let flag = flags.get_mut(index).ok_or(TapeBuildError::InvalidRecordIndex)?;
        if *flag != 0 {
            return Err(TapeBuildError::InvalidRecordIndex);
        }
        *flag = 1;
        Ok(())
    }

    fn run(self) -> Result<String, TapeBuildError> {
        if self.track_paths {
            self.run_with_fixes()
        } else if self.keys_are_json_safe {
            self.run_without_fixes::<true>()
        } else {
            self.run_without_fixes::<false>()
        }
    }

    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn write_key<const KEYS_ARE_JSON_SAFE: bool>(
        &mut self,
        key: &str,
    ) -> Result<(), TapeBuildError> {
        if KEYS_ARE_JSON_SAFE {
            self.node.push_reserved('"');
            self.node.push_str_reserved(key);
            self.node.push_reserved('"');
            Ok(())
        } else {
            push_json_string(&mut self.node, key)
        }
    }

    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn push_node<const RESERVED: bool>(&mut self, value: char) -> Result<(), TapeBuildError> {
        if RESERVED {
            self.node.push_reserved(value);
            Ok(())
        } else {
            self.node.push(value)
        }
    }

    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn push_node_str<const RESERVED: bool>(&mut self, value: &str) -> Result<(), TapeBuildError> {
        if RESERVED {
            self.node.push_str_reserved(value);
            Ok(())
        } else {
            self.node.push_str(value)
        }
    }

    #[allow(clippy::too_many_lines)]
    fn run_without_fixes<const KEYS_ARE_JSON_SAFE: bool>(
        mut self,
    ) -> Result<String, TapeBuildError> {
        let mut containers = Vec::new();
        containers.try_reserve(64).map_err(|_| TapeBuildError::CapacityOverflow)?;
        let mut next_value = Some(self.tape.root());

        loop {
            if let Some(value) = next_value.take() {
                match value.kind() {
                    ValueKind::Missing => return Err(TapeBuildError::InvalidRecordIndex),
                    ValueKind::Scalar => {
                        if value.needs_fix() {
                            return Err(TapeBuildError::InvalidRecordIndex);
                        }
                        if let Some(value) = value.as_inline_u32() {
                            self.node.push_u32_reserved(value);
                        } else {
                            self.push_node_str::<KEYS_ARE_JSON_SAFE>(
                                self.tape
                                    .scalar(value)
                                    .ok_or(TapeBuildError::InvalidRecordIndex)?,
                            )?;
                        }
                    }
                    ValueKind::Object => {
                        let object = value.as_object().ok_or(TapeBuildError::InvalidRecordIndex)?;
                        Self::mark(&mut self.objects, object)?;
                        let record = self
                            .tape
                            .object_record(object)
                            .ok_or(TapeBuildError::InvalidRecordIndex)?;
                        self.push_node::<KEYS_ARE_JSON_SAFE>('{')?;
                        containers.try_reserve(1).map_err(|_| TapeBuildError::CapacityOverflow)?;
                        containers.push(ContainerWork::Object {
                            next: record.first_field,
                            remaining: record.field_count,
                            first: true,
                        });
                    }
                    ValueKind::List => {
                        let list = value.as_list().ok_or(TapeBuildError::InvalidRecordIndex)?;
                        Self::mark(&mut self.lists, list)?;
                        let record = self
                            .tape
                            .list_record(list)
                            .ok_or(TapeBuildError::InvalidRecordIndex)?;
                        self.push_node::<KEYS_ARE_JSON_SAFE>('[')?;
                        containers.try_reserve(1).map_err(|_| TapeBuildError::CapacityOverflow)?;
                        containers.push(ContainerWork::List {
                            next: record.first_value,
                            remaining: record.length,
                            first: true,
                        });
                    }
                }
                continue;
            }

            let Some(container) = containers.last().copied() else {
                break;
            };
            match container {
                ContainerWork::Object { next, remaining, first } => {
                    if remaining == 0 {
                        if !next.is_none() {
                            return Err(TapeBuildError::InvalidRecordIndex);
                        }
                        self.push_node::<KEYS_ARE_JSON_SAFE>('}')?;
                        containers.pop();
                        continue;
                    }
                    let field_index = next
                        .get()
                        .map(RecordIndex::new)
                        .ok_or(TapeBuildError::InvalidRecordIndex)?;
                    Self::mark(&mut self.fields, field_index)?;
                    let field = self
                        .tape
                        .field_record(field_index)
                        .ok_or(TapeBuildError::InvalidRecordIndex)?;
                    let key =
                        self.tape.checked_key(field).ok_or(TapeBuildError::InvalidRecordIndex)?;
                    if !first {
                        self.push_node::<KEYS_ARE_JSON_SAFE>(',')?;
                    }
                    self.write_key::<KEYS_ARE_JSON_SAFE>(key)?;
                    self.push_node::<KEYS_ARE_JSON_SAFE>(':')?;
                    *containers.last_mut().ok_or(TapeBuildError::InvalidRecordIndex)? =
                        ContainerWork::Object {
                            next: field.next,
                            remaining: remaining - 1,
                            first: false,
                        };
                    next_value = Some(field.value);
                }
                ContainerWork::List { next, remaining, first } => {
                    if remaining == 0 {
                        if !next.is_none() {
                            return Err(TapeBuildError::InvalidRecordIndex);
                        }
                        self.push_node::<KEYS_ARE_JSON_SAFE>(']')?;
                        containers.pop();
                        continue;
                    }
                    let value_index = next
                        .get()
                        .map(RecordIndex::new)
                        .ok_or(TapeBuildError::InvalidRecordIndex)?;
                    Self::mark(&mut self.values, value_index)?;
                    let item = self
                        .tape
                        .list_value_record(value_index)
                        .ok_or(TapeBuildError::InvalidRecordIndex)?;
                    if !first {
                        self.push_node::<KEYS_ARE_JSON_SAFE>(',')?;
                    }
                    *containers.last_mut().ok_or(TapeBuildError::InvalidRecordIndex)? =
                        ContainerWork::List {
                            next: item.next,
                            remaining: remaining - 1,
                            first: false,
                        };
                    next_value = Some(item.value);
                }
            }
        }

        self.into_payload::<KEYS_ARE_JSON_SAFE>()
    }

    fn run_with_fixes(mut self) -> Result<String, TapeBuildError> {
        self.push_work(Work::Value { value: self.tape.root(), fix_owner: None })?;
        while let Some(work) = self.work.pop() {
            match work {
                Work::Value { value, fix_owner } => self.write_value(value, fix_owner)?,
                Work::Object { object, next, remaining, first, fix_recorded } => {
                    self.write_object_field(object, next, remaining, first, fix_recorded)?
                }
                Work::List { next, remaining, index, first } => {
                    self.write_list_value(next, remaining, index, first)?
                }
                Work::PopPath => {
                    self.path.pop().ok_or(TapeBuildError::InvalidRecordIndex)?;
                }
            }
        }
        if !self.path.is_empty() {
            return Err(TapeBuildError::InvalidRecordIndex);
        }
        self.into_payload::<false>()
    }

    fn into_payload<const RESERVED: bool>(mut self) -> Result<String, TapeBuildError> {
        self.push_node_str::<RESERVED>(",\"fixes\":[")?;
        if RESERVED {
            debug_assert!(self.fixes.value.is_empty());
        }
        let fixes = std::mem::take(&mut self.fixes.value);
        self.push_node_str::<RESERVED>(&fixes)?;
        self.push_node_str::<RESERVED>("]}")?;
        if self.node.value.len() > PROGRAM_TRANSFER_MAX_BYTES {
            return Err(TapeBuildError::CapacityOverflow);
        }
        Ok(self.node.into_string())
    }

    fn write_value(
        &mut self,
        value: ValueRef,
        fix_owner: Option<RecordIndex>,
    ) -> Result<(), TapeBuildError> {
        match value.kind() {
            ValueKind::Missing => Err(TapeBuildError::InvalidRecordIndex),
            ValueKind::Scalar => {
                if value.needs_fix() && fix_owner.is_none() {
                    return Err(TapeBuildError::InvalidRecordIndex);
                }
                if let Some(value) = value.as_inline_u32() {
                    self.node.push_u32(value)
                } else {
                    self.node.push_str(
                        self.tape.scalar(value).ok_or(TapeBuildError::InvalidRecordIndex)?,
                    )
                }
            }
            ValueKind::Object => {
                let object = value.as_object().ok_or(TapeBuildError::InvalidRecordIndex)?;
                Self::mark(&mut self.objects, object)?;
                let record =
                    self.tape.object_record(object).ok_or(TapeBuildError::InvalidRecordIndex)?;
                self.node.push('{')?;
                self.push_work(Work::Object {
                    object,
                    next: record.first_field,
                    remaining: record.field_count,
                    first: true,
                    fix_recorded: false,
                })
            }
            ValueKind::List => {
                let list = value.as_list().ok_or(TapeBuildError::InvalidRecordIndex)?;
                Self::mark(&mut self.lists, list)?;
                let record =
                    self.tape.list_record(list).ok_or(TapeBuildError::InvalidRecordIndex)?;
                self.node.push('[')?;
                self.push_work(Work::List {
                    next: record.first_value,
                    remaining: record.length,
                    index: 0,
                    first: true,
                })
            }
        }
    }

    fn write_object_field(
        &mut self,
        object: RecordIndex,
        next: RecordIndex,
        remaining: u32,
        first: bool,
        mut fix_recorded: bool,
    ) -> Result<(), TapeBuildError> {
        if remaining == 0 {
            if !next.is_none() {
                return Err(TapeBuildError::InvalidRecordIndex);
            }
            return self.node.push('}');
        }
        let field_index =
            next.get().map(RecordIndex::new).ok_or(TapeBuildError::InvalidRecordIndex)?;
        Self::mark(&mut self.fields, field_index)?;
        let field =
            self.tape.field_record(field_index).ok_or(TapeBuildError::InvalidRecordIndex)?;
        let key = self.tape.checked_key(field).ok_or(TapeBuildError::InvalidRecordIndex)?;
        if field.value.needs_fix() {
            if !matches!(field.value.kind(), ValueKind::Scalar) {
                return Err(TapeBuildError::InvalidRecordIndex);
            }
            if !fix_recorded {
                write_fix_path(&mut self.fixes, &self.path, &mut self.first_fix)?;
                fix_recorded = true;
            }
        }
        if !first {
            self.node.push(',')?;
        }
        if self.keys_are_json_safe {
            self.write_key::<true>(key)?;
        } else {
            self.write_key::<false>(key)?;
        }
        self.node.push(':')?;
        self.push_work(Work::Object {
            object,
            next: field.next,
            remaining: remaining - 1,
            first: false,
            fix_recorded,
        })?;
        if self.track_paths {
            self.push_work(Work::PopPath)?;
            self.push_work(Work::Value { value: field.value, fix_owner: Some(object) })?;
            self.push_path(PathSegment::Key(key))
        } else {
            self.push_work(Work::Value { value: field.value, fix_owner: Some(object) })
        }
    }

    fn write_list_value(
        &mut self,
        next: RecordIndex,
        remaining: u32,
        index: u32,
        first: bool,
    ) -> Result<(), TapeBuildError> {
        if remaining == 0 {
            if !next.is_none() {
                return Err(TapeBuildError::InvalidRecordIndex);
            }
            return self.node.push(']');
        }
        let value_index =
            next.get().map(RecordIndex::new).ok_or(TapeBuildError::InvalidRecordIndex)?;
        Self::mark(&mut self.values, value_index)?;
        let item =
            self.tape.list_value_record(value_index).ok_or(TapeBuildError::InvalidRecordIndex)?;
        if !first {
            self.node.push(',')?;
        }
        self.push_work(Work::List {
            next: item.next,
            remaining: remaining - 1,
            index: index.checked_add(1).ok_or(TapeBuildError::CapacityOverflow)?,
            first: false,
        })?;
        if self.track_paths {
            self.push_work(Work::PopPath)?;
            self.push_work(Work::Value { value: item.value, fix_owner: None })?;
            self.push_path(PathSegment::Index(index))
        } else {
            self.push_work(Work::Value { value: item.value, fix_owner: None })
        }
    }
}

struct OwnedProgramSerializer {
    tape: FlatTape,
    node: BoundedString,
    keys_are_json_safe: bool,
}

impl OwnedProgramSerializer {
    fn new(
        tape: FlatTape,
        capacity: usize,
        keys_are_json_safe: bool,
    ) -> Result<Self, TapeBuildError> {
        let mut node = BoundedString::with_capacity(capacity)?;
        node.push_str("{\"version\":")?;
        node.push_u32(u32::from(PROGRAM_TRANSFER_VERSION))?;
        node.push_str(",\"node\":")?;
        Ok(Self { tape, node, keys_are_json_safe })
    }

    fn run(self) -> Result<String, TapeBuildError> {
        if self.keys_are_json_safe { self.run_inner::<true>() } else { self.run_inner::<false>() }
    }

    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn write_key<const KEYS_ARE_JSON_SAFE: bool>(
        node: &mut BoundedString,
        key: &str,
    ) -> Result<(), TapeBuildError> {
        if KEYS_ARE_JSON_SAFE {
            node.push_reserved('"');
            node.push_str_reserved(key);
            node.push_reserved('"');
            Ok(())
        } else {
            push_json_string(node, key)
        }
    }

    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn push_node<const RESERVED: bool>(
        node: &mut BoundedString,
        value: char,
    ) -> Result<(), TapeBuildError> {
        if RESERVED {
            node.push_reserved(value);
            Ok(())
        } else {
            node.push(value)
        }
    }

    #[allow(clippy::too_many_lines)]
    fn run_inner<const KEYS_ARE_JSON_SAFE: bool>(mut self) -> Result<String, TapeBuildError> {
        let mut containers = Vec::new();
        containers.try_reserve(64).map_err(|_| TapeBuildError::CapacityOverflow)?;
        let mut next_value = Some(self.tape.root());

        loop {
            if let Some(value) = next_value.take() {
                match value.kind() {
                    ValueKind::Missing => return Err(TapeBuildError::InvalidRecordIndex),
                    ValueKind::Scalar => {
                        if value.needs_fix() {
                            return Err(TapeBuildError::InvalidRecordIndex);
                        }
                        if let Some(value) = value.as_inline_u32() {
                            self.node.push_u32_reserved(value);
                        } else {
                            let scalar = self
                                .tape
                                .scalar(value)
                                .ok_or(TapeBuildError::InvalidRecordIndex)?;
                            if KEYS_ARE_JSON_SAFE {
                                self.node.push_str_reserved(scalar);
                            } else {
                                self.node.push_str(scalar)?;
                            }
                        }
                    }
                    ValueKind::Object => {
                        let object = value.as_object().ok_or(TapeBuildError::InvalidRecordIndex)?;
                        let record = self.tape.take_object_record_for_transfer(object)?;
                        Self::push_node::<KEYS_ARE_JSON_SAFE>(&mut self.node, '{')?;
                        containers.try_reserve(1).map_err(|_| TapeBuildError::CapacityOverflow)?;
                        containers.push(ContainerWork::Object {
                            next: record.first_field,
                            remaining: record.field_count,
                            first: true,
                        });
                    }
                    ValueKind::List => {
                        let list = value.as_list().ok_or(TapeBuildError::InvalidRecordIndex)?;
                        let record = self.tape.take_list_record_for_transfer(list)?;
                        Self::push_node::<KEYS_ARE_JSON_SAFE>(&mut self.node, '[')?;
                        containers.try_reserve(1).map_err(|_| TapeBuildError::CapacityOverflow)?;
                        containers.push(ContainerWork::List {
                            next: record.first_value,
                            remaining: record.length,
                            first: true,
                        });
                    }
                }
                continue;
            }

            let Some(container) = containers.last().copied() else {
                break;
            };
            match container {
                ContainerWork::Object { next, remaining, first } => {
                    if remaining == 0 {
                        if !next.is_none() {
                            return Err(TapeBuildError::InvalidRecordIndex);
                        }
                        Self::push_node::<KEYS_ARE_JSON_SAFE>(&mut self.node, '}')?;
                        containers.pop();
                        continue;
                    }
                    let field_index = next
                        .get()
                        .map(RecordIndex::new)
                        .ok_or(TapeBuildError::InvalidRecordIndex)?;
                    let field = self.tape.take_field_record_for_transfer(field_index)?;
                    if !first {
                        Self::push_node::<KEYS_ARE_JSON_SAFE>(&mut self.node, ',')?;
                    }
                    {
                        let key = self
                            .tape
                            .checked_key(field)
                            .ok_or(TapeBuildError::InvalidRecordIndex)?;
                        Self::write_key::<KEYS_ARE_JSON_SAFE>(&mut self.node, key)?;
                    }
                    Self::push_node::<KEYS_ARE_JSON_SAFE>(&mut self.node, ':')?;
                    *containers.last_mut().ok_or(TapeBuildError::InvalidRecordIndex)? =
                        ContainerWork::Object {
                            next: field.next,
                            remaining: remaining - 1,
                            first: false,
                        };
                    next_value = Some(field.value);
                }
                ContainerWork::List { next, remaining, first } => {
                    if remaining == 0 {
                        if !next.is_none() {
                            return Err(TapeBuildError::InvalidRecordIndex);
                        }
                        Self::push_node::<KEYS_ARE_JSON_SAFE>(&mut self.node, ']')?;
                        containers.pop();
                        continue;
                    }
                    let value_index = next
                        .get()
                        .map(RecordIndex::new)
                        .ok_or(TapeBuildError::InvalidRecordIndex)?;
                    let item = self.tape.take_list_value_record_for_transfer(value_index)?;
                    if !first {
                        Self::push_node::<KEYS_ARE_JSON_SAFE>(&mut self.node, ',')?;
                    }
                    *containers.last_mut().ok_or(TapeBuildError::InvalidRecordIndex)? =
                        ContainerWork::List {
                            next: item.next,
                            remaining: remaining - 1,
                            first: false,
                        };
                    next_value = Some(item.value);
                }
            }
        }

        self.node.push_str_reserved(",\"fixes\":[]}");
        if self.node.value.len() > PROGRAM_TRANSFER_MAX_BYTES {
            return Err(TapeBuildError::CapacityOverflow);
        }
        Ok(self.node.into_string())
    }
}

#[derive(Clone, Copy)]
struct BinaryValue(u32);

impl BinaryValue {
    fn new(tag: u32, index: u32) -> Result<Self, TapeBuildError> {
        if tag > BINARY_INLINE_U32_TAG || index > BINARY_VALUE_INDEX_MASK {
            return Err(TapeBuildError::CapacityOverflow);
        }
        Ok(Self((tag << BINARY_VALUE_TAG_SHIFT) | index))
    }

    const fn tag(self) -> u32 {
        self.0 >> BINARY_VALUE_TAG_SHIFT
    }

    const fn index(self) -> u32 {
        self.0 & BINARY_VALUE_INDEX_MASK
    }
}

#[derive(Clone, Copy, Default)]
struct BinaryObject {
    field_start: u32,
    field_count: u32,
}

#[derive(Clone, Copy)]
struct BinaryField {
    key: u32,
    value: BinaryValue,
}

#[derive(Clone, Copy, Default)]
struct BinaryList {
    value_start: u32,
    value_count: u32,
}

#[derive(Clone, Copy)]
enum BinaryPathSegment {
    Key(u32),
    Index(u32),
}

#[derive(Clone, Copy)]
struct BinaryPathNode {
    parent: Option<u32>,
    segment: BinaryPathSegment,
}

#[derive(Clone, Copy)]
enum BinaryPending {
    Object { source: RecordIndex, wire: u32, path: Option<u32> },
    List { source: RecordIndex, wire: u32, path: Option<u32> },
}

#[derive(Clone, Copy)]
struct InternSlot {
    hash: u32,
    id: u32,
}

const EMPTY_INTERN_SLOT: InternSlot = InternSlot { hash: 0, id: u32::MAX };

fn intern_slots(upper: usize) -> Result<(Vec<InternSlot>, usize), TapeBuildError> {
    let slots = upper.max(1).checked_next_power_of_two().ok_or(TapeBuildError::CapacityOverflow)?;
    let mut table = Vec::new();
    table.try_reserve_exact(slots).map_err(|_| TapeBuildError::CapacityOverflow)?;
    table.resize(slots, EMPTY_INTERN_SLOT);
    Ok((table, slots - 1))
}

#[allow(clippy::inline_always)]
#[inline(always)]
fn intern_hash(value: &str) -> u32 {
    let mut hash = 0x811c_9dc5_u32;
    for byte in value.bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

struct BinaryProgramSerializer {
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
    fn new(tape: FlatTape) -> Result<Self, TapeBuildError> {
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

    fn run(mut self) -> Result<ProgramBinaryTransfer, TapeBuildError> {
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

impl FlatTape {
    /// Serializes a concrete Program and its OXC special-value fix paths into one bounded payload.
    ///
    /// The walk is iterative and rejects missing, cyclic, shared, truncated, and over-limit tape
    /// records before the payload crosses Node-API.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError`] for invalid tapes or payloads above
    /// [`PROGRAM_TRANSFER_MAX_BYTES`].
    pub fn program_transfer(&self) -> Result<String, TapeBuildError> {
        if self.schema_version() != SCHEMA_VERSION {
            return Err(TapeBuildError::InvalidRecordIndex);
        }
        ProgramSerializer::new(self)?.run()
    }

    /// Consumes a concrete Program while serializing its transfer payload.
    ///
    /// The no-fix path uses the consumed record tables themselves as visit markers, preserving
    /// the same invalid-index, cycle, sharing, truncation, and capacity checks without allocating
    /// four parallel flag tables. Programs containing OXC special-value fixes retain the borrowed
    /// path because fix paths hold key references during traversal.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError`] for invalid tapes or payloads above
    /// [`PROGRAM_TRANSFER_MAX_BYTES`].
    pub fn program_transfer_owned(self) -> Result<String, TapeBuildError> {
        if self.schema_version() != SCHEMA_VERSION {
            return Err(TapeBuildError::InvalidRecordIndex);
        }
        let (capacity, track_paths, keys_are_json_safe) = transfer_layout(&self)?;
        if track_paths {
            self.program_transfer()
        } else {
            OwnedProgramSerializer::new(self, capacity, keys_are_json_safe)?.run()
        }
    }

    /// Consumes an engine-origin Program using metadata retained while its tape was built.
    ///
    /// This avoids the common no-fix field-summary prepass. Special-value Programs fall back to
    /// the complete summary/path serializer. The owned walk still consumes record slots as visit
    /// markers and rejects cyclic, shared, truncated, or invalid reachable graphs.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError`] for invalid tapes or payloads above
    /// [`PROGRAM_TRANSFER_MAX_BYTES`].
    #[doc(hidden)]
    pub fn program_transfer_engine_owned(self) -> Result<String, TapeBuildError> {
        if self.schema_version() != SCHEMA_VERSION {
            return Err(TapeBuildError::InvalidRecordIndex);
        }
        let (capacity, track_paths, keys_are_json_safe) = self.retained_transfer_layout()?;
        if track_paths {
            self.program_transfer()
        } else {
            OwnedProgramSerializer::new(self, capacity, keys_are_json_safe)?.run()
        }
    }

    /// Consumes an engine-origin Program into the private installed-package binary graph format.
    ///
    /// The walk is iterative and rejects missing, cyclic, shared, truncated, invalid, and
    /// over-limit tapes before either payload crosses Node-API.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError`] for an invalid tape or a transfer above
    /// [`PROGRAM_TRANSFER_MAX_BYTES`].
    #[doc(hidden)]
    pub fn program_transfer_engine_binary_owned(
        self,
    ) -> Result<ProgramBinaryTransfer, TapeBuildError> {
        BinaryProgramSerializer::new(self)?.run()
    }
}
