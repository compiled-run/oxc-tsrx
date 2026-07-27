use crate::{FlatTape, RecordIndex, TapeBuildError, ValueKind, ValueRef};

use super::buffer::{BoundedString, push_json_string};
use super::walk::{ContainerWork, PathSegment, Work, transfer_layout, write_fix_path, zero_flags};
use super::{PROGRAM_TRANSFER_MAX_BYTES, PROGRAM_TRANSFER_VERSION};

pub(super) struct ProgramSerializer<'a> {
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
    pub(super) fn new(tape: &'a FlatTape) -> Result<Self, TapeBuildError> {
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

    #[expect(
        clippy::inline_always,
        reason = "a two-instruction bitmap write performed once per visited record"
    )]
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

    pub(super) fn run(self) -> Result<String, TapeBuildError> {
        if self.track_paths {
            self.run_with_fixes()
        } else if self.keys_are_json_safe {
            self.run_without_fixes::<true>()
        } else {
            self.run_without_fixes::<false>()
        }
    }

    #[expect(
        clippy::inline_always,
        reason = "a single-push writer called once per byte on the transfer hot loop"
    )]
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

    #[expect(
        clippy::inline_always,
        reason = "a single-push writer called once per byte on the transfer hot loop"
    )]
    #[inline(always)]
    fn push_node<const RESERVED: bool>(&mut self, value: char) -> Result<(), TapeBuildError> {
        if RESERVED {
            self.node.push_reserved(value);
            Ok(())
        } else {
            self.node.push(value)
        }
    }

    #[expect(
        clippy::inline_always,
        reason = "a single-push writer called once per byte on the transfer hot loop"
    )]
    #[inline(always)]
    fn push_node_str<const RESERVED: bool>(&mut self, value: &str) -> Result<(), TapeBuildError> {
        if RESERVED {
            self.node.push_str_reserved(value);
            Ok(())
        } else {
            self.node.push_str(value)
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "the transfer writer is one linear pass over the tape and every branch shares the output cursor"
    )]
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
                    self.write_object_field(object, next, remaining, first, fix_recorded)?;
                }
                Work::List { next, remaining, index, first } => {
                    self.write_list_value(next, remaining, index, first)?;
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
