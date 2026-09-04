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
pub(in crate::render_thread) mod selection;

use crate::render_thread::frame_windows::GuiFrameRenderState;
use neomacs_display_protocol::frame_glyphs::BufferViewportRegion;
use neomacs_display_protocol::types::DisplayWindowId;

/// One window's viewport motion between two presentations.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::render_thread) struct ScrollObservation {
    pub(in crate::render_thread) window: DisplayWindowId,
    /// The buffer-owned pixels a transition may animate within.
    pub(in crate::render_thread) region: BufferViewportRegion,
    pub(in crate::render_thread) displacement: scroll::ScrollDisplacement,
}

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
        self.compositor.pending.scrolls.clear();
        let (Some(previous), Some(next)) = (self.compositor.current_frame.as_ref(), next) else {
            return;
        };
        let previous_by_window: std::collections::HashMap<_, _> = previous
            .window_infos
            .iter()
            .map(|info| (info.window_id, info))
            .collect();

        for curr in &next.window_infos {
            // The minibuffer does not scroll its own viewport the way a text
            // window does, and animating it fights the echo area.
            if curr.is_minibuffer {
                continue;
            }
            let Some(prev) = previous_by_window.get(&curr.window_id) else {
                continue;
            };
            if prev.window_start == curr.window_start {
                continue;
            }
            // A transition samples retained pixels from the previous
            // presentation and draws them inside the new one's clip. If the two
            // describe different buffer-owned regions -- after a tab, header or
            // mode line appeared, or the window was split -- those pixels are
            // not compatible inputs, so there is nothing safe to animate.
            let (Some(previous_region), Some(region)) = (
                prev.geometry.buffer_viewport(),
                curr.geometry.buffer_viewport(),
            ) else {
                continue;
            };
            if previous_region != region {
                continue;
            }
            let displacement = scroll::displacement(
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
            self.compositor.pending.scrolls.push(ScrollObservation {
                window: curr.window_id,
                region,
                displacement,
            });
        }
    }
}

impl GuiFrameRenderState {
    /// Take the observations measured at the last install, exactly once.
    ///
    /// A frame consumes these; a second render pass over the same retained
    /// presentation must see nothing, or every derived effect re-arms and the
    /// resulting redraw request sustains itself.
    pub(in crate::render_thread) fn take_pending_continuity(
        &mut self,
        accept_derived_effects: bool,
    ) -> super::PendingContinuity {
        let mut pending = std::mem::take(&mut self.compositor.pending);
        pending.accept_derived_effects = accept_derived_effects;
        pending
    }
}

impl GuiFrameRenderState {
    /// Record whether the frame's selection moved into the presentation being
    /// installed. Runs beside `measure_scroll`, for the same reason: the
    /// outgoing presentation is about to be replaced.
    pub(in crate::render_thread) fn observe_selection_change(
        &mut self,
        next: Option<&crate::core::frame_glyphs::FrameGlyphBuffer>,
    ) {
        self.compositor.pending.selection = match (self.compositor.current_frame.as_ref(), next) {
            (Some(previous), Some(next)) => {
                selection::observe_selection(&previous.window_infos, &next.window_infos)
            }
            _ => None,
        };
    }
}
