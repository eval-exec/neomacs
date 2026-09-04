//! Pure frame scheduling: typed frame demand, per-window coalescing, and
//! pacing decisions.
//!
//! Stage 1 of the frame scheduling plan
//! (docs/plans/2026-07-11-cross-platform-frame-scheduling-and-animation-architecture.md).
//!
//! This module has no winit, wgpu, or wall-clock dependency. Every method
//! takes `now` (or a [`FrameTick`]) explicitly, so scheduling decisions are
//! deterministic under test: tests anchor one real `Instant` and derive all
//! other times by `Duration` arithmetic.
//!
//! Semantics:
//! - Demand is declared before drawing. Rendering consumes a [`FramePlan`];
//!   it never latches "render me again" as a side effect.
//! - At most one redraw request is outstanding per native window; duplicate
//!   driving demands coalesce into that request.
//! - Deadline demands ([`Cadence::At`], [`Cadence::MaxRate`]) are keyed by
//!   [`DemandReason`]: resubmitting replaces the previous entry, so a caller
//!   can declare its standing demand every pass without accumulation, and
//!   [`FrameCoordinator::retract`] withdraws a reason that no longer applies.
//! - [`Cadence::MaxRate`] keeps a per-reason phase anchor: consuming a tick
//!   advances the anchor by whole periods, so an interleaved one-shot frame
//!   (e.g. an editor commit) never re-anchors an ambient cadence.
//! - Ineligible (occluded/hidden) windows retain demand but are never asked
//!   to present; regaining eligibility issues exactly one recovery request.

use std::collections::BTreeMap;
use std::num::NonZeroU16;
use std::time::{Duration, Instant};

use neomacs_display_protocol::frame_time::{EventTime, FrameSample};

bitflags::bitflags! {
    /// Broad retained composition groups. Deliberately coarse: one bit per
    /// retained group, not one bit per effect.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) struct LayerMask: u32 {
        const ROOT_CONTENT = 1 << 0;
        const CHILD_FRAMES = 1 << 1;
        const CURSOR_EFFECTS = 1 << 2;
        const TRANSIENT_OVERLAYS = 1 << 3;
        const CHROME = 1 << 4;
        const MEDIA = 1 << 5;
        const TRANSITIONS = 1 << 6;
        const FRAME_POST = 1 << 7;
    }
}

/// Damage granularity within a repainted layer. Begins as full-layer only;
/// rectangle lists arrive with retained-layer work. The interface carries
/// damage now so that full-layer repaint never hardens into an implicit
/// invariant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Damage {
    FullLayer,
}

impl Damage {
    fn combine(self, _other: Damage) -> Damage {
        Damage::FullLayer
    }
}

/// The least expensive category of work capable of producing correct pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Invalidation {
    #[default]
    None,
    /// Recompose existing layer content; sample dynamic state only.
    CompositeOnly { layers: LayerMask },
    /// Repaint the named layers, then compose.
    RepaintLayers { layers: LayerMask, damage: Damage },
    /// A new editor scene generation: rebuild static content.
    RebuildScene,
}

impl Invalidation {
    /// Strongest-wins merge. Equal-strength classes union their layers; a
    /// stronger class absorbs a weaker one because every presented frame
    /// composes all layers anyway.
    pub(crate) fn combine(self, other: Invalidation) -> Invalidation {
        use Invalidation::*;
        match (self, other) {
            (None, x) | (x, None) => x,
            (RebuildScene, _) | (_, RebuildScene) => RebuildScene,
            (
                RepaintLayers {
                    layers: a,
                    damage: da,
                },
                RepaintLayers {
                    layers: b,
                    damage: db,
                },
            ) => RepaintLayers {
                layers: a | b,
                damage: da.combine(db),
            },
            (r @ RepaintLayers { .. }, CompositeOnly { .. })
            | (CompositeOnly { .. }, r @ RepaintLayers { .. }) => r,
            (CompositeOnly { layers: a }, CompositeOnly { layers: b }) => {
                CompositeOnly { layers: a | b }
            }
        }
    }
}

/// When the demanded work should reach the screen.
// Interface variants/fields defined by the scheduling plan; consumed as
// later stages migrate effects onto the coordinator.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cadence {
    /// Fold into whatever frame happens next; never forces a frame.
    OnDemand,
    /// Present as soon as the presentation clock allows.
    NextPresentation,
    /// At most this many frames per second, phase-anchored per reason.
    MaxRate(NonZeroU16),
    /// At a specific deadline (blink timers, scheduled recovery).
    At(EventTime),
}

/// Declares [`DemandReason`] and everything indexed by it from a single list.
/// The variant set, `ALL`, `COUNT` and `name` all come from these lines, so a
/// new reason is one line and cannot leave a hand-maintained table behind.
macro_rules! demand_reasons {
    ($(
        $(#[$variant_meta:meta])*
        $variant:ident => $name:literal,
    )+) => {
        /// Why a frame is wanted. Diagnostic identity, not policy encoded as
        /// strings. Deadline demands are keyed by this, so each reason holds at
        /// most one scheduled deadline per window.
        // Interface variants/fields defined by the scheduling plan; consumed as
        // later stages migrate effects onto the coordinator.
        #[allow(dead_code)]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
        pub(crate) enum DemandReason {
            $($(#[$variant_meta])* $variant,)+
        }

        impl DemandReason {
            /// Every reason, in declaration order. The order is the
            /// counter/report order and matches the derived `Ord`.
            pub(crate) const ALL: [DemandReason; Self::COUNT] =
                [$(DemandReason::$variant,)+];

            /// Number of reasons: the width of [`DemandReason::ALL`] and of
            /// every per-reason counter array.
            pub(crate) const COUNT: usize = [$(DemandReason::$variant,)+].len();

            /// Stable snake_case name for diagnostics output.
            pub(crate) const fn name(self) -> &'static str {
                match self {
                    $(DemandReason::$variant => $name,)+
                }
            }
        }
    };
}

demand_reasons! {
    EditorCommit => "editor_commit",
    CursorAnimation => "cursor_animation",
    /// Infinite ambient compositor-only demand: the cursor color cycle
    /// (Stage 3 tracer bullet). Distinct from CursorAnimation so its MaxRate
    /// phase anchor cannot collide with the blink deadline.
    CursorColorCycle => "cursor_color_cycle",
    FiniteEffect => "finite_effect",
    Transition => "transition",
    Video => "video",
    WebKit => "webkit",
    /// Animated shader surfaces visible in a composited frame
    /// (docs/display-engine/SHADER_SURFACES.md).
    ShaderSurface => "shader_surface",
    /// Installed full-frame post shader whose time uniforms require a fresh
    /// composite even when the editor scene is unchanged.
    FrameShader => "frame_shader",
    Terminal => "terminal",
    Expose => "expose",
    /// A tick the coordinator did not ask for: the platform invalidated the
    /// surface (expose, resize, first map) or a runtime recovery path called
    /// request_redraw on the window directly. Distinct from Expose, which
    /// attributes the coordinator's own re-queue of work a present failed to
    /// deliver.
    PlatformRedraw => "platform_redraw",
    DebugCapture => "debug_capture",
    /// New editor content or blink toggle needing a repaint.
    Redisplay => "redisplay",
    /// Render-effect families (Stage 6). Each names the group animating so
    /// diagnostics can answer "why is this window still rendering?" without
    /// per-effect logging.
    CursorEffect => "cursor_effect",
    WindowEffect => "window_effect",
    TextEffect => "text_effect",
    ScrollEffect => "scroll_effect",
    DecorativeEffect => "decorative_effect",
    TransientEffect => "transient_effect",
}

impl DemandReason {
    /// Index into [`DemandReason::ALL`] / the per-reason counter arrays. The
    /// enum is fieldless with default discriminants, so the cast is the
    /// declaration position, which is `ALL`'s order by construction; density is
    /// pinned by `demand_reason_indices_are_dense`.
    pub(crate) const fn index(self) -> usize {
        self as usize
    }
}

/// Set of [`DemandReason`]s, carried by value on a [`FramePlan`] so a frame can
/// be attributed to what asked for it ("why did this present happen?").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct DemandReasonSet(u32);

impl DemandReasonSet {
    pub(crate) const fn empty() -> Self {
        DemandReasonSet(0)
    }

    fn insert(&mut self, reason: DemandReason) {
        self.0 |= 1 << reason.index();
    }

    pub(crate) fn contains(self, reason: DemandReason) -> bool {
        self.0 & (1 << reason.index()) != 0
    }

    pub(crate) fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Reasons in [`DemandReason::ALL`] order.
    pub(crate) fn iter(self) -> impl Iterator<Item = DemandReason> {
        DemandReason::ALL
            .into_iter()
            .filter(move |r| self.contains(*r))
    }
}

impl FromIterator<DemandReason> for DemandReasonSet {
    fn from_iter<I: IntoIterator<Item = DemandReason>>(iter: I) -> Self {
        let mut set = DemandReasonSet::empty();
        for reason in iter {
            set.insert(reason);
        }
        set
    }
}

/// A declaration that pixels need to change, with reason, scope, and cadence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FrameDemand {
    pub invalidation: Invalidation,
    pub cadence: Cadence,
    pub reason: DemandReason,
}

// Interface variants/fields defined by the scheduling plan; consumed as
// later stages migrate effects onto the coordinator.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClockSource {
    Native,
    Synthetic,
}

/// One opportunity to produce a frame: timing input, not an instruction to
/// rebuild editor state.
// Interface variants/fields defined by the scheduling plan; consumed as
// later stages migrate effects onto the coordinator.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub(crate) struct FrameTick {
    pub frame_time: EventTime,
    pub target_presentation_time: EventTime,
    pub estimated_interval: Duration,
    pub source: ClockSource,
}

// Interface defined by the temporal-presentation plan; consumed when the
// renderer boundary is widened from a bare Instant to a FrameSample.
#[allow(dead_code)]
impl FrameTick {
    /// The portable view of this tick, for code outside the scheduler.
    ///
    /// `FrameTick` is the scheduler's own richer record (it also carries the
    /// clock source). [`FrameSample`] is what sampling code needs and is
    /// nameable from `neomacs-renderer-wgpu`, which cannot see this type.
    pub(crate) fn sample(self) -> FrameSample {
        FrameSample::new(self.frame_time, self.estimated_interval)
    }
}

/// The scheduler's decision about what work one tick performs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RenderWork {
    None,
    CompositeOnly { layers: LayerMask },
    RepaintLayers { layers: LayerMask, damage: Damage },
    RebuildScene,
}

impl RenderWork {
    fn from_invalidation(inv: Invalidation) -> RenderWork {
        match inv {
            Invalidation::None => RenderWork::None,
            Invalidation::CompositeOnly { layers } => RenderWork::CompositeOnly { layers },
            Invalidation::RepaintLayers { layers, damage } => {
                RenderWork::RepaintLayers { layers, damage }
            }
            Invalidation::RebuildScene => RenderWork::RebuildScene,
        }
    }

    fn to_invalidation(self) -> Invalidation {
        match self {
            RenderWork::None => Invalidation::None,
            RenderWork::CompositeOnly { layers } => Invalidation::CompositeOnly { layers },
            RenderWork::RepaintLayers { layers, damage } => {
                Invalidation::RepaintLayers { layers, damage }
            }
            RenderWork::RebuildScene => Invalidation::RebuildScene,
        }
    }
}

/// Pure decision for one tick of one window.
// Interface variants/fields defined by the scheduling plan; consumed as
// later stages migrate effects onto the coordinator.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub(crate) struct FramePlan {
    pub tick: FrameTick,
    pub work: RenderWork,
    pub should_present: bool,
    /// Which demands this frame satisfies. Attribution only — the work class
    /// already encodes what must be drawn.
    pub reasons: DemandReasonSet,
}

/// What the caller should do next for this window. The event loop executes
/// these through a narrow winit adapter; it never derives `ControlFlow` from
/// individual effect fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PacingAction {
    /// Nothing to schedule for this window.
    Sleep,
    /// Ask the platform for one redraw of this window.
    RequestRedraw,
    /// Arm a wake at this deadline (the loop aggregates the earliest).
    WakeAt(EventTime),
}

/// An instant the loop may block until, proven strictly later than the moment
/// the schedule was serviced.
///
/// The field is private and the only constructor is
/// [`FrameCoordinator::service_deadlines`], which first turns every ripe
/// deadline into work. That makes "arm a wait for a deadline that has already
/// elapsed" unrepresentable rather than merely discouraged: the event loop has
/// no way to obtain an `Instant` from the coordinator without servicing first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FutureDeadline(EventTime);

impl FutureDeadline {
    /// The deadline as an [`EventTime`].
    // Used by scheduler tests today; production callers move off the raw
    // `instant()` accessor as the loop's wait path is converted.
    #[allow(dead_code)]
    pub(crate) fn event_time(self) -> EventTime {
        self.0
    }

    /// The deadline as a raw `Instant`, for winit's `ControlFlow::WaitUntil`.
    ///
    /// ADAPTER BOUNDARY ONLY: this is the one foreign API that needs a bare
    /// `Instant`. Nothing else may unwrap a deadline.
    pub(crate) fn instant(self) -> Instant {
        self.0.into_instant()
    }
}

/// How long the event loop may sleep once the schedule has been serviced.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum LoopWake {
    /// No frame deadline: as far as frame demand is concerned the loop may
    /// block indefinitely and wait for an external event.
    #[default]
    Idle,
    /// Wake at this instant, which is strictly in the future.
    At(FutureDeadline),
}

/// The result of servicing the schedule for one event-loop pass: the redraw
/// requests the ripe frame deadlines turned into, whether native video must
/// be serviced again, and the next wake.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct DeadlineService {
    /// Windows needing exactly one platform redraw request, in window order.
    pub redraw: Vec<NativeWindowId>,
    /// A decoder-service deadline became ripe during this event-loop pass.
    ///
    /// This is service work, not permission to render. The event loop must
    /// run the video service at the same `now`, reconcile the returned future
    /// deadline, and service the coordinator once more before it sleeps.
    pub video_service_due: bool,
    /// The loop's next wake deadline.
    pub wake: LoopWake,
}

/// Presentation outcome, fed back as scheduling input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PresentResult {
    Presented,
    /// The native window exists but no editor presentation has reached it yet.
    /// Content ingestion is the producer for the next demand, so retrying an
    /// expose here would create demand with no state change capable of
    /// satisfying it.
    AwaitingContent,
    /// Nothing was rendered (no surface yet, warm-up, etc.); the plan's work
    /// was not shown and is re-queued.
    Skipped,
    Occluded,
    SurfaceLost,
    Timeout,
}

/// Visibility/focus state relevant to presentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WindowPresentationState {
    pub visible: bool,
    pub occluded: bool,
    pub focused: bool,
}

impl Default for WindowPresentationState {
    fn default() -> Self {
        Self {
            visible: true,
            occluded: false,
            focused: true,
        }
    }
}

/// A native top-level window with its own surface and presentation
/// lifecycle. Child Emacs frames composite into a parent and share its
/// clock; callers map child demand to the parent id before submitting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct NativeWindowId(pub u64);

/// Bounded backoff after a surface acquisition timeout (invariant: surface
/// failure cannot produce an immediate retry storm).
const TIMEOUT_BACKOFF: Duration = Duration::from_millis(50);

/// Return the first point on `anchor + n * period` strictly after `now`.
///
/// A compositor may resume after hours or days of suspension. Advancing one
/// period at a time makes wake-up work proportional to every frame that was
/// deliberately skipped; remainder arithmetic preserves the same phase in
/// constant time.
fn next_max_rate_phase(anchor: EventTime, now: EventTime, period: Duration) -> EventTime {
    debug_assert!(anchor <= now);
    let elapsed = now.saturating_since(anchor);
    let remainder_nanos = elapsed.as_nanos() % period.as_nanos();
    let until_next = if remainder_nanos == 0 {
        period
    } else {
        period - Duration::from_nanos(remainder_nanos as u64)
    };
    now.plus(until_next)
}

#[derive(Debug)]
struct ScheduledDemand {
    reason: DemandReason,
    at: EventTime,
    invalidation: Invalidation,
    /// MaxRate period for phase-anchored rescheduling; None for At().
    period: Option<Duration>,
}

#[derive(Debug, Clone, Copy)]
struct MaxRateAnchor {
    reason: DemandReason,
    at: EventTime,
    period: Duration,
}

#[derive(Debug, Default)]
struct DueDemand {
    invalidation: Invalidation,
    /// Whether the due work by itself justifies requesting a frame.
    /// OnDemand contributions record work without driving.
    driving: bool,
    reasons: DemandReasonSet,
}

impl DueDemand {
    fn merge(&mut self, invalidation: Invalidation, driving: bool, reason: DemandReason) {
        // A demand that requires no work is not demand; merging it must not
        // set the driving flag or record a reason.
        if invalidation == Invalidation::None {
            return;
        }
        self.invalidation = self.invalidation.combine(invalidation);
        self.driving |= driving;
        self.reasons.insert(reason);
    }

    fn is_empty(&self) -> bool {
        self.invalidation == Invalidation::None
    }

    fn take(&mut self) -> DueDemand {
        std::mem::take(self)
    }
}

#[derive(Debug, Default)]
struct WindowSched {
    presentation: WindowPresentationState,
    /// One outstanding platform redraw request (coalescing token).
    request_pending: bool,
    /// Demand consumed by the next begin_frame.
    due: DueDemand,
    /// Future deadline demands, at most one per reason.
    scheduled: Vec<ScheduledDemand>,
    /// Phase anchors for MaxRate reasons: next allowed fire time.
    max_rate_anchor: Vec<MaxRateAnchor>,
}

impl WindowSched {
    fn eligible(&self) -> bool {
        self.presentation.visible && !self.presentation.occluded
    }

    fn earliest_deadline(&self) -> Option<EventTime> {
        self.scheduled.iter().map(|s| s.at).min()
    }

    fn has_any_demand(&self) -> bool {
        !self.due.is_empty() || !self.scheduled.is_empty()
    }

    fn anchor_for(&self, reason: DemandReason) -> Option<MaxRateAnchor> {
        self.max_rate_anchor
            .iter()
            .find(|anchor| anchor.reason == reason)
            .copied()
    }

    fn set_anchor(&mut self, reason: DemandReason, at: EventTime, period: Duration) {
        if let Some(anchor) = self
            .max_rate_anchor
            .iter_mut()
            .find(|anchor| anchor.reason == reason)
        {
            anchor.at = at;
            anchor.period = period;
        } else {
            self.max_rate_anchor
                .push(MaxRateAnchor { reason, at, period });
        }
    }

    fn clear_schedule(&mut self, reason: DemandReason) {
        self.scheduled.retain(|demand| demand.reason != reason);
    }

    fn schedule(&mut self, demand: ScheduledDemand) {
        if let Some(existing) = self
            .scheduled
            .iter_mut()
            .find(|s| s.reason == demand.reason)
        {
            *existing = demand;
        } else {
            self.scheduled.push(demand);
        }
    }
}

/// Owner of the policy connecting visual demand to presentation, per native
/// window. Pure: no timers, no platform calls; the runtime executes the
/// returned [`PacingAction`]s and feeds ticks and present results back.
#[derive(Debug, Default)]
pub(crate) struct FrameCoordinator {
    windows: BTreeMap<NativeWindowId, WindowSched>,
    /// Decoder pull/service wake. This is intentionally separate from frame
    /// demand: reaching it services native media state, but does not authorize
    /// a repaint until the video system publishes a ready frame.
    video_service_deadline: Option<EventTime>,
}

impl FrameCoordinator {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    fn window(&mut self, id: NativeWindowId) -> &mut WindowSched {
        self.windows.entry(id).or_default()
    }

    pub(crate) fn remove_window(&mut self, id: NativeWindowId) {
        self.windows.remove(&id);
    }

    /// Replace the decoder's one outstanding service deadline. `None`
    /// withdraws it when no presented session needs another native pull.
    #[cfg(any(feature = "video", test))]
    pub(crate) fn reconcile_video_service_deadline(&mut self, deadline: Option<EventTime>) {
        self.video_service_deadline = deadline;
    }

    /// Convert one newly sampled video frame into compositor damage for the
    /// native window that presents it.
    ///
    /// Decoder service and frame authorization are separate domains: a media
    /// deadline only asks the decoder to run, while this operation is the
    /// explicit boundary that grants a repaint after sampling succeeded.
    #[cfg(any(feature = "video", test))]
    pub(crate) fn submit_ready_video_frame(
        &mut self,
        id: NativeWindowId,
        now: EventTime,
    ) -> PacingAction {
        self.submit_demand(
            id,
            FrameDemand {
                invalidation: Invalidation::RepaintLayers {
                    layers: LayerMask::MEDIA,
                    damage: Damage::FullLayer,
                },
                cadence: Cadence::NextPresentation,
                reason: DemandReason::Video,
            },
            now,
        )
    }

    /// Drop scheduling state for windows that no longer exist, so a
    /// destroyed window's deadlines cannot keep waking the loop.
    pub(crate) fn prune_windows(&mut self, keep: impl Fn(NativeWindowId) -> bool) {
        self.windows.retain(|id, _| keep(*id));
    }

    /// Declare demand. Returns the immediate action; duplicate driving
    /// demands coalesce into one outstanding request per window.
    pub(crate) fn submit_demand(
        &mut self,
        id: NativeWindowId,
        demand: FrameDemand,
        now: EventTime,
    ) -> PacingAction {
        if demand.invalidation == Invalidation::None {
            return PacingAction::Sleep;
        }
        tracing::trace!(
            target: "neomacs::demand_trace",
            "submit id={id:?} reason={:?} cadence={:?} inval={:?}",
            demand.reason,
            demand.cadence,
            demand.invalidation
        );
        let ws = self.window(id);
        match demand.cadence {
            Cadence::OnDemand => {
                ws.due.merge(demand.invalidation, false, demand.reason);
                PacingAction::Sleep
            }
            Cadence::NextPresentation => {
                ws.due.merge(demand.invalidation, true, demand.reason);
                Self::drive(ws)
            }
            Cadence::At(at) if at <= now => {
                // This declaration consumes the deadline, so an earlier
                // record of the same reason must go with it: left behind it
                // is permanently ripe, and every pass would re-arm an
                // already-elapsed wake. Same hazard the MaxRate
                // expired-anchor branch below clears explicitly.
                ws.clear_schedule(demand.reason);
                ws.due.merge(demand.invalidation, true, demand.reason);
                Self::drive(ws)
            }
            Cadence::At(at) => {
                ws.schedule(ScheduledDemand {
                    reason: demand.reason,
                    at,
                    invalidation: demand.invalidation,
                    period: None,
                });
                PacingAction::WakeAt(at)
            }
            Cadence::MaxRate(hz) => {
                let period = Duration::from_secs_f64(1.0 / f64::from(hz.get()));
                match ws.anchor_for(demand.reason) {
                    // First submission (or anchor already reached): fire now
                    // and anchor the phase grid at now + period.
                    None => {
                        ws.set_anchor(demand.reason, now.plus(period), period);
                        ws.due.merge(demand.invalidation, true, demand.reason);
                        Self::drive(ws)
                    }
                    // Configuration changed: discard the old deadline, fire
                    // once now, and establish the new phase grid immediately.
                    Some(anchor) if anchor.period != period => {
                        ws.clear_schedule(demand.reason);
                        ws.set_anchor(demand.reason, now.plus(period), period);
                        ws.due.merge(demand.invalidation, true, demand.reason);
                        Self::drive(ws)
                    }
                    Some(anchor) if anchor.at <= now => {
                        let next = next_max_rate_phase(anchor.at, now, period);
                        // This declaration itself consumes the expired phase.
                        // Remove the matching WaitUntil record so begin_frame
                        // cannot consume it again against the newly-future
                        // anchor.
                        ws.clear_schedule(demand.reason);
                        ws.set_anchor(demand.reason, next, period);
                        ws.due.merge(demand.invalidation, true, demand.reason);
                        Self::drive(ws)
                    }
                    // Anchor in the future: schedule on the existing phase
                    // grid; resubmission is idempotent.
                    Some(anchor) => {
                        ws.schedule(ScheduledDemand {
                            reason: demand.reason,
                            at: anchor.at,
                            invalidation: demand.invalidation,
                            period: Some(period),
                        });
                        PacingAction::WakeAt(anchor.at)
                    }
                }
            }
        }
    }

    fn drive(ws: &mut WindowSched) -> PacingAction {
        if !ws.eligible() {
            return PacingAction::Sleep;
        }
        if ws.request_pending {
            return PacingAction::Sleep;
        }
        ws.request_pending = true;
        PacingAction::RequestRedraw
    }

    /// Withdraw a reason's demand (effect disabled, timer cancelled). Due
    /// work already merged from that reason stays merged; only its standing
    /// deadline and phase anchor are dropped.
    pub(crate) fn retract(&mut self, id: NativeWindowId, reason: DemandReason) {
        let ws = self.window(id);
        ws.scheduled.retain(|s| s.reason != reason);
        ws.max_rate_anchor.retain(|anchor| anchor.reason != reason);
    }

    /// Consume demand for one tick and decide the work.
    pub(crate) fn begin_frame(&mut self, id: NativeWindowId, tick: FrameTick) -> FramePlan {
        let ws = self.window(id);
        // This tick satisfies the outstanding request, whether it came from
        // our request or was platform-initiated (resize, expose).
        ws.request_pending = false;

        // Fold every ripe scheduled deadline into the due work. A very late
        // tick consumes the whole backlog as one plan; MaxRate anchors
        // advance by whole periods (from the scheduled record's own deadline,
        // not from mutable anchor state) so the phase grid survives.
        Self::collect_ripe(ws, tick.frame_time);

        if !ws.eligible() {
            // Retain demand; never present while ineligible.
            return FramePlan {
                tick,
                work: RenderWork::None,
                should_present: false,
                reasons: DemandReasonSet::empty(),
            };
        }

        let due = ws.due.take();
        let work = RenderWork::from_invalidation(due.invalidation);
        FramePlan {
            tick,
            work,
            should_present: work != RenderWork::None,
            reasons: due.reasons,
        }
    }

    /// Grant a frame for a platform-delivered redraw that no demand explains.
    ///
    /// The caller — and only the caller — knows a tick arrived from the window
    /// system rather than from a deadline of ours. Such a tick means the
    /// surface was invalidated (expose, resize, first map) or a runtime
    /// recovery path asked the window for a repaint directly (device loss, GPU
    /// reconfigure, both of which bypass the coordinator). Declining it leaves
    /// stale or absent content on screen, so the frame is granted and named:
    /// every present carries a demand reason (architectural invariant 12).
    ///
    /// Nothing is retained across a surface invalidation, hence a full repaint.
    /// Callers reach this only after [`begin_frame`](Self::begin_frame) planned
    /// no work, so it can never downgrade a real demand's work class.
    pub(crate) fn platform_redraw_plan(
        &mut self,
        id: NativeWindowId,
        tick: FrameTick,
    ) -> FramePlan {
        let ws = self.window(id);
        if !ws.eligible() {
            return FramePlan {
                tick,
                work: RenderWork::None,
                should_present: false,
                reasons: DemandReasonSet::empty(),
            };
        }
        FramePlan {
            tick,
            work: RenderWork::RepaintLayers {
                layers: LayerMask::all(),
                damage: Damage::FullLayer,
            },
            should_present: true,
            reasons: [DemandReason::PlatformRedraw].into_iter().collect(),
        }
    }

    /// Record the presentation outcome and decide the next action.
    pub(crate) fn finish_frame(
        &mut self,
        id: NativeWindowId,
        plan: &FramePlan,
        result: PresentResult,
        now: EventTime,
    ) -> PacingAction {
        let ws = self.window(id);
        match result {
            PresentResult::Presented => {}
            PresentResult::AwaitingContent => {
                // The plan has been consumed.  A committed frame arriving on
                // the display channel will submit Redisplay and drive the next
                // presentation; there is nothing useful to retry before then.
            }
            PresentResult::Skipped => {
                // The plan's work never reached the screen; re-queue it.
                ws.due
                    .merge(plan.work.to_invalidation(), true, DemandReason::Expose);
            }
            PresentResult::Occluded => {
                ws.presentation.occluded = true;
                ws.due
                    .merge(plan.work.to_invalidation(), true, DemandReason::Expose);
                return PacingAction::Sleep;
            }
            PresentResult::SurfaceLost => {
                // Retained content is gone; a full repaint is required once
                // the runtime reconfigures the surface.
                ws.due.merge(
                    Invalidation::RepaintLayers {
                        layers: LayerMask::all(),
                        damage: Damage::FullLayer,
                    },
                    true,
                    DemandReason::Expose,
                );
                ws.due
                    .merge(plan.work.to_invalidation(), true, DemandReason::Expose);
                return Self::drive(ws);
            }
            PresentResult::Timeout => {
                // Bounded retry; never an immediate spin. The retry is
                // scheduled (not just returned) so next_wake_deadline() keeps
                // the recovery alive even if the caller drops the action.
                let invalidation = plan.work.to_invalidation();
                if invalidation != Invalidation::None {
                    ws.schedule(ScheduledDemand {
                        reason: DemandReason::Expose,
                        at: now.plus(TIMEOUT_BACKOFF),
                        invalidation,
                        period: None,
                    });
                }
                return PacingAction::WakeAt(now.plus(TIMEOUT_BACKOFF));
            }
        }
        if ws.due.driving && !ws.due.is_empty() {
            return Self::drive(ws);
        }
        match ws.earliest_deadline() {
            Some(at) => PacingAction::WakeAt(at),
            None => PacingAction::Sleep,
        }
    }

    /// Update visibility/focus/occlusion wholesale. Regaining eligibility with
    /// retained demand issues exactly one recovery request.
    // Full-replace form kept for initialization/tests; the runtime drives
    // per-field transitions through set_occluded/set_focused/set_visible.
    #[allow(dead_code)]
    pub(crate) fn update_window_state(
        &mut self,
        id: NativeWindowId,
        state: WindowPresentationState,
    ) -> PacingAction {
        self.mutate_presentation(id, |p| *p = state)
    }

    /// Mark a window occluded or exposed. Exposure with retained demand
    /// issues one recovery request; occlusion suspends presentation.
    pub(crate) fn set_occluded(&mut self, id: NativeWindowId, occluded: bool) -> PacingAction {
        self.mutate_presentation(id, |p| p.occluded = occluded)
    }

    /// Mark a window minimized/hidden or shown. Same eligibility semantics as
    /// occlusion.
    // Wired when a minimize/hide event source lands (plan Stage 7 policy);
    // Occluded already covers the Wayland/macOS not-showing case. Covered by
    // the scheduler tests.
    #[allow(dead_code)]
    pub(crate) fn set_visible(&mut self, id: NativeWindowId, visible: bool) -> PacingAction {
        self.mutate_presentation(id, |p| p.visible = visible)
    }

    /// Update focus. Focus does not gate presentation, but it is scheduling
    /// input for ambient-effect policy (plan: visibility and power policy).
    pub(crate) fn set_focused(&mut self, id: NativeWindowId, focused: bool) -> PacingAction {
        self.mutate_presentation(id, |p| p.focused = focused)
    }

    /// Whether a window is eligible to present (visible and not occluded).
    pub(crate) fn is_eligible(&self, id: NativeWindowId) -> bool {
        self.windows
            .get(&id)
            .map(|ws| ws.eligible())
            .unwrap_or(true)
    }

    /// Whether a window is focused. Unknown windows default to focused (a
    /// window that has never reported focus should not have ambient effects
    /// suppressed).
    pub(crate) fn is_focused(&self, id: NativeWindowId) -> bool {
        self.windows
            .get(&id)
            .map(|ws| ws.presentation.focused)
            .unwrap_or(true)
    }

    fn mutate_presentation(
        &mut self,
        id: NativeWindowId,
        f: impl FnOnce(&mut WindowPresentationState),
    ) -> PacingAction {
        let ws = self.window(id);
        let was_eligible = ws.eligible();
        f(&mut ws.presentation);
        let now_eligible = ws.eligible();
        if was_eligible && !now_eligible {
            // Any outstanding redraw request is void once the surface is no
            // longer presentable: platforms may drop a pending request when a
            // window is occluded/hidden and never redeliver it on exposure.
            // Clearing it lets the exposure transition issue a fresh one.
            ws.request_pending = false;
        }
        if !was_eligible && now_eligible && ws.has_any_demand() {
            return Self::drive(ws);
        }
        PacingAction::Sleep
    }

    /// Fold every deadline that has come due into the window's due work,
    /// exactly as [`begin_frame`](Self::begin_frame) does for a tick.
    fn collect_ripe(ws: &mut WindowSched, now: EventTime) -> bool {
        let mut ripe = false;
        let mut i = 0;
        while i < ws.scheduled.len() {
            if ws.scheduled[i].at <= now {
                let ScheduledDemand {
                    reason,
                    at,
                    invalidation,
                    period,
                } = ws.scheduled.swap_remove(i);
                ws.due.merge(invalidation, true, reason);
                if let Some(period) = period {
                    ws.set_anchor(reason, next_max_rate_phase(at, now, period), period);
                }
                ripe = true;
            } else {
                i += 1;
            }
        }
        ripe
    }

    /// Service the schedule for one event-loop pass and report the next wake.
    ///
    /// Every ripe frame deadline becomes due work and one platform redraw
    /// request. A ripe decoder-service deadline requests another native-video
    /// service pass without inventing frame work. The returned [`LoopWake`] is
    /// therefore either `Idle` or an instant strictly after `now`. This is
    /// GNU's rule:
    /// `timer_check` runs every ripe timer and loops until the next fire time
    /// is non-zero (keyboard.c:4911-4945) before that value is handed to
    /// `pselect` as the wait (process.c:5490). A deadline used as a timeout
    /// without being run is a zero wait, and a zero wait every pass is a busy
    /// spin.
    ///
    /// This is the only way to obtain a wake instant from the coordinator:
    /// [`FutureDeadline`]'s field is private, so the "arm what you have not
    /// serviced" mistake cannot be written.
    pub(crate) fn service_deadlines(&mut self, now: EventTime) -> DeadlineService {
        let mut redraw = Vec::new();
        for (id, ws) in &mut self.windows {
            if Self::collect_ripe(ws, now) && Self::drive(ws) == PacingAction::RequestRedraw {
                redraw.push(*id);
            }
        }
        let video_service_due = self
            .video_service_deadline
            .is_some_and(|deadline| deadline <= now);
        if video_service_due {
            // Transfer ownership of the ripe deadline to the typed service
            // result. The caller must replace it with the decoder's newly
            // computed future deadline before sleeping.
            self.video_service_deadline = None;
        }
        let wake = match self.next_wake_deadline() {
            // Post-condition of the fold above: nothing at or before `now`
            // survives in an eligible window's schedule.
            Some(at) => {
                debug_assert!(at > now, "a serviced deadline cannot still be ripe");
                LoopWake::At(FutureDeadline(at))
            }
            None => LoopWake::Idle,
        };
        DeadlineService {
            redraw,
            video_service_due,
            wake,
        }
    }

    /// Earliest scheduled frame or decoder-service deadline. Private: the
    /// event loop reaches it only through
    /// [`service_deadlines`](Self::service_deadlines), which guarantees the
    /// value is not already elapsed.
    fn next_wake_deadline(&self) -> Option<EventTime> {
        let frame_deadline = self
            .windows
            .values()
            .filter(|ws| ws.eligible())
            .filter_map(|ws| ws.earliest_deadline())
            .min();
        match (frame_deadline, self.video_service_deadline) {
            (Some(frame), Some(video)) => Some(frame.min(video)),
            (Some(deadline), None) | (None, Some(deadline)) => Some(deadline),
            (None, None) => None,
        }
    }

    /// Active demand reasons for diagnostics ("why is this window still
    /// rendering?").
    #[cfg(test)]
    pub(crate) fn active_reasons(&self, id: NativeWindowId) -> Vec<DemandReason> {
        self.window_demand()
            .find(|(wid, _)| *wid == id)
            .map(|(_, set)| set.iter().collect())
            .unwrap_or_default()
    }

    /// Every tracked window with its currently-active demand reasons: due
    /// one-shots plus standing scheduled deadlines (plan: Observability,
    /// "active demand reasons" per native window). The render loop publishes
    /// this into the per-window diagnostics counters after each demand
    /// reconciliation; it is a read of existing state and schedules nothing.
    pub(crate) fn window_demand(
        &self,
    ) -> impl Iterator<Item = (NativeWindowId, DemandReasonSet)> + '_ {
        self.windows.iter().map(|(id, ws)| {
            let mut reasons = ws.due.reasons;
            for s in &ws.scheduled {
                reasons.insert(s.reason);
            }
            (*id, reasons)
        })
    }

    /// Whether a redraw request is outstanding for this window.
    #[cfg(test)]
    pub(crate) fn request_pending(&self, id: NativeWindowId) -> bool {
        self.windows
            .get(&id)
            .map(|ws| ws.request_pending)
            .unwrap_or(false)
    }
}

#[cfg(test)]
impl FrameCoordinator {
    /// Test-only view of the raw earliest deadline. Runtime code cannot reach
    /// it: only [`FrameCoordinator::service_deadlines`] yields a wake, and only
    /// after the ripe deadlines have become work.
    pub(super) fn next_wake_deadline_unserviced(&self) -> Option<EventTime> {
        self.next_wake_deadline()
    }
}

#[cfg(test)]
#[path = "frame_sched_test.rs"]
mod frame_sched_test;
