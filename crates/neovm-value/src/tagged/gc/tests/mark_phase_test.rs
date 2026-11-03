//! All existing flag combinations have an explicit phase or violate the worker contract.
use super::{MarkPhase, TaggedHeap};

/// Restore synthetic flags even if the classifier or an assertion unwinds.
struct SyntheticPhase<'a>(&'a mut TaggedHeap);

impl Drop for SyntheticPhase<'_> {
    fn drop(&mut self) {
        self.0.mark_in_progress = false;
        self.0.concurrent_mark_running = false;
        self.0.sweep_in_progress = false;
    }
}

#[test]
fn mark_phase_classifies_every_reachable_flag_combination() {
    let mut heap = TaggedHeap::new();
    // The legacy fields are ordered: mark, concurrent worker, sweep.
    for (expected, flags) in [
        (MarkPhase::Idle, [false, false, false]),
        (MarkPhase::StoppedMarking, [true, false, false]),
        (MarkPhase::ConcurrentRunning, [true, true, false]),
        (MarkPhase::PublishingSweep, [true, false, true]),
        (MarkPhase::Sweeping, [false, false, true]),
    ] {
        let phase = SyntheticPhase(&mut heap);
        [
            phase.0.mark_in_progress,
            phase.0.concurrent_mark_running,
            phase.0.sweep_in_progress,
        ] = flags;
        let actual = phase.0.mark_phase();
        // Synthetic flags never describe a real job; restore before assertions
        // or Drop so failure cannot ask teardown to join a nonexistent worker.
        drop(phase);
        assert_eq!(actual, expected);
    }
}

#[cfg(debug_assertions)]
#[test]
fn mark_phase_rejects_every_invalid_worker_flag_combination() {
    let mut heap = TaggedHeap::new();
    for flags in [
        [false, true, false],
        [false, true, true],
        [true, true, true],
    ] {
        let phase = SyntheticPhase(&mut heap);
        [
            phase.0.mark_in_progress,
            phase.0.concurrent_mark_running,
            phase.0.sweep_in_progress,
        ] = flags;
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| phase.0.mark_phase()));
        drop(phase);
        assert!(result.is_err(), "a worker requires marking without sweep");
    }
}
