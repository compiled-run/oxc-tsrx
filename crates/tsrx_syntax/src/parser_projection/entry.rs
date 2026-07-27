use super::actions::Action;
use super::actions::build_header_actions;
use super::actions::build_try_actions;
use super::actions::build_wrapper_actions;
use super::actions::project_actions;
use super::builder::Builder;
use super::mapping::MappedProjection;
use super::marker::collision_free_prefix;
use super::marker::validate_overlay_source;
use super::validate::validate_projection_lane;
use crate::{
    diagnostics::{ProjectionError, to_u32},
    model::{ControlContext, ControlKind, Overlay},
};

/// Builds the legal-TSX projection consumed by the canonical TSRX parser.
///
/// Unlike the lint projection, this parser-only lane retains each authored closing dynamic-tag
/// expression inside collision-free scaffold consumed after the same single OXC parse.
///
/// # Errors
///
/// Returns an error for a stale overlay or a projection scaffold collision.
pub fn project_for_parser(
    source: &str,
    overlay: &Overlay,
) -> Result<MappedProjection, ProjectionError> {
    validate_overlay_source(source, overlay)?;
    validate_projection_lane(overlay)?;
    let prefix = collision_free_prefix(source)?;
    let wrapper_actions = build_wrapper_actions(overlay)?;

    let try_end_actions = build_try_actions(overlay)?;
    let mut parser_code_block_end_actions = overlay
        .parser_code_blocks
        .iter()
        .enumerate()
        .map(|(index, _)| to_u32(index).map(Action::ParserCodeBlockEnd))
        .collect::<Result<Vec<_>, _>>()?;
    parser_code_block_end_actions.sort_unstable_by_key(|action| action.key(overlay));

    let header_actions = build_header_actions(overlay)?;

    let mut builder = Builder::new(source, overlay, &prefix);
    project_actions(
        &mut builder,
        overlay,
        &wrapper_actions,
        &try_end_actions,
        &parser_code_block_end_actions,
        &header_actions,
    )?;
    let mut mapped = builder.finish()?;
    mapped.synthetic_generator_spans = overlay
        .nodes
        .iter()
        .filter(|node| node.context != ControlContext::Statement || node.kind == ControlKind::Try)
        .map(|node| node.span)
        .collect();
    mapped.dynamic_prefix = Some(prefix);
    if !overlay.dynamic_tags.is_empty() {
        mapped.dynamic_count = to_u32(overlay.dynamic_tags.len())?;
        mapped.dynamic_offsets =
            overlay.dynamic_tags.iter().map(|tag| tag.expression.start).collect();
    }
    Ok(mapped)
}
