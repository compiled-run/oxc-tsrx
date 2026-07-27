//! The row types of the four record tables. Source order is carried by `next` links rather than
//! by array position, so a record can be appended without moving its siblings.

use crate::{RecordIndex, StringRange};

use super::value::ValueRef;

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
