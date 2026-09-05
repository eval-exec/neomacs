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
pub(in crate::render_thread) mod theme;

use crate::render_thread::frame_windows::GuiFrameRenderState;
use neomacs_display_protocol::frame_glyphs::BufferViewportRegion;
use neomacs_display_protocol::types::{DisplayWindowId, Rect};

/// One window's viewport motion between two presentations.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::render_thread) struct ScrollObservation {
    pub(in crate::render_thread) window: DisplayWindowId,
    /// The window's own rect, mode line included. Effects that draw *over* a
    /// window rather than blitting its pixels use this.
    pub(in crate::render_thread) bounds: Rect,
    /// Whether the same buffer is shown on both sides.
    pub(in crate::render_thread) same_buffer: bool,
    pub(in crate::render_thread) displacement: scroll::ScrollDisplacement,
    /// Present only when the two presentations describe compatible pixels.
    ///
    /// A transition blits the retained image of the previous presentation into
    /// the new one's clip, so it needs both to own the same region. Effects that
    /// merely draw over the window — momentum glow, line spacing, velocity fade
    /// — sample nothing and must not inherit that restriction.
    pub(in crate::render_thread) transition: Option<TransitionInputs>,
}

/// What a transition needs beyond the observation itself.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::render_thread) struct TransitionInputs {
    /// The buffer-owned pixels the slide animates within.
    pub(in crate::render_thread) region: BufferViewportRegion,
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
            // The producer's guard: a zero buffer id means "no buffer", and
            // comparing positions across one is meaningless.
            if prev.buffer_id == 0 || curr.buffer_id == 0 {
                continue;
            }
            // A transition blits retained pixels from the previous presentation
            // into the new one's clip. If the two describe different
            // buffer-owned regions -- after a tab, header or mode line
            // appeared, or the window was split -- those pixels are not
            // compatible inputs. That disqualifies a *transition*, not the
            // observation: effects that draw over the window still apply.
            let transition = match (
                prev.geometry.buffer_viewport(),
                curr.geometry.buffer_viewport(),
            ) {
                (Some(previous_region), Some(region)) if previous_region == region => {
                    Some(TransitionInputs { region })
                }
                _ => None,
            };
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
                bounds: curr.bounds,
                same_buffer: prev.buffer_id == curr.buffer_id,
                displacement,
                transition,
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

impl GuiFrameRenderState {
    /// Record whether the frame's theme changed into the presentation being
    /// installed.
    ///
    /// Only a positive detection overwrites the pending value. Recomputing it
    /// unconditionally would clear a change detected on a presentation that was
    /// then superseded before any frame drew it, and the user would change
    /// theme and see nothing.
    pub(in crate::render_thread) fn observe_theme_change(
        &mut self,
        next: Option<&crate::core::frame_glyphs::FrameGlyphBuffer>,
    ) {
        if let (Some(previous), Some(next)) = (self.compositor.current_frame.as_ref(), next)
            && let Some(change) = theme::theme_change(previous, next)
        {
            self.compositor.pending.theme = Some(change);
        }
    }
}

/// A window is showing text it was not showing before.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::render_thread) struct ShownTextReplaced {
    pub(in crate::render_thread) window: DisplayWindowId,
    /// The window's own rect. The fade covers mode-line glyphs too, which is
    /// what it did as a producer hint.
    pub(in crate::render_thread) bounds: Rect,
}

impl GuiFrameRenderState {
    /// Record which windows are showing different text than before.
    ///
    /// This needs its own predicate rather than riding on `measure_scroll`,
    /// which skips a window whose `window_start` is unchanged. That skip is
    /// right for a scroll and wrong here: `switch-to-buffer` between two
    /// buffers displayed from the same character position is the common case,
    /// and it changes every glyph while moving the viewport not at all.
    pub(in crate::render_thread) fn observe_shown_text(
        &mut self,
        next: Option<&crate::core::frame_glyphs::FrameGlyphBuffer>,
    ) {
        self.compositor.pending.shown_text_replaced.clear();
        let (Some(previous), Some(next)) = (self.compositor.current_frame.as_ref(), next) else {
            return;
        };
        let previous_by_window: std::collections::HashMap<_, _> = previous
            .window_infos
            .iter()
            .map(|info| (info.window_id, info))
            .collect();

        for curr in &next.window_infos {
            if curr.is_minibuffer {
                continue;
            }
            let Some(prev) = previous_by_window.get(&curr.window_id) else {
                continue;
            };
            if prev.buffer_id == 0 || curr.buffer_id == 0 {
                continue;
            }
            let replaced =
                prev.buffer_id != curr.buffer_id || prev.window_start != curr.window_start;
            if !replaced {
                continue;
            }
            self.compositor
                .pending
                .shown_text_replaced
                .push(ShownTextReplaced {
                    window: curr.window_id,
                    bounds: curr.bounds,
                });
        }
    }
}
