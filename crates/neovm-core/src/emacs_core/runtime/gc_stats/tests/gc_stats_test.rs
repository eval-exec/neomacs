use super::*;

#[test]
fn publish_then_snapshot_round_trips() {
    publish(GcStatsSnapshot {
        collections: 11,
        live_bytes: 2048,
        total_allocated_bytes: 999,
        cons_cells: 5,
        strings: 6,
        vector_cells: 7,
    });
    let got = snapshot();
    assert_eq!(got.collections, 11);
    assert_eq!(got.live_bytes, 2048);
    assert_eq!(got.total_allocated_bytes, 999);
    assert_eq!(got.cons_cells, 5);
    assert_eq!(got.strings, 6);
    assert_eq!(got.vector_cells, 7);
}

/// Integration: a real GC cycle publishes live heap stats. Verifies the
/// wiring in `update_gc_runtime_stats` fires and that the counts-slot
/// mapping (cons=0, vector=2, strings=6) is plausible.
#[test]
fn gc_collect_publishes_live_stats() {
    use crate::emacs_core::eval::Context;

    let mut eval = Context::new();
    eval.gc_collect();

    let snap = snapshot();
    assert!(
        snap.collections >= 1,
        "collections should be >= 1 after gc_collect, got {}",
        snap.collections
    );
    assert!(
        snap.total_allocated_bytes > 0,
        "total_allocated_bytes should be > 0 after bootstrap"
    );
    assert!(
        snap.cons_cells > 0,
        "cons_cells (counts[0]) should be > 0 after bootstrap"
    );
    assert!(
        snap.vector_cells > 0,
        "vector_cells (counts[2]) should be > 0 after bootstrap"
    );
}
