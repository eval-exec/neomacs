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
    /// A layout committed while these panes were still travelling.
    ///
    /// Recorded rather than applied, because a splice must start the next
    /// motion from where the panes were last *placed*, and only a frame knows
    /// that instant. An install carries an `EventTime` and no `FrameSample`;
    /// the next sample has one. Holding it here is what lets the splice happen
    /// at the moment it can be done correctly instead of the moment it was
    /// requested.
    pending: Option<PendingRetarget>,
}

/// Where the panes must go next, once there is a frame to splice at.
#[derive(Clone, Debug, PartialEq)]
struct PendingRetarget {
    /// Destination rects ordered by window id, so a spliced morph is as
    /// reproducible as `try_new`'s sort makes a fresh one.
    destination: Vec<(LiveDisplayWindowId, Rect)>,
    spec: MotionSpec,
    requested: EventTime,
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
            pending: None,
        })
    }

    /// Where these panes are currently heading, ordered by window id.
    ///
    /// The pending retarget's destination when one is queued, because that is
    /// what the next sample will splice to; otherwise the destination of the
    /// changes in flight.
    fn destination(&self) -> Vec<(LiveDisplayWindowId, Rect)> {
        if let Some(pending) = self.pending.as_ref() {
            return pending.destination.clone();
        }
        let mut destination: Vec<(LiveDisplayWindowId, Rect)> =
            self.changes()
                .filter_map(|change| match change {
                    PaneChange::Persisted { window, to, .. }
                    | PaneChange::Entered { window, to } => Some((window, to)),
                    // A pane that is leaving has no destination to head for.
                    PaneChange::Exited { .. } => None,
                })
                .collect();
        destination.sort_by_key(|(window, _)| window.get());
        destination
    }

    /// Whether `next` wants the panes somewhere other than where they are
    /// already heading.
    ///
    /// The guard on retargeting at all. Commits arrive continuously while a
    /// morph runs — a keystroke, a blink, a mode-line clock tick — and almost
    /// none of them move a pane. Retargeting on one restarts the motion from
    /// the current instant, so retargeting on *every* commit pins progress near
    /// zero and the panes crawl instead of travelling: the animation appears
    /// not to happen, and then the layout arrives.
    pub(in crate::render_thread) fn destination_differs_from(&self, next: &[WindowInfo]) -> bool {
        let mut wanted: Vec<(LiveDisplayWindowId, Rect)> =
            panes_by_window(next).into_iter().collect();
        wanted.sort_by_key(|(window, _)| window.get());
        let heading = self.destination();
        if wanted.len() != heading.len() {
            return true;
        }
        wanted
            .iter()
            .zip(heading.iter())
            .any(|((a_id, a), (b_id, b))| a_id != b_id || rect_changed(*a, *b))
    }

    /// Record that `next` arrived while these panes were still moving.
    ///
    /// Replacing the morph outright is what the code did before, and it made
    /// the panes jump: the replacement's `from` rects came from the committed
    /// presentation, which is the *destination* the old motion was still
    /// travelling toward, so every pane snapped forward to a place it had not
    /// reached and then animated away from it.
    pub(in crate::render_thread) fn retarget(
        &mut self,
        next: &[WindowInfo],
        spec: MotionSpec,
        requested: EventTime,
    ) {
        let mut destination: Vec<(LiveDisplayWindowId, Rect)> =
            panes_by_window(next).into_iter().collect();
        destination.sort_by_key(|(window, _)| window.get());
        self.pending = Some(PendingRetarget {
            destination,
            spec,
            requested,
        });
    }

    /// The morph to carry on with at `frame`, applying any pending retarget.
    ///
    /// Returns `None` when the retarget leaves nothing to animate — the panes
    /// are already where the new layout wants them, so the motion is over.
    pub(in crate::render_thread) fn spliced(&self, frame: FrameSample) -> Option<Self> {
        let pending = self.pending.as_ref()?;
        let sample = self.motion.sample(frame);
        // Where the panes actually are on screen right now. This, not the
        // committed layout, is what the next motion must start from.
        let placed: std::collections::HashMap<LiveDisplayWindowId, Rect> = self
            .changes()
            .filter_map(|change| PanePlacement::at(change, sample))
            .map(|placement| (placement.window, placement.bounds))
            .collect();

        let mut changes = Vec::new();
        for (window, to) in &pending.destination {
            match placed.get(window) {
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
        // A window the new layout drops keeps its record at the position it had
        // reached, so step 8 has something to fade rather than a rect from a
        // presentation two commits old.
        for (window, from) in &placed {
            if !pending.destination.iter().any(|(id, _)| id == window) {
                changes.push(PaneChange::Exited {
                    window: *window,
                    from: *from,
                });
            }
        }
        changes.sort_by_key(|change| change.window().get());

        // Hand the speed across so the panes do not visibly stall at the
        // splice. One shared rate for one shared motion: with a single scalar
        // progress driving independent per-pane lerps, per-pane velocity
        // continuity is not expressible, and claiming it would be a lie about
        // what the motion does.
        let motion = Motion::resume(
            pending.spec,
            pending.requested,
            super::super::motion::ProgressRate::new(sample.rate),
        )?;
        let mut changes = changes.into_iter();
        let first = changes.next()?;
        Some(Self {
            motion,
            first,
            additional: changes.collect(),
            pending: None,
        })
    }

    /// Whether a layout arrived while these panes were still travelling.
    pub(in crate::render_thread) const fn has_pending_retarget(&self) -> bool {
        self.pending.is_some()
    }

    pub(in crate::render_thread) fn changes(&self) -> impl Iterator<Item = PaneChange> + '_ {
        std::iter::once(self.first).chain(self.additional.iter().copied())
    }

    /// Where every pane sits at `frame`.
    pub(in crate::render_thread) fn sample(&self, frame: FrameSample) -> LayoutSample {
        let motion = self.motion.sample(frame);
        LayoutSample {
            panes: {
                let mut panes = Vec::new();
                for change in self.changes() {
                    place(change, motion, &mut panes);
                }
                panes
            },
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
    /// Where the pane's top-left sits in the picture it samples.
    ///
    /// While a pane is still travelling, the pixels under it belong to a
    /// different place in that picture than its surface position says. This is
    /// that place, and it is what makes the interaction projection exact.
    pub(in crate::render_thread) content_origin: (f32, f32),
    /// Which picture holds this pane's pixels.
    pub(in crate::render_thread) source: neomacs_renderer_wgpu::PaneSource,
    /// How opaque to draw it, for a pane entering or leaving.
    pub(in crate::render_thread) opacity: f32,
}

/// Linear interpolation at `t`, which may exceed `[0, 1]` for a spring.
fn lerp(from: f32, to: f32, t: f32) -> f32 {
    from + (to - from) * t
}

impl PanePlacement {
    /// Where `change` puts its pane at `motion`.
    ///
    /// Usually one placement; two while a pane whose *width* changed is
    /// crossfading its old wrapping into its new one. See [`Self::place`].
    fn at(change: PaneChange, motion: MotionSample) -> Option<Self> {
        Some(match change {
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
                    source: neomacs_renderer_wgpu::PaneSource::Destination,
                    opacity: 1.0,
                }
            }
            // An entering pane has nowhere to travel from, so it sits at its
            // destination and fades in. Fading rather than appearing outright
            // is what distinguishes it from the frame simply being redrawn: a
            // new window arriving instantly at full opacity is exactly the jump
            // the morph exists to remove.
            PaneChange::Entered { window, to } => Self {
                window,
                bounds: to,
                content_origin: (to.x, to.y),
                source: neomacs_renderer_wgpu::PaneSource::Destination,
                opacity: motion.content_mix.get(),
            },
            // A leaving pane holds still at the rect it had reached and fades
            // out, reading from the *previous* composition. That source is the
            // whole point: its window is absent from the destination, so the
            // composed picture holds no pixels for it at all. Sampling the
            // destination instead would blit whatever replaced it, wearing the
            // departing pane's geometry — which is what made an earlier version
            // of this repaint the settled layout over panes still in motion.
            PaneChange::Exited { window, from } => Self {
                window,
                bounds: from,
                content_origin: (from.x, from.y),
                source: neomacs_renderer_wgpu::PaneSource::Previous,
                opacity: 1.0 - motion.content_mix.get(),
            },
        })
    }
}

/// How much a width must change for the text inside to be worth crossfading.
///
/// Below this a reflow either did not happen or moved nothing a reader would
/// notice, and crossfading two nearly identical pictures only costs a texture
/// and softens the glyphs for the duration.
const REFLOW_WIDTH_EPSILON: f32 = 1.0;

/// Every placement `change` contributes at `motion`.
///
/// A pane that only *moved* contributes one: its text did not rewrap, so the
/// destination picture is correct for it at every instant. A pane whose width
/// changed contributes two, because its text did rewrap — the destination
/// picture shows the new line breaks, and using it alone means the wrapping
/// snaps to its final shape on the very first frame while the geometry spends
/// the whole motion catching up. The second placement is the *previous*
/// picture at the same rect, fading out, so the old wrapping is still visible
/// while the pane is still the old shape.
fn place(change: PaneChange, motion: MotionSample, out: &mut Vec<PanePlacement>) {
    let Some(destination) = PanePlacement::at(change, motion) else {
        return;
    };
    if let PaneChange::Persisted { window, from, to } = change
        && (from.width - to.width).abs() > REFLOW_WIDTH_EPSILON
    {
        // The outgoing wrapping, anchored where the pane used to be so its
        // lines sit where the reader last saw them, fading out as the
        // destination fades in underneath.
        out.push(PanePlacement {
            window,
            bounds: destination.bounds,
            content_origin: (from.x, from.y),
            source: neomacs_renderer_wgpu::PaneSource::Previous,
            opacity: 1.0 - motion.content_mix.get(),
        });
    }
    out.push(destination);
}

/// Every pane's placement for one frame, from one shared motion sample.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::render_thread) struct LayoutSample {
    pub(in crate::render_thread) panes: Vec<PanePlacement>,
    pub(in crate::render_thread) motion: MotionSample,
}

/// What one composition places, and the transform matching those pixels.
///
/// The two travel together because they come from a single sample of a single
/// motion; separating them is how a hit test and a render come to disagree.
#[derive(Default)]
pub(in crate::render_thread) struct PaneLayoutComposition {
    pub(in crate::render_thread) blits: Vec<neomacs_renderer_wgpu::PaneBlit>,
    pub(in crate::render_thread) projection:
        Option<neomacs_display_protocol::InteractionProjection>,
}

impl LayoutSample {
    /// The placements, as the renderer's per-pane blits.
    pub(in crate::render_thread) fn pane_blits(&self) -> Vec<neomacs_renderer_wgpu::PaneBlit> {
        self.panes
            .iter()
            .map(|placement| neomacs_renderer_wgpu::PaneBlit {
                bounds: placement.bounds,
                content_origin: placement.content_origin,
                source: placement.source,
                opacity: placement.opacity,
            })
            .collect()
    }

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
            // Only destination-sourced placements. A reflow ghost shows the
            // *previous* presentation's wrapping, so a point inside it does not
            // name a position in the destination at all — including it would
            // let a click resolve against text that is on its way off screen.
            // Mid-crossfade a click therefore resolves to the destination,
            // which is the layout the user is about to have.
            .filter(|placement| placement.source == neomacs_renderer_wgpu::PaneSource::Destination)
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
