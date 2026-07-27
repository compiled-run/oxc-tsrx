//! The two preconditions for projecting at all: the overlay still matches this source, and a
//! scaffold prefix exists that cannot collide with anything in it.

use crate::{diagnostics::ProjectionError, model::Overlay, scanner::source_fingerprint};

pub(super) fn validate_overlay_source(
    source: &str,
    overlay: &Overlay,
) -> Result<(), ProjectionError> {
    if source.len() != overlay.source_len as usize
        || source_fingerprint(source.as_bytes()) != overlay.source_fingerprint
    {
        return Err(ProjectionError::SourceChanged { offset: 0 });
    }
    Ok(())
}

pub(super) fn collision_free_prefix(source: &str) -> Result<String, ProjectionError> {
    for nonce in 0..=1024_u16 {
        let prefix = format!("_t{nonce:x}_");
        if !source.contains(&prefix) {
            return Ok(prefix);
        }
    }
    Err(ProjectionError::MarkerSpaceExhausted)
}
