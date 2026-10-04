//! SampleGrid quantization and delay arithmetic.

use super::SampleGrid;
use crate::image::ImageFrameDelay;
use std::time::Duration;

fn seconds(total: u64) -> Duration {
    Duration::from_secs(total)
}

#[test]
fn zero_period_or_rate_is_not_a_grid() {
    assert!(SampleGrid::new(Duration::ZERO, 30).is_none());
    assert!(SampleGrid::new(seconds(2), 0).is_none());
}

#[test]
fn slot_count_follows_rate_up_to_the_cap() {
    // 2s at 30fps wants 60 slots.
    assert_eq!(SampleGrid::new(seconds(2), 30).unwrap().slot_count(), 60);
    // 2s at 1fps wants 2 slots.
    assert_eq!(SampleGrid::new(seconds(2), 1).unwrap().slot_count(), 2);
    // A fraction of a second still produces one slot, never zero.
    assert_eq!(
        SampleGrid::new(Duration::from_millis(10), 30).unwrap().slot_count(),
        1
    );
    // 60s at 30fps wants 1800 slots; the cap wins.
    assert_eq!(
        SampleGrid::new(seconds(60), 30).unwrap().slot_count(),
        SampleGrid::MAX_SLOTS
    );
}

#[test]
fn slots_partition_the_period() {
    // 2s at 2fps: four slots of exactly 500ms.
    let grid = SampleGrid::new(seconds(2), 2).unwrap();
    assert_eq!(grid.slot_count(), 4);
    assert_eq!(grid.slot_for(Duration::ZERO), 0);
    assert_eq!(grid.slot_for(Duration::from_millis(499)), 0);
    assert_eq!(grid.slot_for(Duration::from_millis(500)), 1);
    assert_eq!(grid.slot_for(Duration::from_millis(1499)), 2);
    // The wrapped endpoint belongs to the final slot, not slot zero.
    assert_eq!(grid.slot_for(seconds(2)), 3);
}

#[test]
fn wrap_folds_document_time_into_one_period() {
    let grid = SampleGrid::new(seconds(2), 2).unwrap();
    assert_eq!(grid.wrap(Duration::ZERO), Duration::ZERO);
    assert_eq!(grid.wrap(seconds(2)), Duration::ZERO);
    assert_eq!(grid.wrap(seconds(5)), seconds(1));
}

#[test]
fn slot_starts_divide_the_period() {
    let grid = SampleGrid::new(seconds(2), 2).unwrap();
    assert_eq!(grid.slot_start(0), Some(Duration::ZERO));
    assert_eq!(grid.slot_start(1), Some(Duration::from_millis(500)));
    assert_eq!(grid.slot_start(3), Some(Duration::from_millis(1500)));
    assert_eq!(grid.slot_start(4), None);
}

#[test]
fn slot_delay_is_exact_when_period_divides_cleanly() {
    let grid = SampleGrid::new(seconds(2), 2).unwrap();
    let delay = grid.slot_delay().expect("delay");
    assert_eq!(delay.seconds(), Some(0.5));
}

#[test]
fn slot_delay_stays_rational_when_it_does_not() {
    // 2s over 60 slots is 33⅓ ms: 100/3 in milliseconds, not 33.
    let grid = SampleGrid::new(seconds(2), 30).unwrap();
    let ImageFrameDelay::Milliseconds {
        numerator,
        denominator,
    } = grid.slot_delay().expect("delay")
    else {
        panic!("expected an exact millisecond delay");
    };
    assert_eq!((numerator, denominator.get()), (100, 3));
}
