//! Which panes moved between two presentations, and where they are mid-motion.
//!
//! Splitting a window, deleting one, or changing a frame's size rearranges
//! every pane at once. Installed as a single presentation, that arrives as a
//! jump: the pane the user was reading is suddenly half as wide, somewhere
//! else. What the compositor can do about it is visible in the two
//! presentations — the same window appears in both with a different rect — so
//! nothing needs to be declared for it to animate.
//!
//! # A morph is a whole-frame fact, not a per-pane one
//!
//! Panes tile: they share edges, and the edges have to stay shared throughout.
//! Animating each pane against its own clock would let two sides of a split
//! disagree about where their boundary is, and a one-pixel gap or overlap on a
//! moving seam is far more visible than the motion itself. So one [`Motion`] is
//! sampled once per frame and every pane is placed from that single sample.
//!
//! # Identity is the live window id
//!
//! Panes are matched by [`LiveDisplayWindowId`], which is the only identity that
//! survives a layout change: rects change by definition, buffers move between
//! windows, and ordering in `window_infos` is a producer detail. A window
//! present in both presentations *persisted* and can be moved; one only in the
//! destination *entered* and has no previous rect to come from; one only in the
//! source *exited* and has nothing left to draw from a retained scene.
//!
//! # What travels, and what does not
//!
//! Panes travel. The frame's overlays — the tool bar, scroll indicators, and
//! any child frame — stay where the destination presentation puts them, and
//! that is deliberate rather than an omission. A child frame's position is
//! computed by the evaluator against the layout it is being shown for, so
//! carrying it along a pane's path would move it away from the coordinates it
//! was placed at; the tool bar belongs to the frame and not to any pane at
//! all. The cursor is the exception that needs no special case: it is drawn as
//! part of a window's content, so it is already inside the picture each pane
//! samples and travels with its own pane for free.

use crate::render_thread::frame_compositor::motion::{Motion, MotionSample};
use neomacs_display_protocol::frame_glyphs::WindowInfo;
use neomacs_display_protocol::frame_time::{EventTime, FrameSample};
use neomacs_display_protocol::motion_spec::MotionSpec;
use neomacs_display_protocol::types::{LiveDisplayWindowId, Rect};

/// What happened to one pane between two presentations.
#[derive(Clone, Copy, Debug, PartialEq, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub(in crate::render_thread) enum PaneChange {
    /// The same window, in a different place. Its content can be moved.
    Persisted {
        window: LiveDisplayWindowId,
        from: Rect,
        to: Rect,
    },
    /// A window the destination has and the source did not.
    ///
    /// There is nothing to move it from, so it is placed at its destination.
    /// Fading it in is step 8's business, once snapshots exist to fade.
    Entered {
        window: LiveDisplayWindowId,
        to: Rect,
    },
    /// A window the source had and the destination does not.
    ///
    /// Retained here so the compositor knows the pane is leaving rather than
    /// simply gone; drawing it during its exit needs a snapshot, which is
    /// step 8.
    Exited {
        window: LiveDisplayWindowId,
        from: Rect,
    },
}

impl PaneChange {
    pub(in crate::render_thread) const fn window(self) -> LiveDisplayWindowId {
        match self {
            Self::Persisted { window, .. }
            | Self::Entered { window, .. }
            | Self::Exited { window, .. } => window,
        }
    }
}

/// A layout change in progress. Non-empty by construction.
///
/// [`Self::try_new`] returns `None` rather than an empty morph, so holding one
/// means there is something to animate — no caller has to check.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::render_thread) struct PaneLayoutMorph {
    motion: Motion,
    first: PaneChange,
    additional: Vec<PaneChange>,
}

/// Rects closer than this are the same rect.
///
/// Layout arrives as `f32` from integer cell arithmetic, so exact equality
/// would call a pane "moved" over a rounding difference and animate a frame
/// that is not moving.
const RECT_EPSILON: f32 = 0.5;

fn rect_changed(from: Rect, to: Rect) -> bool {
    (from.x - to.x).abs() > RECT_EPSILON
        || (from.y - to.y).abs() > RECT_EPSILON
        || (from.width - to.width).abs() > RECT_EPSILON
        || (from.height - to.height).abs() > RECT_EPSILON
}

/// The panes a presentation offers, keyed by live window id.
///
/// A window whose id is a placeholder is skipped rather than matched: it has no
/// identity to match *by*, so pairing two of them would be pairing whatever
/// happened to be published in the same slot.
fn panes_by_window(windows: &[WindowInfo]) -> std::collections::HashMap<LiveDisplayWindowId, Rect> {
    windows
        .iter()
        .filter(|info| !info.is_minibuffer)
        .filter_map(|info| {
            LiveDisplayWindowId::try_from(info.window_id)
                .ok()
                .map(|window| (window, info.bounds))
        })
        .collect()
}

impl PaneLayoutMorph {
    /// The morph from `previous` to `next`, if there is one to animate.
    ///
    /// Returns `None` when no pane moved, entered or left, and when `spec` is
    /// [`MotionSpec::Instant`] — a policy that asks for no motion must not
    /// allocate one, which is what keeps a reduced-motion setting free rather
    /// than merely fast.
    ///
    /// The minibuffer is excluded throughout. It resizes on nearly every
    /// command as the echo area grows and shrinks, and sliding it would put a
    /// moving pane under the user's cursor while they type.
    pub(in crate::render_thread) fn try_new(
        previous: &[WindowInfo],
        next: &[WindowInfo],
        spec: MotionSpec,
        origin: EventTime,
    ) -> Option<Self> {
        let motion = Motion::start(spec, origin)?;
        let before = panes_by_window(previous);
        let after = panes_by_window(next);

        let mut changes = Vec::new();
        for (window, to) in &after {
            match before.get(window) {
                Some(&from) if rect_changed(from, *to) => changes.push(PaneChange::Persisted {
                    window: *window,
                    from,
                    to: *to,
                }),
                Some(_) => {}
                None => changes.push(PaneChange::Entered {
                    window: *window,
                    to: *to,
                }),
            }
        }
        for (window, from) in &before {
            if !after.contains_key(window) {
                changes.push(PaneChange::Exited {
                    window: *window,
                    from: *from,
                });
            }
        }

        // Ordered by window id so a morph is reproducible: the maps above
        // iterate in hash order, and a projection whose pane order varied
        // between two runs of the same layout change would resolve overlapping
        // hits differently each time.
        changes.sort_by_key(|change| change.window().get());

        let mut changes = changes.into_iter();
        let first = changes.next()?;
        Some(Self {
            motion,
            first,
            additional: changes.collect(),
        })
    }

    pub(in crate::render_thread) fn changes(&self) -> impl Iterator<Item = PaneChange> + '_ {
        std::iter::once(self.first).chain(self.additional.iter().copied())
    }

    /// Where every pane sits at `frame`.
    pub(in crate::render_thread) fn sample(&self, frame: FrameSample) -> LayoutSample {
        let motion = self.motion.sample(frame);
        LayoutSample {
            panes: self
                .changes()
                .map(|change| PanePlacement::at(change, motion))
                .collect(),
            motion,
        }
    }
}

/// Where one pane is drawn, and which of its content that shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::render_thread) struct PanePlacement {
    pub(in crate::render_thread) window: LiveDisplayWindowId,
    /// The pane's rect on the surface right now.
    pub(in crate::render_thread) bounds: Rect,
    /// Where the pane's top-left sits in the destination presentation.
    ///
    /// While a pane is still travelling, the pixels under it belong to a
    /// different place in the destination than its surface position says. This
    /// is that place, and it is what makes the interaction projection exact.
    pub(in crate::render_thread) content_origin: (f32, f32),
}

/// Linear interpolation at `t`, which may exceed `[0, 1]` for a spring.
fn lerp(from: f32, to: f32, t: f32) -> f32 {
    from + (to - from) * t
}

impl PanePlacement {
    fn at(change: PaneChange, motion: MotionSample) -> Self {
        match change {
            PaneChange::Persisted { window, from, to } => {
                let t = motion.progress;
                let bounds = Rect {
                    x: lerp(from.x, to.x, t),
                    y: lerp(from.y, to.y, t),
                    width: lerp(from.width, to.width, t),
                    height: lerp(from.height, to.height, t),
                };
                Self {
                    window,
                    bounds,
                    // The pane shows its destination content throughout, so the
                    // content under its moving top-left is the destination's
                    // top-left. Interpolating the content origin instead would
                    // scroll the text inside the pane as it travelled.
                    content_origin: (to.x, to.y),
                }
            }
            // An entering pane has nowhere to come from and a leaving one has
            // nowhere to go, so both stay put for the duration. Step 8 gives
            // them snapshots to fade.
            PaneChange::Entered { window, to } => Self {
                window,
                bounds: to,
                content_origin: (to.x, to.y),
            },
            PaneChange::Exited { window, from } => Self {
                window,
                bounds: from,
                content_origin: (from.x, from.y),
            },
        }
    }
}

/// Every pane's placement for one frame, from one shared motion sample.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::render_thread) struct LayoutSample {
    pub(in crate::render_thread) panes: Vec<PanePlacement>,
    pub(in crate::render_thread) motion: MotionSample,
}

impl LayoutSample {
    /// The interaction projection matching exactly what this sample draws.
    ///
    /// Built here, from the same placements the layout pass renders, so the two
    /// cannot drift apart: there is no second place that computes a transform.
    pub(in crate::render_thread) fn projection(
        &self,
        presentation: neomacs_display_protocol::PresentationId,
    ) -> neomacs_display_protocol::InteractionProjection {
        let panes = self
            .panes
            .iter()
            .filter_map(|placement| {
                let clip = neomacs_display_protocol::GeometryRect::<
                    neomacs_display_protocol::RootSurfaceSpace,
                    neomacs_display_protocol::LogicalPixels,
                >::new(
                    placement.bounds.x,
                    placement.bounds.y,
                    placement.bounds.width,
                    placement.bounds.height,
                )
                .ok()?;
                let origin = neomacs_display_protocol::GeometryPoint::<
                    neomacs_display_protocol::PresentationFrameSpace,
                    neomacs_display_protocol::LogicalPixels,
                >::from_px(
                    placement.content_origin.0, placement.content_origin.1
                )
                .ok()?;
                neomacs_display_protocol::PaneProjection::new(placement.window, clip, origin).ok()
            })
            .collect();
        neomacs_display_protocol::InteractionProjection::new(presentation, panes)
    }
}

#[cfg(test)]
#[path = "pane_layout_test.rs"]
mod tests;
