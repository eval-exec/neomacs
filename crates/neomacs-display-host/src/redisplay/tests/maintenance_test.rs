use super::*;
use std::cell::Cell;

#[test]
fn ready_acquisition_uses_a_bounded_batch() {
    let calls = Cell::new(0);
    let result = run_budgeted(
        || {
            calls.set(calls.get() + 1);
            (Some(ScrollCoverageProgress::Continue), false)
        },
        || Duration::ZERO,
    );
    assert_eq!(calls.get(), 4);
    assert_eq!(result, (Some(ScrollCoverageProgress::Continue), false));
}

#[test]
fn acquisition_yields_on_elapsed_budget_publication_or_worker_wait() {
    let calls = Cell::new(0);
    run_budgeted(
        || {
            calls.set(calls.get() + 1);
            (Some(ScrollCoverageProgress::Continue), false)
        },
        || {
            if calls.get() < 2 {
                Duration::ZERO
            } else {
                Duration::from_millis(1)
            }
        },
    );
    assert_eq!(calls.get(), 2);
    for result in [
        (Some(ScrollCoverageProgress::Continue), true),
        (Some(ScrollCoverageProgress::WorkerPending), false),
        (None, false),
    ] {
        calls.set(0);
        assert_eq!(
            run_budgeted(
                || {
                    calls.set(calls.get() + 1);
                    result
                },
                || Duration::ZERO
            ),
            result
        );
        assert_eq!(calls.get(), 1);
    }
}
