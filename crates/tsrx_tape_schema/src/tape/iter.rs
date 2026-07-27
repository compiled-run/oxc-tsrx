//! Walking a record's `next` chain as an iterator, with and without the index of each step.

use crate::RecordIndex;

use super::FlatTape;
use super::bounds::index_usize;
use super::record::FieldRecord;
use super::value::ValueRef;

pub struct FieldIter<'a> {
    pub(super) tape: &'a FlatTape,
    pub(super) next: RecordIndex,
}

pub struct IndexedFieldIter<'a> {
    pub(super) tape: &'a FlatTape,
    pub(super) next: RecordIndex,
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
    pub(super) tape: &'a FlatTape,
    pub(super) next: RecordIndex,
}

pub struct IndexedValueIter<'a> {
    pub(super) tape: &'a FlatTape,
    pub(super) next: RecordIndex,
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
