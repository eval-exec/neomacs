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
use crate::render_thread::render_quality::{GeometryRole, WindowAnimationSpecs};
use neomacs_display_protocol::frame_glyphs::WindowInfo;
use neomacs_display_protocol::frame_time::{EventTime, FrameSample};
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
        /// How thick this pane's chrome was in the picture being animated
        /// away. Cutting the outgoing blit on these is what lets a mode line
        /// travel to its new edge instead of being clipped away at its old one.
        insets: ChromeInsets,
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
    /// One motion for every pane that travels.
    ///
    /// One, not one per pane, and not one per role: `placed_bounds` reads a
    /// single progress and adjacent panes tile edge to edge. Two panes on two
    /// clocks disagree about where their shared edge is, and a one-pixel gap on
    /// a moving seam is far more visible than the motion itself. niri gives
    /// every window its own curve because its windows have gaps between them.
    ///
    /// `None` when no pane persists, or when the geometry slot is disabled --
    /// then every pane is placed at rest, which for an entering or leaving pane
    /// is exactly where it belongs.
    geometry: Option<Motion>,
    /// Opacity of entering panes. Independent, and safe to be: an `Entered`
    /// pane is placed at its destination unconditionally, so its curve cannot
    /// move a seam.
    open: Option<Motion>,
    /// Opacity of leaving panes. Independent for the same reason -- an `Exited`
    /// pane holds the rect it had, and what *uncovers* it is the geometry.
    close: Option<Motion>,
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
    specs: WindowAnimationSpecs,
    requested: EventTime,
}

/// One sample of each role, taken from one frame.
///
/// Passed around together so a placement cannot read one role's progress and
/// another's opacity by accident -- which is silent, and looks like a curve
/// that is subtly wrong rather than like a bug.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::render_thread) struct RoleSamples {
    pub(in crate::render_thread) geometry: MotionSample,
    open: MotionSample,
    /// `None` when the close slot is disabled, which is not the same as a
    /// close motion that has arrived.
    ///
    /// Every other role collapses a disabled slot to [`rest`], because
    /// "arrived" is what a disabled slot should look like: a pane that is not
    /// travelling is at its destination, one that is not fading in is opaque.
    /// A departing pane inverts that. Its opacity runs `1 - mix`, so "arrived"
    /// would mean *gone* -- a window that vanishes the instant it is deleted,
    /// which is precisely the jump the morph exists to remove. Disabled has to
    /// mean "never fades", and only an absent sample can say that.
    close: Option<MotionSample>,
}

/// A role with no motion samples as *arrived*, never as absent.
///
/// A disabled slot must place its panes at their destination. Sampling it as
/// progress zero would instead pin them at the start forever, so "open on,
/// resize off" would leave every pane at its old rect for the length of the
/// open fade.
impl RoleSamples {
    /// How opaque the outgoing picture of a *resizing* pane should be.
    ///
    /// Held near one while the pane is far from its destination, falling only
    /// as it arrives -- not `1 - progress`, which is what made a `C-x 3` show
    /// the left window's scroll bar at its final position from the first frame.
    ///
    /// Holding it is defensible for the pane's *body*: a window whose lines are
    /// truncated renders, at width w, very nearly the leftmost w pixels of its
    /// old picture, so the old picture clipped to the travelling rect is close
    /// to right rather than a placeholder.
    ///
    /// It is NOT right for the pane's *chrome*, and an earlier version of this
    /// comment claimed otherwise. A window's rect ends in a right fringe, a
    /// right divider, a scroll bar and a mode line, and all four are at or
    /// determined by the right edge -- the mode line's right-aligned segments
    /// most of all. Clipping the old picture puts old body pixels exactly where
    /// the new chrome belongs, so the destination's divider and mode line are
    /// hidden on the first frame and arrive through this crossfade. That is why
    /// it cannot simply be held at 1.0 and snapped: every window re-chromes,
    /// even one that never rewraps a line.
    ///
    /// niri and Hyprland avoid the question by scaling the window's pixels onto
    /// the animating rect. That is fine for a photo or a toolbar and bad for
    /// text: the source is already-rasterized, hinted glyphs, and a split
    /// scales one axis only.
    ///
    /// # Why this is not skipped for windows that do not rewrap
    ///
    /// The obvious optimisation is to hold this at 1.0 and snap when the pane's
    /// lines are truncated, since narrowing then changes no line break. It buys
    /// nothing, and the reason is worth writing down because the idea is
    /// recurrent.
    ///
    /// A crossfade between two pictures that *agree* is exact, not blurry --
    /// blending an image with itself returns the image. Measured on a 40-line
    /// buffer of long lines, comparing the full-width window's leftmost 600px
    /// against the same region after `split-window-right`: with
    /// `truncate-lines` t the body differs by 454 of 282000 pixels (0.16%,
    /// essentially the cursor), and with wrapping enabled by 26265 (9.3%). So
    /// in the truncating case this crossfade is already invisible over the
    /// body, and in the wrapping case it is doing exactly the work it exists
    /// for.
    ///
    /// What stays soft when truncating is the chrome, and that is the one thing
    /// a skip could not fix: the fringe, divider, scroll bar and mode line
    /// differ at the two widths in every window, rewrapping or not.
    fn outgoing_opacity(self) -> f32 {
        let arrived = self.geometry.content_mix.get();
        1.0 - arrived * arrived * arrived
    }

    /// Whether every role has arrived.
    ///
    /// All of them, not the geometry alone: an entering pane's fade outlasts a
    /// spring that has already settled, and dropping the morph at the first
    /// role to finish would cut that fade off mid-way. A role with no motion
    /// samples as arrived, so it never holds the morph open.
    pub(in crate::render_thread) fn finished(self) -> bool {
        self.geometry.finished && self.open.finished && self.close.is_none_or(|c| c.finished)
    }

    /// How opaque a pane that is leaving should be drawn.
    ///
    /// Opaque when the close slot is disabled: the pane holds the ground it
    /// has not yet given up and is *uncovered* by whatever grows across it,
    /// rather than blended with it. That is the shipped default, because
    /// fading a departing pane over a backdrop already showing the settled
    /// layout is a double exposure -- the deleted window and the pane replacing
    /// it both half-visible for the length of the motion.
    fn departing_opacity(self) -> f32 {
        self.close
            .map_or(1.0, |close| 1.0 - close.content_mix.get())
    }
}

fn rest() -> MotionSample {
    MotionSample {
        progress: 1.0,
        content_mix: neomacs_display_protocol::motion_spec::UnitInterval::clamp(1.0),
        rate: 0.0,
        finished: true,
    }
}

/// Rects closer than this are the same rect.
///
/// Layout arrives as `f32` from integer cell arithmetic, so exact equality
/// would call a pane "moved" over a rounding difference and animate a frame
/// that is not moving.
const RECT_EPSILON: f32 = 0.5;

/// The physical pixel grid every placement has to land on.
///
/// A morph interpolates rects as floats, so a pane's edge lands mid-texel on
/// almost every frame. The pass draws a 1:1 blit -- no magnification -- but the
/// snapshot is sampled through a `FilterMode::Linear` sampler, so a fractional
/// offset still resamples: already-rasterized, hinted glyphs get bilinearly
/// mixed with their neighbours, at a phase that walks from frame to frame. Text
/// softens for the length of the motion and hinted stems shimmer.
///
/// That is the same principle this module refuses to break when it declines to
/// *scale* a pane's content -- the translation term was simply overlooked.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::render_thread) struct PixelGrid {
    scale: f32,
}

impl PixelGrid {
    /// The grid for a surface at `scale_factor` device pixels per logical one.
    pub(in crate::render_thread) fn new(scale_factor: f64) -> Self {
        let scale = scale_factor as f32;
        Self {
            scale: if scale.is_finite() && scale > 0.0 {
                scale
            } else {
                1.0
            },
        }
    }

    /// Snap one edge to a device pixel.
    ///
    /// Device, not logical: on a 2x display a logical `.5` *is* a whole device
    /// pixel, and rounding it away would discard precision the screen can show.
    fn edge(self, logical: f32) -> f32 {
        (logical * self.scale).round() / self.scale
    }

    /// Snap a rect by its **edges**, never by origin and extent separately.
    ///
    /// Rounding `x` and `width` independently is what opens a one-pixel seam
    /// between tiled neighbours -- it is the reported swayfx and Hyprland
    /// symptom. Two adjacent panes share an edge *value*, so rounding edges
    /// makes them round to the same number and the seam stays closed. This is
    /// the one place where fixing the resampling could reintroduce the tearing
    /// we do not have.
    fn rect(self, rect: Rect) -> Rect {
        let x = self.edge(rect.x);
        let y = self.edge(rect.y);
        Rect::new(
            x,
            y,
            self.edge(rect.x + rect.width) - x,
            self.edge(rect.y + rect.height) - y,
        )
    }

    /// Snap a placement whole, so the pixels it draws and the rect the
    /// interaction projection clips to stay the same rectangle.
    fn placement(self, mut placement: PanePlacement) -> PanePlacement {
        placement.bounds = self.rect(placement.bounds);
        placement.painted = self.rect(placement.painted);
        // The source origin too: the offset from destination pixel to source
        // texel is `content_origin - bounds.x`, and only whole-texel offsets
        // sample without filtering. Snapping one end and not the other leaves
        // the phase fractional and fixes nothing.
        placement.content_origin = (
            self.edge(placement.content_origin.0),
            self.edge(placement.content_origin.1),
        );
        placement
    }
}

/// Which geometry slot a morph's changes ask for, or `None` if no pane travels.
///
/// Resize wins a mixed morph: in an edge-to-edge tiling, moving one pane's edge
/// resizes its neighbour, so the mixed morph is the common one, and resize is
/// the change the reflow crossfade and the vacated strip are built for.
fn geometry_role(changes: impl Iterator<Item = PaneChange>) -> Option<GeometryRole> {
    let mut role = None;
    for change in changes {
        if let PaneChange::Persisted { from, to, .. } = change {
            if (from.width - to.width).abs() > RECT_EPSILON
                || (from.height - to.height).abs() > RECT_EPSILON
            {
                return Some(GeometryRole::Resize);
            }
            role = Some(GeometryRole::Movement);
        }
    }
    role
}

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
fn panes_by_window(
    windows: &[WindowInfo],
) -> std::collections::HashMap<LiveDisplayWindowId, (Rect, ChromeInsets)> {
    windows
        .iter()
        .filter(|info| !info.is_minibuffer)
        .filter_map(|info| {
            LiveDisplayWindowId::try_from(info.window_id)
                .ok()
                .map(|window| (window, (info.bounds, ChromeInsets::of(info.geometry))))
        })
        .collect()
}

/// Just the rects, for the callers that only compare destinations.
fn rects_by_window(windows: &[WindowInfo]) -> std::collections::HashMap<LiveDisplayWindowId, Rect> {
    panes_by_window(windows)
        .into_iter()
        .map(|(window, (rect, _))| (window, rect))
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
        specs: WindowAnimationSpecs,
        origin: EventTime,
    ) -> Option<Self> {
        let before = panes_by_window(previous);
        let after = panes_by_window(next);

        let mut changes = Vec::new();
        for (window, (to, _)) in &after {
            match before.get(window) {
                // The insets come from the *previous* presentation: they
                // describe the picture being animated away, which is the only
                // one this pane's outgoing patches sample.
                Some(&(from, insets)) if rect_changed(from, *to) => {
                    changes.push(PaneChange::Persisted {
                        window: *window,
                        from,
                        to: *to,
                        insets,
                    });
                }
                Some(_) => {}
                None => changes.push(PaneChange::Entered {
                    window: *window,
                    to: *to,
                }),
            }
        }
        for (window, (from, _)) in &before {
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

        // The motions are built *after* the diff, and only for roles that have
        // changes. Building all of them up front would keep the morph -- and
        // its standing frame demand -- alive for the slowest slot even when a
        // single role is in play.
        let (geometry, open, close) = Self::motions(&changes, specs, origin);
        if geometry.is_none() && open.is_none() && close.is_none() {
            // Every role that has changes asked for no motion. A policy that
            // wants none must not allocate one: that is what keeps a
            // reduced-motion setting free rather than merely fast.
            return None;
        }

        let mut changes = changes.into_iter();
        let first = changes.next()?;
        Some(Self {
            geometry,
            open,
            close,
            first,
            additional: changes.collect(),
            pending: None,
        })
    }

    /// One motion per role that `changes` actually asks for.
    fn motions(
        changes: &[PaneChange],
        specs: WindowAnimationSpecs,
        origin: EventTime,
    ) -> (Option<Motion>, Option<Motion>, Option<Motion>) {
        let geometry = geometry_role(changes.iter().copied())
            .and_then(|role| Motion::start(specs.geometry(role), origin));
        let open = changes
            .iter()
            .any(|change| matches!(change, PaneChange::Entered { .. }))
            .then(|| Motion::start(specs.open, origin))
            .flatten();
        let close = changes
            .iter()
            .any(|change| matches!(change, PaneChange::Exited { .. }))
            .then(|| Motion::start(specs.close, origin))
            .flatten();
        (geometry, open, close)
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
            rects_by_window(next).into_iter().collect();
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
        specs: WindowAnimationSpecs,
        requested: EventTime,
    ) {
        let mut destination: Vec<(LiveDisplayWindowId, Rect)> =
            rects_by_window(next).into_iter().collect();
        destination.sort_by_key(|(window, _)| window.get());
        self.pending = Some(PendingRetarget {
            destination,
            specs,
            requested,
        });
    }

    /// The morph to carry on with at `frame`, applying any pending retarget.
    ///
    /// Returns `None` when the retarget leaves nothing to animate — the panes
    /// are already where the new layout wants them, so the motion is over.
    pub(in crate::render_thread) fn spliced(&self, frame: FrameSample) -> Option<Self> {
        let pending = self.pending.as_ref()?;
        let samples = self.sample_roles(frame);
        let vacated = self.vacated_at(samples);
        // Where the panes actually are on screen right now. This, not the
        // committed layout, is what the next motion must start from.
        let placed: std::collections::HashMap<LiveDisplayWindowId, Rect> = self
            .changes()
            .filter_map(|change| PanePlacement::at(change, samples, &vacated))
            .map(|placement| (placement.window, placement.bounds))
            .collect();

        // The insets travel with the window across a splice. A retarget does
        // not repin the outgoing picture -- it is still the one this morph
        // started from -- so the chrome thickness inside it is unchanged, and
        // re-deriving it from the new presentation would describe a picture
        // nothing is sampling.
        let insets_before: std::collections::HashMap<LiveDisplayWindowId, ChromeInsets> = self
            .changes()
            .filter_map(|change| match change {
                PaneChange::Persisted { window, insets, .. } => Some((window, insets)),
                PaneChange::Entered { .. } | PaneChange::Exited { .. } => None,
            })
            .collect();

        let mut changes = Vec::new();
        for (window, to) in &pending.destination {
            match placed.get(window) {
                Some(&from) if rect_changed(from, *to) => changes.push(PaneChange::Persisted {
                    window: *window,
                    from,
                    to: *to,
                    insets: insets_before
                        .get(window)
                        .copied()
                        .unwrap_or(ChromeInsets::ZERO),
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

        // Hand each role's speed across so the panes do not visibly stall at
        // the splice, and hand across the *matching* rate: resuming geometry
        // from an opacity's rate is silently wrong and looks like a curve that
        // is merely mistuned. Within geometry there is one shared rate for one
        // shared motion -- with a single scalar progress driving independent
        // per-pane lerps, per-pane velocity continuity is not expressible, and
        // claiming it would be a lie about what the motion does.
        let resume = |spec, rate: f32| {
            Motion::resume(
                spec,
                pending.requested,
                super::super::motion::ProgressRate::new(rate),
            )
        };
        let role = geometry_role(changes.iter().copied());
        let geometry =
            role.and_then(|role| resume(pending.specs.geometry(role), samples.geometry.rate));
        let open = changes
            .iter()
            .any(|change| matches!(change, PaneChange::Entered { .. }))
            .then(|| resume(pending.specs.open, samples.open.rate))
            .flatten();
        let close = changes
            .iter()
            .any(|change| matches!(change, PaneChange::Exited { .. }))
            .then(|| resume(pending.specs.close, samples.close.map_or(0.0, |c| c.rate)))
            .flatten();
        // Only when *no* role is live is there nothing left to animate. A `?`
        // on one role's resume would abort the splice on another's behalf.
        if geometry.is_none() && open.is_none() && close.is_none() {
            return None;
        }
        let mut changes = changes.into_iter();
        let first = changes.next()?;
        Some(Self {
            geometry,
            open,
            close,
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
    /// Where every shrinking pane is currently giving ground up.
    ///
    /// Needed wherever an entering pane is placed, because it arrives through
    /// that gap: `spliced` reads it for the same reason `sample` does, so a
    /// layout arriving mid-slide carries the pane on from where it actually is
    /// rather than snapping it to its destination.
    fn vacated_at(&self, motion: RoleSamples) -> Vec<VacatedStrip> {
        self.changes()
            .filter_map(|change| match change {
                PaneChange::Persisted { from, to, .. } => {
                    Some(vacated_strips(from, to, placed_bounds(change, motion)))
                }
                PaneChange::Entered { .. } | PaneChange::Exited { .. } => None,
            })
            .flatten()
            .collect()
    }

    /// One sample of every role at `frame`.
    fn sample_roles(&self, frame: FrameSample) -> RoleSamples {
        RoleSamples {
            geometry: self.geometry.map_or_else(rest, |m| m.sample(frame)),
            open: self.open.map_or_else(rest, |m| m.sample(frame)),
            close: self.close.map(|m| m.sample(frame)),
        }
    }

    pub(in crate::render_thread) fn sample(
        &self,
        frame: FrameSample,
        grid: PixelGrid,
    ) -> LayoutSample {
        let motion = self.sample_roles(frame);
        // What the surviving panes have reached, computed before anything is
        // placed because a departing pane has to be trimmed to what is left.
        //
        // Only panes that survive claim ground. An `Entered` pane sits at its
        // destination from the first frame, so counting it would erase a
        // departing pane immediately -- which is the jump the morph exists to
        // remove.
        let claimed: Vec<Rect> = self
            .changes()
            .filter_map(|change| match change {
                PaneChange::Persisted { to, .. } => intersection(placed_bounds(change, motion), to),
                PaneChange::Entered { .. } | PaneChange::Exited { .. } => None,
            })
            .collect();
        let vacated = self.vacated_at(motion);
        let mut panes = Vec::new();
        for change in self.changes() {
            place(change, motion, &claimed, &vacated, &mut panes);
        }
        // Snapped once, here, after every placement exists: one rect for what
        // is drawn and what is hit-tested, landing on the device grid.
        for placement in &mut panes {
            *placement = grid.placement(*placement);
        }
        LayoutSample { panes, motion }
    }
}

/// What is left of `rect` once every claimed area is taken out of it.
///
/// `None` as soon as one step leaves something this cannot express, because a
/// departing pane draws the old picture *over* the destination: a quad that is
/// too large hides the pane taking its place, and dropping it merely means the
/// old window vanishes a little early.
fn unclaimed(rect: Rect, claimed: &[Rect]) -> Option<Rect> {
    claimed
        .iter()
        .try_fold(rect, |left, taken| remainder(left, *taken))
}

/// Where one pane is drawn, and which of its content that shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::render_thread) struct PanePlacement {
    pub(in crate::render_thread) window: LiveDisplayWindowId,
    /// The pane's rect on the surface right now -- where the pane *is*.
    ///
    /// Adjacent panes' `bounds` tile without a gap at every instant, which is
    /// what keeps a moving seam from tearing.
    pub(in crate::render_thread) bounds: Rect,
    /// The part of `bounds` that actually shows this pane's content.
    ///
    /// Smaller than `bounds` for a shrinking pane: it is larger than the
    /// content it owns, and the rest of it is the vacated strip showing the old
    /// picture. This is the rect the pass paints *and* the rect the interaction
    /// projection clips to, and it is one field because they were once two
    /// computations -- the renderer clamped, the projection did not, so the
    /// projection claimed the strip and translated a click there by the pane's
    /// whole travel. On an 800px frame a click at 700 resolved to 900.
    pub(in crate::render_thread) painted: Rect,
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
    /// One placement each. A shrinking or growing pane contributes more, and
    /// [`place`] is what adds them.
    fn at(change: PaneChange, motion: RoleSamples, vacated: &[VacatedStrip]) -> Option<Self> {
        Some(match change {
            PaneChange::Persisted {
                window, from, to, ..
            } => {
                let bounds = placed_bounds(change, motion);
                if !grows(from, to) {
                    return Some(Self {
                        window,
                        bounds,
                        painted: Rect {
                            width: bounds.width.min(to.width),
                            height: bounds.height.min(to.height),
                            ..bounds
                        },
                        // The pane shows its destination content throughout, so
                        // the content under its moving top-left is the
                        // destination's top-left. Interpolating the content
                        // origin instead would scroll the text inside the pane
                        // as it travelled.
                        content_origin: (to.x, to.y),
                        source: neomacs_renderer_wgpu::PaneSource::Destination,
                        opacity: 1.0,
                    });
                }
                // A *growing* pane cannot carry its content. Carrying means
                // "the pane's top-left shows the destination's top-left", and
                // that only works while the pane is at least as large as the
                // content it is showing. A pane smaller than its destination
                // shows the destination's leading corner at its own leading
                // corner -- the correct pixels in the wrong place, sliding into
                // position over the length of the motion.
                //
                // That is what `delete-window` looked like: the survivor grows
                // into the space the deleted window gave up, so the whole new
                // layout slid in from the side while the remnant of the deleted
                // window sat beside it. With both windows on one buffer it read
                // as the text having been duplicated.
                //
                // So a growing pane is anchored where it belongs and revealed
                // where it has reached. Destination coordinates are screen
                // coordinates -- the destination picture *is* the settled frame
                // -- which makes the reveal a plain clip.
                let visible = intersection(bounds, to)?;
                Self {
                    window,
                    bounds,
                    painted: visible,
                    content_origin: (visible.x, visible.y),
                    source: neomacs_renderer_wgpu::PaneSource::Destination,
                    opacity: 1.0,
                }
            }
            // An entering pane has nowhere to travel from, so it sits at its
            // destination and fades in. Fading rather than appearing outright
            // is what distinguishes it from the frame simply being redrawn: a
            // new window arriving instantly at full opacity is exactly the jump
            // the morph exists to remove.
            PaneChange::Entered { window, to } => {
                // Slid in from the divider, not revealed in place. Its size is
                // constant, so its content can travel with it -- the case a
                // resizing pane cannot use, because there the destination
                // picture is laid out for a size the pane does not yet have.
                let (dx, dy) = entry_offset(to, vacated);
                let arriving = Rect {
                    x: to.x + dx,
                    y: to.y + dy,
                    ..to
                };
                Self {
                    window,
                    bounds: arriving,
                    painted: arriving,
                    content_origin: (to.x, to.y),
                    source: neomacs_renderer_wgpu::PaneSource::Destination,
                    opacity: motion.open.content_mix.get(),
                }
            }
            // A leaving pane holds still at the rect it had reached, reading
            // from the *previous* composition. That source is the whole point:
            // its window is absent from the destination, so the composed
            // picture holds no pixels for it at all.
            //
            // Opaque, and clipped by its replacement in [`place`]. Fading it
            // would blend the old window with whatever grows over it for the
            // length of the motion, which is the same double exposure a
            // crossfaded vacated strip produces.
            PaneChange::Exited { window, from } => Self {
                window,
                bounds: from,
                painted: from,
                content_origin: (from.x, from.y),
                source: neomacs_renderer_wgpu::PaneSource::Previous,
                opacity: motion.departing_opacity(),
            },
        })
    }
}

/// Whether a pane ends up larger than it started, on either axis.
///
/// The one thing that decides whether a pane can carry its content: see
/// [`PanePlacement::at`].
fn grows(from: Rect, to: Rect) -> bool {
    to.width - from.width > REFLOW_WIDTH_EPSILON || to.height - from.height > REFLOW_WIDTH_EPSILON
}

/// The whole rect a pane covers at `motion`, before any clipping.
fn placed_bounds(change: PaneChange, motion: RoleSamples) -> Rect {
    match change {
        PaneChange::Persisted { from, to, .. } => {
            let t = motion.geometry.progress;
            Rect {
                x: lerp(from.x, to.x, t),
                y: lerp(from.y, to.y, t),
                // Clamped because `t` is deliberately not. A spring overshoots
                // past 1.0 and that is the point -- it is what makes the motion
                // read as physical, and `MotionSample::progress` documents that
                // clamping it would delete exactly that. But an extent is not a
                // position: `x` extrapolating past its destination is a picture
                // of something, a negative width is not. A pane collapsing to
                // near nothing (an echo area, say) inverts within a few percent
                // of overshoot, and `GeometryRect::new` then rejects it -- which
                // drops the pane from the interaction projection and falls the
                // hit test silently back to identity.
                width: lerp(from.width, to.width, t).max(0.0),
                height: lerp(from.height, to.height, t).max(0.0),
            }
        }
        PaneChange::Entered { to, .. } => to,
        PaneChange::Exited { from, .. } => from,
    }
}

/// The overlap of two rects, or `None` when they are disjoint.
///
/// Touching rects overlap in a zero-extent rect rather than in nothing, and
/// that degenerate answer is load-bearing: a pane growing from zero width
/// starts exactly on the edge it will grow away from, and a `None` there would
/// leave it unplaced on the first frame. Its neighbour is placed, so the two
/// would disagree about where their shared edge is at the one instant the edge
/// is easiest to see. The zero-extent quad draws nothing.
fn intersection(a: Rect, b: Rect) -> Option<Rect> {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    let right = (a.x + a.width).min(b.x + b.width);
    let bottom = (a.y + a.height).min(b.y + b.height);
    (right >= x && bottom >= y).then(|| Rect::new(x, y, right - x, bottom - y))
}

/// The part of `rect` that `covered` has not taken, when that remainder is a
/// single rectangle.
///
/// `None` means "draw nothing": either `covered` swallows `rect` entirely, or
/// what is left is an L-shape this cannot express. Dropping in that case is the
/// safe direction -- an old-picture placement that is too large draws over the
/// pane replacing it, which is worse than one that is missing.
fn remainder(rect: Rect, covered: Rect) -> Option<Rect> {
    let Some(overlap) = intersection(rect, covered) else {
        return Some(rect);
    };
    let spans_rows = overlap.y <= rect.y && overlap.y + overlap.height >= rect.y + rect.height;
    let spans_cols = overlap.x <= rect.x && overlap.x + overlap.width >= rect.x + rect.width;
    if spans_rows && spans_cols {
        return None;
    }
    if spans_rows {
        if overlap.x <= rect.x {
            let x = overlap.x + overlap.width;
            return Some(Rect::new(x, rect.y, rect.x + rect.width - x, rect.height));
        }
        if overlap.x + overlap.width >= rect.x + rect.width {
            return Some(Rect::new(rect.x, rect.y, overlap.x - rect.x, rect.height));
        }
    }
    if spans_cols {
        if overlap.y <= rect.y {
            let y = overlap.y + overlap.height;
            return Some(Rect::new(rect.x, y, rect.width, rect.y + rect.height - y));
        }
        if overlap.y + overlap.height >= rect.y + rect.height {
            return Some(Rect::new(rect.x, rect.y, rect.width, overlap.y - rect.y));
        }
    }
    None
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
/// destination picture is correct for it at every instant.
///
/// A pane that *shrank* contributes up to three, because two different things
/// are true of it at once. Its text rewrapped, so the destination picture shows
/// line breaks the pane is not yet the right shape for — that is the reflow
/// crossfade, over the area the pane keeps. And it has not yet given up the
/// area it is vacating — that is the strip, and it is what a split actually
/// looks like: the old picture holding its ground while the divider sweeps
/// across it.
///
/// The strip is opaque, and that is the whole point. Fading it instead —
/// which is what a single full-width crossfade does — leaves the old text and
/// the new pane's text both visible across the vacated area for the length of
/// the motion. That renders as doubled, half-transparent text and reads as a
/// dissolve rather than a split, because with every pane already at its
/// destination nothing in the frame is moving. Holding it opaque means the
/// frame is partitioned at every instant: new pane, old picture, new pane —
/// and the boundary between them is the divider, travelling.
fn place(
    change: PaneChange,
    motion: RoleSamples,
    claimed: &[Rect],
    vacated: &[VacatedStrip],
    out: &mut Vec<PanePlacement>,
) {
    let bounds = placed_bounds(change, motion);
    if let PaneChange::Persisted {
        window, from, to, ..
    } = change
    {
        // The pane's old picture over the area it keeps, fading out as the
        // destination underneath fades in. Anchored where the old picture
        // actually *is* -- never carried along with the travelling pane, which
        // is what made `delete-window` draw doubled, half-transparent text
        // across the whole frame: the ghost sampled the old picture at the
        // pane's old origin but drew it at the pane's current one, so the two
        // pictures showed the same buffer at two wrap widths from two
        // different offsets.
        //
        // A growing pane keeps everything it had, so its ghost is its whole old
        // rect standing still while the destination is revealed over it. It
        // must not be dropped: a delete-window is *only* visible because of it.
        // The survivor's own area is where the divider, the second mode line
        // and the reflowed text all change, and a pure geometric reveal leaves
        // that area showing the destination from the first frame -- while the
        // ground the survivor has not yet taken shows old and new pixels that
        // are identical. The whole change would snap on frame one.
        if grows(from, to) {
            out.push(PanePlacement {
                window,
                bounds: from,
                painted: from,
                content_origin: (from.x, from.y),
                source: neomacs_renderer_wgpu::PaneSource::Previous,
                opacity: motion.outgoing_opacity(),
            });
        } else if from.width - to.width > REFLOW_WIDTH_EPSILON
            || from.height - to.height > REFLOW_WIDTH_EPSILON
        {
            // Either axis. Rewrapping is not the only thing that changes when a
            // pane shrinks: a mode line moves with the bottom edge, a header
            // line with the top, fringes and margins with their sides. The
            // outgoing picture covers that chrome until the pane arrives, and
            // is needed on whichever axis gives ground.
            //
            // Cut into bands rather than blitted whole, so the chrome inside it
            // rides the interpolated edge instead of being clipped away while
            // its replacement dissolves in somewhere else. `ChromeInsets` say
            // how thick the bands are; at zero this collapses to a single patch
            // covering `bounds` and reproduces a plain whole-picture blit
            // exactly.
            let insets = insets_of(change);
            // Cut an axis only where the pane is actually giving ground on it.
            // An unchanged axis has chrome that does not move, so cutting it
            // would emit three spans describing one unbroken picture.
            //
            // Neither axis can be *growing* here: `grows` is checked first and
            // takes the branch above. That matters, because a growing axis has
            // no source pixels past its old end and the clamp inside `cut_axis`
            // would leave a gap for the backdrop to show through.
            let (lead_x, trail_x) = if from.width - to.width > REFLOW_WIDTH_EPSILON {
                (insets.left, insets.right)
            } else {
                (0.0, 0.0)
            };
            let (lead_y, trail_y) = if from.height - to.height > REFLOW_WIDTH_EPSILON {
                (insets.top, insets.bottom)
            } else {
                (0.0, 0.0)
            };
            // The area the pane keeps. Inside it a patch is crossfading into
            // its replacement; beyond it the pane simply has not let go yet, so
            // the old picture is opaque.
            let kept_right = bounds.x + bounds.width.min(to.width);
            let kept_bottom = bounds.y + bounds.height.min(to.height);
            let columns = split_spans_at(
                cut_axis(from.x, from.width, bounds.x, bounds.width, lead_x, trail_x),
                kept_right,
            );
            let rows = split_spans_at(
                cut_axis(
                    from.y,
                    from.height,
                    bounds.y,
                    bounds.height,
                    lead_y,
                    trail_y,
                ),
                kept_bottom,
            );
            for row in &rows {
                for column in &columns {
                    let inside_kept =
                        column.dest < kept_right - 0.01 && row.dest < kept_bottom - 0.01;
                    out.push(PanePlacement {
                        window,
                        bounds: Rect::new(column.dest, row.dest, column.extent, row.extent),
                        painted: Rect::new(column.dest, row.dest, column.extent, row.extent),
                        content_origin: (column.source, row.source),
                        source: neomacs_renderer_wgpu::PaneSource::Previous,
                        opacity: if inside_kept {
                            motion.outgoing_opacity()
                        } else {
                            1.0
                        },
                    });
                }
            }
        }
    }
    let Some(mut placement) = PanePlacement::at(change, motion, vacated) else {
        return;
    };
    if let PaneChange::Exited { from, .. } = change {
        let Some(left) = unclaimed(placement.bounds, claimed) else {
            return;
        };
        placement.content_origin = old_picture_origin(from, bounds, left);
        placement.bounds = left;
        placement.painted = left;
    }
    out.push(placement);
}

/// Where in the old picture the content drawn at `visible` came from.
///
/// The pane's own content is anchored to the pane, not to the screen: a pane
/// that moves carries its old pixels with it, so a point's old position is its
/// offset within the current rect, applied to where the rect used to be.
fn old_picture_origin(from: Rect, bounds: Rect, visible: Rect) -> (f32, f32) {
    (
        from.x + (visible.x - bounds.x),
        from.y + (visible.y - bounds.y),
    )
}

/// How far an entering pane still is from where it will settle.
///
/// A window created by a split is carved out of its neighbour, so it arrives
/// *through* the ground that neighbour is giving up: its leading edge rides the
/// vacated strip's edge, which is the divider. That ties its position to the
/// geometry motion rather than to its own curve -- deliberately, because two
/// clocks either side of a moving seam is exactly what tears one. `window-open`
/// shapes its opacity; the divider decides where it is.
///
/// Placing it at its destination instead is what made a split read as "the new
/// buffer was already there": the pane was uncovered in place, so nothing about
/// it ever moved and it never looked attached to the window it belongs to.
///
/// `(0, 0)` when no vacated strip touches it -- a window that appears without
/// anything shrinking to make room has nowhere to travel from, and fades in
/// where it belongs.
fn entry_offset(to: Rect, vacated: &[VacatedStrip]) -> (f32, f32) {
    vacated
        .iter()
        .find(|strip| {
            intersection(strip.bounds, to).is_some_and(|shared| {
                shared.width > REFLOW_WIDTH_EPSILON && shared.height > REFLOW_WIDTH_EPSILON
            })
        })
        .map_or((0.0, 0.0), |strip| match strip.axis {
            StripAxis::Horizontal => (strip.bounds.x + strip.bounds.width - to.x, 0.0),
            StripAxis::Vertical => (0.0, strip.bounds.y + strip.bounds.height - to.y),
        })
}

/// The four edge insets that make one window a nine-patch.
///
/// Derived from `outer` and `text_body`, the only two unconditional rects a
/// presentation publishes per window, because every optional band — fringe,
/// margin, scroll bar, divider, mode/header/tab line — is already inside one of
/// these four sums. Reading the twelve optional rects would add twelve `None`
/// cases and answer the same question.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(in crate::render_thread) struct ChromeInsets {
    top: f32,
    bottom: f32,
    left: f32,
    right: f32,
}

impl ChromeInsets {
    #[cfg(test)]
    pub(in crate::render_thread) const fn for_test(
        top: f32,
        bottom: f32,
        left: f32,
        right: f32,
    ) -> Self {
        Self {
            top,
            bottom,
            left,
            right,
        }
    }

    const ZERO: Self = Self {
        top: 0.0,
        bottom: 0.0,
        left: 0.0,
        right: 0.0,
    };
}

impl ChromeInsets {
    /// The insets of the picture being animated away.
    ///
    /// All zero for a window whose regions were never materialized. That is a
    /// correct answer rather than a fallback that loses the animation: at zero
    /// the cut yields one patch covering the whole rect, which is a plain
    /// whole-picture blit — exactly what this replaced.
    fn of(geometry: neomacs_display_protocol::frame_glyphs::PresentedWindowGeometry) -> Self {
        use neomacs_display_protocol::frame_glyphs::PresentedWindowGeometry as Geometry;
        let Geometry::Complete { regions, .. } = geometry else {
            return Self::default();
        };
        let (outer, body) = (regions.outer, regions.text_body);
        Self {
            top: (body.y - outer.y).max(0.0),
            bottom: (outer.y + outer.height - body.y - body.height).max(0.0),
            left: (body.x - outer.x).max(0.0),
            right: (outer.x + outer.width - body.x - body.width).max(0.0),
        }
    }
}

/// How thick this pane's chrome was in the picture being animated away.
const fn insets_of(change: PaneChange) -> ChromeInsets {
    match change {
        PaneChange::Persisted { insets, .. } => insets,
        // An entering pane has no old picture, and a leaving pane's old picture
        // is drawn at its old rect where its chrome is already correct.
        PaneChange::Entered { .. } | PaneChange::Exited { .. } => ChromeInsets::ZERO,
    }
}

/// One patch's extent on one axis: where it lands, and where it came from.
///
/// Within a span the mapping from destination to source is a pure translation,
/// which is what lets a span be split at an arbitrary boundary without
/// recomputing anything.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Span {
    dest: f32,
    source: f32,
    extent: f32,
}

/// Cut one axis of the outgoing picture into leading chrome, a clipped middle,
/// and trailing chrome.
///
/// This is the nine-patch. A window is not a uniform picture: its chrome is
/// anchored to its edges — a mode line and a horizontal scroll bar ride the
/// bottom, a header and tab line the top, fringes, margins, scroll bars and
/// dividers the sides — while only the text body between them reflows. Blitting
/// the whole old picture as one rectangle can only translate or clip all of it
/// together, so a shrinking pane's old mode line gets clipped away at the
/// bottom of the frame while its replacement dissolves in at the new edge. Cut
/// into bands, the old mode line *travels* to where the new one is and lands on
/// it.
///
/// `lead` and `trail` are the source's own thicknesses, used unchanged: the
/// blit is 1:1 and interpolating a chrome thickness would mean scaling glyphs.
/// Trailing wins a squeeze, which matches GNU — a window too short for its
/// chrome drops the header line before the mode line.
fn cut_axis(
    src: f32,
    src_extent: f32,
    dst: f32,
    dst_extent: f32,
    lead: f32,
    trail: f32,
) -> Vec<Span> {
    let lead = lead.min(dst_extent).min(src_extent).max(0.0);
    let trail = trail.min(dst_extent - lead).min(src_extent - lead).max(0.0);
    let middle = (dst_extent - lead - trail)
        .min(src_extent - lead - trail)
        .max(0.0);
    [
        Span {
            dest: dst,
            source: src,
            extent: lead,
        },
        Span {
            dest: dst + lead,
            source: src + lead,
            extent: middle,
        },
        Span {
            dest: dst + dst_extent - trail,
            source: src + src_extent - trail,
            extent: trail,
        },
    ]
    .into_iter()
    .filter(|span| span.extent > 0.0)
    .collect()
}

/// Split every span that straddles `boundary`, so each one is wholly on one
/// side of it.
///
/// The boundary is the edge of the area the pane keeps. Patches inside it are
/// crossfading into their replacement; patches beyond it are ground the pane
/// has not yet given up and are opaque. Splitting here means a patch never has
/// to carry two opacities.
fn split_spans_at(spans: Vec<Span>, boundary: f32) -> Vec<Span> {
    spans
        .into_iter()
        .flat_map(|span| {
            let offset = boundary - span.dest;
            if offset > 0.0 && offset < span.extent {
                vec![
                    Span {
                        extent: offset,
                        ..span
                    },
                    Span {
                        dest: boundary,
                        source: span.source + offset,
                        extent: span.extent - offset,
                    },
                ]
            } else {
                vec![span]
            }
        })
        .collect()
}

/// One rectangle of the old picture that a shrinking pane has not vacated.
struct VacatedStrip {
    bounds: Rect,
    content_origin: (f32, f32),
    axis: StripAxis,
}

/// Which way a pane gave ground up.
///
/// Kept because an entering pane arrives *through* the gap: the axis says
/// whether it slides in horizontally or vertically.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StripAxis {
    Horizontal,
    Vertical,
}

/// The strips of `bounds` that lie beyond what the pane will keep.
///
/// Anchored to the pane's *old* rect, not to the screen: the strip shows the
/// picture that was under this pane, so as a pane that both moves and shrinks
/// travels, the old text travels with it rather than standing still while the
/// pane slides out from under it.
fn vacated_strips(from: Rect, to: Rect, bounds: Rect) -> impl Iterator<Item = VacatedStrip> {
    let strip = move |rect: Rect, axis: StripAxis| VacatedStrip {
        bounds: rect,
        content_origin: old_picture_origin(from, bounds, rect),
        axis,
    };
    // Gated on the *change* being a shrink, not on the instantaneous width.
    // Reading only `bounds` meant a GROWING pane briefly measured wider than
    // its destination while overshooting, and published an opaque slab of
    // stale pre-change pixels -- drawn last, over the neighbour it had just
    // uncovered. This is also what the doc above already claims: the strips of
    // `bounds` beyond what the pane *will keep*.
    let shrinks_horizontally = from.width - to.width > REFLOW_WIDTH_EPSILON;
    let shrinks_vertically = from.height - to.height > REFLOW_WIDTH_EPSILON;
    let horizontal =
        (shrinks_horizontally && bounds.width - to.width > REFLOW_WIDTH_EPSILON).then(|| {
            strip(
                Rect::new(
                    bounds.x + to.width,
                    bounds.y,
                    bounds.width - to.width,
                    bounds.height,
                ),
                StripAxis::Horizontal,
            )
        });
    let vertical =
        (shrinks_vertically && bounds.height - to.height > REFLOW_WIDTH_EPSILON).then(|| {
            strip(
                Rect::new(
                    bounds.x,
                    bounds.y + to.height,
                    bounds.width,
                    bounds.height - to.height,
                ),
                StripAxis::Vertical,
            )
        });
    horizontal.into_iter().chain(vertical)
}

/// Every pane's placement for one frame, from one shared motion sample.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::render_thread) struct LayoutSample {
    pub(in crate::render_thread) panes: Vec<PanePlacement>,
    pub(in crate::render_thread) motion: RoleSamples,
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
                bounds: placement.painted,
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
                    placement.painted.x,
                    placement.painted.y,
                    placement.painted.width,
                    placement.painted.height,
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
