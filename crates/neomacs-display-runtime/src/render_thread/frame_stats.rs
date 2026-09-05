//! Frame-scheduling evidence counters (Stage 0 of the frame scheduling plan,
//! docs/plans/2026-07-11-cross-platform-frame-scheduling-and-animation-architecture.md).
//!
//! Process-wide monotonic counters incremented from the render thread's
//! scheduling and presentation hot points. They change no policy; they exist
//! so scheduling changes can be judged by structural evidence (wakeups,
//! requests, commits, glyph-build passes, presents, commit-to-present
//! latency) instead of by CPU percentages alone.
//!
//! All counters are relaxed atomics: every writer runs on the render thread,
//! and readers only need eventually-consistent totals.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use neomacs_display_protocol::frame_time::EventTime;

use super::frame_sched::{DemandReason, DemandReasonSet, FramePlan, NativeWindowId, RenderWork};

/// One event-loop iteration (about_to_wait or user-event wake).
pub(super) static EVENT_LOOP_WAKEUPS: AtomicU64 = AtomicU64::new(0);
/// Redraw requests issued to winit windows.
pub(super) static REDRAW_REQUESTS: AtomicU64 = AtomicU64::new(0);
/// Redraw requests the loop issued because a scheduled deadline came due
/// rather than because a producer re-declared its demand. Proof that the
/// deadline-service step engages: demands with no per-pass producer (the
/// bounded Expose retry after a present produces nothing) reach the screen
/// only through this path, and before it existed their elapsed deadline was
/// re-armed as a zero-length wait on every pass -- a busy spin.
pub(super) static DEADLINE_SERVICED_REDRAWS: AtomicU64 = AtomicU64::new(0);
/// RedrawRequested events received from winit.
pub(super) static REDRAW_EVENTS: AtomicU64 = AtomicU64::new(0);
/// Editor scene commits ingested from the evaluator channel.
pub(super) static SCENE_COMMITS: AtomicU64 = AtomicU64::new(0);
/// Root glyph build/render passes (the expensive static-scene path).
pub(super) static ROOT_GLYPH_PASSES: AtomicU64 = AtomicU64::new(0);
/// Surface presents on top-level frame windows.
pub(super) static SURFACE_PRESENTS: AtomicU64 = AtomicU64::new(0);
/// Frame plans by render work class (frame scheduling plan, Stage 2).
pub(super) static PLAN_NONE: AtomicU64 = AtomicU64::new(0);
pub(super) static PLAN_COMPOSITE_ONLY: AtomicU64 = AtomicU64::new(0);
pub(super) static PLAN_REPAINT_LAYERS: AtomicU64 = AtomicU64::new(0);
pub(super) static PLAN_REBUILD_SCENE: AtomicU64 = AtomicU64::new(0);
/// Retained static-scene rebuilds — one per scene generation while the
/// cursor-only fast path is active (frame scheduling plan, Stage 4).
pub(super) static RETAINED_STATIC_BUILDS: AtomicU64 = AtomicU64::new(0);
/// Frames served by the retained-static composite fast path (blit + cursor,
/// no glyph pipeline).
pub(super) static COMPOSITE_ONLY_FRAMES: AtomicU64 = AtomicU64::new(0);
/// Full-frame textures the GPU budget refused. Each one costs that frame its
/// offscreen composition (so its transitions and pane motion) or its post
/// shader, and the frame is still drawn. Structurally zero unless the render
/// thread is genuinely near its ceiling; a nonzero count is the evidence that
/// distinguishes "the animation is disabled" from "the animation could not be
/// afforded".
pub(super) static FULL_FRAME_TEXTURE_REFUSALS: AtomicU64 = AtomicU64::new(0);
/// Planned frames attributed per demand reason: one increment per reason a
/// planned frame satisfies, so an idle session's frames can be traced to what
/// asked for them. Indexed by [`super::frame_sched::DemandReason::index`]; a
/// frame driven by several reasons increments each.
static PLAN_DEMAND_REASONS: [AtomicU64; super::frame_sched::DemandReason::COUNT] =
    [const { AtomicU64::new(0) }; super::frame_sched::DemandReason::COUNT];
/// Frames rendered without any demand reason to name. Structurally zero: the
/// redraw handler falls back to a platform-redraw plan when nothing it
/// scheduled explains the tick. Kept as standing evidence for architectural
/// invariant 12 ("every scheduled frame has at least one inspectable demand
/// reason"), which a counter can attest and a code reading cannot.
pub(super) static UNATTRIBUTED_PRESENT_ATTEMPTS: AtomicU64 = AtomicU64::new(0);
/// Microseconds from the most recently consumed scene commit to its present.
pub(super) static LAST_COMMIT_TO_PRESENT_US: AtomicU64 = AtomicU64::new(0);
/// Worst observed commit-to-present latency in microseconds.
pub(super) static MAX_COMMIT_TO_PRESENT_US: AtomicU64 = AtomicU64::new(0);

/// Commit-to-present latency histogram: sample counts per log-scale microsecond
/// bucket (upper bounds in [`FRAME_TIME_BUCKET_UPPER_US`]). Lets a reader derive
/// frame-time percentiles (p50/p95/p99) rather than only last/max.
static FRAME_TIME_BUCKETS: [AtomicU64; 8] = [
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
];

/// Inclusive upper bound (microseconds) of each `FRAME_TIME_BUCKETS` slot; the
/// last is unbounded (`u64::MAX`), i.e. frames slower than 66 ms.
pub const FRAME_TIME_BUCKET_UPPER_US: [u64; 8] =
    [1_000, 2_000, 4_000, 8_000, 16_000, 33_000, 66_000, u64::MAX];

fn record_frame_time(latency_us: u64) {
    let idx = FRAME_TIME_BUCKET_UPPER_US
        .iter()
        .position(|&upper| latency_us <= upper)
        .unwrap_or(FRAME_TIME_BUCKET_UPPER_US.len() - 1);
    FRAME_TIME_BUCKETS[idx].fetch_add(1, Ordering::Relaxed);
}

/// Monotonic anchor for converting observations to storable microsecond ticks.
static EPOCH: std::sync::OnceLock<EventTime> = std::sync::OnceLock::new();
/// Microsecond tick (vs EPOCH) of the oldest scene commit not yet presented;
/// 0 = none pending. Approximate under multiple windows, which is acceptable
/// for Stage 0 evidence.
static PENDING_COMMIT_TICK_US: AtomicU64 = AtomicU64::new(0);
/// Microsecond tick of the last periodic snapshot log.
static LAST_LOG_TICK_US: AtomicU64 = AtomicU64::new(0);
/// Wakeup total at the last periodic snapshot log.
static LAST_LOG_WAKEUPS: AtomicU64 = AtomicU64::new(0);

fn tick_us(now: EventTime) -> u64 {
    let epoch = *EPOCH.get_or_init(|| now);
    // Saturates at 0 for the first caller; monotonic afterwards. +1 keeps a
    // real tick from colliding with the 0 = "none pending" sentinel.
    now.saturating_since(epoch).as_micros() as u64 + 1
}

pub(super) fn count(counter: &AtomicU64) {
    counter.fetch_add(1, Ordering::Relaxed);
}

/// Per-native-window demand attribution (frame scheduling plan,
/// Observability: "counters per native window and process-wide totals ...
/// active demand reasons"). Written only from the render thread at points
/// that are already awake (demand reconciliation and frame planning), so a
/// fully idle loop never touches it; read by the diagnostics snapshot.
struct WindowDemandStats {
    /// Demand reasons active as of the last reconciliation.
    active: DemandReasonSet,
    /// Planned frames per demand reason for this window, indexed like
    /// [`FrameSchedSnapshot::demand_reasons`].
    plans: [u64; DemandReason::COUNT],
}

static WINDOW_DEMAND: Mutex<BTreeMap<u64, WindowDemandStats>> = Mutex::new(BTreeMap::new());

/// The scheduler's loop-global pseudo-window (`LOOP_WINDOW`): not a native
/// window, so it is excluded from per-window attribution. Its demand still
/// counts in the process-wide totals.
const LOOP_PSEUDO_WINDOW: u64 = u64::MAX;

/// Replace the per-window active-reason sets with the coordinator's current
/// view. Windows absent from `demand` (destroyed/pruned) drop their stats:
/// per-window attribution does not outlive the window.
pub(super) fn publish_window_demand(
    demand: impl Iterator<Item = (NativeWindowId, DemandReasonSet)>,
) {
    let mut map = WINDOW_DEMAND.lock().unwrap();
    let mut old = std::mem::take(&mut *map);
    for (id, set) in demand {
        if id.0 == LOOP_PSEUDO_WINDOW {
            continue;
        }
        let plans = old
            .remove(&id.0)
            .map(|stats| stats.plans)
            .unwrap_or([0; DemandReason::COUNT]);
        map.insert(id.0, WindowDemandStats { active: set, plans });
    }
}

/// Record a frame plan: its render work class and the demands it satisfies,
/// both process-wide and attributed to the window the plan is for.
pub(super) fn count_plan(window: NativeWindowId, plan: &FramePlan) {
    let counter = match plan.work {
        RenderWork::None => &PLAN_NONE,
        RenderWork::CompositeOnly { .. } => &PLAN_COMPOSITE_ONLY,
        RenderWork::RepaintLayers { .. } => &PLAN_REPAINT_LAYERS,
        RenderWork::RebuildScene => &PLAN_REBUILD_SCENE,
    };
    count(counter);
    for reason in plan.reasons.iter() {
        count(&PLAN_DEMAND_REASONS[reason.index()]);
    }
    if plan.reasons.is_empty() || window.0 == LOOP_PSEUDO_WINDOW {
        return;
    }
    let mut map = WINDOW_DEMAND.lock().unwrap();
    let stats = map.entry(window.0).or_insert_with(|| WindowDemandStats {
        active: DemandReasonSet::empty(),
        plans: [0; DemandReason::COUNT],
    });
    for reason in plan.reasons.iter() {
        stats.plans[reason.index()] += 1;
    }
}

/// Record that a scene commit arrived; starts the commit-to-present clock if
/// no earlier commit is still waiting to reach the screen.
pub(super) fn note_scene_commit(now: EventTime) {
    count(&SCENE_COMMITS);
    let tick = tick_us(now);
    let _ = PENDING_COMMIT_TICK_US.compare_exchange(0, tick, Ordering::Relaxed, Ordering::Relaxed);
}

/// Record a top-level present; closes the commit-to-present measurement when
/// a commit is pending.
pub(super) fn note_present(now: EventTime) {
    count(&SURFACE_PRESENTS);
    let pending = PENDING_COMMIT_TICK_US.swap(0, Ordering::Relaxed);
    if pending != 0 {
        let latency_us = tick_us(now).saturating_sub(pending);
        LAST_COMMIT_TO_PRESENT_US.store(latency_us, Ordering::Relaxed);
        MAX_COMMIT_TO_PRESENT_US.fetch_max(latency_us, Ordering::Relaxed);
        record_frame_time(latency_us);
    }
}

/// Point-in-time copy of every counter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameSchedSnapshot {
    pub wakeups: u64,
    pub redraw_requests: u64,
    /// Subset of `redraw_requests` issued by the event loop's deadline
    /// service (see [`DEADLINE_SERVICED_REDRAWS`]).
    pub deadline_serviced_redraws: u64,
    pub redraw_events: u64,
    pub scene_commits: u64,
    pub root_glyph_passes: u64,
    pub presents: u64,
    pub plan_none: u64,
    pub plan_composite_only: u64,
    pub plan_repaint_layers: u64,
    pub plan_rebuild_scene: u64,
    pub retained_static_builds: u64,
    pub composite_only_frames: u64,
    pub last_commit_to_present_us: u64,
    pub max_commit_to_present_us: u64,
    /// Commit-to-present latency histogram (counts per bucket; bounds in
    /// [`FRAME_TIME_BUCKET_UPPER_US`]).
    pub frame_time_buckets: [u64; 8],
    /// Planned frames per demand reason, indexed as [`DEMAND_REASON_NAMES`].
    pub demand_reasons: [u64; super::frame_sched::DemandReason::COUNT],
    /// Frames rendered with no demand reason. Must stay 0.
    pub unattributed_present_attempts: u64,
}

/// Names of the [`FrameSchedSnapshot::demand_reasons`] slots, in index order.
pub const DEMAND_REASON_NAMES: [&str; super::frame_sched::DemandReason::COUNT] = {
    let mut names = [""; super::frame_sched::DemandReason::COUNT];
    let all = super::frame_sched::DemandReason::ALL;
    let mut i = 0;
    while i < all.len() {
        names[all[i].index()] = all[i].name();
        i += 1;
    }
    names
};

/// Point-in-time per-native-window demand attribution: the reasons currently
/// keeping the window rendering plus its cumulative planned-frame counts per
/// reason (frame scheduling plan, Observability).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowFrameSnapshot {
    /// Scheduler window id: the Emacs frame id once adopted, 0 for the
    /// primary window before adoption.
    pub window: u64,
    /// Currently-active demand reasons, in [`DEMAND_REASON_NAMES`] order.
    pub active_reasons: Vec<&'static str>,
    /// Planned frames per demand reason, indexed as [`DEMAND_REASON_NAMES`].
    pub demand_reasons: [u64; super::frame_sched::DemandReason::COUNT],
}

/// Copy of the per-window demand attribution, sorted by window id. Safe from
/// any thread; the render thread holds the lock only for map-sized updates.
pub fn window_snapshots() -> Vec<WindowFrameSnapshot> {
    WINDOW_DEMAND
        .lock()
        .unwrap()
        .iter()
        .map(|(id, stats)| WindowFrameSnapshot {
            window: *id,
            active_reasons: stats.active.iter().map(DemandReason::name).collect(),
            demand_reasons: stats.plans,
        })
        .collect()
}

pub fn snapshot() -> FrameSchedSnapshot {
    FrameSchedSnapshot {
        wakeups: EVENT_LOOP_WAKEUPS.load(Ordering::Relaxed),
        redraw_requests: REDRAW_REQUESTS.load(Ordering::Relaxed),
        deadline_serviced_redraws: DEADLINE_SERVICED_REDRAWS.load(Ordering::Relaxed),
        redraw_events: REDRAW_EVENTS.load(Ordering::Relaxed),
        scene_commits: SCENE_COMMITS.load(Ordering::Relaxed),
        root_glyph_passes: ROOT_GLYPH_PASSES.load(Ordering::Relaxed),
        presents: SURFACE_PRESENTS.load(Ordering::Relaxed),
        plan_none: PLAN_NONE.load(Ordering::Relaxed),
        plan_composite_only: PLAN_COMPOSITE_ONLY.load(Ordering::Relaxed),
        plan_repaint_layers: PLAN_REPAINT_LAYERS.load(Ordering::Relaxed),
        plan_rebuild_scene: PLAN_REBUILD_SCENE.load(Ordering::Relaxed),
        retained_static_builds: RETAINED_STATIC_BUILDS.load(Ordering::Relaxed),
        composite_only_frames: COMPOSITE_ONLY_FRAMES.load(Ordering::Relaxed),
        last_commit_to_present_us: LAST_COMMIT_TO_PRESENT_US.load(Ordering::Relaxed),
        max_commit_to_present_us: MAX_COMMIT_TO_PRESENT_US.load(Ordering::Relaxed),
        frame_time_buckets: std::array::from_fn(|i| FRAME_TIME_BUCKETS[i].load(Ordering::Relaxed)),
        demand_reasons: std::array::from_fn(|i| PLAN_DEMAND_REASONS[i].load(Ordering::Relaxed)),
        unattributed_present_attempts: UNATTRIBUTED_PRESENT_ATTEMPTS.load(Ordering::Relaxed),
    }
}

const LOG_INTERVAL_US: u64 = 5_000_000;

/// Log a snapshot at debug level at most every 5 seconds, and only when the
/// loop actually woke since the previous log. Called from about_to_wait, so a
/// fully idle loop logs nothing at all.
pub(super) fn maybe_log_snapshot(now: EventTime) {
    let tick = tick_us(now);
    let last = LAST_LOG_TICK_US.load(Ordering::Relaxed);
    if tick.saturating_sub(last) < LOG_INTERVAL_US {
        return;
    }
    if LAST_LOG_TICK_US
        .compare_exchange(last, tick, Ordering::Relaxed, Ordering::Relaxed)
        .is_err()
    {
        return;
    }
    let snap = snapshot();
    let last_wakeups = LAST_LOG_WAKEUPS.swap(snap.wakeups, Ordering::Relaxed);
    if snap.wakeups == last_wakeups {
        return;
    }
    let elapsed_s = tick.saturating_sub(last) as f64 / 1_000_000.0;
    let demand_reasons = DEMAND_REASON_NAMES
        .iter()
        .zip(snap.demand_reasons.iter())
        .filter(|(_, count)| **count > 0)
        .map(|(name, count)| format!("{name}={count}"))
        .collect::<Vec<_>>()
        .join(" ");
    tracing::debug!(
        wakeups = snap.wakeups,
        wakeups_per_s = format!("{:.1}", (snap.wakeups - last_wakeups) as f64 / elapsed_s),
        redraw_requests = snap.redraw_requests,
        deadline_serviced_redraws = snap.deadline_serviced_redraws,
        redraw_events = snap.redraw_events,
        scene_commits = snap.scene_commits,
        root_glyph_passes = snap.root_glyph_passes,
        presents = snap.presents,
        plan_none = snap.plan_none,
        plan_composite_only = snap.plan_composite_only,
        plan_repaint_layers = snap.plan_repaint_layers,
        plan_rebuild_scene = snap.plan_rebuild_scene,
        retained_static_builds = snap.retained_static_builds,
        full_frame_texture_refusals = FULL_FRAME_TEXTURE_REFUSALS.load(Ordering::Relaxed),
        composite_only_frames = snap.composite_only_frames,
        last_commit_to_present_us = snap.last_commit_to_present_us,
        max_commit_to_present_us = snap.max_commit_to_present_us,
        demand_reasons = demand_reasons,
        unattributed_present_attempts = snap.unattributed_present_attempts,
        "frame_sched_stats"
    );
}
