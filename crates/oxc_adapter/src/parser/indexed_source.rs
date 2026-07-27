use std::borrow::Cow;

use miette::{MietteError, MietteSpanContents, SourceCode, SourceSpan, SpanContents};

/// Borrowed source with a one-time line index for output-sensitive diagnostic rendering.
#[derive(Debug)]
pub(super) struct IndexedSource<'a> {
    name: &'a str,
    source: &'a str,
    line_breaks: Vec<usize>,
    line_starts: Vec<usize>,
}

impl<'a> IndexedSource<'a> {
    pub(super) fn new(name: &'a str, source: &'a str) -> Self {
        let bytes = source.as_bytes();
        let mut line_breaks = Vec::new();
        let mut line_starts = vec![0];
        let mut offset = 0;
        while offset < bytes.len() {
            match bytes[offset] {
                b'\r' => {
                    line_breaks.push(offset);
                    offset += 1;
                    if bytes.get(offset) == Some(&b'\n') {
                        offset += 1;
                    }
                    line_starts.push(offset);
                }
                b'\n' => {
                    line_breaks.push(offset);
                    offset += 1;
                    line_starts.push(offset);
                }
                _ => offset += 1,
            }
        }
        Self { name, source, line_breaks, line_starts }
    }

    fn window_base(
        &self,
        span_offset: usize,
        context_lines_before: usize,
    ) -> Result<(usize, usize), MietteError> {
        let line = self.line_breaks.partition_point(|line_break| *line_break < span_offset);
        // Miette attributes a span on the LF byte of CRLF to the line ending at that CRLF.
        // Select that effective line directly instead of delegating from source offset zero.
        let on_crlf_lf = span_offset > 0
            && self.source.as_bytes().get(span_offset - 1) == Some(&b'\r')
            && self.source.as_bytes().get(span_offset) == Some(&b'\n');
        let effective_line = if on_crlf_lf { line.saturating_sub(1) } else { line };
        let base_line = effective_line.saturating_sub(context_lines_before);
        let base_offset = *self.line_starts.get(base_line).ok_or(MietteError::OutOfBounds)?;
        Ok((base_line, base_offset))
    }
}

impl SourceCode for IndexedSource<'_> {
    fn read_span<'a>(
        &'a self,
        span: &SourceSpan,
        context_lines_before: usize,
        context_lines_after: usize,
    ) -> Result<MietteSpanContents<'a>, MietteError> {
        let span_offset = usize::try_from(span.offset()).map_err(|_| MietteError::OutOfBounds)?;
        let (base_line, base_offset) = self.window_base(span_offset, context_lines_before)?;
        let base_u32 = u32::try_from(base_offset).map_err(|_| MietteError::OutOfBounds)?;
        let adjusted_offset =
            span.offset().checked_sub(base_u32).ok_or(MietteError::OutOfBounds)?;
        let adjusted_span: SourceSpan = (adjusted_offset, span.len()).into();
        let inner = <str as SourceCode>::read_span(
            self.source.get(base_offset..).ok_or(MietteError::OutOfBounds)?,
            &adjusted_span,
            context_lines_before,
            context_lines_after,
        )?;
        let global_offset =
            inner.span().offset().checked_add(base_u32).ok_or(MietteError::OutOfBounds)?;
        Ok(MietteSpanContents::new_named(
            Cow::Borrowed(self.name),
            inner.data(),
            (global_offset, inner.span().len()).into(),
            base_line + inner.line(),
            inner.column(),
            base_line + inner.line_count(),
        ))
    }

    fn name(&self) -> Option<&str> {
        Some(self.name)
    }
}

#[cfg(test)]
mod tests {
    use miette::{NamedSource, SourceCode, SourceSpan, SpanContents};

    use super::IndexedSource;

    #[test]
    fn indexed_source_matches_miette_for_lf_crlf_eof_and_context_windows() {
        for source in [
            "first\nsecond\nthird\nfourth",
            "first\r\nsecond\rthird\n",
            "first\r\nsecond\r\n",
            "one very long line without a newline",
        ] {
            let indexed = IndexedSource::new("Exact.tsrx", source);
            let reference = NamedSource::new("Exact.tsrx", source.to_owned());
            for (offset, length) in
                [(0, 1), (2, 5), (6, 3), (source.len() - 1, 1), (source.len(), 0)]
            {
                let span: SourceSpan = (
                    u32::try_from(offset).expect("offset"),
                    u32::try_from(length).expect("length"),
                )
                    .into();
                for context in 0..=2 {
                    let actual = indexed.read_span(&span, context, context).expect("indexed span");
                    let expected =
                        reference.read_span(&span, context, context).expect("reference span");
                    assert_eq!(
                        actual.data(),
                        expected.data(),
                        "source={source:?} offset={offset} length={length} context={context}"
                    );
                    assert_eq!(actual.span(), expected.span());
                    assert_eq!(actual.line(), expected.line());
                    assert_eq!(actual.column(), expected.column());
                    assert_eq!(actual.line_count(), expected.line_count());
                    assert_eq!(actual.name(), expected.name());
                }
            }
        }

        let indexed = IndexedSource::new("Empty.tsrx", "");
        let reference = NamedSource::new("Empty.tsrx", String::new());
        let eof: SourceSpan = (0, 0).into();
        for context in 0..=2 {
            let actual =
                indexed.read_span(&eof, context, context).expect("indexed empty-source EOF");
            let expected =
                reference.read_span(&eof, context, context).expect("reference empty-source EOF");
            assert_eq!(actual.data(), expected.data());
            assert_eq!(actual.span(), expected.span());
            assert_eq!(actual.line(), expected.line());
            assert_eq!(actual.column(), expected.column());
            assert_eq!(actual.line_count(), expected.line_count());
            assert_eq!(actual.name(), expected.name());
        }
    }

    #[test]
    fn late_crlf_lf_spans_use_a_bounded_indexed_window() {
        let source = "prefix\r\n".repeat(512);
        let late_lf = source.rfind('\n').expect("late LF");
        let indexed = IndexedSource::new("Late.tsrx", &source);
        let (base_line, base_offset) = indexed.window_base(late_lf, 0).expect("indexed base");
        assert_eq!(base_line, 511);
        assert_eq!(base_offset, "prefix\r\n".len() * 511);
        assert!(base_offset > source.len() / 2);

        let span: SourceSpan = (u32::try_from(late_lf).expect("late LF offset"), 1_u32).into();
        let reference = NamedSource::new("Late.tsrx", source.clone());
        for context in 0..=2 {
            let actual =
                indexed.read_span(&span, context, context).expect("indexed late CRLF span");
            let expected =
                reference.read_span(&span, context, context).expect("reference late CRLF span");
            assert_eq!(actual.data(), expected.data());
            assert_eq!(actual.span(), expected.span());
            assert_eq!(actual.line(), expected.line());
            assert_eq!(actual.column(), expected.column());
            assert_eq!(actual.line_count(), expected.line_count());
            assert_eq!(actual.name(), expected.name());
        }
    }
}
