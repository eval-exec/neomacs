//! Numeric current-matrix capture from a completed frame-build attempt.
//!
//! This sidecar is owned by the speculative frame until sealing succeeds.
//! It contains no Lisp state; independent mutators/readers share initialized
//! numeric rows through Arc. Query-only builds never publish this observation.

use neomacs_display_protocol::glyph_matrix::FrameDisplayState;
use neomacs_display_protocol::posn_object_extent::PosnMatrixSnapshot;
use neovm_core::window::WindowPresentationSnapshot;
use std::sync::Arc;

pub(super) fn capture_terminal_object_extents(
    state: &FrameDisplayState,
    publications: &mut [WindowPresentationSnapshot],
) {
    for publication in publications {
        let window = publication.window_id();
        let matrix = state
            .window_matrices
            .iter()
            .find(|entry| entry.window_id.get() as u64 == window.0)
            .map(|entry| {
                Arc::new(PosnMatrixSnapshot::from_terminal_window(
                    state,
                    entry,
                    neovm_core::encoding::char_width,
                ))
            });
        publication.display_snapshot_mut().posn_matrix = matrix;
    }
}

#[cfg(test)]
#[path = "tests/posn_capture_test.rs"]
mod tests;
