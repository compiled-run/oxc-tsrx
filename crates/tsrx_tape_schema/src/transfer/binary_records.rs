use crate::{RecordIndex, TapeBuildError};

pub(super) const BINARY_SCALAR_TAG: u32 = 0;
pub(super) const BINARY_OBJECT_TAG: u32 = 1;
pub(super) const BINARY_LIST_TAG: u32 = 2;
pub(super) const BINARY_INLINE_U32_TAG: u32 = 3;
const BINARY_VALUE_TAG_SHIFT: u32 = 30;
const BINARY_VALUE_INDEX_MASK: u32 = (1 << BINARY_VALUE_TAG_SHIFT) - 1;

#[derive(Clone, Copy)]
pub(super) struct BinaryValue(pub(super) u32);

impl BinaryValue {
    pub(super) fn new(tag: u32, index: u32) -> Result<Self, TapeBuildError> {
        if tag > BINARY_INLINE_U32_TAG || index > BINARY_VALUE_INDEX_MASK {
            return Err(TapeBuildError::CapacityOverflow);
        }
        Ok(Self((tag << BINARY_VALUE_TAG_SHIFT) | index))
    }

    pub(super) const fn tag(self) -> u32 {
        self.0 >> BINARY_VALUE_TAG_SHIFT
    }

    pub(super) const fn index(self) -> u32 {
        self.0 & BINARY_VALUE_INDEX_MASK
    }
}

#[derive(Clone, Copy, Default)]
pub(super) struct BinaryObject {
    pub(super) field_start: u32,
    pub(super) field_count: u32,
}

#[derive(Clone, Copy)]
pub(super) struct BinaryField {
    pub(super) key: u32,
    pub(super) value: BinaryValue,
}

#[derive(Clone, Copy, Default)]
pub(super) struct BinaryList {
    pub(super) value_start: u32,
    pub(super) value_count: u32,
}

#[derive(Clone, Copy)]
pub(super) enum BinaryPathSegment {
    Key(u32),
    Index(u32),
}

#[derive(Clone, Copy)]
pub(super) struct BinaryPathNode {
    pub(super) parent: Option<u32>,
    pub(super) segment: BinaryPathSegment,
}

#[derive(Clone, Copy)]
pub(super) enum BinaryPending {
    Object { source: RecordIndex, wire: u32, path: Option<u32> },
    List { source: RecordIndex, wire: u32, path: Option<u32> },
}

#[derive(Clone, Copy)]
pub(super) struct InternSlot {
    pub(super) hash: u32,
    pub(super) id: u32,
}

const EMPTY_INTERN_SLOT: InternSlot = InternSlot { hash: 0, id: u32::MAX };

pub(super) fn intern_slots(upper: usize) -> Result<(Vec<InternSlot>, usize), TapeBuildError> {
    let slots = upper.max(1).checked_next_power_of_two().ok_or(TapeBuildError::CapacityOverflow)?;
    let mut table = Vec::new();
    table.try_reserve_exact(slots).map_err(|_| TapeBuildError::CapacityOverflow)?;
    table.resize(slots, EMPTY_INTERN_SLOT);
    Ok((table, slots - 1))
}

#[expect(
    clippy::inline_always,
    reason = "a short hash evaluated once per interned key on the transfer hot loop"
)]
#[inline(always)]
pub(super) fn intern_hash(value: &str) -> u32 {
    let mut hash = 0x811c_9dc5_u32;
    for byte in value.bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}
