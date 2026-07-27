//! The parser-only projection lane: authored TSRX rewritten into legal TSX that a single OXC
//! parse will accept.

mod actions;
mod builder;
mod entry;
mod mapping;
mod marker;
mod validate;

pub use entry::project_for_parser;
pub use mapping::MappedProjection;
