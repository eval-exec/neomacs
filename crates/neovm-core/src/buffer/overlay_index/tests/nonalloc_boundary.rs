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

fn assert_cold_and_published_queries(index: &OverlayIndex, endpoints: &[usize]) {
    assert!(index.endpoints.get().is_none());
    let queries = [
        (0, 20),
        (1, 2),
        (2, 3),
        (3, 4),
        (4, 5),
        (5, 6),
        (6, 6),
        (6, 7),
        (7, 12),
        (10, 12),
        (12, 20),
        (20, 12),
    ];
    for (position, limit) in queries {
        let expected = endpoints
            .iter()
            .copied()
            .filter(|endpoint| position < *endpoint && *endpoint <= limit)
            .min()
            .map(EmacsBytePos::new);
        assert_eq!(
            index.next_boundary_after_no_alloc(
                EmacsBytePos::new(position),
                EmacsBytePos::new(limit)
            ),
            expected,
            "cold query ({position}, {limit}]"
        );
        assert!(
            index.endpoints.get().is_none(),
            "borrowed query must not publish the allocating endpoint index"
        );
    }
    // Only the canonical query may publish; both representations must agree.
    for (position, limit) in queries {
        assert_eq!(
            index.next_boundary_after_no_alloc(
                EmacsBytePos::new(position),
                EmacsBytePos::new(limit)
            ),
            index.next_boundary_after(EmacsBytePos::new(position), EmacsBytePos::new(limit)),
            "canonical query ({position}, {limit}]"
        );
    }
    assert!(index.endpoints.get().is_some());
    for (position, limit) in queries {
        assert_eq!(
            index.next_boundary_after_no_alloc(
                EmacsBytePos::new(position),
                EmacsBytePos::new(limit)
            ),
            index.next_boundary_after(EmacsBytePos::new(position), EmacsBytePos::new(limit)),
            "published query ({position}, {limit}]"
        );
    }
}

#[test]
fn no_alloc_boundary_keeps_nested_equal_and_zero_length_endpoints_cold() {
    crate::test_utils::init_test_tracing();
    let mut index = OverlayIndex::new();
    for (start, end) in [(2, 12), (4, 6), (6, 6), (6, 10), (12, 12)] {
        assert!(index.attach(overlay(start, end), range(start, end)));
    }
    assert_cold_and_published_queries(&index, &[2, 12, 4, 6, 6, 6, 6, 10, 12, 12]);
}

#[test]
fn no_alloc_boundary_observes_moved_and_detached_cold_records() {
    crate::test_utils::init_test_tracing();
    let mut index = OverlayIndex::new();
    let moved = overlay(2, 12);
    let removed = overlay(4, 6);
    let stationary = overlay(6, 6);
    for (value, bounds) in [
        (moved, range(2, 12)),
        (removed, range(4, 6)),
        (stationary, range(6, 6)),
    ] {
        assert!(index.attach(value, bounds));
    }
    assert_eq!(index.move_to(moved, range(1, 10)), Some(range(2, 12)));
    assert_eq!(index.detach(removed), Some(range(4, 6)));
    assert_cold_and_published_queries(&index, &[1, 10, 6, 6]);
}

#[test]
fn no_alloc_boundary_observes_deletion_clipping_without_publication() {
    crate::test_utils::init_test_tracing();
    let mut index = OverlayIndex::new();
    let outer = overlay(2, 12);
    let clipped = overlay(4, 7);
    let zero = overlay(10, 10);
    for (value, bounds) in [
        (outer, range(2, 12)),
        (clipped, range(4, 7)),
        (zero, range(10, 10)),
    ] {
        assert!(index.attach(value, bounds));
    }
    index.adjust_for_text_edit(OverlayTextEdit::Delete { range: range(5, 9) });
    assert_eq!(index.range(outer), Some(range(2, 8)));
    assert_eq!(index.range(clipped), Some(range(4, 5)));
    assert_eq!(index.range(zero), Some(range(6, 6)));
    assert_cold_and_published_queries(&index, &[2, 8, 4, 5, 6, 6]);
}
