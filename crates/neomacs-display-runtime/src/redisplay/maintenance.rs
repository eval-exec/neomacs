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
#[path = "tests/maintenance_test.rs"]
mod tests;
