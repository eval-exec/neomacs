//! The metrics data model exposed over `/metrics` and `/live`.
//!
//! Field names are the JSON API contract — later phases and external consumers
//! (agents, Perfetto, dashboards) depend on them exactly.

use serde::Serialize;
use std::collections::BTreeMap;

/// A point-in-time snapshot of neomacs performance metrics.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct MetricsSnapshot {
    pub frame: FrameMetrics,
    pub gc: GcMetrics,
}

/// Render / frame-scheduling counters. Mirrors the display runtime's
/// `FrameSchedSnapshot`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct FrameMetrics {
    pub presents: u64,
    pub scene_commits: u64,
    pub wakeups: u64,
    /// Redraw requests issued because a scheduled frame deadline came due,
    /// rather than because a producer re-declared its demand. The engagement
    /// counter for the event loop's deadline-service step: demands with no
    /// per-pass producer (the bounded retry after a present produces nothing)
    /// reach the screen only through this path.
    #[serde(default)]
    pub deadline_serviced_redraws: u64,
    /// Latency from scene commit to present for the last frame (microseconds).
    pub last_commit_to_present_us: u64,
    /// Worst commit-to-present latency observed (microseconds).
    pub max_commit_to_present_us: u64,
    /// Frame-time (commit-to-present) percentiles in microseconds, from the
    /// render thread's latency histogram. 0 until frames have been presented.
    pub frame_p50_us: u64,
    pub frame_p95_us: u64,
    pub frame_p99_us: u64,
    pub composite_only_frames: u64,
    pub retained_static_builds: u64,
    /// Planned frames attributed per demand reason ("why did this frame
    /// happen?"), keyed by the scheduler's reason names. A frame driven by
    /// several reasons counts once under each.
    #[serde(default)]
    pub demand_reasons: BTreeMap<String, u64>,
    /// Frames rendered with no demand reason at all. Zero by construction; a
    /// nonzero value means a present escaped attribution.
    #[serde(default)]
    pub unattributed_presents: u64,
    /// Demand reasons currently active on any window: the process-wide union
    /// of every window's `active_reasons` ("why is anything still
    /// rendering?"). Empty at idle.
    #[serde(default)]
    pub active_reasons: Vec<String>,
    /// Per-native-window demand attribution, keyed by the scheduler's window
    /// id (the Emacs frame id once adopted; 0 for the primary window before
    /// adoption). Design-doc Observability: "counters per native window and
    /// process-wide totals ... active demand reasons".
    #[serde(default)]
    pub windows: BTreeMap<u64, WindowFrameMetrics>,
}

/// One native window's demand attribution.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct WindowFrameMetrics {
    /// Demand reasons currently keeping this window rendering. Empty for an
    /// idle window.
    pub active_reasons: Vec<String>,
    /// Planned frames attributed per demand reason for this window; reasons
    /// with a zero count are omitted.
    pub demand_reasons: BTreeMap<String, u64>,
}

/// Estimate a latency percentile from a bucketed histogram, returning the upper
/// bound (microseconds) of the bucket the percentile `q` (0.0..=1.0) falls in.
/// `bounds[i]` is the inclusive upper bound of `buckets[i]`; a `u64::MAX` top
/// bound (unbounded bucket) is reported as the previous finite bound ("at
/// least"). Returns 0 when no samples have been recorded.
pub fn percentile_from_buckets(buckets: &[u64], bounds: &[u64], q: f64) -> u64 {
    let total: u64 = buckets.iter().sum();
    if total == 0 {
        return 0;
    }
    let target = ((total as f64) * q).ceil() as u64;
    let mut cum: u64 = 0;
    for (i, &count) in buckets.iter().enumerate() {
        cum += count;
        if cum >= target {
            let bound = bounds.get(i).copied().unwrap_or(0);
            return if bound == u64::MAX {
                // Unbounded top bucket: report its lower edge as "at least".
                bounds
                    .get(i.wrapping_sub(1))
                    .copied()
                    .filter(|&b| b != u64::MAX)
                    .unwrap_or(0)
            } else {
                bound
            };
        }
    }
    0
}

/// Lisp GC / heap counters. Mirrors the published `GcStatsSnapshot`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct GcMetrics {
    pub collections: u64,
    pub live_bytes: u64,
    pub total_allocated_bytes: u64,
    pub cons_cells: u64,
    pub strings: u64,
    pub vector_cells: u64,
}
