use super::actions::Action;
use super::actions::HeaderManifest;
use super::actions::TryManifest;
use super::actions::WrapperManifest;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ProjectionPurpose {
    Types,
    Parser,
}

#[expect(
    dead_code,
    reason = "the manifests the shared action builders emit are consumed by the formatter lift lane in `projection`, not by the parser lane"
)]
struct BuiltProjection {
    mapped: MappedProjection,
    prefix: String,
    wrappers: Vec<WrapperManifest>,
    headers: Vec<HeaderManifest>,
    tries: Vec<TryManifest>,
}

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
    Ok(build_projection_with_purpose(source, overlay, true, ProjectionPurpose::Parser)?.mapped)
}

fn build_projection_with_purpose(
    source: &str,
    overlay: &Overlay,
    record_segments: bool,
    purpose: ProjectionPurpose,
) -> Result<BuiltProjection, ProjectionError> {
    validate_overlay_source(source, overlay)?;
    validate_projection_lane(overlay, purpose)?;
    let prefix = collision_free_prefix(source)?;
    let (wrapper_actions, wrappers) = build_wrapper_actions(overlay)?;

    let (try_end_actions, tries) = build_try_actions(source, overlay)?;
    let mut parser_code_block_end_actions = overlay
        .parser_code_blocks
        .iter()
        .enumerate()
        .map(|(index, _)| to_u32(index).map(Action::ParserCodeBlockEnd))
        .collect::<Result<Vec<_>, _>>()?;
    parser_code_block_end_actions.sort_unstable_by_key(|action| action.key(overlay));

    let (header_actions, headers) =
        build_header_actions(overlay, purpose == ProjectionPurpose::Types)?;

    let mut builder = Builder::new(source, overlay, &prefix, record_segments, purpose);
    project_actions(
        &mut builder,
        overlay,
        purpose,
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
    if record_segments && (!overlay.dynamic_tags.is_empty() || purpose == ProjectionPurpose::Parser)
    {
        mapped.dynamic_prefix = Some(prefix.clone());
    }
    if record_segments && !overlay.dynamic_tags.is_empty() {
        mapped.dynamic_count = to_u32(overlay.dynamic_tags.len())?;
        mapped.dynamic_offsets =
            overlay.dynamic_tags.iter().map(|tag| tag.expression.start).collect();
    }
    Ok(BuiltProjection { mapped, prefix, wrappers, headers, tries })
}
