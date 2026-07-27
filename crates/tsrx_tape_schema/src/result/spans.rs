//! Explicitly nullable spans and string ranges, and the fallible mapper each table applies when a
//! whole result is moved into another coordinate domain.

use crate::{StringRange, TapeSpan};

/// Explicit nullable span. A present empty span remains distinct from `None`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OptionalTapeSpan {
    present: u8,
    reserved: [u8; 3],
    value: TapeSpan,
}

impl OptionalTapeSpan {
    pub const NONE: Self = Self { present: 0, reserved: [0; 3], value: TapeSpan::new(0, 0) };

    #[must_use]
    pub const fn some(value: TapeSpan) -> Self {
        Self { present: 1, reserved: [0; 3], value }
    }

    #[must_use]
    pub const fn get(self) -> Option<TapeSpan> {
        if self.present == 0 { None } else { Some(self.value) }
    }

    #[must_use]
    pub const fn is_some(self) -> bool {
        self.present != 0
    }

    #[must_use]
    pub const fn is_none(self) -> bool {
        !self.is_some()
    }
}

impl Default for OptionalTapeSpan {
    fn default() -> Self {
        Self::NONE
    }
}

/// Explicit nullable packed string range. A present empty string remains distinct from `None`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OptionalStringRange {
    present: u8,
    reserved: [u8; 3],
    value: StringRange,
}

impl OptionalStringRange {
    pub const NONE: Self = Self { present: 0, reserved: [0; 3], value: StringRange::new(0, 0) };

    #[must_use]
    pub const fn some(value: StringRange) -> Self {
        Self { present: 1, reserved: [0; 3], value }
    }

    #[must_use]
    pub const fn get(self) -> Option<StringRange> {
        if self.present == 0 { None } else { Some(self.value) }
    }

    #[must_use]
    pub const fn is_some(self) -> bool {
        self.present != 0
    }

    #[must_use]
    pub const fn is_none(self) -> bool {
        !self.is_some()
    }
}

impl Default for OptionalStringRange {
    fn default() -> Self {
        Self::NONE
    }
}

/// One packed string value and its source span.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ValueSpanRecord {
    pub value: StringRange,
    pub span: TapeSpan,
}

impl ValueSpanRecord {
    #[must_use]
    pub const fn new(value: StringRange, span: TapeSpan) -> Self {
        Self { value, span }
    }
}

/// Explicit nullable value/span record.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OptionalValueSpanRecord {
    present: u8,
    reserved: [u8; 3],
    value: ValueSpanRecord,
}

impl OptionalValueSpanRecord {
    pub const NONE: Self = Self {
        present: 0,
        reserved: [0; 3],
        value: ValueSpanRecord::new(StringRange::new(0, 0), TapeSpan::new(0, 0)),
    };

    #[must_use]
    pub const fn some(value: ValueSpanRecord) -> Self {
        Self { present: 1, reserved: [0; 3], value }
    }

    #[must_use]
    pub const fn get(self) -> Option<ValueSpanRecord> {
        if self.present == 0 { None } else { Some(self.value) }
    }

    #[must_use]
    pub const fn is_some(self) -> bool {
        self.present != 0
    }

    #[must_use]
    pub const fn is_none(self) -> bool {
        !self.is_some()
    }
}

impl Default for OptionalValueSpanRecord {
    fn default() -> Self {
        Self::NONE
    }
}

pub(super) fn try_map_span<E>(
    span: &mut TapeSpan,
    mapper: &mut impl FnMut(TapeSpan) -> Result<TapeSpan, E>,
) -> Result<(), E> {
    *span = mapper(*span)?;
    Ok(())
}

pub(super) fn try_map_optional_span<E>(
    span: &mut OptionalTapeSpan,
    mapper: &mut impl FnMut(TapeSpan) -> Result<TapeSpan, E>,
) -> Result<(), E> {
    if span.present != 0 {
        try_map_span(&mut span.value, mapper)?;
    }
    Ok(())
}

pub(super) fn try_map_optional_value_span<E>(
    value: &mut OptionalValueSpanRecord,
    mapper: &mut impl FnMut(TapeSpan) -> Result<TapeSpan, E>,
) -> Result<(), E> {
    if value.present != 0 {
        try_map_span(&mut value.value.span, mapper)?;
    }
    Ok(())
}
