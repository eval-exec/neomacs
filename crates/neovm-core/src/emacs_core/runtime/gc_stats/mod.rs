//! Process-global publication of GC/heap counters.
//!
//! The diagnostics server runs on its own thread and must not touch the
//! `Context`/`TaggedHeap` (owned by the Lisp thread). So after each GC cycle the
//! Lisp thread publishes a small snapshot into these relaxed atomics, and the
//! diagnostics thread reads them lock-free — mirroring the `frame_stats`
//! pattern in the display runtime (readers `pub`, the writer `pub(crate)`).

use std::sync::atomic::{AtomicU64, Ordering};

static COLLECTIONS: AtomicU64 = AtomicU64::new(0);
static LIVE_BYTES: AtomicU64 = AtomicU64::new(0);
static TOTAL_ALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);
static CONS_CELLS: AtomicU64 = AtomicU64::new(0);
static STRINGS: AtomicU64 = AtomicU64::new(0);
static VECTOR_CELLS: AtomicU64 = AtomicU64::new(0);

/// A snapshot of published GC/heap counters.
///
/// The object counts (`cons_cells`, `strings`, `vector_cells`) come from the
/// `TaggedHeap` memory-use-count array. Symbols are interned rather than heap
/// allocated in that array, so they are intentionally not exposed here.
/// `total_allocated_bytes` is monotonic, so a consumer can derive allocation
/// rate by diffing successive snapshots.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GcStatsSnapshot {
    pub collections: u64,
    pub live_bytes: u64,
    pub total_allocated_bytes: u64,
    pub cons_cells: u64,
    pub strings: u64,
    pub vector_cells: u64,
}

/// Publish the latest counters. Called on the Lisp thread after a GC cycle.
///
/// `pub(crate)`: only the GC path (in `eval`) produces these; external crates
/// read via [`snapshot`] and must not be able to overwrite the counters.
pub(crate) fn publish(snap: GcStatsSnapshot) {
    COLLECTIONS.store(snap.collections, Ordering::Relaxed);
    LIVE_BYTES.store(snap.live_bytes, Ordering::Relaxed);
    TOTAL_ALLOCATED_BYTES.store(snap.total_allocated_bytes, Ordering::Relaxed);
    CONS_CELLS.store(snap.cons_cells, Ordering::Relaxed);
    STRINGS.store(snap.strings, Ordering::Relaxed);
    VECTOR_CELLS.store(snap.vector_cells, Ordering::Relaxed);
}

/// Read the most recently published counters. Safe from any thread.
pub fn snapshot() -> GcStatsSnapshot {
    GcStatsSnapshot {
        collections: COLLECTIONS.load(Ordering::Relaxed),
        live_bytes: LIVE_BYTES.load(Ordering::Relaxed),
        total_allocated_bytes: TOTAL_ALLOCATED_BYTES.load(Ordering::Relaxed),
        cons_cells: CONS_CELLS.load(Ordering::Relaxed),
        strings: STRINGS.load(Ordering::Relaxed),
        vector_cells: VECTOR_CELLS.load(Ordering::Relaxed),
    }
}

#[cfg(test)]
mod tests {
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
}
