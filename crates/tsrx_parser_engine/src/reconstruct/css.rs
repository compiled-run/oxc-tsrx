use super::edits::append_node_head;
use super::spans::{AuthoredStart, record_authored_span};
use crate::TsrxParseError;
use tsrx_syntax::ByteSpan;
use tsrx_tape_schema::{
    FlatTape, ListRecord, ListValueRecord, ObjectRecord, RecordIndex, ValueRef,
};

#[derive(Debug, Clone, Copy, Default)]
struct CssListBuilder {
    first: RecordIndex,
    last: RecordIndex,
    length: u32,
}

impl CssListBuilder {
    fn push(&mut self, tape: &mut FlatTape, value: ValueRef) -> Result<(), TsrxParseError> {
        let entry =
            tape.push_list_value_record(ListValueRecord { value, next: RecordIndex::NONE })?;
        if self.first.is_none() {
            self.first = entry;
        } else {
            tape.set_list_value_next(self.last, entry)?;
        }
        self.last = entry;
        self.length = self
            .length
            .checked_add(1)
            .ok_or(TsrxParseError::Unsupported("CSS list length overflow"))?;
        Ok(())
    }

    fn finish(self, tape: &mut FlatTape) -> Result<RecordIndex, TsrxParseError> {
        tape.push_list_record(ListRecord { first_value: self.first, length: self.length })
            .map_err(Into::into)
    }
}

struct CssTapeBuilder<'tape, 'source, 'starts> {
    tape: &'tape mut FlatTape,
    source: &'source str,
    coordinates: CssCoordinates,
    starts: &'starts mut Vec<AuthoredStart>,
}

impl CssTapeBuilder<'_, '_, '_> {
    fn stylesheet(&mut self) -> Result<RecordIndex, TsrxParseError> {
        let children = self.rule_children(0, self.source.len())?;
        let sheet = self.node(r#""StyleSheet""#, 0, self.source.len())?;
        self.tape.append_field(sheet, "children", ValueRef::list(children))?;
        let source = self.tape.push_json_string_scalar(self.source)?;
        self.tape.append_field(sheet, "source", source)?;
        Ok(sheet)
    }

    fn rule_children(&mut self, start: usize, end: usize) -> Result<RecordIndex, TsrxParseError> {
        let mut list = CssListBuilder::default();
        let mut cursor = start;
        while cursor < end {
            skip_css_trivia(self.source.as_bytes(), &mut cursor, end);
            if cursor >= end {
                break;
            }
            let before = cursor;
            let node = if self.source.as_bytes()[cursor] == b'@' {
                self.at_rule(&mut cursor, end)?
            } else {
                self.rule(&mut cursor, end)?
            };
            if let Some(node) = node {
                list.push(self.tape, ValueRef::object(node))?;
            }
            if cursor <= before {
                cursor = before + 1;
            }
        }
        list.finish(self.tape)
    }

    fn at_rule(
        &mut self,
        cursor: &mut usize,
        end: usize,
    ) -> Result<Option<RecordIndex>, TsrxParseError> {
        let start = *cursor;
        let bytes = self.source.as_bytes();
        let mut name_end = start + 1;
        while name_end < end
            && matches!(bytes[name_end], b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_')
        {
            name_end += 1;
        }
        let Some((delimiter, kind)) = scan_css_delimiter(bytes, name_end, end) else {
            *cursor = end;
            return Ok(None);
        };
        if kind == b'}' {
            *cursor = delimiter;
            return Ok(None);
        }

        let (prelude_start, prelude_end) = trim_css_range(bytes, name_end, delimiter);
        let (block, node_end) = if kind == b'{' {
            let closing = find_css_block_end(bytes, delimiter, end);
            let content_end = closing.unwrap_or(end);
            let children = self.rule_children(delimiter + 1, content_end)?;
            let block_end = closing.map_or(end, |closing| closing + 1);
            let block = self.block(delimiter, block_end, children)?;
            (Some(block), block_end)
        } else {
            (None, delimiter + 1)
        };
        *cursor = node_end;

        let node = self.node(r#""Atrule""#, start, node_end)?;
        let name = self
            .tape
            .push_json_string_scalar(self.source.get(start + 1..name_end).unwrap_or(""))?;
        self.tape.append_field(node, "name", name)?;
        let prelude = self
            .tape
            .push_json_string_scalar(self.source.get(prelude_start..prelude_end).unwrap_or(""))?;
        self.tape.append_field(node, "prelude", prelude)?;
        let block = if let Some(block) = block {
            ValueRef::object(block)
        } else {
            self.tape.push_scalar("null")?
        };
        self.tape.append_field(node, "block", block)?;
        Ok(Some(node))
    }

    fn rule(
        &mut self,
        cursor: &mut usize,
        end: usize,
    ) -> Result<Option<RecordIndex>, TsrxParseError> {
        let start = *cursor;
        let bytes = self.source.as_bytes();
        let Some((delimiter, kind)) = scan_css_delimiter(bytes, start, end) else {
            *cursor = end;
            return Ok(None);
        };
        if kind != b'{' {
            *cursor = if kind == b';' { delimiter + 1 } else { delimiter };
            return Ok(None);
        }
        let (selector_start, selector_end) = trim_css_range(bytes, start, delimiter);
        let closing = find_css_block_end(bytes, delimiter, end);
        let node_end = closing.map_or(end, |closing| closing + 1);
        *cursor = node_end;
        if selector_start == selector_end {
            return Ok(None);
        }

        let prelude = self.selector_list(selector_start, selector_end)?;
        let empty = CssListBuilder::default().finish(self.tape)?;
        let block = self.block(delimiter, node_end, empty)?;
        let rule = self.node(r#""Rule""#, selector_start, node_end)?;
        self.tape.append_field(rule, "prelude", ValueRef::object(prelude))?;
        self.tape.append_field(rule, "block", ValueRef::object(block))?;
        Ok(Some(rule))
    }

    fn selector_list(&mut self, start: usize, end: usize) -> Result<RecordIndex, TsrxParseError> {
        let bytes = self.source.as_bytes();
        let mut selectors = CssListBuilder::default();
        let mut segment_start = start;
        let mut cursor = start;
        let mut quote = None;
        let mut escaped = false;
        let mut parentheses = 0_u32;
        let mut brackets = 0_u32;
        while cursor <= end {
            let at_end = cursor == end;
            let byte = (!at_end).then(|| bytes[cursor]);
            if quote.is_some() {
                if escaped {
                    escaped = false;
                } else if byte == Some(b'\\') {
                    escaped = true;
                } else if byte == quote {
                    quote = None;
                }
                cursor += 1;
                continue;
            }
            match byte {
                Some(b'\'' | b'"') => quote = byte,
                Some(b'(') => parentheses = parentheses.saturating_add(1),
                Some(b')') => parentheses = parentheses.saturating_sub(1),
                Some(b'[') => brackets = brackets.saturating_add(1),
                Some(b']') => brackets = brackets.saturating_sub(1),
                Some(b'/') if bytes.get(cursor + 1) == Some(&b'*') => {
                    cursor = skip_css_comment(bytes, cursor + 2, end);
                    continue;
                }
                Some(b',') if parentheses == 0 && brackets == 0 => {
                    self.push_complex_selector(&mut selectors, segment_start, cursor)?;
                    segment_start = cursor + 1;
                }
                None => self.push_complex_selector(&mut selectors, segment_start, end)?,
                _ => {}
            }
            cursor += 1;
        }
        let selector_list = self.node(r#""SelectorList""#, start, end)?;
        let selectors = selectors.finish(self.tape)?;
        self.tape.append_field(selector_list, "children", ValueRef::list(selectors))?;
        Ok(selector_list)
    }

    fn push_complex_selector(
        &mut self,
        selectors: &mut CssListBuilder,
        start: usize,
        end: usize,
    ) -> Result<(), TsrxParseError> {
        let bytes = self.source.as_bytes();
        let (mut start, end) = trim_css_range(bytes, start, end);
        skip_css_trivia(bytes, &mut start, end);
        if start >= end {
            return Ok(());
        }
        let selector = self.node(r#""ComplexSelector""#, start, end)?;
        let children = CssListBuilder::default().finish(self.tape)?;
        self.tape.append_field(selector, "children", ValueRef::list(children))?;
        selectors.push(self.tape, ValueRef::object(selector))
    }

    fn block(
        &mut self,
        start: usize,
        end: usize,
        children: RecordIndex,
    ) -> Result<RecordIndex, TsrxParseError> {
        let block = self.node(r#""Block""#, start, end)?;
        self.tape.append_field(block, "children", ValueRef::list(children))?;
        Ok(block)
    }

    fn node(
        &mut self,
        kind: &str,
        start: usize,
        end: usize,
    ) -> Result<RecordIndex, TsrxParseError> {
        let object = self.tape.push_object_record(ObjectRecord::default())?;
        let span = ByteSpan::new(
            self.coordinates.utf16_offset(start)?,
            self.coordinates.utf16_offset(end)?,
        );
        append_node_head(self.tape, object, kind, span)?;
        record_authored_span(self.starts, object, span);
        Ok(object)
    }
}

pub(super) fn build_style_children(
    tape: &mut FlatTape,
    css: &str,
    starts: &mut Vec<AuthoredStart>,
) -> Result<RecordIndex, TsrxParseError> {
    let stylesheet =
        CssTapeBuilder { tape, source: css, coordinates: CssCoordinates::new(css), starts }
            .stylesheet()?;
    let mut children = CssListBuilder::default();
    children.push(tape, ValueRef::object(stylesheet))?;
    children.finish(tape)
}

struct CssCoordinates {
    adjustments: Vec<(usize, usize)>,
}

impl CssCoordinates {
    fn new(source: &str) -> Self {
        let mut adjustments = Vec::new();
        let mut reduction = 0_usize;
        for (start, character) in source.char_indices() {
            let utf8 = character.len_utf8();
            let utf16 = character.len_utf16();
            if utf8 != utf16 {
                reduction += utf8 - utf16;
                adjustments.push((start + utf8, reduction));
            }
        }
        Self { adjustments }
    }

    fn utf16_offset(&self, utf8_offset: usize) -> Result<u32, TsrxParseError> {
        let completed = self.adjustments.partition_point(|(end, _)| *end <= utf8_offset);
        let reduction = completed
            .checked_sub(1)
            .and_then(|index| self.adjustments.get(index))
            .map_or(0, |(_, reduction)| *reduction);
        let offset = utf8_offset
            .checked_sub(reduction)
            .ok_or(TsrxParseError::Unsupported("invalid CSS UTF-16 offset"))?;
        u32::try_from(offset).map_err(|_| TsrxParseError::Unsupported("CSS offset exceeds 4 GiB"))
    }
}

fn trim_css_range(bytes: &[u8], mut start: usize, mut end: usize) -> (usize, usize) {
    while start < end && bytes[start].is_ascii_whitespace() {
        start += 1;
    }
    while end > start && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    (start, end)
}

fn skip_css_trivia(bytes: &[u8], cursor: &mut usize, end: usize) {
    loop {
        while *cursor < end && bytes[*cursor].is_ascii_whitespace() {
            *cursor += 1;
        }
        if *cursor + 1 < end && bytes[*cursor..].starts_with(b"/*") {
            *cursor = skip_css_comment(bytes, *cursor + 2, end);
        } else {
            return;
        }
    }
}

fn skip_css_comment(bytes: &[u8], mut cursor: usize, end: usize) -> usize {
    while cursor + 1 < end {
        if bytes[cursor] == b'*' && bytes[cursor + 1] == b'/' {
            return cursor + 2;
        }
        cursor += 1;
    }
    end
}

fn scan_css_delimiter(bytes: &[u8], mut cursor: usize, end: usize) -> Option<(usize, u8)> {
    let mut quote = None;
    let mut escaped = false;
    let mut parentheses = 0_u32;
    let mut brackets = 0_u32;
    while cursor < end {
        let byte = bytes[cursor];
        if quote.is_some() {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if Some(byte) == quote {
                quote = None;
            }
            cursor += 1;
            continue;
        }
        match byte {
            b'\'' | b'"' => quote = Some(byte),
            b'/' if bytes.get(cursor + 1) == Some(&b'*') => {
                cursor = skip_css_comment(bytes, cursor + 2, end);
                continue;
            }
            b'(' => parentheses = parentheses.saturating_add(1),
            b')' => parentheses = parentheses.saturating_sub(1),
            b'[' => brackets = brackets.saturating_add(1),
            b']' => brackets = brackets.saturating_sub(1),
            b'{' | b';' | b'}' if parentheses == 0 && brackets == 0 => {
                return Some((cursor, byte));
            }
            _ => {}
        }
        cursor += 1;
    }
    None
}

fn find_css_block_end(bytes: &[u8], opening: usize, end: usize) -> Option<usize> {
    let mut cursor = opening + 1;
    let mut depth = 1_u32;
    let mut quote = None;
    let mut escaped = false;
    while cursor < end {
        let byte = bytes[cursor];
        if quote.is_some() {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if Some(byte) == quote {
                quote = None;
            }
            cursor += 1;
            continue;
        }
        match byte {
            b'\'' | b'"' => quote = Some(byte),
            b'/' if bytes.get(cursor + 1) == Some(&b'*') => {
                cursor = skip_css_comment(bytes, cursor + 2, end);
                continue;
            }
            b'{' => depth = depth.saturating_add(1),
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(cursor);
                }
            }
            _ => {}
        }
        cursor += 1;
    }
    None
}
