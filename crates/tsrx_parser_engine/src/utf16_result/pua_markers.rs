use crate::TsrxParseError;

pub(super) fn apply_pua_markers(
    value: &mut [u16],
    markers: &[Option<u16>],
) -> Result<(), TsrxParseError> {
    let positions = value
        .iter()
        .enumerate()
        .filter_map(|(index, unit)| (*unit == 0xe000).then_some(index))
        .collect::<Vec<_>>();
    if positions.len() != markers.len() {
        return Err(TsrxParseError::Adapter(format!(
            "OXC placeholder count {} does not match source producer count {}",
            positions.len(),
            markers.len()
        )));
    }
    for (position, marker) in positions.into_iter().zip(markers) {
        if let Some(unit) = marker {
            value[position] = *unit;
        }
    }
    Ok(())
}

pub(super) fn javascript_quoted_pua_markers(
    value: &[u16],
) -> Result<Vec<Option<u16>>, TsrxParseError> {
    let Some((&quote, inner)) = value.split_first() else {
        return Err(TsrxParseError::Adapter("empty quoted source span".to_string()));
    };
    if !matches!(quote, unit if unit == u16::from(b'\'') || unit == u16::from(b'"'))
        || inner.last().copied() != Some(quote)
    {
        return Err(TsrxParseError::Adapter(
            "quoted source span has unmatched delimiters".to_string(),
        ));
    }
    javascript_pua_markers(&inner[..inner.len() - 1])
}

pub(super) fn javascript_pua_markers(value: &[u16]) -> Result<Vec<Option<u16>>, TsrxParseError> {
    let mut markers = Vec::new();
    let mut index = 0_usize;
    while index < value.len() {
        if value[index] != u16::from(b'\\') {
            index += push_literal_marker(value, index, &mut markers);
            continue;
        }
        let escaped = *value.get(index + 1).ok_or_else(|| {
            TsrxParseError::Adapter("parsed source ends in a backslash".to_string())
        })?;
        if escaped == 0xe000 || (0xd800..=0xdfff).contains(&escaped) {
            index += 1 + push_literal_marker(value, index + 1, &mut markers);
            continue;
        }
        if escaped == u16::from(b'u') {
            if value.get(index + 2).copied() == Some(u16::from(b'{')) {
                let close = value[index + 3..]
                    .iter()
                    .position(|unit| *unit == u16::from(b'}'))
                    .map(|relative| index + 3 + relative)
                    .ok_or_else(|| {
                        TsrxParseError::Adapter("parsed braced Unicode escape is open".to_string())
                    })?;
                if parse_ascii_radix(&value[index + 3..close], 16) == Some(0xe000) {
                    markers.push(None);
                }
                index = close + 1;
                continue;
            }
            let end = index.checked_add(6).ok_or_else(|| {
                TsrxParseError::Adapter("Unicode escape index overflow".to_string())
            })?;
            let digits = value.get(index + 2..end).ok_or_else(|| {
                TsrxParseError::Adapter("parsed Unicode escape is truncated".to_string())
            })?;
            if parse_ascii_radix(digits, 16) == Some(0xe000) {
                markers.push(None);
            }
            index = end;
            continue;
        }
        if escaped == u16::from(b'x') {
            index = index
                .checked_add(4)
                .ok_or_else(|| TsrxParseError::Adapter("hex escape index overflow".to_string()))?;
            continue;
        }
        if escaped == u16::from(b'\r') && value.get(index + 2).copied() == Some(u16::from(b'\n')) {
            index += 3;
        } else {
            index += 2;
        }
    }
    Ok(markers)
}

pub(super) fn jsx_quoted_pua_markers(value: &[u16]) -> Result<Vec<Option<u16>>, TsrxParseError> {
    let Some((&quote, inner)) = value.split_first() else {
        return Err(TsrxParseError::Adapter("empty JSX quoted span".to_string()));
    };
    if inner.last().copied() != Some(quote) {
        return Err(TsrxParseError::Adapter(
            "JSX quoted span has unmatched delimiters".to_string(),
        ));
    }
    Ok(jsx_pua_markers(&inner[..inner.len() - 1]))
}

pub(super) fn jsx_pua_markers(value: &[u16]) -> Vec<Option<u16>> {
    // OXC remains authoritative for JSX entity normalization. Actual source scalars and lone
    // units are the only position-keyed placeholder producers patched here.
    literal_pua_markers(value)
}

pub(super) fn literal_pua_markers(value: &[u16]) -> Vec<Option<u16>> {
    let mut markers = Vec::new();
    let mut index = 0_usize;
    while index < value.len() {
        index += push_literal_marker(value, index, &mut markers);
    }
    markers
}

fn push_literal_marker(value: &[u16], index: usize, output: &mut Vec<Option<u16>>) -> usize {
    let unit = value[index];
    if (0xd800..=0xdbff).contains(&unit)
        && value.get(index + 1).is_some_and(|next| (0xdc00..=0xdfff).contains(next))
    {
        return 2;
    }
    if unit == 0xe000 {
        output.push(None);
    } else if (0xd800..=0xdfff).contains(&unit) {
        output.push(Some(unit));
    }
    1
}

fn parse_ascii_radix(value: &[u16], radix: u32) -> Option<u32> {
    if value.is_empty() {
        return None;
    }
    value.iter().try_fold(0_u32, |output, unit| {
        let character = char::from_u32(u32::from(*unit))?;
        let digit = character.to_digit(radix)?;
        output.checked_mul(radix)?.checked_add(digit)
    })
}
