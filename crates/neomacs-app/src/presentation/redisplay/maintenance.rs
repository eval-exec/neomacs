//! Cooperative acquisition budget for one frontend service opportunity.
use neomacs_layout_engine::engine::ScrollCoverageProgress;
use std::time::Duration;

pub(super) fn run_budgeted(
    mut step: impl FnMut() -> (Option<ScrollCoverageProgress>, bool),
    mut elapsed: impl FnMut() -> Duration,
) -> (Option<ScrollCoverageProgress>, bool) {
    // Each engine slice has its own source/storage bounds. At most four
    // slices (512 source characters) can run here; check the 1 ms cooperative
    // time budget between slices. A ready publication yields immediately so
    // the renderer can receive it, and pending workers are never busy-polled.
    let mut result = step();
    for _ in 1..4 {
        if result != (Some(ScrollCoverageProgress::Continue), false)
            || elapsed() >= Duration::from_millis(1)
        {
            break;
        }
        result = step();
    }
    result
}

#[cfg(test)]
mod tests {
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
}
