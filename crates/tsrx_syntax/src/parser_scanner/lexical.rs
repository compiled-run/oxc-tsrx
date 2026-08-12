//! Skipping the JavaScript token forms whose interiors must not be searched for TSRX syntax:
//! strings, templates, regexes, comments, numbers, and identifiers.

use crate::diagnostics::{ProjectionError, to_u32};

use super::Scanner;
use super::surrogates::OpaqueSurrogateContext;

impl Scanner<'_> {
    pub(super) fn scan_template(&mut self, start: usize) -> Result<usize, ProjectionError> {
        let mut index = start + 1;
        let mut raw_start = index;
        let mut escaped = false;
        while index < self.bytes.len() {
            let byte = self.bytes[index];
            if escaped {
                escaped = false;
                index += 1;
            } else if byte == b'\\' {
                escaped = true;
                index += 1;
            } else if byte == b'`' {
                self.mark_surrogates(raw_start, index, OpaqueSurrogateContext::TemplateRaw);
                return Ok(index + 1);
            } else if byte == b'$' && self.bytes.get(index + 1) == Some(&b'{') {
                self.mark_surrogates(raw_start, index, OpaqueSurrogateContext::TemplateRaw);
                index = self.scan_expression_region(index + 2, Some(b'}'))?;
                raw_start = index;
            } else {
                index += 1;
            }
        }
        Err(ProjectionError::UnterminatedSyntax {
            offset: to_u32(start)?,
            construct: "template literal",
        })
    }

    pub(super) fn skip_template_raw(
        &self,
        start: usize,
        end: usize,
    ) -> Result<usize, ProjectionError> {
        let mut index = start + 1;
        let mut raw_start = index;
        let mut escaped = false;
        let mut braces = 0usize;
        while index < end {
            let byte = self.bytes[index];
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'`' && braces == 0 {
                self.mark_surrogates(raw_start, index, OpaqueSurrogateContext::TemplateRaw);
                return Ok(index + 1);
            } else if byte == b'$' && self.bytes.get(index + 1) == Some(&b'{') {
                if braces == 0 {
                    self.mark_surrogates(raw_start, index, OpaqueSurrogateContext::TemplateRaw);
                }
                braces += 1;
                index += 1;
            } else if byte == b'}' && braces > 0 {
                braces -= 1;
                if braces == 0 {
                    raw_start = index + 1;
                }
            }
            index += 1;
        }
        Err(ProjectionError::UnterminatedSyntax {
            offset: to_u32(start)?,
            construct: "template literal",
        })
    }

    pub(super) fn skip_quote(&self, start: usize, quote: u8) -> Result<usize, ProjectionError> {
        let mut index = start + 1;
        let mut escaped = false;
        while index < self.bytes.len() {
            let byte = self.bytes[index];
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == quote {
                self.mark_surrogates(start + 1, index, OpaqueSurrogateContext::QuotedString);
                return Ok(index + 1);
            } else if matches!(byte, b'\n' | b'\r') {
                break;
            }
            index += 1;
        }
        Err(ProjectionError::UnterminatedSyntax {
            offset: to_u32(start)?,
            construct: "quoted string",
        })
    }

    /// JSX quoted attribute values may contain literal line terminators. JavaScript strings may
    /// not, so keep this separate from `skip_quote` rather than weakening the ordinary lexical
    /// boundary used everywhere else in the scanner.
    pub(super) fn skip_jsx_quote(&self, start: usize, quote: u8) -> Result<usize, ProjectionError> {
        let mut index = start + 1;
        let mut escaped = false;
        while index < self.bytes.len() {
            let byte = self.bytes[index];
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == quote {
                self.mark_surrogates(start + 1, index, OpaqueSurrogateContext::QuotedString);
                return Ok(index + 1);
            }
            index += 1;
        }
        Err(ProjectionError::UnterminatedSyntax {
            offset: to_u32(start)?,
            construct: "quoted JSX attribute",
        })
    }

    pub(super) fn skip_regex(&self, start: usize) -> Result<usize, ProjectionError> {
        let mut index = start + 1;
        let mut escaped = false;
        let mut in_class = false;
        while index < self.bytes.len() {
            let byte = self.bytes[index];
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'[' {
                in_class = true;
            } else if byte == b']' {
                in_class = false;
            } else if byte == b'/' && !in_class {
                self.mark_surrogates(start + 1, index, OpaqueSurrogateContext::RegexBody);
                index += 1;
                while let Some(width) = self.identifier_continue_width(index) {
                    index += width;
                }
                return Ok(index);
            } else if matches!(byte, b'\n' | b'\r') {
                break;
            }
            index += 1;
        }
        Err(ProjectionError::UnterminatedSyntax {
            offset: to_u32(start)?,
            construct: "regular expression literal",
        })
    }

    pub(super) fn skip_number(&self, mut index: usize) -> usize {
        while self
            .bytes
            .get(index)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.'))
        {
            index += 1;
        }
        index
    }

    /// Returns true for TypeScript type arguments and generic-arrow parameter lists that begin
    /// where an expression could otherwise begin with JSX. This is deliberately a narrow
    /// disambiguation: ordinary JSX remains committed by `committed_jsx_opening`, while the forms
    /// TypeScript requires to disambiguate generic arrows (`extends`, a default, or a trailing
    /// comma) are left for OXC.
    pub(super) fn looks_like_typescript_type_parameters(&self, start: usize) -> bool {
        if start > 0
            && self.bytes.get(start - 1).is_some_and(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$' | b']' | b')')
            })
        {
            return true;
        }

        let name_start = start + 1;
        if self.identifier_start_width(name_start).is_none() {
            return false;
        }
        let name_end = self.skip_identifier(name_start);
        let marker = self.skip_ascii_whitespace(name_end, self.bytes.len());
        if !matches!(self.bytes.get(marker), Some(b',' | b'='))
            && !self.bare_keyword_at(marker, b"extends")
        {
            return false;
        }

        self.type_parameter_list_precedes_parameters(name_end)
    }

    fn type_parameter_list_precedes_parameters(&self, mut index: usize) -> bool {
        let mut depth = 1_u32;
        while let Some(&byte) = self.bytes.get(index) {
            match byte {
                b'\'' | b'"' => {
                    let Ok(end) = self.skip_quote(index, byte) else {
                        return false;
                    };
                    index = end;
                }
                b'/' if self.bytes.get(index + 1) == Some(&b'*') => {
                    let Ok(end) = self.skip_block_comment(index) else {
                        return false;
                    };
                    index = end;
                }
                b'/' if self.bytes.get(index + 1) == Some(&b'/') => {
                    index = self.skip_line_comment(index + 2);
                }
                b'<' => {
                    depth = depth.saturating_add(1);
                    index += 1;
                }
                b'>' if self.bytes.get(index.wrapping_sub(1)) != Some(&b'=') => {
                    depth -= 1;
                    index += 1;
                    if depth == 0 {
                        return self
                            .skip_trivia(index)
                            .is_ok_and(|next| self.bytes.get(next) == Some(&b'('));
                    }
                }
                _ => index += 1,
            }
        }
        false
    }

    pub(super) fn skip_line_comment(&self, mut index: usize) -> usize {
        let start = index;
        while index < self.bytes.len() && !matches!(self.bytes[index], b'\n' | b'\r') {
            index += 1;
        }
        self.mark_surrogates(start, index, OpaqueSurrogateContext::Comment);
        index
    }

    pub(super) fn skip_block_comment(&self, start: usize) -> Result<usize, ProjectionError> {
        let mut index = start + 2;
        while index + 1 < self.bytes.len() {
            if self.bytes[index..index + 2] == *b"*/" {
                self.mark_surrogates(start + 2, index, OpaqueSurrogateContext::Comment);
                return Ok(index + 2);
            }
            index += 1;
        }
        Err(ProjectionError::UnterminatedSyntax {
            offset: to_u32(start)?,
            construct: "block comment",
        })
    }

    pub(super) fn skip_trivia(&self, mut index: usize) -> Result<usize, ProjectionError> {
        loop {
            while self.bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
                index += 1;
            }
            if self.bytes.get(index..index + 2) == Some(b"//") {
                index = self.skip_line_comment(index + 2);
            } else if self.bytes.get(index..index + 2) == Some(b"/*") {
                index = self.skip_block_comment(index)?;
            } else {
                return Ok(index);
            }
        }
    }

    pub(super) fn skip_ascii_whitespace(&self, mut index: usize, end: usize) -> usize {
        while index < end && self.bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        index
    }

    pub(super) fn lazy_pattern_start(&self, ampersand: usize) -> Option<usize> {
        let pattern_start = self.skip_ascii_whitespace(ampersand.checked_add(1)?, self.bytes.len());
        if !matches!(self.bytes.get(pattern_start), Some(b'[' | b'{')) {
            return None;
        }
        let mut keyword_end = ampersand;
        while keyword_end > 0 && self.bytes[keyword_end - 1].is_ascii_whitespace() {
            keyword_end -= 1;
        }
        let mut keyword_start = keyword_end;
        while keyword_start > 0 && self.bytes[keyword_start - 1].is_ascii_alphabetic() {
            keyword_start -= 1;
        }
        if matches!(self.bytes.get(keyword_start..keyword_end), Some(b"let" | b"const" | b"var")) {
            return Some(pattern_start);
        }

        let open = (0..ampersand).rev().find(|index| !self.bytes[*index].is_ascii_whitespace())?;
        if self.bytes[open] != b'(' {
            return None;
        }
        let mut name_end = open;
        while name_end > 0 && self.bytes[name_end - 1].is_ascii_whitespace() {
            name_end -= 1;
        }
        let mut name_start = name_end;
        while name_start > 0
            && (self.bytes[name_start - 1].is_ascii_alphanumeric()
                || matches!(self.bytes[name_start - 1], b'_' | b'$'))
        {
            name_start -= 1;
        }
        let mut function_end = name_start;
        while function_end > 0 && self.bytes[function_end - 1].is_ascii_whitespace() {
            function_end -= 1;
        }
        function_end
            .checked_sub("function".len())
            .filter(|start| self.bytes.get(*start..function_end) == Some(b"function"))
            .map(|_| pattern_start)
    }

    pub(super) fn keyword_at(&self, index: usize, keyword: &[u8]) -> bool {
        let end = index + 1 + keyword.len();
        self.bytes.get(index) == Some(&b'@')
            && self.bytes.get(index + 1..end) == Some(keyword)
            && identifier_continue_width(self.bytes, end).is_none()
    }

    pub(super) fn bare_keyword_at(&self, index: usize, keyword: &[u8]) -> bool {
        let end = index + keyword.len();
        self.bytes.get(index..end) == Some(keyword)
            && identifier_continue_width(self.bytes, end).is_none()
            && !identifier_continue_before(self.bytes, index)
    }

    pub(super) const fn after_keyword(index: usize, keyword: &[u8]) -> usize {
        index + 1 + keyword.len()
    }

    pub(super) const fn after_bare_keyword(index: usize, keyword: &[u8]) -> usize {
        index + keyword.len()
    }

    #[inline]
    pub(super) fn identifier_start_width(&self, index: usize) -> Option<usize> {
        identifier_start_width(self.bytes, index)
    }

    #[inline]
    pub(super) fn identifier_continue_width(&self, index: usize) -> Option<usize> {
        identifier_continue_width(self.bytes, index)
    }

    pub(super) fn skip_identifier(&self, mut index: usize) -> usize {
        let Some(width) = self.identifier_start_width(index) else {
            return index;
        };
        index += width;
        while let Some(width) = self.identifier_continue_width(index) {
            index += width;
        }
        index
    }
}

pub(super) fn trim_ascii_end(bytes: &[u8], start: usize, mut end: usize) -> usize {
    while end > start && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    end
}

pub(super) fn previous_significant_byte(bytes: &[u8], before: usize) -> Option<u8> {
    bytes[..before].iter().rfind(|byte| !byte.is_ascii_whitespace()).copied()
}

pub(super) fn unsupported_at_construct(bytes: &[u8], index: usize) -> Option<&'static str> {
    const UNSUPPORTED: [(&[u8], &str); 1] = [(b"await", "@await control flow")];
    UNSUPPORTED.iter().find_map(|(keyword, construct)| {
        let end = index + 1 + keyword.len();
        (bytes.get(index + 1..end) == Some(*keyword)
            && identifier_continue_width(bytes, end).is_none())
        .then_some(*construct)
    })
}

#[inline]
pub(super) fn identifier_start_width(bytes: &[u8], index: usize) -> Option<usize> {
    let byte = *bytes.get(index)?;
    if is_identifier_start(byte) {
        return Some(1);
    }
    if byte.is_ascii() {
        return None;
    }
    let (character, width) = decode_non_ascii_utf8(bytes, index)?;
    (unicode_identifier_start(character) || matches!(character, '\u{e000}' | '\u{ffff}'))
        .then_some(width)
}

/// The structural scanner only needs to preserve expression state, not validate identifiers.
/// After a proven start, consuming a complete non-ASCII scalar is deliberately conservative: all
/// ECMAScript `ID_Continue` scalars are covered without a generated Unicode table, while invalid
/// UTF-8 (including raw WTF-8 surrogate triples) remains active and unconsumed.
#[inline]
pub(super) fn identifier_continue_width(bytes: &[u8], index: usize) -> Option<usize> {
    let byte = *bytes.get(index)?;
    if is_identifier_continue(byte) {
        return Some(1);
    }
    if byte.is_ascii() {
        return None;
    }
    decode_non_ascii_utf8(bytes, index).map(|(_, width)| width)
}

fn identifier_continue_before(bytes: &[u8], index: usize) -> bool {
    let Some(mut start) = index.checked_sub(1) else {
        return false;
    };
    if bytes[start].is_ascii() {
        return is_identifier_continue(bytes[start]);
    }
    let lower_bound = index.saturating_sub(4);
    while start > lower_bound && bytes[start] & 0b1100_0000 == 0b1000_0000 {
        start -= 1;
    }
    identifier_continue_width(bytes, start).is_some_and(|width| start + width == index)
}

#[inline]
fn decode_non_ascii_utf8(bytes: &[u8], index: usize) -> Option<(char, usize)> {
    let width = match *bytes.get(index)? {
        0xC2..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF4 => 4,
        _ => return None,
    };
    let end = index.checked_add(width)?;
    let encoded = bytes.get(index..end)?;
    let character = std::str::from_utf8(encoded).ok()?.chars().next()?;
    Some((character, width))
}

#[inline]
fn unicode_identifier_start(character: char) -> bool {
    character.is_alphabetic()
        || matches!(
            character,
            '\u{1885}' | '\u{1886}' | '\u{2118}' | '\u{212E}' | '\u{309B}' | '\u{309C}'
        )
}

pub(crate) const fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || matches!(byte, b'_' | b'$')
}

pub(crate) const fn is_identifier_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$')
}
