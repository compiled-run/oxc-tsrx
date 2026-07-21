use std::{error::Error, fmt};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectionError {
    SourceTooLarge,
    SourceChanged {
        offset: u32,
    },
    UnsupportedSyntax {
        offset: u32,
        construct: &'static str,
    },
    UnterminatedSyntax {
        offset: u32,
        construct: &'static str,
    },
    MalformedSyntax {
        offset: u32,
        expected: &'static str,
    },
    MarkerSpaceExhausted,
    MarkerMissing {
        index: usize,
    },
    MarkerDuplicated {
        index: usize,
    },
    MarkerReordered {
        index: usize,
    },
    MarkerTargetChanged {
        index: usize,
        expected: &'static str,
    },
    MarkerResidual,
    ScaffoldMismatch {
        index: usize,
    },
    StructuralMismatch,
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
                write!(
                    formatter,
                    "unterminated {construct} starting at byte {offset}"
                )
            }
            Self::MalformedSyntax { offset, expected } => {
                write!(
                    formatter,
                    "malformed TSRX at byte {offset}: expected {expected}"
                )
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
