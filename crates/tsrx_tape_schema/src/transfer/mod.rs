//! Getting one Program out of the tape and across a process or language boundary.

mod binary;
mod binary_records;
mod buffer;
mod common_keys;
mod entry;
mod json;
mod json_owned;
mod walk;

pub use entry::{
    PROGRAM_BINARY_TRANSFER_MAGIC, PROGRAM_BINARY_TRANSFER_VERSION, PROGRAM_TRANSFER_MAX_BYTES,
    PROGRAM_TRANSFER_VERSION, ProgramBinaryTransfer,
};
