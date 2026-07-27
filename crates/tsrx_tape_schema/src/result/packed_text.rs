use std::{fmt, mem};

use crate::{StringRange, TapeBuildError};

use super::bounds::{slice_range, string_range};

const SURROGATE_PLACEHOLDER: char = '\u{e000}';
const SURROGATE_PLACEHOLDER_UTF8: &[u8] = b"\xee\x80\x80";
const REJECTION_PLACEHOLDER_UTF8: &[u8] = b"\xef\xbf\xbf";

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct PackedTextFixup {
    pub(super) byte_start: u32,
    unit: u16,
    reserved: u16,
}

/// Borrowed view over one lossless JavaScript string in packed result storage.
///
/// Well-formed values remain ordinary UTF-8 and expose [`Self::as_str`]. A value containing an
/// unpaired UTF-16 surrogate carries sparse position-keyed fixups; callers can materialize its
/// exact JavaScript code units with [`Self::write_utf16`] without confusing an authored private-use
/// scalar for a placeholder.
#[derive(Clone, Copy)]
pub struct PackedTextRef<'a> {
    utf8: &'a str,
    byte_start: u32,
    fixups: &'a [PackedTextFixup],
}

impl fmt::Debug for PackedTextRef<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug = formatter.debug_struct("PackedTextRef");
        if self.fixups.is_empty() {
            debug.field("utf8", &self.utf8);
        } else {
            debug.field("utf8", &"<lossless UTF-16 text>").field("fixup_count", &self.fixups.len());
        }
        debug.field("byte_start", &self.byte_start).finish()
    }
}

impl<'a> PackedTextRef<'a> {
    /// Returns the value as UTF-8 when it contains no unpaired UTF-16 surrogate.
    #[must_use]
    pub const fn as_str(self) -> Option<&'a str> {
        if self.fixups.is_empty() { Some(self.utf8) } else { None }
    }

    /// Appends the exact JavaScript UTF-16 code units to `output`.
    pub fn write_utf16(self, output: &mut Vec<u16>) {
        output.reserve(self.utf8.encode_utf16().count());
        let mut fixups = self.fixups.iter().peekable();
        for (relative, character) in self.utf8.char_indices() {
            let Ok(relative) = u32::try_from(relative) else {
                debug_assert!(false, "packed text range exceeds 32 bits");
                return;
            };
            let Some(absolute) = self.byte_start.checked_add(relative) else {
                debug_assert!(false, "packed text range overflows 32 bits");
                return;
            };
            if fixups.peek().is_some_and(|fixup| fixup.byte_start == absolute) {
                if let Some(fixup) = fixups.next() {
                    debug_assert_eq!(character, SURROGATE_PLACEHOLDER);
                    output.push(fixup.unit);
                }
            } else {
                let mut units = [0_u16; 2];
                output.extend(character.encode_utf16(&mut units).iter().copied());
            }
        }
        let unconsumed = fixups.peek().is_some();
        debug_assert!(!unconsumed, "unconsumed surrogate fixups");
    }

    /// Returns the exact JavaScript UTF-16 code units.
    #[must_use]
    pub fn to_utf16(self) -> Vec<u16> {
        let mut output = Vec::new();
        self.write_utf16(&mut output);
        output
    }
}

/// Owned lossless packed JavaScript text released from a result table.
///
/// Ranges held by the table's destructively taken records remain valid against this storage. An
/// unpaired UTF-16 surrogate is retained as a sparse position-keyed fixup and never exposed as an
/// authored private-use scalar.
#[derive(Debug, Default)]
pub struct OwnedPackedTextStorage {
    storage: PackedTextStorage,
}

impl OwnedPackedTextStorage {
    /// Returns the entire storage as UTF-8 only when it contains no unpaired surrogate fixups.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        self.storage.fixups.is_none().then_some(&self.storage.utf8)
    }

    /// Returns one lossless packed value by its original table range.
    #[must_use]
    pub fn text(&self, range: StringRange) -> Option<PackedTextRef<'_>> {
        self.storage.text(range)
    }

    /// Returns one range as UTF-8 only when it contains no unpaired surrogate fixups.
    #[must_use]
    pub fn string(&self, range: StringRange) -> Option<&str> {
        self.text(range)?.as_str()
    }

    /// Returns true when the destructively released packed storage has no bytes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.storage.utf8.is_empty()
    }
}

#[derive(Default)]
pub(super) struct PackedTextStorage {
    pub(super) utf8: String,
    pub(super) fixups: Option<Vec<PackedTextFixup>>,
}

impl fmt::Debug for PackedTextStorage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug = formatter.debug_struct("PackedTextStorage");
        if self.fixups.is_none() {
            debug.field("utf8", &self.utf8);
        } else {
            debug
                .field("utf8", &"<lossless UTF-16 text>")
                .field("fixup_count", &self.fixups.as_ref().map_or(0, Vec::len));
        }
        debug.finish()
    }
}

impl PackedTextStorage {
    pub(super) const fn new() -> Self {
        Self { utf8: String::new(), fixups: None }
    }

    pub(super) fn len(&self) -> usize {
        self.utf8.len()
    }

    pub(super) fn is_released(&self) -> bool {
        self.utf8.capacity() == 0
            && self.fixups.as_ref().is_none_or(|fixups| fixups.capacity() == 0)
    }

    pub(super) fn as_str(&self) -> Option<&str> {
        self.fixups.is_none().then_some(&self.utf8)
    }

    pub(super) fn utf8_storage_mut(&mut self) -> &mut String {
        &mut self.utf8
    }

    pub(super) fn push_str(&mut self, value: &str) -> Result<StringRange, TapeBuildError> {
        let range = string_range(self.utf8.len(), value.len())?;
        self.utf8.push_str(value);
        Ok(range)
    }

    pub(super) fn push_utf16(&mut self, value: &[u16]) -> Result<StringRange, TapeBuildError> {
        let start = self.utf8.len();
        u32::try_from(start).map_err(|_| TapeBuildError::CapacityOverflow)?;
        let fixup_start = self.fixups.as_ref().map_or(0, Vec::len);
        let mut index = 0_usize;
        while index < value.len() {
            let unit = value[index];
            if (0xd800..=0xdbff).contains(&unit)
                && value.get(index + 1).is_some_and(|next| (0xdc00..=0xdfff).contains(next))
            {
                let high = u32::from(unit - 0xd800);
                let low = u32::from(value[index + 1] - 0xdc00);
                let scalar = 0x1_0000 + (high << 10) + low;
                self.utf8.push(char::from_u32(scalar).expect("validated surrogate pair"));
                index += 2;
                continue;
            }
            if (0xd800..=0xdfff).contains(&unit) {
                let Ok(byte_start) = u32::try_from(self.utf8.len()) else {
                    self.truncate(start, fixup_start);
                    return Err(TapeBuildError::CapacityOverflow);
                };
                self.utf8.push(SURROGATE_PLACEHOLDER);
                self.fixups.get_or_insert_with(|| Vec::with_capacity(4)).push(PackedTextFixup {
                    byte_start,
                    unit,
                    reserved: 0,
                });
                index += 1;
                continue;
            }
            self.utf8.push(char::from_u32(u32::from(unit)).expect("non-surrogate BMP scalar"));
            index += 1;
        }
        let range = match string_range(start, self.utf8.len() - start) {
            Ok(range) => range,
            Err(error) => {
                self.truncate(start, fixup_start);
                return Err(error);
            }
        };
        Ok(range)
    }

    pub(super) fn repair_utf16(
        &mut self,
        range: StringRange,
        value: &[u16],
    ) -> Result<(), TapeBuildError> {
        self.repair_utf16_batch(std::iter::once((range, value)))
    }

    pub(super) fn repair_utf16_batch<'a>(
        &mut self,
        repairs: impl IntoIterator<Item = (StringRange, &'a [u16])>,
    ) -> Result<(), TapeBuildError> {
        let mut positioned = Vec::new();
        let mut rejection_placeholders = Vec::new();
        let mut previous_end = None;
        for (range, value) in repairs {
            if previous_end.is_some_and(|end| range.start < end) {
                return Err(TapeBuildError::InvalidRecordIndex);
            }
            previous_end = Some(
                range.start.checked_add(range.length).ok_or(TapeBuildError::CapacityOverflow)?,
            );
            if let Some(existing) = self.fixups.as_deref() {
                let end = range
                    .start
                    .checked_add(range.length)
                    .ok_or(TapeBuildError::CapacityOverflow)?;
                let first = existing.partition_point(|fixup| fixup.byte_start < range.start);
                let last = existing.partition_point(|fixup| fixup.byte_start < end);
                if first != last {
                    return Err(TapeBuildError::InvalidRecordIndex);
                }
            }
            self.validate_utf16_repair(range, value, &mut positioned, &mut rejection_placeholders)?;
        }
        if positioned.is_empty() {
            return Ok(());
        }
        if self.fixups.is_none() {
            self.normalize_rejection_placeholders(&rejection_placeholders)?;
            self.fixups = Some(positioned);
            return Ok(());
        }
        if self
            .fixups
            .as_ref()
            .and_then(|existing| existing.last())
            .zip(positioned.first())
            .is_some_and(|(old, new)| old.byte_start < new.byte_start)
        {
            self.normalize_rejection_placeholders(&rejection_placeholders)?;
            self.fixups.as_mut().expect("checked existing fixup storage").extend(positioned);
            return Ok(());
        }
        let existing = self.fixups.as_deref().expect("checked fixup storage");
        let mut merged = Vec::with_capacity(
            existing.len().checked_add(positioned.len()).ok_or(TapeBuildError::CapacityOverflow)?,
        );
        let mut existing_index = 0_usize;
        let mut repair_index = 0_usize;
        while existing_index < existing.len() && repair_index < positioned.len() {
            match existing[existing_index].byte_start.cmp(&positioned[repair_index].byte_start) {
                std::cmp::Ordering::Less => {
                    merged.push(existing[existing_index]);
                    existing_index += 1;
                }
                std::cmp::Ordering::Greater => {
                    merged.push(positioned[repair_index]);
                    repair_index += 1;
                }
                std::cmp::Ordering::Equal => {
                    return Err(TapeBuildError::InvalidRecordIndex);
                }
            }
        }
        merged.extend_from_slice(&existing[existing_index..]);
        merged.extend_from_slice(&positioned[repair_index..]);
        self.normalize_rejection_placeholders(&rejection_placeholders)?;
        self.fixups = Some(merged);
        Ok(())
    }

    fn validate_utf16_repair(
        &self,
        range: StringRange,
        value: &[u16],
        positioned: &mut Vec<PackedTextFixup>,
        rejection_placeholders: &mut Vec<u32>,
    ) -> Result<(), TapeBuildError> {
        let existing =
            slice_range(&self.utf8, range).ok_or(TapeBuildError::InvalidRecordIndex)?.as_bytes();
        let positioned_start = positioned.len();
        let mut byte_offset = 0_usize;
        let mut index = 0_usize;
        while index < value.len() {
            let unit = value[index];
            if (0xd800..=0xdbff).contains(&unit)
                && value.get(index + 1).is_some_and(|next| (0xdc00..=0xdfff).contains(next))
            {
                let high = u32::from(unit - 0xd800);
                let low = u32::from(value[index + 1] - 0xdc00);
                let scalar = 0x1_0000 + (high << 10) + low;
                let character = char::from_u32(scalar).ok_or(TapeBuildError::InvalidRecordIndex)?;
                let mut buffer = [0_u8; 4];
                let encoded = character.encode_utf8(&mut buffer).as_bytes();
                if existing.get(byte_offset..byte_offset + encoded.len()) != Some(encoded) {
                    positioned.truncate(positioned_start);
                    return Err(TapeBuildError::InvalidRecordIndex);
                }
                byte_offset += encoded.len();
                index += 2;
                continue;
            }
            if (0xd800..=0xdfff).contains(&unit) {
                let placeholder = existing.get(byte_offset..byte_offset + 3);
                if placeholder != Some(SURROGATE_PLACEHOLDER_UTF8)
                    && placeholder != Some(REJECTION_PLACEHOLDER_UTF8)
                {
                    positioned.truncate(positioned_start);
                    return Err(TapeBuildError::InvalidRecordIndex);
                }
                let relative =
                    u32::try_from(byte_offset).map_err(|_| TapeBuildError::CapacityOverflow)?;
                let byte_start =
                    range.start.checked_add(relative).ok_or(TapeBuildError::CapacityOverflow)?;
                positioned.push(PackedTextFixup { byte_start, unit, reserved: 0 });
                if placeholder == Some(REJECTION_PLACEHOLDER_UTF8) {
                    rejection_placeholders.push(byte_start);
                }
                byte_offset += 3;
                index += 1;
                continue;
            }
            let character =
                char::from_u32(u32::from(unit)).ok_or(TapeBuildError::InvalidRecordIndex)?;
            let mut buffer = [0_u8; 4];
            let encoded = character.encode_utf8(&mut buffer).as_bytes();
            if existing.get(byte_offset..byte_offset + encoded.len()) != Some(encoded) {
                positioned.truncate(positioned_start);
                return Err(TapeBuildError::InvalidRecordIndex);
            }
            byte_offset += encoded.len();
            index += 1;
        }
        if byte_offset != existing.len() {
            positioned.truncate(positioned_start);
            return Err(TapeBuildError::InvalidRecordIndex);
        }
        Ok(())
    }

    fn normalize_rejection_placeholders(
        &mut self,
        positions: &[u32],
    ) -> Result<(), TapeBuildError> {
        if positions.is_empty() {
            return Ok(());
        }
        let source = self.utf8.as_bytes();
        let mut normalized = Vec::with_capacity(source.len());
        let mut cursor = 0_usize;
        for &position in positions {
            let position =
                usize::try_from(position).map_err(|_| TapeBuildError::InvalidRecordIndex)?;
            let end = position
                .checked_add(REJECTION_PLACEHOLDER_UTF8.len())
                .ok_or(TapeBuildError::CapacityOverflow)?;
            if position < cursor || source.get(position..end) != Some(REJECTION_PLACEHOLDER_UTF8) {
                return Err(TapeBuildError::InvalidRecordIndex);
            }
            normalized.extend_from_slice(
                source.get(cursor..position).ok_or(TapeBuildError::InvalidRecordIndex)?,
            );
            normalized.extend_from_slice(SURROGATE_PLACEHOLDER_UTF8);
            cursor = end;
        }
        normalized
            .extend_from_slice(source.get(cursor..).ok_or(TapeBuildError::InvalidRecordIndex)?);
        self.utf8 =
            String::from_utf8(normalized).map_err(|_| TapeBuildError::InvalidRecordIndex)?;
        Ok(())
    }

    pub(super) fn text(&self, range: StringRange) -> Option<PackedTextRef<'_>> {
        let utf8 = slice_range(&self.utf8, range)?;
        let end = range.start.checked_add(range.length)?;
        let all_fixups = self.fixups.as_deref().unwrap_or(&[]);
        let first = all_fixups.partition_point(|fixup| fixup.byte_start < range.start);
        let last = all_fixups.partition_point(|fixup| fixup.byte_start < end);
        let fixups = all_fixups.get(first..last)?;
        for fixup in fixups {
            let relative = usize::try_from(fixup.byte_start.checked_sub(range.start)?).ok()?;
            if !utf8.get(relative..).is_some_and(|tail| tail.starts_with(SURROGATE_PLACEHOLDER)) {
                return None;
            }
        }
        Some(PackedTextRef { utf8, byte_start: range.start, fixups })
    }

    pub(super) fn truncate(&mut self, utf8_length: usize, fixup_length: usize) {
        self.utf8.truncate(utf8_length);
        if let Some(fixups) = self.fixups.as_mut() {
            fixups.truncate(fixup_length);
            if fixups.is_empty() {
                self.fixups = None;
            }
        }
    }

    pub(super) fn take_utf8(&mut self) -> Result<String, TapeBuildError> {
        if self.fixups.as_ref().is_some_and(|fixups| !fixups.is_empty()) {
            return Err(TapeBuildError::InvalidRecordIndex);
        }
        self.fixups = None;
        Ok(mem::take(&mut self.utf8))
    }

    pub(super) fn take_owned(&mut self) -> OwnedPackedTextStorage {
        OwnedPackedTextStorage { storage: mem::take(self) }
    }
}
