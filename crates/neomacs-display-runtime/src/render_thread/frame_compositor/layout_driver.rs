//! What the compositor is doing about the layout, as a state machine.
//!
//! This exists because the previous shape — an `Option<PaneLayoutMorph>` mutated
//! from ten places — produced four bugs of one kind. Each was a case nobody
//! wrote: a commit that changed no layout *cancelled* the motion; then, once
//! that was fixed, a commit that changed no layout *restarted* it, pinning
//! progress near zero so the panes crawled and never arrived; a leaving pane
//! repainted over the animation; and the compositor-only path discarded a
//! settled projection. None of them was a wrong decision — each was a decision
//! nobody was asked to make, because an `Option` plus scattered `if`s never
//! poses the question.
//!
//! Here the question is a `match`. Transitions consume `self` and return the
//! next state, so every (state, event) pair is a cell the compiler requires,
//! and a new state cannot be added without visiting each event that must handle
//! it.
//!
//! Two events drive it: a commit arriving from the evaluator, and a frame about
//! to be drawn. Nothing else may touch the motion.

use super::continuity::pane_layout::{PaneLayoutComposition, PaneLayoutMorph};
use neomacs_display_protocol::frame_glyphs::WindowInfo;
use neomacs_display_protocol::frame_time::{EventTime, FrameSample};
use neomacs_display_protocol::motion_spec::MotionSpec;
use neomacs_display_protocol::{InteractionProjection, PresentationId};

/// What one commit wants, against what the last composed frame showed.
///
/// Computed once per install and passed to the driver, rather than re-derived
/// at each decision point. The two disagreeing versions of "did the layout
/// change?" are what let the cancel and restart bugs coexist.
pub(in crate::render_thread) struct LayoutDelta<'a> {
    pub(in crate::render_thread) previous: &'a [WindowInfo],
    pub(in crate::render_thread) next: &'a [WindowInfo],
}

/// What the compositor is doing about the layout.
#[derive(Default)]
pub(in crate::render_thread) enum LayoutDriver {
    /// The panes are where the presentation says they are.
    #[default]
    Settled,
    /// The panes are travelling between two layouts.
    Animating(PaneLayoutMorph),
}

impl LayoutDriver {
    /// Whether this state needs frames drawn to make progress.
    ///
    /// The frame coordinator schedules on standing demands, and a morph's state
    /// lives here rather than in a renderer effect list — so it was invisible to
    /// every demand and advanced only on frames some *other* activity happened
    /// to schedule. Asking the driver is what stops a future animating state
    /// from repeating that.
    pub(in crate::render_thread) const fn wants_frames(&self) -> bool {
        matches!(self, Self::Animating(_))
    }

    /// A commit arrived.
    pub(in crate::render_thread) fn on_commit(
        self,
        delta: LayoutDelta<'_>,
        spec: MotionSpec,
        at: EventTime,
    ) -> Self {
        match self {
            // Nothing in flight: a rearrangement starts one, anything else is
            // still nothing. `try_new` answers both by returning `None` when no
            // pane moved.
            Self::Settled => PaneLayoutMorph::try_new(delta.previous, delta.next, spec, at)
                .map_or(Self::Settled, Self::Animating),
            Self::Animating(mut morph) => {
                // Retarget only when this commit wants the panes somewhere other
                // than where they are already heading. A retarget restarts the
                // motion from this instant, and commits arrive continuously
                // while one runs — a keystroke, a blink, a mode-line tick — so
                // retargeting on all of them is what made the panes crawl.
                if morph.destination_differs_from(delta.next) {
                    morph.retarget(delta.next, spec, at);
                }
                Self::Animating(morph)
            }
        }
    }

    /// A frame is about to be drawn.
    ///
    /// Returns the placements to draw and the transform matching them, both
    /// from one sample of one motion — separating them is how a render and a
    /// hit test come to disagree.
    pub(in crate::render_thread) fn on_frame(
        self,
        presentation: PresentationId,
        frame: FrameSample,
    ) -> (Self, PaneLayoutComposition) {
        match self {
            Self::Settled => (Self::Settled, PaneLayoutComposition::default()),
            Self::Animating(morph) => {
                // Apply any retarget recorded since the last frame, starting the
                // new motion from where these panes actually are.
                let morph = match morph.spliced(frame) {
                    Some(spliced) => spliced,
                    None if morph.has_pending_retarget() => {
                        // The retarget left nothing to animate: the panes are
                        // already where the new layout wants them. Settle, but
                        // still hand out the transform — this frame places
                        // nothing, so the compositor-only path takes it, and
                        // that path used to drop the projection entirely.
                        return (
                            Self::Settled,
                            PaneLayoutComposition {
                                blits: Vec::new(),
                                projection: Some(InteractionProjection::settled(presentation)),
                            },
                        );
                    }
                    None => morph,
                };
                let sample = morph.sample(frame);
                // The only externally visible evidence that a morph is running.
                // A pane morph leaves no trace in the frame snapshot — that
                // reports the layout engine's output, and a morph happens
                // downstream of it — and it is far faster than a screenshot can
                // sample, so without this the question "did it animate?" can
                // only be answered by rebuilding with a probe. That cost real
                // time twice.
                tracing::debug!(
                    progress = sample.motion.progress,
                    panes = sample.panes.len(),
                    first_pane_width = sample.panes.first().map(|pane| pane.bounds.width),
                    "pane morph placed"
                );
                let composition = PaneLayoutComposition {
                    blits: sample.pane_blits(),
                    projection: Some(sample.projection(presentation)),
                };
                if sample.motion.finished {
                    // The last frame still draws the panes, at their
                    // destination; only then is the motion over.
                    (Self::Settled, composition)
                } else {
                    (Self::Animating(morph), composition)
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "layout_driver_test.rs"]
mod tests;
