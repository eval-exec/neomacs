use super::*;
use crate::buffer::BufferId;
use crate::heap_types::OverlayData;

fn overlay(start: usize, end: usize) -> Value {
    Value::make_overlay(OverlayData {
        serial: 0,
        plist: Value::NIL,
        buffer: Some(BufferId(1)),
        start,
        end,
        position_handle: None,
        front_advance: false,
        rear_advance: false,
    })
}

fn range(start: usize, end: usize) -> EmacsByteRange {
    EmacsByteRange::from_usize(start, end)
}

#[test]
fn same_start_order_survives_balancing_and_removal() {
    crate::test_utils::init_test_tracing();
    let mut index = OverlayIndex::new();
    let overlays: Vec<_> = (0..65).map(|_| overlay(2, 8)).collect();
    for overlay in &overlays {
        assert!(index.attach(*overlay, range(2, 8)));
    }

    let mut expected: Vec<_> = overlays
        .iter()
        .rev()
        .map(|overlay| overlay.bits())
        .collect();
    let actual: Vec<_> = index
        .overlays_at(EmacsBytePos::new(4))
        .into_iter()
        .map(|overlay| overlay.bits())
        .collect();
    assert_eq!(actual, expected);
    let removed = overlays[32];
    assert_eq!(index.detach(removed), Some(range(2, 8)));
    expected.retain(|identity| *identity != removed.bits());
    let actual: Vec<_> = index
        .overlays_at(EmacsBytePos::new(4))
        .into_iter()
        .map(|overlay| overlay.bits())
        .collect();
    assert_eq!(actual, expected);
    index.assert_invariants();
}

#[test]
fn detach_non_root_entry_preserves_interval_augmentation() {
    crate::test_utils::init_test_tracing();
    let mut index = OverlayIndex::new();
    let middle = overlay(20, 21);
    let left = overlay(10, 100);
    let right = overlay(30, 31);
    let far_right = overlay(40, 41);
    for (overlay, range) in [
        (middle, range(20, 21)),
        (left, range(10, 100)),
        (right, range(30, 31)),
        (far_right, range(40, 41)),
    ] {
        assert!(index.attach(overlay, range));
    }

    assert_eq!(index.detach(middle), Some(range(20, 21)));
    assert_eq!(index.overlays_at(EmacsBytePos::new(35)), vec![left]);
    assert_eq!(
        index.overlays_at(EmacsBytePos::new(40)),
        vec![left, far_right]
    );
}

#[test]
fn high_fanout_split_borrow_and_merge_preserve_all_indexes() {
    crate::test_utils::init_test_tracing();
    let mut index = OverlayIndex::new();
    let entries: Vec<_> = (0..257)
        .map(|entry| {
            let start = 10 + entry * 3;
            (overlay(start, start + 2), range(start, start + 2))
        })
        .collect();
    for (overlay, range) in &entries {
        assert!(index.attach(*overlay, *range));
    }
    index.assert_invariants();

    // Removing alternating runs forces both sibling borrowing and merging at
    // leaf level; draining most of the tree then collapses branch levels.
    for offset in [0, 2, 1] {
        for (entry, (overlay, expected)) in entries.iter().enumerate() {
            if entry % 3 == offset {
                assert_eq!(index.detach(*overlay), Some(*expected));
                index.assert_invariants();
            }
        }
    }
    assert!(index.is_empty());
    index.assert_invariants();
}

#[test]
fn sorted_batch_constructs_balanced_authoritative_indexes() {
    crate::test_utils::init_test_tracing();
    let entries: Vec<_> = (0..2_000)
        .map(|index| {
            let start = 10 + index * 2;
            let value = overlay(start, start + 1);
            (value, range(start, start + 1))
        })
        .collect();
    let mut index = OverlayIndex::new();

    assert!(index.attach_batch(&entries, OverlayBatchOrder::AttachmentSequence));
    assert!(index.interval_height() <= 12);
    index.assert_invariants();

    for (value, expected) in entries.iter().step_by(199) {
        assert_eq!(index.range(*value), Some(*expected));
        assert_eq!(
            index.overlays_at(expected.start()),
            vec![*value],
            "batch-built interval query diverged"
        );
        assert_eq!(
            value.as_overlay_data().unwrap().current_range(),
            (expected.start().get(), expected.end().get())
        );
    }
}

#[test]
fn batch_order_type_distinguishes_attachment_from_query_order() {
    crate::test_utils::init_test_tracing();
    let first = overlay(2, 8);
    let second = overlay(2, 8);
    let third = overlay(2, 8);
    let entries = vec![
        (first, range(2, 8)),
        (second, range(2, 8)),
        (third, range(2, 8)),
    ];

    let mut attached = OverlayIndex::new();
    assert!(attached.attach_batch(&entries, OverlayBatchOrder::AttachmentSequence));
    assert_eq!(
        attached
            .overlays_at(EmacsBytePos::new(4))
            .into_iter()
            .map(Value::bits)
            .collect::<Vec<_>>(),
        vec![third.bits(), second.bits(), first.bits()]
    );

    let mut restored = OverlayIndex::new();
    assert!(restored.attach_batch(&entries, OverlayBatchOrder::AscendingQueryOrder));
    assert_eq!(
        restored
            .overlays_at(EmacsBytePos::new(4))
            .into_iter()
            .map(Value::bits)
            .collect::<Vec<_>>(),
        vec![first.bits(), second.bits(), third.bits()]
    );
}

#[test]
fn deferred_endpoint_index_publishes_once_then_tracks_mutations() {
    crate::test_utils::init_test_tracing();
    crate::buffer::overlay::reset_endpoint_publication_interval_read_count();
    let mut index = OverlayIndex::new();
    let first = overlay(10, 12);
    let second = overlay(20, 23);
    assert!(index.attach(first, range(10, 12)));
    assert!(index.attach(second, range(20, 23)));
    assert!(index.endpoints.get().is_none());

    assert_eq!(
        index.next_boundary_after(EmacsBytePos::new(1), EmacsBytePos::new(100)),
        Some(EmacsBytePos::new(10))
    );
    assert!(index.endpoints.get().is_some());
    assert_eq!(
        crate::buffer::overlay::endpoint_publication_interval_read_count(),
        1
    );

    assert_eq!(
        index.next_boundary_after(EmacsBytePos::new(10), EmacsBytePos::new(100)),
        Some(EmacsBytePos::new(12))
    );
    assert_eq!(
        crate::buffer::overlay::endpoint_publication_interval_read_count(),
        1,
        "a published endpoint index must not touch the interval tree"
    );

    let third = overlay(30, 34);
    assert!(index.attach(third, range(30, 34)));
    assert_eq!(index.detach(second), Some(range(20, 23)));
    assert_eq!(
        index.previous_boundary_before(EmacsBytePos::new(100), EmacsBytePos::ZERO),
        Some(EmacsBytePos::new(34))
    );
    index.assert_invariants();
}

#[test]
fn reverse_endpoint_stream_is_the_exact_inverse_across_tree_levels() {
    crate::test_utils::init_test_tracing();
    let mut index = OverlayIndex::new();
    for entry in 0..65 {
        let start = 2 + entry * 3;
        let value = overlay(start, start + 2);
        assert!(index.attach(value, range(start, start + 2)));
    }

    let bounds = range(0, 256);
    let forward = index
        .endpoint_records_strictly_within(bounds, OverlayPropertyFilter::unfiltered())
        .map(|endpoint| (endpoint.position, endpoint.overlay.bits(), endpoint.kind))
        .collect::<Vec<_>>();
    let reverse = index
        .endpoint_records_strictly_within_reverse(bounds, OverlayPropertyFilter::unfiltered())
        .map(|endpoint| (endpoint.position, endpoint.overlay.bits(), endpoint.kind))
        .collect::<Vec<_>>();

    assert_eq!(reverse, forward.into_iter().rev().collect::<Vec<_>>());
}
