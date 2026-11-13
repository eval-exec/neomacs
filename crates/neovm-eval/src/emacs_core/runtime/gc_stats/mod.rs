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
#[path = "tests/gc_stats_test.rs"]
mod tests;
