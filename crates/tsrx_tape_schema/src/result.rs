use std::{fmt, mem};

use crate::{ListRange, ProjectedCommentKind, RecordIndex, StringRange, TapeBuildError, TapeSpan};

const SURROGATE_PLACEHOLDER: char = '\u{e000}';
const SURROGATE_PLACEHOLDER_UTF8: &[u8] = b"\xee\x80\x80";
const REJECTION_PLACEHOLDER_UTF8: &[u8] = b"\xef\xbf\xbf";

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct PackedTextFixup {
    byte_start: u32,
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
struct PackedTextStorage {
    utf8: String,
    fixups: Option<Vec<PackedTextFixup>>,
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
    const fn new() -> Self {
        Self { utf8: String::new(), fixups: None }
    }

    fn len(&self) -> usize {
        self.utf8.len()
    }

    fn is_released(&self) -> bool {
        self.utf8.capacity() == 0
            && self.fixups.as_ref().is_none_or(|fixups| fixups.capacity() == 0)
    }

    fn as_str(&self) -> Option<&str> {
        self.fixups.is_none().then_some(&self.utf8)
    }

    fn utf8_storage_mut(&mut self) -> &mut String {
        &mut self.utf8
    }

    fn push_str(&mut self, value: &str) -> Result<StringRange, TapeBuildError> {
        let range = string_range(self.utf8.len(), value.len())?;
        self.utf8.push_str(value);
        Ok(range)
    }

    fn push_utf16(&mut self, value: &[u16]) -> Result<StringRange, TapeBuildError> {
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

    fn repair_utf16(&mut self, range: StringRange, value: &[u16]) -> Result<(), TapeBuildError> {
        self.repair_utf16_batch(std::iter::once((range, value)))
    }

    fn repair_utf16_batch<'a>(
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

    fn text(&self, range: StringRange) -> Option<PackedTextRef<'_>> {
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

    fn truncate(&mut self, utf8_length: usize, fixup_length: usize) {
        self.utf8.truncate(utf8_length);
        if let Some(fixups) = self.fixups.as_mut() {
            fixups.truncate(fixup_length);
            if fixups.is_empty() {
                self.fixups = None;
            }
        }
    }

    fn take_utf8(&mut self) -> Result<String, TapeBuildError> {
        if self.fixups.as_ref().is_some_and(|fixups| !fixups.is_empty()) {
            return Err(TapeBuildError::InvalidRecordIndex);
        }
        self.fixups = None;
        Ok(mem::take(&mut self.utf8))
    }

    fn take_owned(&mut self) -> OwnedPackedTextStorage {
        OwnedPackedTextStorage { storage: mem::take(self) }
    }
}

/// Whether a canonical parse produced a complete, recovered, or failed result.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParseCompleteness {
    Complete = 1,
    Recovered = 2,
    Failed = 3,
}

/// Coordinate system used by every span in one result.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CoordinateDomain {
    ProjectedUtf8Bytes = 1,
    AuthoredUtf8Bytes = 2,
    OriginalUtf16Units = 3,
}

/// The public kind of a static import name.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImportNameKind {
    Name = 1,
    NamespaceObject = 2,
    Default = 3,
}

/// The public kind of the imported side of a static export entry.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExportImportNameKind {
    Name = 1,
    All = 2,
    AllButDefault = 3,
    None = 4,
}

/// The public kind of the exported side of a static export entry.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExportExportNameKind {
    Name = 1,
    Default = 2,
    None = 3,
}

/// The public kind of the local side of a static export entry.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExportLocalNameKind {
    Name = 1,
    Default = 2,
    None = 3,
}

/// Stable diagnostic severity independent of the pinned OXC revision.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiagnosticSeverity {
    Error = 1,
    Warning = 2,
    Advice = 3,
}

/// The pass that emitted a diagnostic.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiagnosticPhase {
    Grammar = 1,
    Semantic = 2,
    Recovery = 3,
}

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

/// One optional module name and span paired with its context-specific kind.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ModuleNameRecord<K = ImportNameKind> {
    pub kind: K,
    pub name: OptionalStringRange,
    pub span: OptionalTapeSpan,
}

impl<K> ModuleNameRecord<K> {
    #[must_use]
    pub const fn new(kind: K, name: OptionalStringRange, span: OptionalTapeSpan) -> Self {
        Self { kind, name, span }
    }
}

/// One entry belonging to a static import statement.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StaticImportEntryRecord {
    pub import_name: ModuleNameRecord<ImportNameKind>,
    pub local_name: ValueSpanRecord,
    pub is_type: bool,
}

impl StaticImportEntryRecord {
    #[must_use]
    pub const fn new(
        import_name: ModuleNameRecord<ImportNameKind>,
        local_name: ValueSpanRecord,
        is_type: bool,
    ) -> Self {
        Self { import_name, local_name, is_type }
    }
}

/// One source-ordered static import statement.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StaticImportRecord {
    pub span: TapeSpan,
    pub module_request: ValueSpanRecord,
    pub entries: ListRange,
}

impl StaticImportRecord {
    #[must_use]
    pub const fn new(span: TapeSpan, module_request: ValueSpanRecord, entries: ListRange) -> Self {
        Self { span, module_request, entries }
    }
}

/// One entry belonging to a static export statement.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StaticExportEntryRecord {
    pub span: TapeSpan,
    pub module_request: OptionalValueSpanRecord,
    pub import_name: ModuleNameRecord<ExportImportNameKind>,
    pub export_name: ModuleNameRecord<ExportExportNameKind>,
    pub local_name: ModuleNameRecord<ExportLocalNameKind>,
    pub is_type: bool,
}

impl StaticExportEntryRecord {
    #[must_use]
    pub const fn new(
        span: TapeSpan,
        module_request: OptionalValueSpanRecord,
        import_name: ModuleNameRecord<ExportImportNameKind>,
        export_name: ModuleNameRecord<ExportExportNameKind>,
        local_name: ModuleNameRecord<ExportLocalNameKind>,
        is_type: bool,
    ) -> Self {
        Self { span, module_request, import_name, export_name, local_name, is_type }
    }
}

/// One source-ordered static export statement.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StaticExportRecord {
    pub span: TapeSpan,
    pub entries: ListRange,
}

impl StaticExportRecord {
    #[must_use]
    pub const fn new(span: TapeSpan, entries: ListRange) -> Self {
        Self { span, entries }
    }
}

/// One dynamic import expression.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DynamicImportRecord {
    pub span: TapeSpan,
    pub module_request: TapeSpan,
}

impl DynamicImportRecord {
    #[must_use]
    pub const fn new(span: TapeSpan, module_request: TapeSpan) -> Self {
        Self { span, module_request }
    }
}

/// Owned, OXC-independent flat module record.
#[derive(Debug, Default)]
pub struct ModuleTable {
    has_module_syntax: bool,
    static_imports: Vec<StaticImportRecord>,
    static_import_entries: Vec<StaticImportEntryRecord>,
    static_exports: Vec<StaticExportRecord>,
    static_export_entries: Vec<StaticExportEntryRecord>,
    dynamic_imports: Vec<DynamicImportRecord>,
    import_metas: Vec<TapeSpan>,
    strings: PackedTextStorage,
}

impl ModuleTable {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            has_module_syntax: false,
            static_imports: Vec::new(),
            static_import_entries: Vec::new(),
            static_exports: Vec::new(),
            static_export_entries: Vec::new(),
            dynamic_imports: Vec::new(),
            import_metas: Vec::new(),
            strings: PackedTextStorage::new(),
        }
    }

    pub fn set_has_module_syntax(&mut self, has_module_syntax: bool) {
        self.has_module_syntax = has_module_syntax;
    }

    /// Maps every source span stored by this module table in one allocation-free pass.
    ///
    /// Packed string and list ranges are storage coordinates and are never passed to `mapper`.
    /// Each present source span is passed exactly once. If `mapper` fails, its error is returned
    /// immediately; spans mapped before that error remain updated, so callers should discard the
    /// table on failure.
    ///
    /// # Errors
    ///
    /// Returns the first error produced by `mapper`.
    pub fn try_map_spans<E>(
        &mut self,
        mut mapper: impl FnMut(TapeSpan) -> Result<TapeSpan, E>,
    ) -> Result<(), E> {
        for record in &mut self.static_imports {
            try_map_span(&mut record.span, &mut mapper)?;
            try_map_span(&mut record.module_request.span, &mut mapper)?;
        }
        for entry in &mut self.static_import_entries {
            try_map_optional_span(&mut entry.import_name.span, &mut mapper)?;
            try_map_span(&mut entry.local_name.span, &mut mapper)?;
        }
        for record in &mut self.static_exports {
            try_map_span(&mut record.span, &mut mapper)?;
        }
        for entry in &mut self.static_export_entries {
            try_map_span(&mut entry.span, &mut mapper)?;
            try_map_optional_value_span(&mut entry.module_request, &mut mapper)?;
            try_map_optional_span(&mut entry.import_name.span, &mut mapper)?;
            try_map_optional_span(&mut entry.export_name.span, &mut mapper)?;
            try_map_optional_span(&mut entry.local_name.span, &mut mapper)?;
        }
        for record in &mut self.dynamic_imports {
            try_map_span(&mut record.span, &mut mapper)?;
            try_map_span(&mut record.module_request, &mut mapper)?;
        }
        for span in &mut self.import_metas {
            try_map_span(span, &mut mapper)?;
        }
        Ok(())
    }

    /// Destructively takes the static-import columns for authored reconstruction.
    #[must_use]
    pub fn take_static_imports(
        &mut self,
    ) -> (Vec<StaticImportRecord>, Vec<StaticImportEntryRecord>) {
        (mem::take(&mut self.static_imports), mem::take(&mut self.static_import_entries))
    }

    /// Destructively takes the static-export columns for authored reconstruction.
    #[must_use]
    pub fn take_static_exports(
        &mut self,
    ) -> (Vec<StaticExportRecord>, Vec<StaticExportEntryRecord>) {
        (mem::take(&mut self.static_exports), mem::take(&mut self.static_export_entries))
    }

    /// Destructively takes the dynamic-import column for authored reconstruction.
    #[must_use]
    pub fn take_dynamic_imports(&mut self) -> Vec<DynamicImportRecord> {
        mem::take(&mut self.dynamic_imports)
    }

    /// Destructively takes the `import.meta` column for authored reconstruction.
    #[must_use]
    pub fn take_import_metas(&mut self) -> Vec<TapeSpan> {
        mem::take(&mut self.import_metas)
    }

    /// Destructively takes the packed module string buffer.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::InvalidRecordIndex`] if lossless surrogate fixups remain; use
    /// [`Self::take_text_storage`] for arbitrary JavaScript strings.
    pub fn take_string_storage(&mut self) -> Result<String, TapeBuildError> {
        self.strings.take_utf8()
    }

    /// Destructively takes module text without losing unpaired UTF-16 surrogate fixups.
    #[must_use]
    pub fn take_text_storage(&mut self) -> OwnedPackedTextStorage {
        self.strings.take_owned()
    }

    /// Returns true after every owned record and string allocation has been mem-taken.
    #[must_use]
    pub fn is_storage_released(&self) -> bool {
        self.static_imports.capacity() == 0
            && self.static_import_entries.capacity() == 0
            && self.static_exports.capacity() == 0
            && self.static_export_entries.capacity() == 0
            && self.dynamic_imports.capacity() == 0
            && self.import_metas.capacity() == 0
            && self.strings.is_released()
    }

    #[must_use]
    pub const fn has_module_syntax(&self) -> bool {
        self.has_module_syntax
    }

    /// Appends a module-owned string to packed storage.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] if the 32-bit range limit is exceeded.
    pub fn push_string(&mut self, value: &str) -> Result<StringRange, TapeBuildError> {
        self.strings.push_str(value)
    }

    /// Appends exact JavaScript UTF-16 code units to packed module storage.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] if the 32-bit range limit is exceeded.
    pub fn push_utf16(&mut self, value: &[u16]) -> Result<StringRange, TapeBuildError> {
        self.strings.push_utf16(value)
    }

    /// Attaches collision-safe UTF-16 fixups to an existing same-width module value.
    ///
    /// # Errors
    ///
    /// Returns an error unless `value` encodes to the exact existing UTF-8/placeholder bytes.
    pub fn repair_utf16(
        &mut self,
        range: StringRange,
        value: &[u16],
    ) -> Result<(), TapeBuildError> {
        self.strings.repair_utf16(range, value)
    }

    /// Attaches collision-safe UTF-16 fixups to ordered, disjoint module values in one merge.
    ///
    /// # Errors
    ///
    /// Returns an error unless ranges are ordered and disjoint and every value encodes to its
    /// exact existing UTF-8/placeholder bytes.
    pub fn repair_utf16_batch<'a>(
        &mut self,
        repairs: impl IntoIterator<Item = (StringRange, &'a [u16])>,
    ) -> Result<(), TapeBuildError> {
        self.strings.repair_utf16_batch(repairs)
    }

    #[must_use]
    pub fn string(&self, range: StringRange) -> Option<&str> {
        self.text(range)?.as_str()
    }

    /// Returns one lossless packed JavaScript string, including unpaired UTF-16 units.
    #[must_use]
    pub fn text(&self, range: StringRange) -> Option<PackedTextRef<'_>> {
        self.strings.text(range)
    }

    /// Returns the complete packed buffer only when every value is valid UTF-8.
    #[must_use]
    pub fn string_storage(&self) -> Option<&str> {
        self.strings.as_str()
    }

    #[must_use]
    pub fn value(&self, value: ValueSpanRecord) -> Option<&str> {
        self.string(value.value)
    }

    /// Returns one lossless module value, including unpaired UTF-16 units.
    #[must_use]
    pub fn value_text(&self, value: ValueSpanRecord) -> Option<PackedTextRef<'_>> {
        self.text(value.value)
    }

    #[must_use]
    pub fn optional_value(&self, value: OptionalValueSpanRecord) -> Option<&str> {
        self.value(value.get()?)
    }

    #[must_use]
    #[expect(
        clippy::needless_pass_by_value,
        reason = "the record is a small copyable index pair; borrowing it would add an indirection to a field read"
    )]
    pub fn name<K>(&self, name: ModuleNameRecord<K>) -> Option<&str> {
        self.string(name.name.get()?)
    }

    /// Captures the checked start cursor for a directly emitted static-import entry group.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] if the cursor exceeds the 32-bit range domain.
    pub fn begin_static_import_entries(&self) -> Result<u32, TapeBuildError> {
        begin_direct_range(&self.static_import_entries)
    }

    /// Finishes a directly emitted static-import entry group.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] if the checked range exceeds the 32-bit
    /// table domain. Returns
    /// [`TapeBuildError::InvalidRecordIndex`] unless exactly `expected_length` entries were
    /// appended after `start`.
    pub fn finish_static_import_entries(
        &self,
        start: u32,
        expected_length: u32,
    ) -> Result<ListRange, TapeBuildError> {
        finish_direct_range(&self.static_import_entries, start, expected_length)
    }

    /// Appends an import entry.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] at the 32-bit table limit.
    pub fn push_static_import_entry(
        &mut self,
        record: StaticImportEntryRecord,
    ) -> Result<RecordIndex, TapeBuildError> {
        push_record(&mut self.static_import_entries, record)
    }

    /// Appends a contiguous import-entry group without an intermediate allocation.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] at the 32-bit table limit.
    pub fn append_static_import_entries<I>(
        &mut self,
        records: I,
    ) -> Result<ListRange, TapeBuildError>
    where
        I: IntoIterator<Item = StaticImportEntryRecord>,
    {
        append_records(&mut self.static_import_entries, records)
    }

    /// Appends a static import statement.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] at the 32-bit table limit.
    pub fn push_static_import(
        &mut self,
        record: StaticImportRecord,
    ) -> Result<RecordIndex, TapeBuildError> {
        push_record(&mut self.static_imports, record)
    }

    /// Appends an export entry.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] at the 32-bit table limit.
    pub fn push_static_export_entry(
        &mut self,
        record: StaticExportEntryRecord,
    ) -> Result<RecordIndex, TapeBuildError> {
        push_record(&mut self.static_export_entries, record)
    }

    /// Captures the checked start cursor for a directly emitted static-export entry group.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] if the cursor exceeds the 32-bit range domain.
    pub fn begin_static_export_entries(&self) -> Result<u32, TapeBuildError> {
        begin_direct_range(&self.static_export_entries)
    }

    /// Finishes a directly emitted static-export entry group.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] if the checked range exceeds the 32-bit
    /// table domain. Returns
    /// [`TapeBuildError::InvalidRecordIndex`] unless exactly `expected_length` entries were
    /// appended after `start`.
    pub fn finish_static_export_entries(
        &self,
        start: u32,
        expected_length: u32,
    ) -> Result<ListRange, TapeBuildError> {
        finish_direct_range(&self.static_export_entries, start, expected_length)
    }

    /// Appends a contiguous export-entry group without an intermediate allocation.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] at the 32-bit table limit.
    pub fn append_static_export_entries<I>(
        &mut self,
        records: I,
    ) -> Result<ListRange, TapeBuildError>
    where
        I: IntoIterator<Item = StaticExportEntryRecord>,
    {
        append_records(&mut self.static_export_entries, records)
    }

    /// Appends a static export statement.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] at the 32-bit table limit.
    pub fn push_static_export(
        &mut self,
        record: StaticExportRecord,
    ) -> Result<RecordIndex, TapeBuildError> {
        push_record(&mut self.static_exports, record)
    }

    /// Appends a dynamic import expression.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] at the 32-bit table limit.
    pub fn push_dynamic_import(
        &mut self,
        record: DynamicImportRecord,
    ) -> Result<RecordIndex, TapeBuildError> {
        push_record(&mut self.dynamic_imports, record)
    }

    /// Appends an `import.meta` span.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] at the 32-bit table limit.
    pub fn push_import_meta(&mut self, span: TapeSpan) -> Result<RecordIndex, TapeBuildError> {
        push_record(&mut self.import_metas, span)
    }

    #[must_use]
    pub fn static_imports(&self) -> &[StaticImportRecord] {
        &self.static_imports
    }

    #[must_use]
    pub fn static_import_entries(&self, range: ListRange) -> Option<&[StaticImportEntryRecord]> {
        range_slice(&self.static_import_entries, range)
    }

    #[must_use]
    pub fn static_exports(&self) -> &[StaticExportRecord] {
        &self.static_exports
    }

    #[must_use]
    pub fn static_export_entries(&self, range: ListRange) -> Option<&[StaticExportEntryRecord]> {
        range_slice(&self.static_export_entries, range)
    }

    #[must_use]
    pub fn dynamic_imports(&self) -> &[DynamicImportRecord] {
        &self.dynamic_imports
    }

    #[must_use]
    pub fn import_metas(&self) -> &[TapeSpan] {
        &self.import_metas
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.static_imports.is_empty()
            && self.static_exports.is_empty()
            && self.dynamic_imports.is_empty()
            && self.import_metas.is_empty()
    }
}

/// One source-ordered comment with its delimiter-free value.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CommentRecord {
    pub kind: ProjectedCommentKind,
    pub span: TapeSpan,
    pub value: StringRange,
}

impl CommentRecord {
    #[must_use]
    pub const fn new(kind: ProjectedCommentKind, span: TapeSpan, value: StringRange) -> Self {
        Self { kind, span, value }
    }
}

/// Independent packed comment table.
#[derive(Debug, Default)]
pub struct CommentTable {
    records: Vec<CommentRecord>,
    strings: PackedTextStorage,
}

impl CommentTable {
    #[must_use]
    pub const fn new() -> Self {
        Self { records: Vec::new(), strings: PackedTextStorage::new() }
    }

    /// Maps every comment source span in one allocation-free pass.
    ///
    /// Comment values and their packed string ranges are left untouched. If `mapper` fails, its
    /// error is returned immediately; spans mapped before that error remain updated, so callers
    /// should discard the table on failure.
    ///
    /// # Errors
    ///
    /// Returns the first error produced by `mapper`.
    pub fn try_map_spans<E>(
        &mut self,
        mut mapper: impl FnMut(TapeSpan) -> Result<TapeSpan, E>,
    ) -> Result<(), E> {
        for record in &mut self.records {
            try_map_span(&mut record.span, &mut mapper)?;
        }
        Ok(())
    }

    /// Appends one comment and copies its delimiter-free value once.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] at a 32-bit table or string limit.
    pub fn push(
        &mut self,
        kind: ProjectedCommentKind,
        span: TapeSpan,
        value: &str,
    ) -> Result<RecordIndex, TapeBuildError> {
        table_has_room(&self.records)?;
        let value = self.strings.push_str(value)?;
        push_record(&mut self.records, CommentRecord::new(kind, span, value))
    }

    /// Appends one comment from exact JavaScript UTF-16 code units.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] at a 32-bit table or string limit.
    pub fn push_utf16(
        &mut self,
        kind: ProjectedCommentKind,
        span: TapeSpan,
        value: &[u16],
    ) -> Result<RecordIndex, TapeBuildError> {
        table_has_room(&self.records)?;
        let string_start = self.strings.len();
        let fixup_start = self.strings.fixups.as_ref().map_or(0, Vec::len);
        let value = self.strings.push_utf16(value)?;
        match push_record(&mut self.records, CommentRecord::new(kind, span, value)) {
            Ok(index) => Ok(index),
            Err(error) => {
                self.strings.truncate(string_start, fixup_start);
                Err(error)
            }
        }
    }

    /// Attaches collision-safe UTF-16 fixups to an existing same-width comment value.
    ///
    /// # Errors
    ///
    /// Returns an error unless `value` encodes to the exact existing UTF-8/placeholder bytes.
    pub fn repair_utf16(
        &mut self,
        range: StringRange,
        value: &[u16],
    ) -> Result<(), TapeBuildError> {
        self.strings.repair_utf16(range, value)
    }

    /// Attaches collision-safe UTF-16 fixups to ordered, disjoint comment values in one merge.
    ///
    /// # Errors
    ///
    /// Returns an error unless ranges are ordered and disjoint and every value encodes to its
    /// exact existing UTF-8/placeholder bytes.
    pub fn repair_utf16_batch<'a>(
        &mut self,
        repairs: impl IntoIterator<Item = (StringRange, &'a [u16])>,
    ) -> Result<(), TapeBuildError> {
        self.strings.repair_utf16_batch(repairs)
    }

    #[must_use]
    pub fn records(&self) -> &[CommentRecord] {
        &self.records
    }

    #[must_use]
    pub fn value(&self, comment: &CommentRecord) -> Option<&str> {
        self.value_text(comment)?.as_str()
    }

    /// Returns one lossless comment value, including unpaired UTF-16 units.
    #[must_use]
    pub fn value_text(&self, comment: &CommentRecord) -> Option<PackedTextRef<'_>> {
        self.strings.text(comment.value)
    }

    /// Returns the complete packed buffer only when every value is valid UTF-8.
    #[must_use]
    pub fn string_storage(&self) -> Option<&str> {
        self.strings.as_str()
    }

    /// Destructively takes comment records for authored reconstruction.
    #[must_use]
    pub fn take_records(&mut self) -> Vec<CommentRecord> {
        mem::take(&mut self.records)
    }

    /// Destructively takes the packed comment string buffer.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::InvalidRecordIndex`] if lossless surrogate fixups remain; use
    /// [`Self::take_text_storage`] for arbitrary JavaScript strings.
    pub fn take_string_storage(&mut self) -> Result<String, TapeBuildError> {
        self.strings.take_utf8()
    }

    /// Destructively takes comment text without losing unpaired UTF-16 surrogate fixups.
    #[must_use]
    pub fn take_text_storage(&mut self) -> OwnedPackedTextStorage {
        self.strings.take_owned()
    }

    /// Returns true after every owned record and string allocation has been mem-taken.
    #[must_use]
    pub fn is_storage_released(&self) -> bool {
        self.records.capacity() == 0 && self.strings.is_released()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

/// One source label attached to a diagnostic.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DiagnosticLabelRecord {
    pub span: TapeSpan,
    pub message: OptionalStringRange,
    pub primary: bool,
}

impl DiagnosticLabelRecord {
    #[must_use]
    pub const fn new(span: TapeSpan, message: OptionalStringRange, primary: bool) -> Self {
        Self { span, message, primary }
    }
}

/// One structured grammar or semantic diagnostic.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DiagnosticRecord {
    pub phase: DiagnosticPhase,
    pub severity: DiagnosticSeverity,
    pub message: StringRange,
    pub labels: ListRange,
    pub help: OptionalStringRange,
    pub note: OptionalStringRange,
    pub code_scope: OptionalStringRange,
    pub code_number: OptionalStringRange,
    pub url: OptionalStringRange,
    pub codeframe: OptionalStringRange,
}

impl DiagnosticRecord {
    #[expect(
        clippy::too_many_arguments,
        reason = "a const field-by-field constructor: one parameter per record field"
    )]
    #[must_use]
    pub const fn new(
        phase: DiagnosticPhase,
        severity: DiagnosticSeverity,
        message: StringRange,
        labels: ListRange,
        help: OptionalStringRange,
        note: OptionalStringRange,
        code_scope: OptionalStringRange,
        code_number: OptionalStringRange,
        url: OptionalStringRange,
        codeframe: OptionalStringRange,
    ) -> Self {
        Self {
            phase,
            severity,
            message,
            labels,
            help,
            note,
            code_scope,
            code_number,
            url,
            codeframe,
        }
    }
}

/// Independent packed diagnostic and label tables.
#[derive(Debug, Default)]
pub struct DiagnosticTable {
    records: Vec<DiagnosticRecord>,
    labels: Vec<DiagnosticLabelRecord>,
    strings: PackedTextStorage,
}

/// Append-only packed-string writer supplied to diagnostic render callbacks.
///
/// Callers cannot construct it or mutate previously packed bytes.
pub struct PackedStringWriter<'a> {
    storage: &'a mut String,
}

impl fmt::Write for PackedStringWriter<'_> {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        self.storage.push_str(value);
        Ok(())
    }
}

impl DiagnosticTable {
    #[must_use]
    pub const fn new() -> Self {
        Self { records: Vec::new(), labels: Vec::new(), strings: PackedTextStorage::new() }
    }

    /// Maps every diagnostic-label source span in one allocation-free pass.
    ///
    /// Diagnostic records contain only packed table/string ranges, not source spans, and are left
    /// untouched. If `mapper` fails, its error is returned immediately; spans mapped before that
    /// error remain updated, so callers should discard the table on failure.
    ///
    /// # Errors
    ///
    /// Returns the first error produced by `mapper`.
    pub fn try_map_spans<E>(
        &mut self,
        mut mapper: impl FnMut(TapeSpan) -> Result<TapeSpan, E>,
    ) -> Result<(), E> {
        for label in &mut self.labels {
            try_map_span(&mut label.span, &mut mapper)?;
        }
        Ok(())
    }

    /// Appends a diagnostic-owned string to packed storage.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] if the 32-bit range limit is exceeded.
    pub fn push_string(&mut self, value: &str) -> Result<StringRange, TapeBuildError> {
        self.strings.push_str(value)
    }

    /// Appends exact JavaScript UTF-16 code units to packed diagnostic storage.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] if the 32-bit range limit is exceeded.
    pub fn push_utf16(&mut self, value: &[u16]) -> Result<StringRange, TapeBuildError> {
        self.strings.push_utf16(value)
    }

    /// Attaches collision-safe UTF-16 fixups to ordered, disjoint diagnostic strings in one merge.
    ///
    /// # Errors
    ///
    /// Returns an error unless ranges are ordered and disjoint and every value encodes to its
    /// exact existing UTF-8/placeholder bytes.
    pub fn repair_utf16_batch<'a>(
        &mut self,
        repairs: impl IntoIterator<Item = (StringRange, &'a [u16])>,
    ) -> Result<(), TapeBuildError> {
        self.strings.repair_utf16_batch(repairs)
    }

    /// Appends an optional diagnostic-owned string.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] if the 32-bit range limit is exceeded.
    pub fn push_optional_string(
        &mut self,
        value: Option<&str>,
    ) -> Result<OptionalStringRange, TapeBuildError> {
        value.map_or(Ok(OptionalStringRange::NONE), |value| {
            self.push_string(value).map(OptionalStringRange::some)
        })
    }

    #[must_use]
    pub fn string(&self, range: StringRange) -> Option<&str> {
        self.text(range)?.as_str()
    }

    /// Returns one lossless diagnostic string, including unpaired UTF-16 units.
    #[must_use]
    pub fn text(&self, range: StringRange) -> Option<PackedTextRef<'_>> {
        self.strings.text(range)
    }

    #[must_use]
    pub fn optional_string(&self, range: OptionalStringRange) -> Option<&str> {
        self.string(range.get()?)
    }

    /// Returns one optional lossless diagnostic string.
    #[must_use]
    pub fn optional_text(&self, range: OptionalStringRange) -> Option<PackedTextRef<'_>> {
        self.text(range.get()?)
    }

    /// Returns the complete packed buffer only when every value is valid UTF-8.
    #[must_use]
    pub fn string_storage(&self) -> Option<&str> {
        self.strings.as_str()
    }

    /// Destructively takes diagnostic and label columns for authored reconstruction.
    #[must_use]
    pub fn take_records_and_labels(
        &mut self,
    ) -> (Vec<DiagnosticRecord>, Vec<DiagnosticLabelRecord>) {
        (mem::take(&mut self.records), mem::take(&mut self.labels))
    }

    /// Destructively takes the packed diagnostic string buffer.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::InvalidRecordIndex`] if lossless surrogate fixups remain; use
    /// [`Self::take_text_storage`] for arbitrary JavaScript strings.
    pub fn take_string_storage(&mut self) -> Result<String, TapeBuildError> {
        self.strings.take_utf8()
    }

    /// Destructively takes diagnostic text without losing unpaired UTF-16 surrogate fixups.
    #[must_use]
    pub fn take_text_storage(&mut self) -> OwnedPackedTextStorage {
        self.strings.take_owned()
    }

    /// Returns true after every owned record and string allocation has been mem-taken.
    #[must_use]
    pub fn is_storage_released(&self) -> bool {
        self.records.capacity() == 0 && self.labels.capacity() == 0 && self.strings.is_released()
    }

    /// Appends one diagnostic label.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] at a 32-bit table or string limit.
    pub fn push_label(
        &mut self,
        span: TapeSpan,
        message: Option<&str>,
    ) -> Result<RecordIndex, TapeBuildError> {
        self.push_labeled(span, message, false)
    }

    /// Appends one diagnostic label with its codeframe-primary marker.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] at a 32-bit table or string limit.
    pub fn push_labeled(
        &mut self,
        span: TapeSpan,
        message: Option<&str>,
        primary: bool,
    ) -> Result<RecordIndex, TapeBuildError> {
        table_has_room(&self.labels)?;
        let message = self.push_optional_string(message)?;
        push_record(&mut self.labels, DiagnosticLabelRecord::new(span, message, primary))
    }

    /// Captures the checked start cursor for a directly emitted diagnostic-label group.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] if the cursor exceeds the 32-bit range domain.
    pub fn begin_labels(&self) -> Result<u32, TapeBuildError> {
        begin_direct_range(&self.labels)
    }

    /// Finishes a directly emitted diagnostic-label group.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] if the checked range exceeds the 32-bit
    /// table domain. Returns
    /// [`TapeBuildError::InvalidRecordIndex`] unless exactly `expected_length` labels were
    /// appended after `start`.
    pub fn finish_labels(
        &self,
        start: u32,
        expected_length: u32,
    ) -> Result<ListRange, TapeBuildError> {
        finish_direct_range(&self.labels, start, expected_length)
    }

    /// Appends a contiguous diagnostic-label group without an intermediate allocation.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] at a 32-bit table or string limit.
    pub fn append_labels<'a, I>(&mut self, labels: I) -> Result<ListRange, TapeBuildError>
    where
        I: IntoIterator<Item = (TapeSpan, Option<&'a str>, bool)>,
    {
        let start = self.labels.len();
        let string_start = self.strings.len();
        let fixup_start = self.strings.fixups.as_ref().map_or(0, Vec::len);
        for (span, message, primary) in labels {
            if let Err(error) = self.push_labeled(span, message, primary) {
                self.labels.truncate(start);
                self.strings.truncate(string_start, fixup_start);
                return Err(error);
            }
        }
        list_range(start, self.labels.len() - start)
    }

    /// Appends a fully packed diagnostic record.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] at the 32-bit table limit.
    pub fn push_record(&mut self, record: DiagnosticRecord) -> Result<RecordIndex, TapeBuildError> {
        push_record(&mut self.records, record)
    }

    /// Packs and appends one diagnostic while retaining every public OXC metadata string.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError::CapacityOverflow`] at a 32-bit table or string limit.
    #[expect(clippy::too_many_arguments, reason = "one parameter per diagnostic record field")]
    pub fn push_diagnostic(
        &mut self,
        phase: DiagnosticPhase,
        severity: DiagnosticSeverity,
        message: &str,
        labels: ListRange,
        help: Option<&str>,
        note: Option<&str>,
        code_scope: Option<&str>,
        code_number: Option<&str>,
        url: Option<&str>,
        codeframe: Option<&str>,
    ) -> Result<RecordIndex, TapeBuildError> {
        table_has_room(&self.records)?;
        let string_start = self.strings.len();
        let fixup_start = self.strings.fixups.as_ref().map_or(0, Vec::len);
        let packed = (|| {
            Ok(DiagnosticRecord::new(
                phase,
                severity,
                self.push_string(message)?,
                labels,
                self.push_optional_string(help)?,
                self.push_optional_string(note)?,
                self.push_optional_string(code_scope)?,
                self.push_optional_string(code_number)?,
                self.push_optional_string(url)?,
                self.push_optional_string(codeframe)?,
            ))
        })();
        let record = match packed {
            Ok(record) => record,
            Err(error) => {
                self.strings.truncate(string_start, fixup_start);
                return Err(error);
            }
        };
        push_record(&mut self.records, record)
    }

    /// Replaces the optional codeframe string for a previously appended diagnostic.
    ///
    /// # Errors
    ///
    /// Returns an invalid-index or capacity error without changing the record.
    pub fn set_codeframe(
        &mut self,
        diagnostic: RecordIndex,
        codeframe: Option<&str>,
    ) -> Result<(), TapeBuildError> {
        let index = index_usize(diagnostic);
        self.records.get(index).ok_or(TapeBuildError::InvalidRecordIndex)?;
        let codeframe = self.push_optional_string(codeframe)?;
        self.records[index].codeframe = codeframe;
        Ok(())
    }

    /// Renders a codeframe directly into packed storage without a temporary owned string.
    ///
    /// The inner result reports a formatter failure. Either failure path leaves the diagnostic and
    /// packed storage unchanged.
    ///
    /// # Errors
    ///
    /// Returns an invalid-index or capacity error without changing the record.
    pub fn write_codeframe<F>(
        &mut self,
        diagnostic: RecordIndex,
        render: F,
    ) -> Result<Result<(), fmt::Error>, TapeBuildError>
    where
        F: FnOnce(&mut PackedStringWriter<'_>) -> fmt::Result,
    {
        let index = index_usize(diagnostic);
        self.records.get(index).ok_or(TapeBuildError::InvalidRecordIndex)?;
        let start = self.strings.len();
        let fixup_start = self.strings.fixups.as_ref().map_or(0, Vec::len);
        u32::try_from(start).map_err(|_| TapeBuildError::CapacityOverflow)?;
        let rendered = {
            let mut writer = PackedStringWriter { storage: self.strings.utf8_storage_mut() };
            render(&mut writer)
        };
        if let Err(error) = rendered {
            self.strings.truncate(start, fixup_start);
            return Ok(Err(error));
        }
        let codeframe = match string_range(start, self.strings.len() - start) {
            Ok(range) => OptionalStringRange::some(range),
            Err(error) => {
                self.strings.truncate(start, fixup_start);
                return Err(error);
            }
        };
        self.records[index].codeframe = codeframe;
        Ok(Ok(()))
    }

    #[must_use]
    pub fn records(&self) -> &[DiagnosticRecord] {
        &self.records
    }

    #[must_use]
    pub fn labels(&self, range: ListRange) -> Option<&[DiagnosticLabelRecord]> {
        range_slice(&self.labels, range)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

fn try_map_span<E>(
    span: &mut TapeSpan,
    mapper: &mut impl FnMut(TapeSpan) -> Result<TapeSpan, E>,
) -> Result<(), E> {
    *span = mapper(*span)?;
    Ok(())
}

fn try_map_optional_span<E>(
    span: &mut OptionalTapeSpan,
    mapper: &mut impl FnMut(TapeSpan) -> Result<TapeSpan, E>,
) -> Result<(), E> {
    if span.present != 0 {
        try_map_span(&mut span.value, mapper)?;
    }
    Ok(())
}

fn try_map_optional_value_span<E>(
    value: &mut OptionalValueSpanRecord,
    mapper: &mut impl FnMut(TapeSpan) -> Result<TapeSpan, E>,
) -> Result<(), E> {
    if value.present != 0 {
        try_map_span(&mut value.value.span, mapper)?;
    }
    Ok(())
}

fn table_has_room<T>(records: &[T]) -> Result<(), TapeBuildError> {
    checked_record_index(records.len()).map(|_| ())
}

fn push_record<T>(records: &mut Vec<T>, record: T) -> Result<RecordIndex, TapeBuildError> {
    let index = checked_record_index(records.len())?;
    records.push(record);
    Ok(index)
}

fn append_records<T, I>(records: &mut Vec<T>, values: I) -> Result<ListRange, TapeBuildError>
where
    I: IntoIterator<Item = T>,
{
    let start = records.len();
    for value in values {
        if let Err(error) = push_record(records, value) {
            records.truncate(start);
            return Err(error);
        }
    }
    list_range(start, records.len() - start)
}

fn begin_direct_range<T>(records: &[T]) -> Result<u32, TapeBuildError> {
    checked_range_cursor(records.len())
}

fn finish_direct_range<T>(
    records: &[T],
    start: u32,
    expected_length: u32,
) -> Result<ListRange, TapeBuildError> {
    checked_direct_range(start, expected_length, checked_range_cursor(records.len())?)
}

fn checked_direct_range(
    start: u32,
    expected_length: u32,
    actual_end: u32,
) -> Result<ListRange, TapeBuildError> {
    let expected_end =
        start.checked_add(expected_length).ok_or(TapeBuildError::CapacityOverflow)?;
    if actual_end != expected_end {
        return Err(TapeBuildError::InvalidRecordIndex);
    }
    Ok(ListRange::new(start, expected_length))
}

fn checked_record_index(length: usize) -> Result<RecordIndex, TapeBuildError> {
    let index = checked_range_cursor(length)?;
    if index == RecordIndex::NONE.into_raw() {
        return Err(TapeBuildError::CapacityOverflow);
    }
    Ok(RecordIndex::new(index))
}

fn checked_range_cursor(length: usize) -> Result<u32, TapeBuildError> {
    u32::try_from(length).map_err(|_| TapeBuildError::CapacityOverflow)
}

fn list_range(start: usize, length: usize) -> Result<ListRange, TapeBuildError> {
    let start = u32::try_from(start).map_err(|_| TapeBuildError::CapacityOverflow)?;
    let length = u32::try_from(length).map_err(|_| TapeBuildError::CapacityOverflow)?;
    start.checked_add(length).ok_or(TapeBuildError::CapacityOverflow)?;
    Ok(ListRange::new(start, length))
}

fn string_range(start: usize, length: usize) -> Result<StringRange, TapeBuildError> {
    let start = u32::try_from(start).map_err(|_| TapeBuildError::CapacityOverflow)?;
    let length = u32::try_from(length).map_err(|_| TapeBuildError::CapacityOverflow)?;
    start.checked_add(length).ok_or(TapeBuildError::CapacityOverflow)?;
    Ok(StringRange::new(start, length))
}

fn slice_range(storage: &str, range: StringRange) -> Option<&str> {
    let start = usize::try_from(range.start).ok()?;
    let length = usize::try_from(range.length).ok()?;
    storage.get(start..start.checked_add(length)?)
}

fn range_slice<T>(records: &[T], range: ListRange) -> Option<&[T]> {
    let start = usize::try_from(range.start).ok()?;
    let length = usize::try_from(range.length).ok()?;
    records.get(start..start.checked_add(length)?)
}

fn index_usize(index: RecordIndex) -> usize {
    index.into_raw() as usize
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use super::{
        CommentTable, DiagnosticPhase, DiagnosticSeverity, DiagnosticTable, DynamicImportRecord,
        ExportExportNameKind, ExportImportNameKind, ExportLocalNameKind, ImportNameKind,
        ModuleNameRecord, ModuleTable, OptionalStringRange, OptionalTapeSpan,
        OptionalValueSpanRecord, StaticExportEntryRecord, StaticExportRecord,
        StaticImportEntryRecord, StaticImportRecord, ValueSpanRecord, checked_direct_range,
        checked_range_cursor, checked_record_index,
    };
    use crate::{
        ListRange, ProjectedCommentKind, RecordIndex, StringRange, TapeBuildError, TapeSpan,
    };

    fn direct_import_entry(offset: u32) -> StaticImportEntryRecord {
        StaticImportEntryRecord::new(
            ModuleNameRecord::new(
                ImportNameKind::Name,
                OptionalStringRange::NONE,
                OptionalTapeSpan::some(TapeSpan::new(offset, offset + 1)),
            ),
            ValueSpanRecord::new(StringRange::new(0, 0), TapeSpan::new(offset + 2, offset + 3)),
            false,
        )
    }

    fn direct_export_entry(offset: u32) -> StaticExportEntryRecord {
        StaticExportEntryRecord::new(
            TapeSpan::new(offset, offset + 1),
            OptionalValueSpanRecord::NONE,
            ModuleNameRecord::new(
                ExportImportNameKind::None,
                OptionalStringRange::NONE,
                OptionalTapeSpan::NONE,
            ),
            ModuleNameRecord::new(
                ExportExportNameKind::None,
                OptionalStringRange::NONE,
                OptionalTapeSpan::NONE,
            ),
            ModuleNameRecord::new(
                ExportLocalNameKind::None,
                OptionalStringRange::NONE,
                OptionalTapeSpan::NONE,
            ),
            false,
        )
    }

    #[test]
    fn module_groups_are_contiguous_and_strings_are_owned() {
        let mut table = ModuleTable::new();
        table.set_has_module_syntax(true);
        let request = table.push_string("pkg").expect("module request");
        let local = table.push_string("local").expect("local name");
        let imported = table.push_string("value").expect("imported name");
        let entries = table
            .append_static_import_entries([StaticImportEntryRecord::new(
                ModuleNameRecord::new(
                    ImportNameKind::Name,
                    OptionalStringRange::some(imported),
                    OptionalTapeSpan::some(TapeSpan::new(9, 14)),
                ),
                ValueSpanRecord::new(local, TapeSpan::new(18, 23)),
                false,
            )])
            .expect("entry range");
        table
            .push_static_import(StaticImportRecord::new(
                TapeSpan::new(0, 35),
                ValueSpanRecord::new(request, TapeSpan::new(30, 33)),
                entries,
            ))
            .expect("static import");

        assert!(table.has_module_syntax());
        let import = table.static_imports()[0];
        assert_eq!(table.value(import.module_request), Some("pkg"));
        let entry = table.static_import_entries(import.entries).expect("entries")[0];
        assert_eq!(table.name(entry.import_name), Some("value"));
        assert_eq!(table.value(entry.local_name), Some("local"));
    }

    #[test]
    fn independent_comment_and_diagnostic_strings_are_packed() {
        let mut comments = CommentTable::new();
        comments.push(ProjectedCommentKind::Block, TapeSpan::new(0, 7), "note").expect("comment");
        assert_eq!(comments.value(&comments.records()[0]), Some("note"));

        let mut diagnostics = DiagnosticTable::new();
        let labels =
            diagnostics.append_labels([(TapeSpan::new(3, 4), Some("here"), true)]).expect("labels");
        diagnostics
            .push_diagnostic(
                DiagnosticPhase::Grammar,
                DiagnosticSeverity::Error,
                "Unexpected token",
                labels,
                None,
                None,
                Some("parser"),
                Some("1001"),
                None,
                None,
            )
            .expect("diagnostic");
        let record = diagnostics.records()[0];
        assert_eq!(diagnostics.string(record.message), Some("Unexpected token"));
        assert_eq!(diagnostics.optional_string(record.code_scope), Some("parser"));
        assert_eq!(
            diagnostics
                .optional_string(diagnostics.labels(record.labels).expect("labels")[0].message),
            Some("here")
        );
        assert!(diagnostics.labels(record.labels).expect("labels")[0].primary);

        diagnostics
            .write_codeframe(RecordIndex::new(0), |writer| writer.write_str("rendered frame"))
            .expect("codeframe capacity")
            .expect("codeframe render");
        let record = diagnostics.records()[0];
        assert_eq!(diagnostics.optional_string(record.codeframe), Some("rendered frame"));
        let storage_before_failure =
            diagnostics.string_storage().expect("fixup-free diagnostic storage").to_owned();
        let codeframe_before_failure = record.codeframe;
        assert!(matches!(
            diagnostics.write_codeframe(RecordIndex::new(0), |writer| {
                writer.write_str("discarded partial frame")?;
                Err(std::fmt::Error)
            }),
            Ok(Err(_))
        ));
        assert_eq!(diagnostics.string_storage(), Some(storage_before_failure.as_str()));
        assert_eq!(diagnostics.records()[0].codeframe, codeframe_before_failure);
    }

    #[test]
    #[expect(
        clippy::too_many_lines,
        reason = "one exhaustive table test over every result-table span"
    )]
    fn every_result_table_source_span_is_mapped_once_without_touching_strings() {
        let mut module = ModuleTable::new();
        let text_units = [u16::from(b'n'), 0xd800, u16::from(b'm')];
        let text = module.push_utf16(&text_units).expect("module text");

        let import_entries = module
            .append_static_import_entries([StaticImportEntryRecord::new(
                ModuleNameRecord::new(
                    ImportNameKind::Name,
                    OptionalStringRange::some(text),
                    OptionalTapeSpan::some(TapeSpan::new(5, 6)),
                ),
                ValueSpanRecord::new(text, TapeSpan::new(7, 8)),
                false,
            )])
            .expect("import entries");
        module
            .push_static_import(StaticImportRecord::new(
                TapeSpan::new(1, 2),
                ValueSpanRecord::new(text, TapeSpan::new(3, 4)),
                import_entries,
            ))
            .expect("static import");

        let export_entries = module
            .append_static_export_entries([StaticExportEntryRecord::new(
                TapeSpan::new(11, 12),
                OptionalValueSpanRecord::some(ValueSpanRecord::new(text, TapeSpan::new(13, 14))),
                ModuleNameRecord::new(
                    ExportImportNameKind::Name,
                    OptionalStringRange::some(text),
                    OptionalTapeSpan::some(TapeSpan::new(15, 16)),
                ),
                ModuleNameRecord::new(
                    ExportExportNameKind::Name,
                    OptionalStringRange::some(text),
                    OptionalTapeSpan::some(TapeSpan::new(17, 18)),
                ),
                ModuleNameRecord::new(
                    ExportLocalNameKind::Name,
                    OptionalStringRange::some(text),
                    OptionalTapeSpan::some(TapeSpan::new(19, 20)),
                ),
                false,
            )])
            .expect("export entries");
        module
            .push_static_export(StaticExportRecord::new(TapeSpan::new(9, 10), export_entries))
            .expect("static export");
        module
            .push_dynamic_import(DynamicImportRecord::new(
                TapeSpan::new(21, 22),
                TapeSpan::new(23, 24),
            ))
            .expect("dynamic import");
        module.push_import_meta(TapeSpan::new(25, 26)).expect("import meta");

        assert_eq!(module.string_storage(), None);
        let module_storage_before = module.strings.utf8.clone();
        let module_capacities_before = [
            module.static_imports.capacity(),
            module.static_import_entries.capacity(),
            module.static_exports.capacity(),
            module.static_export_entries.capacity(),
            module.dynamic_imports.capacity(),
            module.import_metas.capacity(),
        ];
        let module_string_capacity_before = module.strings.utf8.capacity();
        let mut mapped = Vec::new();
        module
            .try_map_spans(|span| {
                mapped.push(span);
                Ok::<_, ()>(TapeSpan::new(span.start + 100, span.end + 100))
            })
            .expect("module spans");

        assert_eq!(
            mapped,
            (1..=25).step_by(2).map(|start| TapeSpan::new(start, start + 1)).collect::<Vec<_>>()
        );
        assert_eq!(module.strings.utf8, module_storage_before);
        assert_eq!(
            [
                module.static_imports.capacity(),
                module.static_import_entries.capacity(),
                module.static_exports.capacity(),
                module.static_export_entries.capacity(),
                module.dynamic_imports.capacity(),
                module.import_metas.capacity(),
            ],
            module_capacities_before
        );
        assert_eq!(module.strings.utf8.capacity(), module_string_capacity_before);
        assert_eq!(module.text(text).expect("module text").to_utf16(), text_units);
        let import = module.static_imports()[0];
        assert_eq!(import.span, TapeSpan::new(101, 102));
        assert_eq!(import.module_request.span, TapeSpan::new(103, 104));
        let import_entry = module.static_import_entries(import.entries).expect("import entries")[0];
        assert_eq!(import_entry.import_name.span.get(), Some(TapeSpan::new(105, 106)));
        assert_eq!(import_entry.local_name.span, TapeSpan::new(107, 108));
        let export = module.static_exports()[0];
        assert_eq!(export.span, TapeSpan::new(109, 110));
        let export_entry = module.static_export_entries(export.entries).expect("export entries")[0];
        assert_eq!(export_entry.span, TapeSpan::new(111, 112));
        assert_eq!(
            export_entry.module_request.get().expect("module request").span,
            TapeSpan::new(113, 114)
        );
        assert_eq!(export_entry.import_name.span.get(), Some(TapeSpan::new(115, 116)));
        assert_eq!(export_entry.export_name.span.get(), Some(TapeSpan::new(117, 118)));
        assert_eq!(export_entry.local_name.span.get(), Some(TapeSpan::new(119, 120)));
        assert_eq!(
            module.dynamic_imports()[0],
            DynamicImportRecord::new(TapeSpan::new(121, 122), TapeSpan::new(123, 124))
        );
        assert_eq!(module.import_metas(), &[TapeSpan::new(125, 126)]);

        let mut comments = CommentTable::new();
        comments.push(ProjectedCommentKind::Line, TapeSpan::new(31, 32), "keep").expect("comment");
        let comment_storage_before =
            comments.string_storage().expect("fixup-free comment storage").to_owned();
        let comment_capacities_before =
            (comments.records.capacity(), comments.strings.utf8.capacity());
        comments
            .try_map_spans(|span| Ok::<_, ()>(TapeSpan::new(span.start + 100, span.end + 100)))
            .expect("comment spans");
        assert_eq!(comments.records()[0].span, TapeSpan::new(131, 132));
        assert_eq!(comments.string_storage(), Some(comment_storage_before.as_str()));
        assert_eq!(
            (comments.records.capacity(), comments.strings.utf8.capacity()),
            comment_capacities_before
        );
        assert_eq!(comments.value(&comments.records()[0]), Some("keep"));

        let mut diagnostics = DiagnosticTable::new();
        let labels = diagnostics
            .append_labels([
                (TapeSpan::new(41, 42), Some("first"), true),
                (TapeSpan::new(43, 44), None, false),
            ])
            .expect("labels");
        diagnostics
            .push_diagnostic(
                DiagnosticPhase::Grammar,
                DiagnosticSeverity::Error,
                "keep diagnostic",
                labels,
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .expect("diagnostic");
        let diagnostic_storage_before =
            diagnostics.string_storage().expect("fixup-free diagnostic storage").to_owned();
        let diagnostic_capacities_before = (
            diagnostics.records.capacity(),
            diagnostics.labels.capacity(),
            diagnostics.strings.utf8.capacity(),
        );
        diagnostics
            .try_map_spans(|span| Ok::<_, ()>(TapeSpan::new(span.start + 100, span.end + 100)))
            .expect("diagnostic spans");
        let labels = diagnostics.labels(diagnostics.records()[0].labels).expect("labels");
        assert_eq!(labels[0].span, TapeSpan::new(141, 142));
        assert_eq!(labels[1].span, TapeSpan::new(143, 144));
        assert_eq!(diagnostics.string_storage(), Some(diagnostic_storage_before.as_str()));
        assert_eq!(
            (
                diagnostics.records.capacity(),
                diagnostics.labels.capacity(),
                diagnostics.strings.utf8.capacity(),
            ),
            diagnostic_capacities_before
        );
        assert_eq!(diagnostics.string(diagnostics.records()[0].message), Some("keep diagnostic"));
    }

    #[test]
    fn result_table_span_mappers_return_the_exact_mapper_error() {
        let mut module = ModuleTable::new();
        module
            .push_dynamic_import(DynamicImportRecord::new(TapeSpan::new(1, 2), TapeSpan::new(3, 4)))
            .expect("dynamic import");
        assert_eq!(
            module.try_map_spans(|span| {
                if span == TapeSpan::new(3, 4) {
                    Err("module endpoint is unmapped")
                } else {
                    Ok(span)
                }
            }),
            Err("module endpoint is unmapped")
        );

        let mut comments = CommentTable::new();
        comments.push(ProjectedCommentKind::Line, TapeSpan::new(5, 6), "comment").expect("comment");
        assert_eq!(
            comments.try_map_spans(|_| Err::<TapeSpan, _>("comment endpoint is unmapped")),
            Err("comment endpoint is unmapped")
        );

        let mut diagnostics = DiagnosticTable::new();
        diagnostics.append_labels([(TapeSpan::new(7, 8), None, true)]).expect("label");
        assert_eq!(
            diagnostics.try_map_spans(|_| Err::<TapeSpan, _>("label endpoint is unmapped")),
            Err("label endpoint is unmapped")
        );
    }

    #[test]
    fn packed_text_fixups_are_sparse_position_keyed_and_lossless() {
        let mut comments = CommentTable::new();
        comments
            .push(ProjectedCommentKind::Line, TapeSpan::new(0, 4), "plain")
            .expect("plain comment");
        let units = [0xe000, 0xd800, 0xd83d, 0xde00, 0xdc00, 0xe000];
        comments
            .push_utf16(ProjectedCommentKind::Block, TapeSpan::new(5, 13), &units)
            .expect("lossless comment");

        let plain = comments.value_text(&comments.records()[0]).expect("plain text");
        assert_eq!(plain.as_str(), Some("plain"));
        assert_eq!(plain.to_utf16(), "plain".encode_utf16().collect::<Vec<_>>());

        let repaired = comments.value_text(&comments.records()[1]).expect("repaired text");
        assert_eq!(repaired.as_str(), None);
        assert_eq!(repaired.to_utf16(), units);
        assert_eq!(comments.string_storage(), None);
        let table_debug = format!("{comments:?}");
        let text_debug = format!("{repaired:?}");
        assert!(table_debug.contains("<lossless UTF-16 text>"));
        assert!(text_debug.contains("<lossless UTF-16 text>"));
        assert!(!table_debug.contains('\u{e000}'));
        assert!(!text_debug.contains('\u{e000}'));
    }

    #[test]
    fn existing_packed_values_accept_only_collision_safe_same_width_repairs() {
        let mut module = ModuleTable::new();
        let range = module
            .push_string(&format!("{}x{}", '\u{e000}', '\u{e000}'))
            .expect("placeholder-width module value");
        let units = [0xe000, u16::from(b'x'), 0xd800];
        module.repair_utf16(range, &units).expect("position-keyed repair");
        assert_eq!(module.text(range).expect("module text").to_utf16(), units);
        assert_eq!(
            module.repair_utf16(range, &units),
            Err(TapeBuildError::InvalidRecordIndex),
            "the same packed range cannot consume one fixup twice"
        );
        assert_eq!(
            module.repair_utf16(range, &[0xe000, u16::from(b'y'), 0xd800]),
            Err(TapeBuildError::InvalidRecordIndex)
        );
        assert_eq!(
            module.repair_utf16(range, &[0xe000, u16::from(b'x'), 0xe000]),
            Err(TapeBuildError::InvalidRecordIndex),
            "a later repair cannot silently remove an existing position-keyed fixup"
        );

        let mut batch = ModuleTable::new();
        let ranges = std::iter::repeat_with(|| {
            batch.push_string(&format!("{}x", '\u{e000}')).expect("placeholder-width batch value")
        })
        .take(4_096)
        .collect::<Vec<_>>();
        let repaired = [0xd800, u16::from(b'x')];
        batch
            .repair_utf16_batch(ranges.iter().copied().map(|range| (range, repaired.as_slice())))
            .expect("one ordered linear batch");
        assert!(
            ranges
                .iter()
                .all(|range| { batch.text(*range).expect("batch text").to_utf16() == repaired })
        );
        assert_eq!(batch.strings.fixups.as_ref().map(Vec::len), Some(4_096));

        let mut diagnostics = DiagnosticTable::new();
        let labels = diagnostics.append_labels(std::iter::empty()).expect("empty labels");
        diagnostics
            .push_diagnostic(
                DiagnosticPhase::Grammar,
                DiagnosticSeverity::Error,
                "message",
                labels,
                None,
                None,
                None,
                None,
                None,
                Some(&format!("{}x{}", '\u{e000}', '\u{e000}')),
            )
            .expect("diagnostic");
        let record = diagnostics.records()[0];
        let codeframe = record.codeframe.get().expect("codeframe range");
        diagnostics
            .repair_utf16_batch(std::iter::once((codeframe, units.as_slice())))
            .expect("lossless codeframe");
        let record = diagnostics.records()[0];
        assert_eq!(
            diagnostics.optional_text(record.codeframe).expect("codeframe text").to_utf16(),
            units
        );
    }

    #[test]
    fn rejection_placeholder_repairs_are_position_keyed_and_preserve_authored_noncharacters() {
        let mut module = ModuleTable::new();
        let packed = format!("{}|{}|{}", '\u{ffff}', '\u{ffff}', '\u{e000}');
        let range = module.push_string(&packed).expect("same-width rejection fixture");
        let units = [0xffff, u16::from(b'|'), 0xd800, u16::from(b'|'), 0xe000];

        module.repair_utf16(range, &units).expect("position-keyed U+FFFF repair");

        assert_eq!(module.text(range).expect("lossless text").to_utf16(), units);
        assert_eq!(
            module.strings.utf8,
            format!("{}|{}|{}", '\u{ffff}', '\u{e000}', '\u{e000}'),
            "only the tracked rejection position is normalized to packed U+E000 storage"
        );
        assert_eq!(module.strings.fixups.as_ref().map(Vec::len), Some(1));
        assert_eq!(
            module.strings.fixups.as_ref().expect("one fixup")[0].byte_start,
            range.start + 4,
            "the authored leading U+FFFF remains an ordinary scalar"
        );
    }

    #[test]
    fn direct_module_entry_ranges_are_exact_checked_and_keep_empty_cursors() {
        let mut table = ModuleTable::new();

        let imports = table.begin_static_import_entries().expect("import start");
        table.push_static_import_entry(direct_import_entry(1)).expect("first import entry");
        table.push_static_import_entry(direct_import_entry(5)).expect("second import entry");
        assert_eq!(
            table.finish_static_import_entries(imports, 2).expect("import range"),
            ListRange::new(0, 2)
        );
        let empty_imports = table.begin_static_import_entries().expect("empty import start");
        assert_eq!(
            table.finish_static_import_entries(empty_imports, 0).expect("empty import range"),
            ListRange::new(2, 0)
        );
        assert_eq!(
            table.finish_static_import_entries(imports, 1),
            Err(TapeBuildError::InvalidRecordIndex)
        );

        let exports = table.begin_static_export_entries().expect("export start");
        table.push_static_export_entry(direct_export_entry(10)).expect("export entry");
        assert_eq!(
            table.finish_static_export_entries(exports, 1).expect("export range"),
            ListRange::new(0, 1)
        );
        let empty_exports = table.begin_static_export_entries().expect("empty export start");
        assert_eq!(
            table.finish_static_export_entries(empty_exports, 0).expect("empty export range"),
            ListRange::new(1, 0)
        );
    }

    #[test]
    fn direct_label_ranges_and_cursor_arithmetic_fail_closed() {
        let mut table = DiagnosticTable::new();
        let labels = table.begin_labels().expect("label start");
        table.push_labeled(TapeSpan::new(1, 2), Some("one"), true).expect("first label");
        table.push_labeled(TapeSpan::new(3, 4), None, false).expect("second label");
        assert_eq!(table.finish_labels(labels, 2).expect("label range"), ListRange::new(0, 2));
        let empty = table.begin_labels().expect("empty label start");
        assert_eq!(table.finish_labels(empty, 0).expect("empty label range"), ListRange::new(2, 0));
        assert_eq!(table.finish_labels(labels, 1), Err(TapeBuildError::InvalidRecordIndex));
        assert_eq!(table.finish_labels(u32::MAX, 0), Err(TapeBuildError::InvalidRecordIndex));
        assert_eq!(table.finish_labels(u32::MAX - 1, 2), Err(TapeBuildError::CapacityOverflow));
        assert_eq!(table.finish_labels(u32::MAX - 1, 1), Err(TapeBuildError::InvalidRecordIndex));
        assert_eq!(
            checked_range_cursor(usize::try_from(u32::MAX).expect("u32 fits usize")),
            Ok(u32::MAX)
        );
        assert_eq!(
            checked_record_index(usize::try_from(u32::MAX).expect("u32 fits usize")),
            Err(TapeBuildError::CapacityOverflow)
        );
        assert_eq!(
            checked_record_index(usize::try_from(u32::MAX - 1).expect("u32 fits usize"))
                .map(RecordIndex::into_raw),
            Ok(u32::MAX - 1)
        );
        assert_eq!(checked_direct_range(u32::MAX, 0, u32::MAX), Ok(ListRange::new(u32::MAX, 0)));
        assert_eq!(
            checked_direct_range(u32::MAX - 1, 1, u32::MAX),
            Ok(ListRange::new(u32::MAX - 1, 1))
        );
        assert_eq!(
            checked_direct_range(u32::MAX - 1, 2, u32::MAX),
            Err(TapeBuildError::CapacityOverflow)
        );
    }

    #[test]
    fn destructive_parts_release_every_source_allocation() {
        let mut module = ModuleTable::new();
        module.static_imports.reserve(1);
        module.static_import_entries.reserve(1);
        module.static_exports.reserve(1);
        module.static_export_entries.reserve(1);
        module.dynamic_imports.reserve(1);
        module.import_metas.reserve(1);
        module.strings.push_str("module").expect("module string");
        let imports = module.take_static_imports();
        let exports = module.take_static_exports();
        let dynamics = module.take_dynamic_imports();
        let metas = module.take_import_metas();
        let module_strings = module.take_string_storage().expect("UTF-8 module strings");
        assert!(!module_strings.is_empty());
        assert!(imports.0.capacity() > 0 && imports.1.capacity() > 0);
        assert!(exports.0.capacity() > 0 && exports.1.capacity() > 0);
        assert!(dynamics.capacity() > 0 && metas.capacity() > 0);
        assert!(module.is_storage_released());

        let mut comments = CommentTable::new();
        comments.records.reserve(1);
        comments.strings.push_str("comment").expect("comment string");
        let comment_records = comments.take_records();
        let comment_strings = comments.take_string_storage().expect("UTF-8 comment strings");
        assert!(comment_records.capacity() > 0);
        assert_eq!(comment_strings, "comment");
        assert!(comments.is_storage_released());

        let mut diagnostics = DiagnosticTable::new();
        diagnostics.records.reserve(1);
        diagnostics.labels.reserve(1);
        diagnostics.strings.push_str("diagnostic").expect("diagnostic string");
        let diagnostic_parts = diagnostics.take_records_and_labels();
        let diagnostic_strings =
            diagnostics.take_string_storage().expect("UTF-8 diagnostic strings");
        assert!(diagnostic_parts.0.capacity() > 0 && diagnostic_parts.1.capacity() > 0);
        assert_eq!(diagnostic_strings, "diagnostic");
        assert!(diagnostics.is_storage_released());

        let mut lossless = CommentTable::new();
        let record = lossless
            .push_utf16(ProjectedCommentKind::Line, TapeSpan::new(0, 3), &[0xd800])
            .expect("lossless comment");
        assert_eq!(
            lossless.take_string_storage(),
            Err(TapeBuildError::InvalidRecordIndex),
            "the UTF-8-only destructive API must not drop sparse fixups"
        );
        let records = lossless.take_records();
        let text = lossless.take_text_storage();
        assert_eq!(
            text.text(
                records[usize::try_from(record.into_raw()).expect("record index fits usize")].value,
            )
            .expect("released lossless text")
            .to_utf16(),
            [0xd800]
        );
        assert!(lossless.is_storage_released());
    }
}
