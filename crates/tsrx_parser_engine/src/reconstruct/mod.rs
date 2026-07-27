//! Rewriting a projected OXC tape in place into the tree the author actually wrote.
//! `program` fixes the order the passes run in and `spans` closes them out, mapping every
//! reachable node back into authored coordinates.

mod access;
mod code_blocks;
mod control;
mod css;
mod dynamic_tags;
mod edits;
mod if_chain;
mod jsx_statements;
mod layout_text;
mod loops;
mod objects;
mod program;
mod scaffold;
mod spans;
mod style;
mod switch;
mod try_catch;

pub(super) use program::reconstruct_projected;
pub(super) use spans::finalize_reachable_spans;
