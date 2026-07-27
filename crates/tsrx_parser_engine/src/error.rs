//! The one operational error the public entry points return.
//! Rejected authored grammar is result data, not an error, so only unsupported coordinates,
//! exhausted capacity, and broken internal invariants arrive here.

use std::{error::Error, fmt};

use oxc_adapter::parser::ProjectedParseError;
use tsrx_syntax::ProjectionError;
use tsrx_tape_schema::TapeBuildError;

#[derive(Debug, PartialEq, Eq)]
pub enum TsrxParseError {
    AuthoredGrammar(String),
    Unsupported(&'static str),
    ResourceExhausted(&'static str),
    Projection(String),
    Adapter(String),
    Tape(TapeBuildError),
}

impl fmt::Display for TsrxParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AuthoredGrammar(message) => write!(formatter, "invalid TSRX grammar: {message}"),
            Self::Unsupported(shape) => write!(formatter, "unsupported TSRX parser shape: {shape}"),
            Self::ResourceExhausted(message) => formatter.write_str(message),
            Self::Projection(error) => write!(formatter, "TSRX projection failed: {error}"),
            Self::Adapter(error) => write!(formatter, "OXC adapter failed: {error}"),
            Self::Tape(error) => error.fmt(formatter),
        }
    }
}

impl Error for TsrxParseError {}

impl TsrxParseError {
    #[must_use]
    pub const fn is_resource_exhausted(&self) -> bool {
        matches!(self, Self::ResourceExhausted(_) | Self::Tape(TapeBuildError::CapacityOverflow))
    }
}

impl From<TapeBuildError> for TsrxParseError {
    fn from(error: TapeBuildError) -> Self {
        match error {
            TapeBuildError::CapacityOverflow => {
                Self::ResourceExhausted("TSRX tape exceeds its 32-bit limit")
            }
            TapeBuildError::InvalidRecordIndex => Self::Tape(error),
        }
    }
}

impl From<ProjectedParseError> for TsrxParseError {
    fn from(error: ProjectedParseError) -> Self {
        match error {
            ProjectedParseError::Tape(error) => error.into(),
            ProjectedParseError::Invariant(message) => {
                Self::Adapter(format!("projected OXC invariant failed: {message}"))
            }
        }
    }
}

impl From<ProjectionError> for TsrxParseError {
    fn from(error: ProjectionError) -> Self {
        match error {
            ProjectionError::SourceTooLarge => {
                Self::ResourceExhausted("TSRX source exceeds the 4 GiB span limit")
            }
            ProjectionError::MarkerSpaceExhausted => {
                Self::ResourceExhausted("TSRX marker namespace is exhausted")
            }
            error => Self::Projection(error.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operational_capacity_errors_keep_their_resource_classification() {
        let direct = TsrxParseError::from(TapeBuildError::CapacityOverflow);
        assert!(direct.is_resource_exhausted());
        assert_eq!(direct.to_string(), "TSRX tape exceeds its 32-bit limit");

        let projected =
            TsrxParseError::from(ProjectedParseError::Tape(TapeBuildError::CapacityOverflow));
        assert!(projected.is_resource_exhausted());

        let invalid = TsrxParseError::from(TapeBuildError::InvalidRecordIndex);
        assert!(!invalid.is_resource_exhausted());

        let source = TsrxParseError::from(ProjectionError::SourceTooLarge);
        assert!(source.is_resource_exhausted());
    }
}
