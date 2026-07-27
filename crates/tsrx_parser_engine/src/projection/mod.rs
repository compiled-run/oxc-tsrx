//! Everything the parser has to prove about a projection before it will trust a tape built from
//! it, plus the offset map between authored and projected coordinates.

mod clauses;
mod comments;
mod embedded;
mod gaps;
mod mapping;
mod marker;
mod marker_validation;
mod overlay;
mod text;

pub(super) use comments::reconstruct_comments;
pub(super) use gaps::validate_projection;
pub(super) use mapping::{
    map_affine_span, map_endpoint, project_authored_end, project_authored_start,
};
pub(super) use overlay::validate_overlay;
