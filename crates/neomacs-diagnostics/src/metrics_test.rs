use crate::metrics::{FrameMetrics, GcMetrics, MetricsSnapshot, WindowFrameMetrics};

#[test]
fn snapshot_serializes_stable_json_shape() {
    let snap = MetricsSnapshot {
        frame: FrameMetrics {
            presents: 100,
            scene_commits: 90,
            wakeups: 500,
            deadline_serviced_redraws: 20,
            last_commit_to_present_us: 1200,
            max_commit_to_present_us: 8000,
            frame_p50_us: 2000,
            frame_p95_us: 8000,
            frame_p99_us: 16000,
            composite_only_frames: 10,
            retained_static_builds: 3,
            demand_reasons: [("cursor_animation".to_owned(), 12)].into_iter().collect(),
            unattributed_presents: 0,
            active_reasons: Vec::new(),
            windows: std::collections::BTreeMap::new(),
        },
        gc: GcMetrics {
            collections: 7,
            live_bytes: 4096,
            total_allocated_bytes: 1_000_000,
            cons_cells: 200,
            strings: 40,
            vector_cells: 60,
        },
    };
    let v: serde_json::Value = serde_json::to_value(&snap).unwrap();
    assert_eq!(v["frame"]["presents"], 100);
    assert_eq!(v["frame"]["last_commit_to_present_us"], 1200);
    assert_eq!(v["frame"]["demand_reasons"]["cursor_animation"], 12);
    assert_eq!(v["frame"]["unattributed_presents"], 0);
    assert_eq!(v["frame"]["deadline_serviced_redraws"], 20);
    assert_eq!(v["gc"]["collections"], 7);
    assert_eq!(v["gc"]["cons_cells"], 200);
}

// Schema extension (frame-scheduling plan, Observability: "counters per
// native window and process-wide totals ... active demand reasons"): the
// per-window map and the process-wide active-reason list are part of the
// pinned /metrics contract alongside the cumulative process totals.
#[test]
fn snapshot_serializes_per_window_demand_shape() {
    let snap = MetricsSnapshot {
        frame: FrameMetrics {
            active_reasons: vec!["cursor_animation".to_owned()],
            windows: [(
                1u64,
                WindowFrameMetrics {
                    active_reasons: vec!["cursor_animation".to_owned()],
                    demand_reasons: [("cursor_animation".to_owned(), 4)].into_iter().collect(),
                },
            )]
            .into_iter()
            .collect(),
            ..FrameMetrics::default()
        },
        gc: GcMetrics::default(),
    };
    let v: serde_json::Value = serde_json::to_value(&snap).unwrap();
    // Process-wide union of currently-active reasons.
    assert_eq!(
        v["frame"]["active_reasons"],
        serde_json::json!(["cursor_animation"])
    );
    // Per-native-window attribution, keyed by the scheduler's window id.
    assert_eq!(
        v["frame"]["windows"]["1"]["active_reasons"],
        serde_json::json!(["cursor_animation"])
    );
    assert_eq!(
        v["frame"]["windows"]["1"]["demand_reasons"]["cursor_animation"],
        4
    );
    // The extension leaves the default shape present-and-empty, not absent.
    let empty: serde_json::Value = serde_json::to_value(&MetricsSnapshot::default()).unwrap();
    assert!(empty["frame"]["windows"].is_object());
    assert!(empty["frame"]["active_reasons"].is_array());
}

#[test]
fn default_snapshot_is_all_zero() {
    let snap = MetricsSnapshot::default();
    assert_eq!(snap.frame.presents, 0);
    assert_eq!(snap.gc.collections, 0);
    let v = serde_json::to_value(&snap).unwrap();
    assert_eq!(v["frame"]["max_commit_to_present_us"], 0);
    assert_eq!(v["gc"]["vector_cells"], 0);
}

#[test]
fn percentile_from_buckets_picks_the_right_bucket() {
    use crate::metrics::percentile_from_buckets;
    // bounds: <=1,<=2,<=4,<=8,<=16,<=33,<=66 (thousands us), then unbounded.
    let bounds = [
        1_000u64,
        2_000,
        4_000,
        8_000,
        16_000,
        33_000,
        66_000,
        u64::MAX,
    ];
    // 100 frames: 90 in <=2ms, 8 in <=8ms, 2 in the unbounded (>66ms) bucket.
    let buckets = [0u64, 90, 0, 8, 0, 0, 0, 2];
    assert_eq!(percentile_from_buckets(&buckets, &bounds, 0.50), 2_000); // median
    assert_eq!(percentile_from_buckets(&buckets, &bounds, 0.95), 8_000); // 95th
    // 99th falls in the unbounded top bucket -> reported as its lower edge 66ms.
    assert_eq!(percentile_from_buckets(&buckets, &bounds, 0.99), 66_000);
    // No samples -> 0.
    assert_eq!(percentile_from_buckets(&[0, 0, 0], &[1, 2, 3], 0.5), 0);
}
