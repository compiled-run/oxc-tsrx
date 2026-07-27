use tsrx_tape_schema::DiagnosticTable;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::{TsrxParseError, source_bridge::PreparedSource};

use super::observer::{RepairCopyLane, Utf16WorkObserver};

pub(super) fn repair_codeframes<W: Utf16WorkObserver>(
    diagnostics: &mut DiagnosticTable,
    source: &PreparedSource<'_>,
    observer: &mut W,
) -> Result<(), TsrxParseError> {
    if diagnostics.is_empty()
        || diagnostics.records().iter().all(|diagnostic| diagnostic.codeframe.get().is_none())
    {
        return Ok(());
    }
    let source_index = CodeframeSourceIndex::new(source)?;
    let mut repaired = Vec::new();
    for diagnostic in diagnostics.records().iter().copied() {
        let Some(range) = diagnostic.codeframe.get() else {
            continue;
        };
        let codeframe = diagnostics.string(range).ok_or_else(|| {
            TsrxParseError::Adapter("fresh diagnostic codeframe is not UTF-8".to_string())
        })?;
        let units = repair_codeframe_units(codeframe, &source_index)?;
        repaired.push((range, units));
    }
    diagnostics
        .repair_utf16_batch(repaired.iter().map(|(range, units)| (*range, units.as_slice())))?;
    observer.record_copy(
        RepairCopyLane::Codeframe,
        repaired.iter().map(|(_, units)| units.len()).sum(),
    );
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct CodeframeSourceLine {
    number: u32,
    byte_start: u32,
    byte_end: u32,
    first_fixup: usize,
    last_fixup: usize,
}

struct CodeframeSourceIndex<'source, 'original> {
    source: &'source PreparedSource<'original>,
    lines: Vec<CodeframeSourceLine>,
}

impl<'source, 'original> CodeframeSourceIndex<'source, 'original> {
    fn new(source: &'source PreparedSource<'original>) -> Result<Self, TsrxParseError> {
        let bytes = source.source().as_bytes();
        let fixups = source.fixups();
        let mut lines = Vec::new();
        let mut byte_start = 0_usize;
        let mut number = 1_u32;
        loop {
            let byte_end = bytes[byte_start..]
                .iter()
                .position(|byte| matches!(*byte, b'\r' | b'\n'))
                .map_or(bytes.len(), |relative| byte_start + relative);
            let start = u32::try_from(byte_start)
                .map_err(|_| TsrxParseError::Unsupported("codeframe line exceeds u32"))?;
            let end = u32::try_from(byte_end)
                .map_err(|_| TsrxParseError::Unsupported("codeframe line exceeds u32"))?;
            let first_fixup = fixups.partition_point(|fixup| fixup.byte_start < start);
            let last_fixup = fixups.partition_point(|fixup| fixup.byte_start < end);
            if first_fixup != last_fixup {
                lines.push(CodeframeSourceLine {
                    number,
                    byte_start: start,
                    byte_end: end,
                    first_fixup,
                    last_fixup,
                });
            }
            if byte_end == bytes.len() {
                break;
            }
            byte_start = byte_end + 1;
            if bytes.get(byte_end) == Some(&b'\r') && bytes.get(byte_start) == Some(&b'\n') {
                byte_start += 1;
            }
            number = number
                .checked_add(1)
                .ok_or(TsrxParseError::Unsupported("codeframe line count exceeds u32"))?;
        }
        Ok(Self { source, lines })
    }

    fn line(&self, number: u32) -> Option<CodeframeSourceLine> {
        self.lines
            .binary_search_by_key(&number, |line| line.number)
            .ok()
            .map(|index| self.lines[index])
    }

    fn source_line(&self, line: CodeframeSourceLine) -> Option<&str> {
        let start = usize::try_from(line.byte_start).ok()?;
        let end = usize::try_from(line.byte_end).ok()?;
        self.source.source().get(start..end)
    }

    fn fixups(&self, line: CodeframeSourceLine) -> &[crate::source_bridge::SourceFixup] {
        &self.source.fixups()[line.first_fixup..line.last_fixup]
    }
}

#[derive(Debug, Clone, Copy)]
struct RenderedSourceLine<'a> {
    number: u32,
    content: &'a str,
    content_byte_start: usize,
}

#[derive(Debug, Clone, Copy)]
struct LineAlignment {
    output_prefix: usize,
    projected_start: usize,
    visible_length: usize,
}

fn repair_codeframe_units(
    codeframe: &str,
    source: &CodeframeSourceIndex<'_, '_>,
) -> Result<Vec<u16>, TsrxParseError> {
    let mut patches = Vec::new();
    let mut line_byte_start = 0_usize;
    for rendered_with_ending in codeframe.split_inclusive('\n') {
        let rendered_text = rendered_with_ending
            .strip_suffix('\n')
            .unwrap_or(rendered_with_ending)
            .strip_suffix('\r')
            .unwrap_or_else(|| {
                rendered_with_ending.strip_suffix('\n').unwrap_or(rendered_with_ending)
            });
        let Some(rendered) = parse_rendered_source_line(rendered_text) else {
            line_byte_start += rendered_with_ending.len();
            continue;
        };
        let Some(source_line) = source.line(rendered.number) else {
            line_byte_start += rendered_with_ending.len();
            continue;
        };
        let authored_line = source.source_line(source_line).ok_or_else(|| {
            TsrxParseError::Adapter("indexed codeframe line is not UTF-8".to_string())
        })?;
        let mut mapped = map_rendered_line(
            rendered,
            authored_line,
            None,
            source.fixups(source_line),
            source_line.byte_start,
            line_byte_start,
            &mut patches,
        )?;
        if !mapped && authored_line.contains('\t') {
            let projection = expand_tabs(authored_line, 4);
            mapped = map_rendered_line(
                rendered,
                &projection.text,
                Some(&projection),
                source.fixups(source_line),
                source_line.byte_start,
                line_byte_start,
                &mut patches,
            )?;
        }
        let line_fixups = source.fixups(source_line);
        if !mapped && line_fixups.iter().any(|fixup| rendered.content.contains(fixup.placeholder()))
        {
            return Err(TsrxParseError::Adapter(format!(
                "displayed codeframe line {} could not be mapped losslessly",
                rendered.number
            )));
        }
        line_byte_start += rendered_with_ending.len();
    }
    for pair in patches.windows(2) {
        if pair[0].0 > pair[1].0 {
            return Err(TsrxParseError::Adapter(
                "codeframe patches are not emitted in rendered order".to_string(),
            ));
        }
        if pair[0].0 == pair[1].0 && pair[0] != pair[1] {
            return Err(TsrxParseError::Adapter(
                "conflicting position-keyed codeframe patches".to_string(),
            ));
        }
    }
    patches.dedup_by_key(|patch| patch.0);
    let mut output = Vec::with_capacity(codeframe.encode_utf16().count());
    let mut patches = patches.into_iter().peekable();
    for (byte_start, character) in codeframe.char_indices() {
        if patches.peek().is_some_and(|(patch_start, _, _)| *patch_start == byte_start) {
            let (_, unit, expected) = patches.next().expect("peeked codeframe patch exists");
            if character != expected {
                return Err(TsrxParseError::Adapter(
                    "codeframe patch does not target a placeholder".to_string(),
                ));
            }
            output.push(unit);
        } else {
            let mut encoded = [0_u16; 2];
            output.extend(character.encode_utf16(&mut encoded).iter().copied());
        }
    }
    if patches.next().is_some() {
        return Err(TsrxParseError::Adapter(
            "codeframe patch is outside rendered output".to_string(),
        ));
    }
    Ok(output)
}

fn parse_rendered_source_line(line: &str) -> Option<RenderedSourceLine<'_>> {
    let ascii = line.find('|').map(|index| (index, 1_usize));
    let unicode = line.find('│').map(|index| (index, '│'.len_utf8()));
    let (separator, separator_width) = match (ascii, unicode) {
        (Some(ascii), Some(unicode)) => ascii.min(unicode),
        (Some(ascii), None) => ascii,
        (None, Some(unicode)) => unicode,
        (None, None) => return None,
    };
    let number = line.get(..separator)?.trim().parse::<u32>().ok()?;
    let mut content_byte_start = separator.checked_add(separator_width)?;
    if line.get(content_byte_start..).is_some_and(|content| content.starts_with(' ')) {
        content_byte_start += 1;
    }
    Some(RenderedSourceLine {
        number,
        content: line.get(content_byte_start..)?,
        content_byte_start,
    })
}

#[derive(Debug)]
struct TabProjection {
    text: String,
    source_to_display: Vec<(usize, usize)>,
}

fn expand_tabs(source: &str, tab_width: usize) -> TabProjection {
    let mut text = String::with_capacity(source.len());
    let mut source_to_display = Vec::with_capacity(source.chars().count());
    let graphemes = (!source.is_ascii()).then(|| {
        source
            .grapheme_indices(true)
            .map(|(byte_start, grapheme)| (byte_start, grapheme.width()))
            .collect::<Vec<_>>()
    });
    let mut grapheme_index = 0_usize;
    let mut column = 0_usize;
    let mut escaped = false;
    for (source_byte, character) in source.char_indices() {
        source_to_display.push((source_byte, text.len()));
        let width = match (escaped, character) {
            (false, '\t') => tab_width - column % tab_width,
            (false, '\u{1b}') => {
                escaped = true;
                0
            }
            (false, _) => graphemes.as_ref().map_or(1, |boundaries| {
                if boundaries
                    .get(grapheme_index)
                    .is_some_and(|(byte_start, _)| *byte_start == source_byte)
                {
                    let width = boundaries[grapheme_index].1;
                    grapheme_index += 1;
                    width
                } else {
                    0
                }
            }),
            (true, 'm') => {
                escaped = false;
                0
            }
            (true, _) => 0,
        };
        if character == '\t' {
            text.extend(std::iter::repeat_n(' ', width));
        } else {
            text.push(character);
        }
        column += width;
    }
    TabProjection { text, source_to_display }
}

fn map_rendered_line(
    rendered: RenderedSourceLine<'_>,
    projected: &str,
    tab_projection: Option<&TabProjection>,
    fixups: &[crate::source_bridge::SourceFixup],
    source_byte_start: u32,
    rendered_line_start: usize,
    patches: &mut Vec<(usize, u16, char)>,
) -> Result<bool, TsrxParseError> {
    let Some(alignment) = align_rendered_content(rendered.content, projected) else {
        return Ok(false);
    };
    let visible_end = alignment
        .projected_start
        .checked_add(alignment.visible_length)
        .ok_or_else(|| TsrxParseError::Adapter("codeframe alignment overflow".to_string()))?;
    for fixup in fixups {
        let relative =
            usize::try_from(fixup.byte_start.checked_sub(source_byte_start).ok_or_else(|| {
                TsrxParseError::Adapter("codeframe fixup precedes its line".to_string())
            })?)
            .map_err(|_| TsrxParseError::Adapter("codeframe fixup overflow".to_string()))?;
        let projected_byte = if let Some(projection) = tab_projection {
            projection
                .source_to_display
                .binary_search_by_key(&relative, |(source_byte, _)| *source_byte)
                .ok()
                .map(|index| projection.source_to_display[index].1)
                .ok_or_else(|| {
                    TsrxParseError::Adapter("tab-expanded fixup is not at a character".to_string())
                })?
        } else {
            relative
        };
        if projected_byte < alignment.projected_start || projected_byte >= visible_end {
            continue;
        }
        let patch = rendered_line_start
            .checked_add(rendered.content_byte_start)
            .and_then(|value| value.checked_add(alignment.output_prefix))
            .and_then(|value| value.checked_add(projected_byte - alignment.projected_start))
            .ok_or_else(|| TsrxParseError::Adapter("codeframe patch overflow".to_string()))?;
        patches.push((patch, fixup.unit, fixup.placeholder()));
    }
    Ok(true)
}

fn align_rendered_content(rendered: &str, projected: &str) -> Option<LineAlignment> {
    if rendered == projected {
        return Some(LineAlignment {
            output_prefix: 0,
            projected_start: 0,
            visible_length: rendered.len(),
        });
    }
    if let Some(start) = unique_substring(projected, rendered) {
        return Some(LineAlignment {
            output_prefix: 0,
            projected_start: start,
            visible_length: rendered.len(),
        });
    }
    let (output_prefix, core) = if let Some(core) = rendered.strip_prefix("...") {
        (3, core)
    } else if let Some(core) = rendered.strip_prefix('…') {
        ('…'.len_utf8(), core)
    } else {
        (0, rendered)
    };
    let core = core.strip_suffix("...").or_else(|| core.strip_suffix('…')).unwrap_or(core);
    let start = unique_substring(projected, core)?;
    Some(LineAlignment { output_prefix, projected_start: start, visible_length: core.len() })
}

fn unique_substring(haystack: &str, needle: &str) -> Option<usize> {
    if needle.is_empty() {
        return None;
    }
    let mut matches = haystack.match_indices(needle);
    let first = matches.next()?.0;
    matches.next().is_none().then_some(first)
}
