use super::*;
use crate::emacs_core::eval::Context;

pub(crate) fn run_gc_tls_ownership_window_snapshot_reset_discards_a_dropped_heap_fixture(
    make_context: fn() -> crate::emacs_core::eval::Context,
) {
    {
        let mut first = make_context();
        first.eval_str("(current-window-configuration)").unwrap();
        assert!(WINDOW_CONFIGURATION_SNAPSHOTS.with(|slot| !slot.borrow().is_empty()));
    }
    let mut next = make_context();
    assert!(WINDOW_CONFIGURATION_SNAPSHOTS.with(|slot| slot.borrow().is_empty()));
    next.gc_collect_exact();
}
