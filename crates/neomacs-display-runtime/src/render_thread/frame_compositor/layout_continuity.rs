//! Whether an install may derive layout motion from the presentation it replaces.
//!
//! Every other fact the compositor measures is derived by diffing two
//! presentations. This one cannot be: two presentations a divider drag
//! produced look exactly like two an ordinary `split-window` produced, and the
//! difference — that a hand is on the divider right now — is not in the
//! pixels. The producer records it, and this is where it is spent.

use neomacs_display_protocol::frame_time::EventTime;

use super::{ReflowImprintsByWindow, ScrollAnchorsByWindow};
use crate::core::frame_glyphs::FrameGlyphBuffer;
use crate::render_thread::frame_windows::GuiFrameRenderState;

/// The layout half of installing one presentation.
///
/// The four layout measures need the incoming presentation, the anchors and
/// imprints taken with it, and the moment of the install. All four inputs live
/// on `Derive` and nowhere else, so the suppressed arm cannot measure anything:
/// a fifth layout measure added later has nothing to reach for there and will
/// not compile into it. That is the difference between this and a boolean —
/// a boolean would let the mistake compile.
pub(in crate::render_thread) enum LayoutContinuity<'a> {
    /// Diff the incoming presentation against the one last drawn.
    Derive {
        next: Option<&'a FrameGlyphBuffer>,
        scroll_anchors: &'a ScrollAnchorsByWindow,
        reflow_imprints: &'a ReflowImprintsByWindow,
        installed_at: EventTime,
    },
    /// The pointer is holding a window edge, so this geometry is where the
    /// user's hand currently is — a position, not a destination to travel to.
    ///
    /// GNU's Lisp owns the number: `adjust-window-trailing-edge` applies
    /// minimum sizes, `window-size-fixed` and charwise rounding, so motion
    /// started here would travel toward a position the next commit has already
    /// clamped away, and the drag would spring and snap back.
    HeldByPointer,
}

impl<'a> LayoutContinuity<'a> {
    /// Classify one install from the presentation being installed.
    pub(in crate::render_thread) fn of(
        next: Option<&'a FrameGlyphBuffer>,
        scroll_anchors: &'a ScrollAnchorsByWindow,
        reflow_imprints: &'a ReflowImprintsByWindow,
        installed_at: EventTime,
    ) -> Self {
        if next.is_some_and(|frame| frame.origin.suppresses_layout_motion()) {
            return Self::HeldByPointer;
        }
        Self::Derive {
            next,
            scroll_anchors,
            reflow_imprints,
            installed_at,
        }
    }

    /// Run the layout half of the install, consuming the decision.
    ///
    /// By value, because the question is answered once per install from one
    /// fact. Re-asking it halfway through is what would let two measures in the
    /// same install disagree about whether a drag is running.
    pub(in crate::render_thread) fn apply(self, state: &mut GuiFrameRenderState) {
        match self {
            Self::Derive {
                next,
                scroll_anchors,
                reflow_imprints,
                installed_at,
            } => {
                state.measure_scroll(next, scroll_anchors);
                state.observe_shown_text(next);
                state.measure_reflow(next, reflow_imprints);
                state.measure_pane_layout(next, installed_at);
            }
            Self::HeldByPointer => state.discard_layout_motion(),
        }
    }
}

impl GuiFrameRenderState {
    /// Drop every layout motion the compositor is carrying or about to arm.
    ///
    /// Clearing matters as much as not measuring. An earlier install's
    /// observations are still pending when no frame drew them, and a pane
    /// morph survives across installs by design — so a drag that only *skipped*
    /// measuring would animate with the last ordinary commit's motion while the
    /// user's hand held the divider still.
    fn discard_layout_motion(&mut self) {
        self.compositor.pending.scrolls.clear();
        self.compositor.pending.shown_text_replaced.clear();
        self.compositor.pending.reflows.clear();
        self.compositor.pane_morph = None;
    }
}

#[cfg(test)]
#[path = "layout_continuity_test.rs"]
mod tests;
