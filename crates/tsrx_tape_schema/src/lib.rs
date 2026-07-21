//! Revision-neutral, OXC-independent records shared by the TSRX parser pipeline.

mod result;
mod tape;
mod transfer;

pub use result::{
    CommentRecord, CommentTable, CoordinateDomain, DiagnosticLabelRecord, DiagnosticPhase,
    DiagnosticRecord, DiagnosticSeverity, DiagnosticTable, DynamicImportRecord,
    ExportExportNameKind, ExportImportNameKind, ExportLocalNameKind, ImportNameKind,
    ModuleNameRecord, ModuleTable, OptionalStringRange, OptionalTapeSpan, OptionalValueSpanRecord,
    OwnedPackedTextStorage, PackedStringWriter, PackedTextRef, ParseCompleteness,
    StaticExportEntryRecord, StaticExportRecord, StaticImportEntryRecord, StaticImportRecord,
    ValueSpanRecord,
};

pub use tape::{
    FieldIter, FieldRecord, FlatTape, IndexedFieldIter, IndexedValueIter, ListRecord,
    ListValueInsertion, ListValueRecord, ObjectRecord, TapeBuildError, ValueIter, ValueKind,
    ValueRef,
};
pub use transfer::{
    PROGRAM_BINARY_TRANSFER_MAGIC, PROGRAM_BINARY_TRANSFER_VERSION, PROGRAM_TRANSFER_MAX_BYTES,
    PROGRAM_TRANSFER_VERSION, ProgramBinaryTransfer,
};

/// Revision-neutral comment kinds crossing the OXC adapter boundary.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProjectedCommentKind {
    Line = 1,
    Block = 2,
}

/// One comment observed in projected TSX before authored reconstruction.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProjectedComment {
    pub kind: ProjectedCommentKind,
    pub span: TapeSpan,
}

/// Current revision of the internal canonical tape schema.
pub const SCHEMA_VERSION: u16 = 1;

/// A compact index into a tape table. `u32::MAX` is the sole missing-value sentinel.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RecordIndex(u32);

impl RecordIndex {
    /// Missing index sentinel.
    pub const NONE: Self = Self(u32::MAX);

    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn is_none(self) -> bool {
        self.0 == u32::MAX
    }

    #[must_use]
    pub const fn get(self) -> Option<u32> {
        if self.is_none() { None } else { Some(self.0) }
    }

    #[must_use]
    pub const fn into_raw(self) -> u32 {
        self.0
    }
}

impl Default for RecordIndex {
    fn default() -> Self {
        Self::NONE
    }
}

/// A half-open byte range in the serialized source domain.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct TapeSpan {
    pub start: u32,
    pub end: u32,
}

impl TapeSpan {
    #[must_use]
    pub const fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }
}

/// One byte range in the tape's packed string storage.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct StringRange {
    pub start: u32,
    pub length: u32,
}

impl StringRange {
    #[must_use]
    pub const fn new(start: u32, length: u32) -> Self {
        Self { start, length }
    }
}

/// One element range in a packed index table.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ListRange {
    pub start: u32,
    pub length: u32,
}

impl ListRange {
    #[must_use]
    pub const fn new(start: u32, length: u32) -> Self {
        Self { start, length }
    }
}

/// Explicit completeness state carried by a parser tape.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Completeness(u8);

impl Completeness {
    pub const EMPTY: Self = Self(0);
    pub const COMPLETE: Self = Self(1 << 0);
    pub const HAS_PROGRAM: Self = Self(1 << 1);
    pub const HAS_MODULE: Self = Self(1 << 2);
    pub const HAS_COMMENTS: Self = Self(1 << 3);
    pub const HAS_ERRORS: Self = Self(1 << 4);

    #[must_use]
    pub const fn with(self, flag: Self) -> Self {
        Self(self.0 | flag.0)
    }

    #[must_use]
    pub const fn contains(self, flag: Self) -> bool {
        self.0 & flag.0 == flag.0
    }

    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }
}

/// TSRX-authored node kinds inserted during reconstruction.
///
/// Numeric values are part of schema version 1 and must not be reordered.
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TsrxNodeTag {
    JsxCodeBlock = 1,
    JsxStyleElement = 2,
    JsxIfExpression = 3,
    JsxForExpression = 4,
    JsxSwitchExpression = 5,
    JsxTryExpression = 6,
    TsrxExpression = 7,
}

impl From<TsrxNodeTag> for u16 {
    fn from(value: TsrxNodeTag) -> Self {
        value as Self
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Completeness, FlatTape, ListRange, RecordIndex, SCHEMA_VERSION, StringRange, TapeSpan,
        TsrxNodeTag,
    };

    const fn assert_copy<T: Copy>() {}

    #[test]
    fn schema_primitives_are_fixed_width_copy_values() {
        assert_eq!(SCHEMA_VERSION, 1);
        assert_copy::<RecordIndex>();
        assert_copy::<TapeSpan>();
        assert_copy::<StringRange>();
        assert_copy::<ListRange>();
        assert_copy::<Completeness>();
        assert_copy::<TsrxNodeTag>();

        assert_eq!(size_of::<RecordIndex>(), 4);
        assert_eq!(size_of::<TapeSpan>(), 8);
        assert_eq!(size_of::<StringRange>(), 8);
        assert_eq!(size_of::<ListRange>(), 8);
    }

    #[test]
    fn none_indices_and_completeness_flags_are_explicit() {
        assert!(RecordIndex::NONE.is_none());
        assert!(!RecordIndex::new(0).is_none());
        assert_eq!(RecordIndex::new(7).get(), Some(7));

        let flags = Completeness::COMPLETE
            .with(Completeness::HAS_PROGRAM)
            .with(Completeness::HAS_MODULE);
        assert!(flags.contains(Completeness::COMPLETE));
        assert!(flags.contains(Completeness::HAS_PROGRAM));
        assert!(flags.contains(Completeness::HAS_MODULE));
        assert!(!flags.contains(Completeness::HAS_ERRORS));
    }

    #[test]
    fn custom_node_tags_are_stable_and_distinct() {
        let tags = [
            TsrxNodeTag::JsxCodeBlock,
            TsrxNodeTag::JsxStyleElement,
            TsrxNodeTag::JsxIfExpression,
            TsrxNodeTag::JsxForExpression,
            TsrxNodeTag::JsxSwitchExpression,
            TsrxNodeTag::JsxTryExpression,
            TsrxNodeTag::TsrxExpression,
        ];
        let mut values = tags.map(u16::from);
        values.sort_unstable();
        assert!(values.windows(2).all(|pair| pair[0] != pair[1]));
    }

    #[test]
    fn json_string_scalars_encode_directly_into_packed_storage() {
        let mut tape = FlatTape::default();
        let escaped = tape
            .push_json_string_scalar("a\"b\\c\n\r\t\x08\x0c\x01")
            .expect("escaped scalar");
        let unicode = tape
            .push_json_string_scalar("é\u{2028}")
            .expect("Unicode scalar");

        assert_eq!(tape.scalar(escaped), Some(r#""a\"b\\c\n\r\t\b\f\u0001""#));
        assert_eq!(tape.scalar(unicode), Some("\"é\u{2028}\""));
        assert_eq!(
            tape.scalar_storage(),
            concat!(r#""a\"b\\c\n\r\t\b\f\u0001""#, "\"é\u{2028}\"")
        );
    }

    #[test]
    fn utf16_json_scalars_preserve_pairs_lone_units_and_private_use_values() {
        let mut tape = FlatTape::default();
        let units = [
            u16::from(b'a'),
            0xe000,
            0xd800,
            0xd83d,
            0xde00,
            0xdc00,
            u16::from(b'"'),
            u16::from(b'\\'),
            0x0001,
        ];
        let value = tape
            .push_json_utf16_scalar(&units)
            .expect("lossless UTF-16 JSON scalar");
        assert_eq!(
            tape.scalar(value),
            Some("\"a\u{e000}\\ud800😀\\udc00\\\"\\\\\\u0001\"")
        );
    }
}
