use crate::{FlatTape, RecordIndex, TapeBuildError, ValueKind};

use super::buffer::{BoundedString, push_json_string};
use super::walk::ContainerWork;
use super::{PROGRAM_TRANSFER_MAX_BYTES, PROGRAM_TRANSFER_VERSION};

pub(super) struct OwnedProgramSerializer {
    tape: FlatTape,
    node: BoundedString,
    keys_are_json_safe: bool,
}

impl OwnedProgramSerializer {
    pub(super) fn new(
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

    pub(super) fn run(self) -> Result<String, TapeBuildError> {
        if self.keys_are_json_safe { self.run_inner::<true>() } else { self.run_inner::<false>() }
    }

    #[expect(
        clippy::inline_always,
        reason = "a single-push writer called once per byte on the transfer hot loop"
    )]
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

    #[expect(
        clippy::inline_always,
        reason = "a single-push writer called once per byte on the transfer hot loop"
    )]
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

    #[expect(
        clippy::too_many_lines,
        reason = "the transfer writer is one linear pass over the tape and every branch shares the output cursor"
    )]
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
