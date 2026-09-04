//! Continuity between immutable editor presentations.
//!
//! This module answers questions about what changed between two sealed
//! presentations — which panes persisted, entered or exited, and how far a
//! viewport moved — so the compositor can decide how to bridge them visually.
//!
//! Everything here derives facts by comparing presentations. It never asks the
//! producer what a change *meant*: geometry is diffed, not declared. Producers
//! supply provenance only where pixels genuinely cannot say (that a buffer
//! replacement was a navigation, for instance).

pub(in crate::render_thread) mod scroll;

use crate::render_thread::frame_windows::GuiFrameRenderState;

impl GuiFrameRenderState {
    /// Measure how far each window's viewport moved into the presentation being
    /// installed, comparing it with the one being replaced.
    ///
    /// Runs at install because that is the only moment both presentations'
    /// anchors exist: the outgoing set is about to be overwritten, and the
    /// incoming set was taken when its presentation was ingested.
    ///
    /// Windows absent from either side contribute nothing — a window that just
    /// appeared did not scroll, and one that vanished has no viewport left to
    /// measure.
    pub(in crate::render_thread) fn measure_scroll(
        &mut self,
        next: Option<&crate::core::frame_glyphs::FrameGlyphBuffer>,
        next_anchors: &super::ScrollAnchorsByWindow,
    ) {
        self.compositor.pending_scroll.clear();
        let (Some(previous), Some(next)) = (self.compositor.current_frame.as_ref(), next) else {
            return;
        };
        let previous_by_window: std::collections::HashMap<_, _> = previous
            .window_infos
            .iter()
            .map(|info| (info.window_id, info))
            .collect();

        for curr in &next.window_infos {
            let Some(prev) = previous_by_window.get(&curr.window_id) else {
                continue;
            };
            if prev.window_start == curr.window_start {
                continue;
            }
            let measured = scroll::displacement(
                prev,
                curr,
                self.compositor
                    .scroll_anchors
                    .get(&curr.window_id)
                    .map_or(&[][..], Vec::as_slice),
                next_anchors
                    .get(&curr.window_id)
                    .map_or(&[][..], Vec::as_slice),
            );
            self.compositor
                .pending_scroll
                .push((curr.window_id, measured));
        }
    }
}
