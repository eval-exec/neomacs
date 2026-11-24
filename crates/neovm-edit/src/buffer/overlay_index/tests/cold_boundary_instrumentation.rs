//! Calling-thread cumulative observations for cold-boundary tests. Callers
//! measure before/after deltas; no counter state enters the production index.
use std::cell::Cell;

use super::OverlayIndex;

thread_local! {
    static RECORD_VISITS: Cell<usize> = const { Cell::new(0) };
}

pub(super) fn record_visit() {
    RECORD_VISITS.with(|visits| visits.set(visits.get() + 1));
}

impl OverlayIndex {
    pub(in crate::buffer) fn cold_boundary_record_visit_count_for_test(&self) -> usize {
        RECORD_VISITS.with(Cell::get)
    }

    pub(in crate::buffer) fn endpoint_index_is_published_for_test(&self) -> bool {
        self.published_endpoints().is_some()
    }
}
