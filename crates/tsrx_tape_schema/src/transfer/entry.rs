//! The envelope constants both formats are versioned by, and the tape methods that produce each
//! of them.

use crate::{FlatTape, SCHEMA_VERSION, TapeBuildError};

use super::binary::BinaryProgramSerializer;
use super::json::ProgramSerializer;
use super::json_owned::OwnedProgramSerializer;
use super::walk::transfer_layout;

/// Revision of the installed-package Program transfer envelope.
pub const PROGRAM_TRANSFER_VERSION: u16 = 1;

/// Hard limit for one Program transfer, including its special-value fix paths.
pub const PROGRAM_TRANSFER_MAX_BYTES: usize = 256 * 1024 * 1024;

/// Magic word for the private installed-package Program graph transfer.
pub const PROGRAM_BINARY_TRANSFER_MAGIC: u32 = 0x4252_5354;

/// Revision of the private installed-package Program graph transfer.
pub const PROGRAM_BINARY_TRANSFER_VERSION: u32 = 1;

/// One private installed-package Program graph transfer.
pub struct ProgramBinaryTransfer {
    pub metadata: String,
    pub words: Vec<u32>,
}

impl FlatTape {
    /// Serializes a concrete Program and its OXC special-value fix paths into one bounded payload.
    ///
    /// The walk is iterative and rejects missing, cyclic, shared, truncated, and over-limit tape
    /// records before the payload crosses Node-API.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError`] for invalid tapes or payloads above
    /// [`PROGRAM_TRANSFER_MAX_BYTES`].
    pub fn program_transfer(&self) -> Result<String, TapeBuildError> {
        if self.schema_version() != SCHEMA_VERSION {
            return Err(TapeBuildError::InvalidRecordIndex);
        }
        ProgramSerializer::new(self)?.run()
    }

    /// Consumes a concrete Program while serializing its transfer payload.
    ///
    /// The no-fix path uses the consumed record tables themselves as visit markers, preserving
    /// the same invalid-index, cycle, sharing, truncation, and capacity checks without allocating
    /// four parallel flag tables. Programs containing OXC special-value fixes retain the borrowed
    /// path because fix paths hold key references during traversal.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError`] for invalid tapes or payloads above
    /// [`PROGRAM_TRANSFER_MAX_BYTES`].
    pub fn program_transfer_owned(self) -> Result<String, TapeBuildError> {
        if self.schema_version() != SCHEMA_VERSION {
            return Err(TapeBuildError::InvalidRecordIndex);
        }
        let (capacity, track_paths, keys_are_json_safe) = transfer_layout(&self)?;
        if track_paths {
            self.program_transfer()
        } else {
            OwnedProgramSerializer::new(self, capacity, keys_are_json_safe)?.run()
        }
    }

    /// Consumes an engine-origin Program using metadata retained while its tape was built.
    ///
    /// This avoids the common no-fix field-summary prepass. Special-value Programs fall back to
    /// the complete summary/path serializer. The owned walk still consumes record slots as visit
    /// markers and rejects cyclic, shared, truncated, or invalid reachable graphs.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError`] for invalid tapes or payloads above
    /// [`PROGRAM_TRANSFER_MAX_BYTES`].
    #[doc(hidden)]
    pub fn program_transfer_engine_owned(self) -> Result<String, TapeBuildError> {
        if self.schema_version() != SCHEMA_VERSION {
            return Err(TapeBuildError::InvalidRecordIndex);
        }
        let (capacity, track_paths, keys_are_json_safe) = self.retained_transfer_layout()?;
        if track_paths {
            self.program_transfer()
        } else {
            OwnedProgramSerializer::new(self, capacity, keys_are_json_safe)?.run()
        }
    }

    /// Consumes an engine-origin Program into the private installed-package binary graph format.
    ///
    /// The walk is iterative and rejects missing, cyclic, shared, truncated, invalid, and
    /// over-limit tapes before either payload crosses Node-API.
    ///
    /// # Errors
    ///
    /// Returns [`TapeBuildError`] for an invalid tape or a transfer above
    /// [`PROGRAM_TRANSFER_MAX_BYTES`].
    #[doc(hidden)]
    pub fn program_transfer_engine_binary_owned(
        self,
    ) -> Result<ProgramBinaryTransfer, TapeBuildError> {
        BinaryProgramSerializer::new(self)?.run()
    }
}
