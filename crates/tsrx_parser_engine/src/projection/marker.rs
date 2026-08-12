//! The grammar of a projected marker comment: how one is encoded, and which ones a `for` header
//! is required to have emitted.

#[derive(Debug, Clone, Copy)]
pub(super) enum MarkerKind {
    Token(u32),
    Style(u32),
    Script(u32),
    WrapperStart(u32),
    WrapperEnd(u32),
    Header { ordinal: u32, part: HeaderPart, boundary: MarkerBoundary },
}

#[derive(Debug, Clone, Copy)]
pub(super) enum HeaderPart {
    Right,
    Index,
    Key,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum MarkerBoundary {
    Start,
    End,
}

pub(super) fn parse_marker(comment: &str) -> Option<(&str, MarkerKind)> {
    let body = comment.strip_prefix("/*")?.strip_suffix("*/")?;
    let nonce_tail = body.strip_prefix("_t")?;
    let nonce_length = nonce_tail.bytes().take_while(u8::is_ascii_hexdigit).count();
    if nonce_length == 0 || nonce_tail.as_bytes().get(nonce_length) != Some(&b'_') {
        return None;
    }
    let prefix_length = 2 + nonce_length + 1;
    let prefix = body.get(..prefix_length)?;
    let marker = body.get(prefix_length..)?;
    if let Some(wrapper) = marker.strip_prefix('N') {
        if let Some(index) = wrapper.strip_suffix("S__").and_then(parse_decimal) {
            return Some((prefix, MarkerKind::WrapperStart(index)));
        }
        if let Some(index) = wrapper.strip_suffix("E__").and_then(parse_decimal) {
            return Some((prefix, MarkerKind::WrapperEnd(index)));
        }
        return None;
    }
    if let Some(index) =
        marker.strip_prefix('S').and_then(|tail| tail.strip_suffix("__")).and_then(parse_decimal)
    {
        return Some((prefix, MarkerKind::Style(index)));
    }
    if let Some(index) =
        marker.strip_prefix('Q').and_then(|tail| tail.strip_suffix("__")).and_then(parse_decimal)
    {
        return Some((prefix, MarkerKind::Script(index)));
    }
    if let Some((&part, tail)) = marker.as_bytes().split_first()
        && matches!(part, b'R' | b'I' | b'K')
    {
        let (part, tail) = (
            match part {
                b'R' => HeaderPart::Right,
                b'I' => HeaderPart::Index,
                b'K' => HeaderPart::Key,
                _ => unreachable!(),
            },
            std::str::from_utf8(tail).ok()?,
        );
        if let Some(index) = tail.strip_suffix("S__").and_then(parse_decimal) {
            return Some((
                prefix,
                MarkerKind::Header { ordinal: index, part, boundary: MarkerBoundary::Start },
            ));
        }
        if let Some(index) = tail.strip_suffix("E__").and_then(parse_decimal) {
            return Some((
                prefix,
                MarkerKind::Header { ordinal: index, part, boundary: MarkerBoundary::End },
            ));
        }
        return None;
    }
    parse_decimal(marker).map(|index| (prefix, MarkerKind::Token(index)))
}

pub(super) fn header_marker_bit(part: HeaderPart, boundary: MarkerBoundary) -> u8 {
    let offset = match part {
        HeaderPart::Right => 0,
        HeaderPart::Index => 2,
        HeaderPart::Key => 4,
    };
    1 << (offset
        + match boundary {
            MarkerBoundary::Start => 0,
            MarkerBoundary::End => 1,
        })
}

pub(super) fn expected_header_markers(header: tsrx_syntax::ForHeader) -> u8 {
    let mut expected = header_marker_bit(HeaderPart::Right, MarkerBoundary::Start)
        | header_marker_bit(HeaderPart::Right, MarkerBoundary::End);
    if !header.index.is_empty() {
        expected |= header_marker_bit(HeaderPart::Index, MarkerBoundary::Start)
            | header_marker_bit(HeaderPart::Index, MarkerBoundary::End);
    }
    if !header.key.is_empty() {
        expected |= header_marker_bit(HeaderPart::Key, MarkerBoundary::Start)
            | header_marker_bit(HeaderPart::Key, MarkerBoundary::End);
    }
    expected
}

pub(super) fn parse_decimal(value: &str) -> Option<u32> {
    if value.is_empty() {
        return None;
    }
    value.bytes().try_fold(0_u32, |number, byte| {
        byte.is_ascii_digit()
            .then_some(u32::from(byte - b'0'))
            .and_then(|digit| number.checked_mul(10)?.checked_add(digit))
    })
}
