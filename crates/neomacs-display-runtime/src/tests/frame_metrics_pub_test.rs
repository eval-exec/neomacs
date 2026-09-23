//! Smoke test that the frame-metrics snapshot getter is public and callable
//! off the render thread (the diagnostics server relies on this).

#[test]
fn frame_metrics_snapshot_is_publicly_callable() {
    // Before any rendering the counters are zero; the point is that the public
    // getter compiles, returns the public snapshot type, and reads lock-free.
    let snap = crate::frame_metrics_snapshot();
    // Touch a few fields to pin the public field names down.
    let _ = snap.presents;
    let _ = snap.scene_commits;
    let _ = snap.last_commit_to_present_us;
    let _ = snap.max_commit_to_present_us;
    let _ = snap.composite_only_frames;
    let _ = snap.retained_static_builds;
    let _ = snap.wakeups;
}
