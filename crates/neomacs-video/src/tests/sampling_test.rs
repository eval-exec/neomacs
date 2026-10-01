use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::sampling::{GpuAllocationTracker, SubmittedWorkRetirement};
use crate::{PreparedVideoDraw, VideoSampleKind, VideoWake};

#[test]
fn prepared_draw_exposes_one_renderer_facing_sample_contract() {
    fn renderer_contract(draw: PreparedVideoDraw<'_>) {
        match draw.sample_kind() {
            VideoSampleKind::Packed => assert!(draw.packed_view().is_some()),
            VideoSampleKind::BiPlanar => assert!(draw.packed_view().is_none()),
        }
        let _ = draw.bind_group();
    }

    let _ = renderer_contract;
}

#[test]
fn gpu_allocation_is_counted_until_the_last_texture_owner_drops() {
    let tracker = GpuAllocationTracker::default();
    let first_owner = tracker.track(4096);
    let second_owner = first_owner.clone();

    assert_eq!(tracker.bytes(), 4096);
    drop(first_owner);
    assert_eq!(tracker.bytes(), 4096);
    drop(second_owner);
    assert_eq!(tracker.bytes(), 0);
}

#[test]
fn final_gpu_allocation_retirement_wakes_budget_observation() {
    let wake_count = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&wake_count);
    let tracker = GpuAllocationTracker::new(VideoWake::new(move || {
        observed.fetch_add(1, Ordering::Relaxed);
    }));

    let owner = tracker.track(4096);
    assert_eq!(wake_count.load(Ordering::Relaxed), 0);
    drop(owner);

    assert_eq!(tracker.bytes(), 0);
    assert_eq!(wake_count.load(Ordering::Relaxed), 1);
}

#[test]
fn native_lease_is_retained_until_the_gpu_completion_callback_runs() {
    struct NativeLease(Arc<AtomicUsize>);

    impl Drop for NativeLease {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    let releases = Arc::new(AtomicUsize::new(0));
    let retirement = SubmittedWorkRetirement::new(NativeLease(Arc::clone(&releases)));
    assert_eq!(releases.load(Ordering::Relaxed), 0);

    let gpu_completion = retirement.into_completion_callback();
    assert_eq!(releases.load(Ordering::Relaxed), 0);

    gpu_completion();
    assert_eq!(releases.load(Ordering::Relaxed), 1);
}
