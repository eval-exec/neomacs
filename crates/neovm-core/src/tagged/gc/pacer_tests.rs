use super::*;

/// Forced (cap-hit) terminations escalate the lead without polluting the
/// EWMAs; the next clean window's full recompute drops it back. The lead
/// is the cap-pressure field detector (`mark_window`/`pace[]` trace
/// lines) — the paced start trigger it once fed was reverted after the
/// release-regime probe stayed dormant (task-3/5 reports).
#[test]
fn pacer_lead_escalates_on_forced_termination_and_recovers_on_clean() {
    let mut heap = TaggedHeap::new();
    heap.gc_threshold = 1_000_000;

    // First forced window seeds the lead from the truncated window's
    // own allocation (the only lower bound available).
    heap.pace_close_mark_window(50_000, 3_000_000, true);
    assert_eq!(heap.pace_lead_bytes, 3_000_000);
    assert_eq!(
        heap.pace_alloc_rate_bps, 0,
        "forced sample must not feed EWMA"
    );
    assert_eq!(heap.pace_mark_dur_us, 0, "forced sample must not feed EWMA");

    // Repeated cap hits double the lead.
    heap.pace_close_mark_window(50_000, 2_000_000, true);
    assert_eq!(heap.pace_lead_bytes, 6_000_000);

    // A clean quiet window recomputes the lead from the EWMAs directly
    // (seeded from this first clean sample): 1KB over 10ms -> ~1KB lead.
    heap.pace_close_mark_window(10_000, 1_024, false);
    assert_eq!(heap.pace_alloc_rate_bps, 102_400);
    assert_eq!(heap.pace_mark_dur_us, 10_000);
    assert_eq!(heap.pace_lead_bytes, 1_024);

    // Zero-wall windows (no stamp) leave the state untouched.
    heap.pace_close_mark_window(0, 999_999, false);
    assert_eq!(heap.pace_lead_bytes, 1_024);

    // Steady storm converges the EWMAs toward the sample: 8MB over
    // 100ms windows -> lead approaches 8MB (alpha 1/2 per cycle).
    for _ in 0..8 {
        heap.pace_close_mark_window(100_000, 8_000_000, false);
    }
    assert!(
        heap.pace_lead_bytes > 7_000_000,
        "lead should converge toward the storm's per-window allocation, got {}",
        heap.pace_lead_bytes
    );
}
