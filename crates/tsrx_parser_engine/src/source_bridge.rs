use tsrx_syntax::{OpaqueSurrogateContext, classify_wtf8_surrogates_detailed};

use crate::TsrxParseError;

const OPAQUE_SURROGATE_PLACEHOLDER_UTF8: [u8; 3] = [0xee, 0x80, 0x80];
const REJECTED_SURROGATE_SENTINEL_UTF8: [u8; 3] = [0xef, 0xbf, 0xbf];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SourceFixup {
    pub(super) utf16_index: u32,
    pub(super) byte_start: u32,
    pub(super) unit: u16,
    pub(super) context: Option<OpaqueSurrogateContext>,
}

impl SourceFixup {
    pub(super) const fn placeholder(self) -> char {
        if self.context.is_some() {
            '\u{e000}'
        } else {
            '\u{ffff}'
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct BoundaryRecord {
    byte_start: u32,
    byte_end: u32,
    utf16_start: u32,
    utf16_end: u32,
}

#[cfg(any(test, feature = "stage4-observer"))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct BridgeWork {
    pub(super) input_units: usize,
    pub(super) utf8_bytes: usize,
    pub(super) boundary_records: usize,
    pub(super) boundary_bytes: usize,
    pub(super) fixup_records: usize,
    pub(super) fixup_bytes: usize,
    pub(super) opaque_fixup_records: usize,
    pub(super) rejection_fixup_records: usize,
    pub(super) sanitized_bytes: usize,
}

#[derive(Debug)]
pub(super) struct PreparedSource<'a> {
    original: &'a [u16],
    utf8: String,
    boundaries: Option<Vec<BoundaryRecord>>,
    fixups: Option<Vec<SourceFixup>>,
    rejected_fixup: Option<usize>,
}

impl<'a> PreparedSource<'a> {
    pub(super) fn new(original: &'a [u16]) -> Result<Self, TsrxParseError> {
        let source_length = u32::try_from(original.len()).map_err(|_| {
            TsrxParseError::ResourceExhausted("UTF-16 source exceeds the 4 GiB span limit")
        })?;
        let mut bytes = Vec::with_capacity(original.len());
        let mut utf16_index = 0_usize;
        while original.get(utf16_index).is_some_and(|unit| *unit <= 0x7f) {
            bytes.push(u8::try_from(original[utf16_index]).map_err(|_| {
                TsrxParseError::Unsupported("ASCII source bridge contains a wide unit")
            })?);
            utf16_index += 1;
        }
        if utf16_index == original.len() {
            let utf8 = String::from_utf8(bytes)
                .map_err(|_| TsrxParseError::Unsupported("invalid ASCII source bridge"))?;
            debug_assert_eq!(u32::try_from(utf8.len()).ok(), Some(source_length));
            return Ok(Self {
                original,
                utf8,
                boundaries: None,
                fixups: None,
                rejected_fixup: None,
            });
        }

        let mut boundaries = Vec::new();
        let mut fixups = Vec::new();
        while utf16_index < original.len() {
            let unit = original[utf16_index];
            let byte_start = checked_u32(bytes.len(), "UTF-8 bridge exceeds 4 GiB")?;
            if (0xd800..=0xdbff).contains(&unit)
                && original
                    .get(utf16_index + 1)
                    .is_some_and(|next| (0xdc00..=0xdfff).contains(next))
            {
                let high = u32::from(unit - 0xd800);
                let low = u32::from(original[utf16_index + 1] - 0xdc00);
                let scalar = char::from_u32(0x1_0000 + (high << 10) + low)
                    .ok_or(TsrxParseError::Unsupported("invalid UTF-16 surrogate pair"))?;
                push_char(&mut bytes, scalar);
                push_boundary(&mut boundaries, byte_start, bytes.len(), utf16_index, 2)?;
                utf16_index += 2;
                continue;
            }
            if (0xd800..=0xdfff).contains(&unit) {
                // Encode the exact unit as WTF-8 only long enough for the TSRX lexical proof.
                bytes.extend_from_slice(&encode_wtf8_surrogate(unit));
                push_boundary(&mut boundaries, byte_start, bytes.len(), utf16_index, 1)?;
                fixups.push(SourceFixup {
                    utf16_index: checked_u32(utf16_index, "UTF-16 source exceeds 4 GiB")?,
                    byte_start,
                    unit,
                    context: None,
                });
                utf16_index += 1;
                continue;
            }
            let scalar = char::from_u32(u32::from(unit))
                .ok_or(TsrxParseError::Unsupported("invalid BMP source scalar"))?;
            push_char(&mut bytes, scalar);
            if scalar.len_utf8() != 1 {
                push_boundary(&mut boundaries, byte_start, bytes.len(), utf16_index, 1)?;
            }
            utf16_index += 1;
        }

        let rejected_fixup = classify_and_sanitize_fixups(&mut bytes, &mut fixups)?;
        let utf8 = String::from_utf8(bytes)
            .map_err(|_| TsrxParseError::Unsupported("prepared source is not UTF-8"))?;
        Ok(Self {
            original,
            utf8,
            boundaries: (!boundaries.is_empty()).then_some(boundaries),
            fixups: (!fixups.is_empty()).then_some(fixups),
            rejected_fixup,
        })
    }

    pub(super) fn source(&self) -> &str {
        &self.utf8
    }

    pub(super) fn is_identity(&self) -> bool {
        self.boundaries.is_none()
    }

    pub(super) fn fixups(&self) -> &[SourceFixup] {
        self.fixups.as_deref().unwrap_or(&[])
    }

    pub(super) fn has_context(&self, context: OpaqueSurrogateContext) -> bool {
        self.fixups()
            .iter()
            .any(|fixup| fixup.context == Some(context))
    }

    pub(super) fn has_program_value_fixups(&self) -> bool {
        self.fixups().iter().any(|fixup| {
            matches!(
                fixup.context,
                Some(
                    OpaqueSurrogateContext::QuotedString
                        | OpaqueSurrogateContext::TemplateRaw
                        | OpaqueSurrogateContext::RegexBody
                        | OpaqueSurrogateContext::JsxText
                        | OpaqueSurrogateContext::RawStyle
                )
            )
        })
    }

    pub(super) fn rejected_fixup(&self) -> Option<SourceFixup> {
        self.rejected_fixup
            .and_then(|index| self.fixups().get(index).copied())
    }

    pub(super) fn map_endpoint(&self, byte_offset: u32) -> Option<u32> {
        let source_length = u32::try_from(self.utf8.len()).ok()?;
        if byte_offset > source_length {
            return None;
        }
        let Some(boundaries) = self.boundaries.as_deref() else {
            return Some(byte_offset);
        };
        let following = boundaries.partition_point(|record| record.byte_start < byte_offset);
        if let Some(record) = boundaries.get(following)
            && byte_offset == record.byte_start
        {
            return Some(record.utf16_start);
        }
        let Some(previous) = following
            .checked_sub(1)
            .and_then(|index| boundaries.get(index))
        else {
            return Some(byte_offset);
        };
        if byte_offset < previous.byte_end {
            return None;
        }
        previous
            .utf16_end
            .checked_add(byte_offset.checked_sub(previous.byte_end)?)
    }

    pub(super) fn original_span(&self, start: u32, end: u32) -> Option<&'a [u16]> {
        let start = usize::try_from(self.map_endpoint(start)?).ok()?;
        let end = usize::try_from(self.map_endpoint(end)?).ok()?;
        self.original.get(start..end)
    }

    pub(super) fn has_fixup_in(&self, start: u32, end: u32) -> bool {
        !self.fixups_in(start, end).is_empty()
    }

    pub(super) fn has_fixup_context_in(
        &self,
        start: u32,
        end: u32,
        context: OpaqueSurrogateContext,
    ) -> bool {
        self.fixups_in(start, end)
            .iter()
            .any(|fixup| fixup.context == Some(context))
    }

    pub(super) fn fixups_in(&self, start: u32, end: u32) -> &[SourceFixup] {
        let fixups = self.fixups();
        let first = fixups.partition_point(|fixup| fixup.byte_start < start);
        let last = fixups.partition_point(|fixup| fixup.byte_start < end);
        fixups.get(first..last).unwrap_or(&[])
    }

    pub(super) fn is_authored_collision_scalar(&self, start: u32, end: u32) -> bool {
        if end.checked_sub(start) != Some(3)
            || self
                .fixups()
                .binary_search_by_key(&start, |fixup| fixup.byte_start)
                .is_ok()
        {
            return false;
        }
        let Ok(start) = usize::try_from(start) else {
            return false;
        };
        let Ok(end) = usize::try_from(end) else {
            return false;
        };
        self.utf8.as_bytes().get(start..end).is_some_and(|bytes| {
            bytes == OPAQUE_SURROGATE_PLACEHOLDER_UTF8 || bytes == REJECTED_SURROGATE_SENTINEL_UTF8
        })
    }

    #[cfg(any(test, feature = "stage4-observer"))]
    pub(super) fn work(&self) -> BridgeWork {
        let opaque_fixup_records = self
            .fixups()
            .iter()
            .filter(|fixup| fixup.context.is_some())
            .count();
        let rejection_fixup_records = self.fixups().len() - opaque_fixup_records;
        BridgeWork {
            input_units: self.original.len(),
            utf8_bytes: self.utf8.len(),
            boundary_records: self.boundaries.as_ref().map_or(0, Vec::len),
            boundary_bytes: self.boundaries.as_ref().map_or(0, |records| {
                records.len().saturating_mul(size_of::<BoundaryRecord>())
            }),
            fixup_records: self.fixups.as_ref().map_or(0, Vec::len),
            fixup_bytes: self.fixups.as_ref().map_or(0, |records| {
                records.len().saturating_mul(size_of::<SourceFixup>())
            }),
            opaque_fixup_records,
            rejection_fixup_records,
            sanitized_bytes: self.fixups.as_ref().map_or(0, |fixups| fixups.len() * 3),
        }
    }
}

fn classify_and_sanitize_fixups(
    bytes: &mut [u8],
    fixups: &mut [SourceFixup],
) -> Result<Option<usize>, TsrxParseError> {
    if fixups.is_empty() {
        return Ok(None);
    }
    let offsets = fixups
        .iter()
        .map(|fixup| fixup.byte_start)
        .collect::<Vec<_>>();
    let classification = classify_wtf8_surrogates_detailed(bytes, &offsets);
    if classification.contexts.len() != fixups.len() {
        return Err(TsrxParseError::Unsupported(
            "surrogate classifier returned the wrong result count",
        ));
    }
    let mut rejected_fixup = None;
    for (index, (fixup, context)) in fixups.iter_mut().zip(classification.contexts).enumerate() {
        fixup.context = context;
        if context.is_none() && rejected_fixup.is_none() {
            rejected_fixup = Some(index);
        }
        let start = usize::try_from(fixup.byte_start)
            .map_err(|_| TsrxParseError::Unsupported("surrogate byte offset overflow"))?;
        let end = start.checked_add(3).ok_or(TsrxParseError::Unsupported(
            "surrogate byte offset overflow",
        ))?;
        let replacement = if context.is_some() {
            &OPAQUE_SURROGATE_PLACEHOLDER_UTF8
        } else {
            &REJECTED_SURROGATE_SENTINEL_UTF8
        };
        bytes
            .get_mut(start..end)
            .ok_or(TsrxParseError::Unsupported(
                "surrogate fixup is outside prepared source",
            ))?
            .copy_from_slice(replacement);
    }
    Ok(rejected_fixup)
}

fn checked_u32(value: usize, message: &'static str) -> Result<u32, TsrxParseError> {
    u32::try_from(value).map_err(|_| TsrxParseError::ResourceExhausted(message))
}

fn push_char(output: &mut Vec<u8>, value: char) {
    let mut encoded = [0_u8; 4];
    output.extend_from_slice(value.encode_utf8(&mut encoded).as_bytes());
}

fn push_boundary(
    output: &mut Vec<BoundaryRecord>,
    byte_start: u32,
    byte_end: usize,
    utf16_start: usize,
    utf16_length: usize,
) -> Result<(), TsrxParseError> {
    let utf16_start = checked_u32(utf16_start, "UTF-16 source exceeds 4 GiB")?;
    output.push(BoundaryRecord {
        byte_start,
        byte_end: checked_u32(byte_end, "UTF-8 bridge exceeds 4 GiB")?,
        utf16_start,
        utf16_end: utf16_start
            .checked_add(checked_u32(utf16_length, "UTF-16 scalar length overflow")?)
            .ok_or(TsrxParseError::ResourceExhausted(
                "UTF-16 source exceeds the 4 GiB span limit",
            ))?,
    });
    Ok(())
}

fn encode_wtf8_surrogate(unit: u16) -> [u8; 3] {
    let scalar = u32::from(unit);
    [
        0xe0 | u8::try_from(scalar >> 12).expect("surrogate top bits fit"),
        0x80 | u8::try_from((scalar >> 6) & 0x3f).expect("surrogate middle bits fit"),
        0x80 | u8::try_from(scalar & 0x3f).expect("surrogate low bits fit"),
    ]
}

#[cfg(test)]
mod tests {
    use std::{hint::black_box, time::Instant};

    use super::{BridgeWork, PreparedSource};

    #[cfg(debug_assertions)]
    const fn require_release_build() {
        panic!("the retained performance campaign must run with --release");
    }

    #[cfg(not(debug_assertions))]
    const fn require_release_build() {}

    fn release_bridge_median(source: &[u16]) -> u128 {
        let iterations = (2_000_000_usize / source.len()).max(8);
        let mut samples = Vec::with_capacity(7);
        for _ in 0..7 {
            let started = Instant::now();
            for _ in 0..iterations {
                let prepared =
                    PreparedSource::new(black_box(source)).expect("release scaling fixture");
                black_box(prepared.source().len());
            }
            samples.push(started.elapsed().as_nanos() / iterations as u128);
        }
        samples.sort_unstable();
        samples[samples.len() / 2]
    }

    fn assert_linear_scaling(label: &str, medians: &[u128]) {
        for pair in medians.windows(2) {
            assert!(
                pair[1] <= pair[0].saturating_mul(7) / 2 + 50_000,
                "{label} doubling exceeded the broad linear ceiling: {pair:?}"
            );
        }
    }

    #[test]
    fn ascii_source_uses_no_boundary_fixup_or_rejection_storage() {
        let original = "function View() @{ <main/> }"
            .encode_utf16()
            .collect::<Vec<_>>();
        let prepared = PreparedSource::new(&original).expect("prepared ASCII source");

        assert!(prepared.is_identity());
        assert_eq!(prepared.source(), "function View() @{ <main/> }");
        assert!(prepared.boundaries.is_none());
        assert!(prepared.fixups.is_none());
        assert!(prepared.rejected_fixup.is_none());
        assert_eq!(
            prepared.work(),
            BridgeWork {
                input_units: original.len(),
                utf8_bytes: original.len(),
                boundary_records: 0,
                boundary_bytes: 0,
                fixup_records: 0,
                fixup_bytes: 0,
                opaque_fixup_records: 0,
                rejection_fixup_records: 0,
                sanitized_bytes: 0,
            }
        );
    }

    #[test]
    fn sparse_boundaries_reject_utf8_continuation_offsets() {
        let original = "aé😀z".encode_utf16().collect::<Vec<_>>();
        let prepared = PreparedSource::new(&original).expect("prepared source");
        assert_eq!(prepared.source(), "aé😀z");
        assert_eq!(prepared.map_endpoint(0), Some(0));
        assert_eq!(prepared.map_endpoint(1), Some(1));
        assert_eq!(prepared.map_endpoint(2), None);
        assert_eq!(prepared.map_endpoint(3), Some(2));
        assert_eq!(prepared.map_endpoint(4), None);
        assert_eq!(prepared.map_endpoint(6), None);
        assert_eq!(prepared.map_endpoint(7), Some(4));
        assert_eq!(prepared.map_endpoint(8), Some(5));
    }

    #[test]
    fn authored_private_use_scalar_is_not_a_surrogate_fixup() {
        let mut original = "const x=\"".encode_utf16().collect::<Vec<_>>();
        original.extend([0xe000, 0xd800, 0xe000]);
        original.extend("\";".encode_utf16());
        let prepared = PreparedSource::new(&original).expect("prepared source");
        assert_eq!(prepared.fixups().len(), 1);
        assert_eq!(prepared.fixups()[0].unit, 0xd800);
        assert_eq!(prepared.rejected_fixup(), None);
    }

    #[test]
    fn active_units_use_an_invalid_sentinel_while_opaque_units_use_the_pua_marker() {
        let mut original = "const active=".encode_utf16().collect::<Vec<_>>();
        original.push(0xd800);
        original.extend("; const opaque=\"".encode_utf16());
        original.push(0xdc00);
        original.extend("\";".encode_utf16());

        let prepared = PreparedSource::new(&original).expect("prepared source");
        assert!(prepared.source().contains('\u{ffff}'));
        assert!(prepared.source().contains('\u{e000}'));
        assert_eq!(prepared.fixups()[0].placeholder(), '\u{ffff}');
        assert_eq!(prepared.fixups()[1].placeholder(), '\u{e000}');
    }

    #[test]
    fn dense_surrogate_storage_is_sparse_bounded_and_classified_in_one_source_pass() {
        const COUNT: usize = 4_096;

        let mut opaque = "const value=\"".encode_utf16().collect::<Vec<_>>();
        for _ in 0..COUNT {
            opaque.extend([0xd800, u16::from(b'x')]);
        }
        opaque.extend("\";".encode_utf16());
        let opaque = PreparedSource::new(&opaque).expect("dense opaque source");
        assert_eq!(opaque.fixups().len(), COUNT);
        assert_eq!(opaque.boundaries.as_ref().map(Vec::len), Some(COUNT));
        assert!(opaque.rejected_fixup().is_none());
        assert_eq!(opaque.source().matches('\u{e000}').count(), COUNT);

        let mut active = "const value=".encode_utf16().collect::<Vec<_>>();
        for _ in 0..COUNT {
            active.extend([0xdc00, u16::from(b'+')]);
        }
        active.extend("0;".encode_utf16());
        let active = PreparedSource::new(&active).expect("dense active source");
        assert_eq!(active.fixups().len(), COUNT);
        assert_eq!(active.boundaries.as_ref().map(Vec::len), Some(COUNT));
        assert_eq!(active.rejected_fixup(), active.fixups().first().copied());
        assert_eq!(active.source().matches('\u{ffff}').count(), COUNT);
        assert_eq!(active.work().input_units, active.original.len());
        assert_eq!(active.work().boundary_records, COUNT);
        assert_eq!(active.work().fixup_records, COUNT);
        assert_eq!(active.work().opaque_fixup_records, 0);
        assert_eq!(active.work().rejection_fixup_records, COUNT);
        assert_eq!(active.work().sanitized_bytes, COUNT * 3);
    }

    #[test]
    #[ignore = "run explicitly in release mode for retained scaling evidence"]
    fn release_bridge_scaling_campaign_is_linear_and_copy_bounded() {
        require_release_build();
        let sizes = [4_096_usize, 8_192, 16_384, 32_768];
        let mut well_formed_medians = Vec::new();
        for size in sizes {
            let mut source = "const value=\"".encode_utf16().collect::<Vec<_>>();
            for _ in 0..size {
                source.extend([0x00e9, 0xd83d, 0xde00]);
            }
            source.extend("\";".encode_utf16());
            let median = release_bridge_median(&source);
            let work = PreparedSource::new(&source)
                .expect("well-formed release work accounting")
                .work();
            assert_eq!(work.boundary_records, size * 2);
            assert_eq!(work.fixup_records, 0);
            assert_eq!(work.opaque_fixup_records, 0);
            assert_eq!(work.rejection_fixup_records, 0);
            assert_eq!(work.sanitized_bytes, 0);
            println!(
                "bridge lane=well_formed size={size} units={} utf8_bytes={} median_ns={median}",
                work.input_units, work.utf8_bytes
            );
            well_formed_medians.push(median);
        }
        assert_linear_scaling("well-formed bridge", &well_formed_medians);

        let mut surrogate_medians = Vec::new();
        for size in sizes {
            let mut source = "const value=\"".encode_utf16().collect::<Vec<_>>();
            for _ in 0..size {
                source.extend([0xd800, u16::from(b'x')]);
            }
            source.extend("\";".encode_utf16());
            let median = release_bridge_median(&source);
            let work = PreparedSource::new(&source)
                .expect("surrogate release work accounting")
                .work();
            assert_eq!(work.boundary_records, size);
            assert_eq!(work.fixup_records, size);
            assert_eq!(work.opaque_fixup_records, size);
            assert_eq!(work.rejection_fixup_records, 0);
            assert_eq!(work.sanitized_bytes, size * 3);
            println!(
                "bridge lane=surrogate size={} units={} utf8_bytes={} median_ns={median}",
                size, work.input_units, work.utf8_bytes
            );
            surrogate_medians.push(median);
        }
        assert_linear_scaling("surrogate bridge", &surrogate_medians);
    }
}
