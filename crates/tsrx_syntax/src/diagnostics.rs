use std::{error::Error, fmt};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectionError {
    SourceTooLarge,
    SourceChanged { offset: u32 },
    UnsupportedSyntax { offset: u32, construct: &'static str },
    UnterminatedSyntax { offset: u32, construct: &'static str },
    MalformedSyntax { offset: u32, expected: &'static str },
    MarkerSpaceExhausted,
    MarkerMissing { index: usize },
    MarkerDuplicated { index: usize },
    MarkerReordered { index: usize },
    MarkerTargetChanged { index: usize, expected: &'static str },
    MarkerResidual,
    ScaffoldMismatch { index: usize },
    StructuralMismatch,
}

impl ProjectionError {
    /// The authored-source UTF-8 byte offset this failure points at, when it has one.
    ///
    /// Four variants carry an offset; the other nine describe a whole-source or marker-level
    /// failure with no position. Every offset is an index into the `&str` handed to [`crate::scan`]
    /// and is taken at a token start, so it always lands on a character boundary. This accessor
    /// exists so a caller that needs the position never has to re-parse the [`fmt::Display`] text,
    /// which embeds the offset in three different places across the four templates.
    #[must_use]
    pub const fn byte_offset(&self) -> Option<u32> {
        match self {
            Self::SourceChanged { offset }
            | Self::UnsupportedSyntax { offset, .. }
            | Self::UnterminatedSyntax { offset, .. }
            | Self::MalformedSyntax { offset, .. } => Some(*offset),
            Self::SourceTooLarge
            | Self::MarkerSpaceExhausted
            | Self::MarkerMissing { .. }
            | Self::MarkerDuplicated { .. }
            | Self::MarkerReordered { .. }
            | Self::MarkerTargetChanged { .. }
            | Self::MarkerResidual
            | Self::ScaffoldMismatch { .. }
            | Self::StructuralMismatch => None,
        }
    }
}

impl fmt::Display for ProjectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourceTooLarge => formatter.write_str("TSRX source exceeds the 4 GiB span limit"),
            Self::SourceChanged { offset } => {
                write!(formatter, "TSRX source changed at structural byte {offset}")
            }
            Self::UnsupportedSyntax { offset, construct } => {
                write!(formatter, "unsupported TSRX {construct} at byte {offset}")
            }
            Self::UnterminatedSyntax { offset, construct } => {
                write!(formatter, "unterminated {construct} starting at byte {offset}")
            }
            Self::MalformedSyntax { offset, expected } => {
                write!(formatter, "malformed TSRX at byte {offset}: expected {expected}")
            }
            Self::MarkerSpaceExhausted => {
                formatter.write_str("unable to create a collision-free TSRX marker namespace")
            }
            Self::MarkerMissing { index } => write!(formatter, "Oxfmt removed TSRX marker {index}"),
            Self::MarkerDuplicated { index } => {
                write!(formatter, "Oxfmt duplicated TSRX marker {index}")
            }
            Self::MarkerReordered { index } => {
                write!(formatter, "Oxfmt reordered TSRX marker {index}")
            }
            Self::MarkerTargetChanged { index, expected } => write!(
                formatter,
                "Oxfmt moved TSRX marker {index} away from expected token `{expected}`"
            ),
            Self::MarkerResidual => formatter.write_str("a TSRX marker survived lifting"),
            Self::ScaffoldMismatch { index } => {
                write!(formatter, "Oxfmt changed TSRX scaffold {index}")
            }
            Self::StructuralMismatch => {
                formatter.write_str("formatted TSRX structure differs from the input")
            }
        }
    }
}

impl Error for ProjectionError {}

pub(crate) fn to_u32(value: usize) -> Result<u32, ProjectionError> {
    u32::try_from(value).map_err(|_| ProjectionError::SourceTooLarge)
}

#[cfg(test)]
mod tests {
    use super::ProjectionError;

    #[test]
    fn byte_offset_covers_exactly_the_positioned_variants() {
        let positioned = [
            ProjectionError::SourceChanged { offset: 7 },
            ProjectionError::UnsupportedSyntax { offset: 7, construct: "construct" },
            ProjectionError::UnterminatedSyntax { offset: 7, construct: "construct" },
            ProjectionError::MalformedSyntax { offset: 7, expected: "expected" },
        ];
        for error in &positioned {
            assert_eq!(error.byte_offset(), Some(7), "{error}");
            // The accessor must agree with the offset the message already prints. Two of these
            // four templates end with it and `MalformedSyntax` buries it mid-string, which is
            // exactly why re-parsing the text is the wrong way to read it.
            assert!(error.to_string().contains("byte 7"), "{error}");
        }

        let positionless = [
            ProjectionError::SourceTooLarge,
            ProjectionError::MarkerSpaceExhausted,
            ProjectionError::MarkerMissing { index: 3 },
            ProjectionError::MarkerDuplicated { index: 3 },
            ProjectionError::MarkerReordered { index: 3 },
            ProjectionError::MarkerTargetChanged { index: 3, expected: "{" },
            ProjectionError::MarkerResidual,
            ProjectionError::ScaffoldMismatch { index: 3 },
            ProjectionError::StructuralMismatch,
        ];
        for error in &positionless {
            assert_eq!(error.byte_offset(), None, "{error}");
            assert!(!error.to_string().contains("byte "), "{error}");
        }
    }

    #[test]
    fn a_real_scanner_failure_points_at_an_authored_character_boundary() {
        let source =
            "export function Broken() @{\n  let \u{3c0} = 1;\n  <main>\n    <h1>hi</h1>\n}\n";
        let error =
            crate::scan(source).expect_err("an unterminated JSX element must fail the scan");
        let offset = error.byte_offset().expect("an unterminated construct is positioned");
        // A UTF-8 byte index into the authored source, not a code unit and not an index into any
        // projection: the multi-byte identifier above shifts it and the assertion follows.
        assert_eq!(offset as usize, source.find("<main>").expect("fixture"));
        assert!(source.is_char_boundary(offset as usize));
        assert_eq!(source.as_bytes()[offset as usize], b'<');
    }
}
