use tsrx_syntax::ProjectionSegment;

pub(crate) fn map_endpoint(
    segments: &[ProjectionSegment],
    point: u32,
    is_start: bool,
) -> Option<u32> {
    let index = segments.partition_point(|segment| {
        if is_start { segment.projected.start <= point } else { segment.projected.start < point }
    });
    let segment = segments.get(index.checked_sub(1)?)?;
    let contains = if is_start {
        segment.projected.start <= point && point < segment.projected.end
    } else {
        segment.projected.start < point && point <= segment.projected.end
    };
    contains.then(|| segment.original_start + (point - segment.projected.start))
}

/// Maps a complete span only when every byte belongs to one unchanged authored segment.
///
/// Empty spans map inside a segment, at the unambiguous outer boundaries, or where two abutting
/// segments agree exactly. A generated gap or discontinuous boundary is never assigned
/// approximately to either side.
pub(crate) fn map_affine_span(
    segments: &[ProjectionSegment],
    span: tsrx_tape_schema::TapeSpan,
) -> Option<tsrx_tape_schema::TapeSpan> {
    if span.start > span.end {
        return None;
    }
    if span.start == span.end {
        let right = map_endpoint(segments, span.start, true);
        let left = map_endpoint(segments, span.end, false);
        let boundary = match (left, right) {
            (Some(left), Some(right)) if left == right => Some(left),
            (None, Some(right))
                if segments
                    .first()
                    .is_some_and(|segment| segment.projected.start == span.start) =>
            {
                Some(right)
            }
            (Some(left), None)
                if segments.last().is_some_and(|segment| segment.projected.end == span.end) =>
            {
                Some(left)
            }
            _ => None,
        }?;
        return Some(tsrx_tape_schema::TapeSpan::new(boundary, boundary));
    }
    let index = segments.partition_point(|segment| segment.projected.start <= span.start);
    let segment = segments.get(index.checked_sub(1)?)?;
    let inside = segment.projected.start <= span.start && span.end <= segment.projected.end;
    inside.then(|| {
        let start = segment.original_start + (span.start - segment.projected.start);
        tsrx_tape_schema::TapeSpan::new(start, start + (span.end - span.start))
    })
}

pub(crate) fn project_authored_start(segments: &[ProjectionSegment], point: u32) -> Option<u32> {
    let index = segments.partition_point(|segment| segment.original_start <= point);
    let segment = segments.get(index.checked_sub(1)?)?;
    let original_end = segment.original_start + (segment.projected.end - segment.projected.start);
    (point < original_end).then(|| segment.projected.start + (point - segment.original_start))
}

pub(crate) fn project_authored_end(segments: &[ProjectionSegment], point: u32) -> Option<u32> {
    let index = segments.partition_point(|segment| segment.original_start < point);
    let segment = segments.get(index.checked_sub(1)?)?;
    let original_end = segment.original_start + (segment.projected.end - segment.projected.start);
    (point <= original_end).then(|| segment.projected.start + (point - segment.original_start))
}

#[cfg(test)]
mod affine_mapping_tests {
    use tsrx_syntax::{ByteSpan, ProjectionSegment};
    use tsrx_tape_schema::TapeSpan;

    use super::{map_affine_span, map_endpoint};

    fn segment(projected_start: u32, projected_end: u32, original_start: u32) -> ProjectionSegment {
        ProjectionSegment {
            projected: ByteSpan::new(projected_start, projected_end),
            original_start,
            fixable: true,
        }
    }

    #[test]
    fn empty_spans_map_only_at_exact_outer_or_continuous_boundaries() {
        let gapped = [segment(0, 5, 100), segment(10, 15, 200)];
        assert_eq!(map_endpoint(&gapped, 0, true), Some(100));
        assert_eq!(map_endpoint(&gapped, 0, false), None);
        assert_eq!(map_endpoint(&gapped, 5, false), Some(105));
        assert_eq!(map_endpoint(&gapped, 5, true), None);
        assert_eq!(map_endpoint(&gapped, 10, false), None);
        assert_eq!(map_endpoint(&gapped, 10, true), Some(200));
        assert_eq!(map_endpoint(&gapped, 15, false), Some(205));
        assert_eq!(map_endpoint(&gapped, 15, true), None);
        assert_eq!(map_affine_span(&gapped, TapeSpan::new(0, 0)), Some(TapeSpan::new(100, 100)));
        assert_eq!(map_affine_span(&gapped, TapeSpan::new(15, 15)), Some(TapeSpan::new(205, 205)));
        assert_eq!(map_affine_span(&gapped, TapeSpan::new(5, 5)), None);
        assert_eq!(map_affine_span(&gapped, TapeSpan::new(10, 10)), None);
        assert_eq!(map_affine_span(&gapped, TapeSpan::new(4, 11)), None);
        assert_eq!(map_affine_span(&gapped, TapeSpan::new(11, 4)), None);

        let discontinuous = [segment(0, 5, 100), segment(5, 10, 200)];
        assert_eq!(map_endpoint(&discontinuous, 5, false), Some(105));
        assert_eq!(map_endpoint(&discontinuous, 5, true), Some(200));
        assert_eq!(map_affine_span(&discontinuous, TapeSpan::new(5, 5)), None);
        assert_eq!(map_affine_span(&discontinuous, TapeSpan::new(4, 6)), None);

        let continuous = [segment(0, 5, 100), segment(5, 10, 105)];
        assert_eq!(map_endpoint(&continuous, 5, false), Some(105));
        assert_eq!(map_endpoint(&continuous, 5, true), Some(105));
        assert_eq!(
            map_affine_span(&continuous, TapeSpan::new(5, 5)),
            Some(TapeSpan::new(105, 105))
        );
        assert_eq!(map_affine_span(&continuous, TapeSpan::new(4, 6)), None);
    }
}
