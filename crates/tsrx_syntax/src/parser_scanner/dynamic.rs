//! Dynamic tags, whose closing expression has to match its opening token for token before the
//! element is accepted at all.

use crate::{
    diagnostics::{ProjectionError, to_u32},
    model::ByteSpan,
};

use super::Scanner;

impl Scanner<'_> {
    pub(super) fn scan_dynamic_expression(
        &self,
        open: usize,
    ) -> Result<(ByteSpan, usize), ProjectionError> {
        if self.bytes.get(open) != Some(&b'{') {
            return Err(ProjectionError::MalformedSyntax {
                offset: to_u32(open)?,
                expected: "a dynamic JSX tag expression",
            });
        }
        let mut index = open + 1;
        let mut braces = 1usize;
        let mut can_start_expression = true;
        while index < self.bytes.len() {
            match self.bytes[index] {
                b'\'' | b'"' => {
                    index = self.skip_quote(index, self.bytes[index])?;
                    can_start_expression = false;
                }
                b'`' => {
                    index = self.skip_template_raw(index, self.bytes.len())?;
                    can_start_expression = false;
                }
                b'/' if self.bytes.get(index + 1) == Some(&b'/') => {
                    index = self.skip_line_comment(index + 2);
                }
                b'/' if self.bytes.get(index + 1) == Some(&b'*') => {
                    index = self.skip_block_comment(index)?;
                }
                b'/' if can_start_expression => {
                    index = self.skip_regex(index)?;
                    can_start_expression = false;
                }
                b'{' => {
                    braces += 1;
                    index += 1;
                    can_start_expression = true;
                }
                b'}' => {
                    braces -= 1;
                    if braces == 0 {
                        return Ok((ByteSpan::new(to_u32(open + 1)?, to_u32(index)?), index + 1));
                    }
                    index += 1;
                    can_start_expression = false;
                }
                _ if self.identifier_start_width(index).is_some() => {
                    index = self.skip_identifier(index);
                    can_start_expression = false;
                }
                byte if byte.is_ascii_digit() => {
                    index = self.skip_number(index);
                    can_start_expression = false;
                }
                b')' | b']' => {
                    index += 1;
                    can_start_expression = false;
                }
                b'.' if self.bytes.get(index + 1) != Some(&b'.') => {
                    index += 1;
                    can_start_expression = false;
                }
                _ => {
                    index += 1;
                    can_start_expression = true;
                }
            }
        }
        Err(ProjectionError::UnterminatedSyntax {
            offset: to_u32(open)?,
            construct: "dynamic JSX tag expression",
        })
    }

    pub(super) fn validate_dynamic_expression(
        &self,
        span: ByteSpan,
        nested_start: usize,
        nested_end: usize,
    ) -> Result<ByteSpan, ProjectionError> {
        let identity = self.dynamic_identity_range(span, nested_start, nested_end)?;
        if identity.is_empty() {
            return Err(ProjectionError::MalformedSyntax {
                offset: span.start,
                expected: "a valid dynamic JSX tag expression",
            });
        }
        Ok(identity)
    }

    pub(super) fn same_dynamic_identity(&self, opening: ByteSpan, closing: ByteSpan) -> bool {
        self.bytes[opening.start as usize..opening.end as usize]
            == self.bytes[closing.start as usize..closing.end as usize]
            || self.span_contains_collision_scalar(opening)
            || self.span_contains_collision_scalar(closing)
    }

    pub(super) fn span_contains_collision_scalar(&self, span: ByteSpan) -> bool {
        self.bytes
            .get(span.start as usize..span.end as usize)
            .is_some_and(contains_collision_scalar)
    }

    fn dynamic_identity_range(
        &self,
        span: ByteSpan,
        nested_start: usize,
        nested_end: usize,
    ) -> Result<ByteSpan, ProjectionError> {
        let mut index = span.start as usize;
        let end = span.end as usize;
        let mut can_start_expression = true;
        let mut first_start = None;
        let mut last_end = span.start as usize;
        let mut previous_end = span.start as usize;
        let mut in_leading_prefix = true;
        let mut leading_unclosed = 0usize;
        let mut other_depth = 0usize;
        let mut trailing_outer_closures = 0usize;
        let mut trailing_inner_end = span.start as usize;
        let mut nested_cursor = nested_start;
        while let Some((token_start, token_end)) = self.next_dynamic_identity_token(
            &mut index,
            end,
            &mut can_start_expression,
            &mut nested_cursor,
            nested_end,
        )? {
            first_start.get_or_insert(token_start);
            let byte = self.bytes[token_start];
            let leading_open = in_leading_prefix && byte == b'(';
            if leading_open {
                leading_unclosed += 1;
                trailing_outer_closures = 0;
            } else {
                in_leading_prefix = false;
                let closes_leading = if byte == b'(' {
                    other_depth += 1;
                    false
                } else if byte == b')' && other_depth > 0 {
                    other_depth -= 1;
                    false
                } else if byte == b')' && leading_unclosed > 0 {
                    leading_unclosed -= 1;
                    true
                } else {
                    false
                };
                if closes_leading {
                    if trailing_outer_closures == 0 {
                        trailing_inner_end = previous_end;
                    }
                    trailing_outer_closures += 1;
                } else {
                    trailing_outer_closures = 0;
                }
            }
            previous_end = token_end;
            last_end = token_end;
        }
        if nested_cursor != nested_end {
            return Err(ProjectionError::StructuralMismatch);
        }
        let Some(first_start) = first_start else {
            return Ok(ByteSpan::new(span.start, span.start));
        };
        if trailing_outer_closures == 0 {
            return Ok(ByteSpan::new(to_u32(first_start)?, to_u32(last_end)?));
        }

        let mut normalized_start = first_start;
        let mut prefix_index = span.start as usize;
        let mut prefix_can_start_expression = true;
        let mut prefix_nested_cursor = nested_start;
        for _ in 0..trailing_outer_closures {
            let Some((token_start, _)) = self.next_dynamic_identity_token(
                &mut prefix_index,
                end,
                &mut prefix_can_start_expression,
                &mut prefix_nested_cursor,
                nested_end,
            )?
            else {
                return Err(ProjectionError::StructuralMismatch);
            };
            if self.bytes[token_start] != b'(' {
                return Err(ProjectionError::StructuralMismatch);
            }
        }
        if let Some((token_start, _)) = self.next_dynamic_identity_token(
            &mut prefix_index,
            end,
            &mut prefix_can_start_expression,
            &mut prefix_nested_cursor,
            nested_end,
        )? {
            normalized_start = token_start;
        }
        if normalized_start > trailing_inner_end {
            normalized_start = trailing_inner_end;
        }
        Ok(ByteSpan::new(to_u32(normalized_start)?, to_u32(trailing_inner_end)?))
    }

    fn next_dynamic_identity_token(
        &self,
        index: &mut usize,
        end: usize,
        can_start_expression: &mut bool,
        nested_cursor: &mut usize,
        nested_end: usize,
    ) -> Result<Option<(usize, usize)>, ProjectionError> {
        while *index < end {
            if *nested_cursor < nested_end {
                let tag = self
                    .dynamic_tags
                    .get(*nested_cursor)
                    .ok_or(ProjectionError::StructuralMismatch)?;
                let subtree_end = tag.subtree_end as usize;
                let tag_end = tag.closing.end as usize;
                if subtree_end <= *nested_cursor
                    || subtree_end > nested_end
                    || tag.opening.start as usize >= tag_end
                    || tag_end > end
                {
                    return Err(ProjectionError::StructuralMismatch);
                }
                if tag.opening.start as usize == *index {
                    let token_start = *index;
                    *index = tag_end;
                    *nested_cursor = subtree_end;
                    *can_start_expression = false;
                    return Ok(Some((token_start, tag_end)));
                }
                if (tag.opening.start as usize) < *index {
                    return Err(ProjectionError::StructuralMismatch);
                }
            }
            if self.bytes[*index].is_ascii_whitespace() {
                *index += 1;
                continue;
            }
            if self.bytes.get(*index..*index + 2) == Some(b"//") {
                *index = self.skip_line_comment(*index + 2).min(end);
                continue;
            }
            if self.bytes.get(*index..*index + 2) == Some(b"/*") {
                *index = self.skip_block_comment(*index)?.min(end);
                continue;
            }
            let token_start = *index;
            match self.bytes[*index] {
                b'\'' | b'"' => {
                    *index = self.skip_quote(*index, self.bytes[*index])?.min(end);
                    *can_start_expression = false;
                }
                b'`' => {
                    *index = self.skip_template_raw(*index, end)?;
                    *can_start_expression = false;
                }
                b'/' if *can_start_expression => {
                    *index = self.skip_regex(*index)?.min(end);
                    *can_start_expression = false;
                }
                b'(' => {
                    *index += 1;
                    *can_start_expression = true;
                }
                b')' => {
                    *index += 1;
                    *can_start_expression = false;
                }
                _ if self.identifier_start_width(*index).is_some() => {
                    *index = self.skip_identifier(*index);
                    *can_start_expression = matches!(
                        &self.bytes[token_start..*index],
                        b"return"
                            | b"throw"
                            | b"case"
                            | b"delete"
                            | b"void"
                            | b"typeof"
                            | b"new"
                            | b"yield"
                            | b"await"
                            | b"in"
                            | b"of"
                            | b"instanceof"
                    );
                }
                byte if byte.is_ascii_digit() => {
                    *index = self.skip_number(*index);
                    *can_start_expression = false;
                }
                b']' | b'}' | b'.' => {
                    *index += 1;
                    *can_start_expression = false;
                }
                _ => {
                    *index += 1;
                    *can_start_expression = true;
                }
            }
            while *nested_cursor < nested_end {
                let tag = self
                    .dynamic_tags
                    .get(*nested_cursor)
                    .ok_or(ProjectionError::StructuralMismatch)?;
                if tag.opening.start as usize >= *index {
                    break;
                }
                let subtree_end = tag.subtree_end as usize;
                let tag_end = tag.closing.end as usize;
                if (tag.opening.start as usize) < token_start
                    || tag_end > *index
                    || subtree_end <= *nested_cursor
                    || subtree_end > nested_end
                {
                    return Err(ProjectionError::StructuralMismatch);
                }
                *nested_cursor = subtree_end;
            }
            return Ok(Some((token_start, *index)));
        }
        Ok(None)
    }

    pub(super) fn collect_dynamic_edge_comments(
        &mut self,
        expression: ByteSpan,
        identity: ByteSpan,
    ) -> Result<(), ProjectionError> {
        if identity.start < expression.start || identity.end > expression.end {
            return Err(ProjectionError::StructuralMismatch);
        }
        self.collect_dynamic_comments_in(expression.start as usize, identity.start as usize)?;
        self.collect_dynamic_comments_in(identity.end as usize, expression.end as usize)
    }

    fn collect_dynamic_comments_in(
        &mut self,
        mut index: usize,
        end: usize,
    ) -> Result<(), ProjectionError> {
        while index < end {
            if self.bytes.get(index..index + 2) == Some(b"//") {
                let comment_end = self.skip_line_comment(index + 2).min(end);
                self.dynamic_comments.push(ByteSpan::new(to_u32(index)?, to_u32(comment_end)?));
                index = comment_end;
            } else if self.bytes.get(index..index + 2) == Some(b"/*") {
                let comment_end = self.skip_block_comment(index)?;
                if comment_end > end {
                    return Err(ProjectionError::StructuralMismatch);
                }
                self.dynamic_comments.push(ByteSpan::new(to_u32(index)?, to_u32(comment_end)?));
                index = comment_end;
            } else {
                index += 1;
            }
        }
        Ok(())
    }
}

pub(super) fn contains_collision_scalar(bytes: &[u8]) -> bool {
    bytes.windows(3).any(|window| window == [0xee, 0x80, 0x80] || window == [0xef, 0xbf, 0xbf])
}
