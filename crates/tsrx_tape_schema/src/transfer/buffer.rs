use crate::TapeBuildError;

use super::PROGRAM_TRANSFER_MAX_BYTES;

pub(super) struct BoundedString {
    pub(super) value: String,
}

impl BoundedString {
    pub(super) fn with_capacity(capacity: usize) -> Result<Self, TapeBuildError> {
        let capacity = capacity.min(PROGRAM_TRANSFER_MAX_BYTES);
        let mut value = String::new();
        value.try_reserve(capacity).map_err(|_| TapeBuildError::CapacityOverflow)?;
        Ok(Self { value })
    }

    pub(super) fn ensure(&mut self, additional: usize) -> Result<(), TapeBuildError> {
        let length =
            self.value.len().checked_add(additional).ok_or(TapeBuildError::CapacityOverflow)?;
        if length > PROGRAM_TRANSFER_MAX_BYTES {
            return Err(TapeBuildError::CapacityOverflow);
        }
        if length > self.value.capacity() {
            self.value
                .try_reserve(length - self.value.len())
                .map_err(|_| TapeBuildError::CapacityOverflow)?;
        }
        Ok(())
    }

    pub(super) fn push(&mut self, value: char) -> Result<(), TapeBuildError> {
        self.ensure(value.len_utf8())?;
        self.value.push(value);
        Ok(())
    }

    pub(super) fn push_str(&mut self, value: &str) -> Result<(), TapeBuildError> {
        self.ensure(value.len())?;
        self.value.push_str(value);
        Ok(())
    }

    #[expect(
        clippy::inline_always,
        reason = "a single-push writer called once per byte on the transfer hot loop"
    )]
    #[inline(always)]
    pub(super) fn push_reserved(&mut self, value: char) {
        debug_assert!(self.value.len() + value.len_utf8() <= self.value.capacity());
        self.value.push(value);
    }

    #[expect(
        clippy::inline_always,
        reason = "a single-push writer called once per byte on the transfer hot loop"
    )]
    #[inline(always)]
    pub(super) fn push_str_reserved(&mut self, value: &str) {
        debug_assert!(self.value.len() + value.len() <= self.value.capacity());
        self.value.push_str(value);
    }

    pub(super) fn push_u32(&mut self, value: u32) -> Result<(), TapeBuildError> {
        self.ensure(10)?;
        self.push_u32_digits(value);
        Ok(())
    }

    #[expect(
        clippy::inline_always,
        reason = "a single-push writer called once per byte on the transfer hot loop"
    )]
    #[inline(always)]
    pub(super) fn push_u32_reserved(&mut self, value: u32) {
        debug_assert!(self.value.len() + 10 <= self.value.capacity());
        self.push_u32_digits(value);
    }

    #[expect(
        clippy::inline_always,
        reason = "a single-push writer called once per byte on the transfer hot loop"
    )]
    #[inline(always)]
    pub(super) fn push_u32_digits(&mut self, mut value: u32) {
        let mut digits = [0_u8; 10];
        let mut start = digits.len();
        loop {
            start -= 1;
            digits[start] = b'0' + (value % 10) as u8;
            value /= 10;
            if value == 0 {
                break;
            }
        }
        let encoded =
            std::str::from_utf8(&digits[start..]).expect("decimal digits are valid UTF-8");
        self.value.push_str(encoded);
    }

    pub(super) fn into_string(self) -> String {
        self.value
    }
}

pub(super) fn push_json_string(
    output: &mut BoundedString,
    value: &str,
) -> Result<(), TapeBuildError> {
    const HEX: &[u8; 16] = b"0123456789abcdef";

    if !value.bytes().any(|byte| byte == b'"' || byte == b'\\' || byte <= 0x1f) {
        output.push('"')?;
        output.push_str(value)?;
        return output.push('"');
    }

    output.push('"')?;
    let mut copied = 0_usize;
    for (index, byte) in value.bytes().enumerate() {
        let escape = match byte {
            b'"' => Some('"'),
            b'\\' => Some('\\'),
            b'\x08' => Some('b'),
            b'\x0c' => Some('f'),
            b'\n' => Some('n'),
            b'\r' => Some('r'),
            b'\t' => Some('t'),
            0x00..=0x1f => Some('\0'),
            _ => None,
        };
        let Some(escape) = escape else {
            continue;
        };
        output.push_str(value.get(copied..index).ok_or(TapeBuildError::InvalidRecordIndex)?)?;
        output.push('\\')?;
        if escape == '\0' {
            output.push_str("u00")?;
            output.push(char::from(HEX[usize::from(byte >> 4)]))?;
            output.push(char::from(HEX[usize::from(byte & 0x0f)]))?;
        } else {
            output.push(escape)?;
        }
        copied = index + 1;
    }
    output.push_str(value.get(copied..).ok_or(TapeBuildError::InvalidRecordIndex)?)?;
    output.push('"')
}
