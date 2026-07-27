//! How one tape value packs its storage lane and its index into a single `u32`, and which tag
//! values are reserved to make that unambiguous.

use crate::{RecordIndex, StringRange};

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
