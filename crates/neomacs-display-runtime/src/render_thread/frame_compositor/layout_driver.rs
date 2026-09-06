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
use crate::render_thread::render_quality::WindowAnimationSpecs;
use neomacs_display_protocol::frame_glyphs::WindowInfo;
use neomacs_display_protocol::frame_time::{EventTime, FrameSample};
use neomacs_display_protocol::{InteractionProjection, PresentationId};
use neomacs_renderer_wgpu::SnapshotLease;

/// What one commit wants, against what the last composed frame showed.
///
/// Computed once per install and passed to the driver, rather than re-derived
/// at each decision point. The two disagreeing versions of "did the layout
/// change?" are what let the cancel and restart bugs coexist.
pub(in crate::render_thread) struct LayoutDelta<'a> {
    pub(in crate::render_thread) previous: &'a [WindowInfo],
    pub(in crate::render_thread) next: &'a [WindowInfo],
}

/// The picture a motion fades *from*.
///
/// Three states rather than an `Option<SnapshotLease>`, because "not asked yet"
/// and "asked, and there was nothing" have to be told apart. Only the first
/// frame of a motion may pin: at that instant the ring's previous slot holds
/// the last frame composed under the *old* layout, and every frame after it
/// holds a frame of the motion itself. An `Option` alone cannot express that,
/// so a motion whose first frame found nothing would quietly pin a frame of
/// itself one frame later — and fading the destination into the destination
/// renders as a completely static frame, which is indistinguishable from the
/// animation not running at all.
pub(in crate::render_thread) enum OutgoingPicture {
    /// No frame of this motion has been drawn yet.
    Unpinned,
    /// Decided, on the motion's first frame. `None` when there was no previous
    /// composition to pin — the frame that started this motion was the first
    /// ever composed offscreen, so there is no picture of the old layout
    /// anywhere and the panes that would fade out of it are dropped instead.
    Pinned(Option<SnapshotLease>),
}

/// What the compositor is doing about the layout.
#[derive(Default)]
pub(in crate::render_thread) enum LayoutDriver {
    /// The panes are where the presentation says they are.
    #[default]
    Settled,
    /// The panes are travelling between two layouts.
    Animating {
        morph: PaneLayoutMorph,
        /// The composed picture as it was *before* this rearrangement.
        ///
        /// A morph fades the old frame out over the new one, so it needs the
        /// old frame for its whole duration — but the composition ring holds
        /// exactly one frame of history, and every frame of the motion pushes
        /// another copy of the *new* layout into it. Read from the ring each
        /// frame, the fade is correct once and then dissolves the destination
        /// into itself, which looks like no animation at all.
        ///
        /// So the morph pins it. `SnapshotLease` is refcounted and the pool
        /// declines to recycle a slot anyone still holds, which is the
        /// mechanism the ring was built around; holding the lease here is what
        /// says "for as long as this motion runs". Settling drops it, and
        /// because the transitions consume `self` there is no path that
        /// forgets to.
        ///
        outgoing: OutgoingPicture,
    },
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
        matches!(self, Self::Animating { .. })
    }

    /// Pin the picture this motion fades *from*, if it has not been pinned.
    ///
    /// Called by the render pass on every frame of a motion, with the ring's
    /// previous composition. Only the first call stores anything: on the first
    /// frame of the motion that lease is the last frame composed under the old
    /// layout, and every later one is a frame of the motion itself.
    ///
    /// Takes the candidate by value rather than a closure because the ring and
    /// this driver are neighbouring fields of the compositor, and a closure
    /// borrowing one while this borrows the other does not compile — a detail
    /// worth naming, since the obvious lazy signature looks preferable.
    pub(in crate::render_thread) fn pin_outgoing(
        &mut self,
        candidate: Option<SnapshotLease>,
    ) -> Option<&SnapshotLease> {
        let Self::Animating { outgoing, .. } = self else {
            return None;
        };
        if let OutgoingPicture::Unpinned = outgoing {
            // The only externally visible evidence of the one thing that
            // decides whether a morph is visible at all. A motion that pins
            // nothing still runs, still places panes, and still logs progress
            // to 1.0 — it just renders as a static frame, so every symptom
            // points at the motion and none of them at the cause.
            tracing::debug!(
                pinned = candidate.is_some(),
                "pane morph pinned its outgoing picture"
            );
            *outgoing = OutgoingPicture::Pinned(candidate);
        }
        match outgoing {
            OutgoingPicture::Pinned(lease) => lease.as_ref(),
            OutgoingPicture::Unpinned => unreachable!("just pinned above"),
        }
    }

    /// A commit arrived.
    pub(in crate::render_thread) fn on_commit(
        self,
        delta: LayoutDelta<'_>,
        specs: WindowAnimationSpecs,
        at: EventTime,
    ) -> Self {
        match self {
            // Nothing in flight: a rearrangement starts one, anything else is
            // still nothing. `try_new` answers both by returning `None` when no
            // pane moved.
            Self::Settled => PaneLayoutMorph::try_new(delta.previous, delta.next, specs, at)
                .map_or(Self::Settled, |morph| Self::Animating {
                    morph,
                    outgoing: OutgoingPicture::Unpinned,
                }),
            Self::Animating {
                mut morph,
                outgoing,
            } => {
                // Retarget only when this commit wants the panes somewhere other
                // than where they are already heading. A retarget restarts the
                // motion from this instant, and commits arrive continuously
                // while one runs — a keystroke, a blink, a mode-line tick — so
                // retargeting on all of them is what made the panes crawl.
                if morph.destination_differs_from(delta.next) {
                    morph.retarget(delta.next, specs, at);
                }
                // The pinned picture is kept across a retarget. It is what the
                // user last saw settled, which a change of destination does not
                // alter; re-pinning here would swap in a frame of the motion
                // itself and fade the destination into the destination.
                Self::Animating { morph, outgoing }
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
            Self::Animating { morph, outgoing } => {
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
                    progress = sample.motion.geometry.progress,
                    panes = sample.panes.len(),
                    // The travelling frontier, not a pane's painted width:
                    // since a placement carries what it *paints*, a shrinking
                    // pane's own quad is its destination size from the first
                    // frame and says nothing about how far along the motion is.
                    frontier = sample
                        .panes
                        .iter()
                        .map(|pane| pane.bounds.x + pane.bounds.width)
                        .fold(0.0_f32, f32::max),
                    "pane morph placed"
                );
                let composition = PaneLayoutComposition {
                    blits: sample.pane_blits(),
                    projection: Some(sample.projection(presentation)),
                };
                if sample.motion.finished() {
                    // The last frame still draws the panes, at their
                    // destination; only then is the motion over.
                    (Self::Settled, composition)
                } else {
                    (Self::Animating { morph, outgoing }, composition)
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "layout_driver_test.rs"]
mod tests;
