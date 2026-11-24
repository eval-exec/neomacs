use super::*;
use crate::buffer::BufferId;
use neovm_value::heap_types::OverlayDataInit;

fn overlay(start: usize, end: usize) -> Value {
    Value::make_overlay(OverlayDataInit {
        serial: neovm_value::heap_types::OverlaySerialSource::Mint,
        plist: Value::NIL,
        buffer: Some(BufferId(1)),
        start,
        end,
        front_advance: false,
        rear_advance: false,
    })
}

#[doc(hidden)]
pub fn range(start: usize, end: usize) -> EmacsByteRange {
    EmacsByteRange::from_usize(start, end)
}

fn assert_cold_and_published_queries(index: &OverlayIndex, endpoints: &[usize]) {
    assert!(index.published_endpoints().is_none());
    let queries = [
        (0, 20),
        (1, 2),
        (2, 3),
        (3, 4),
        (4, 5),
        (5, 6),
        (6, 6),
        (6, 7),
        (7, 6),
        (7, 12),
        (10, 12),
        (12, 20),
        (20, 12),
    ];
    let next_expected = |position, limit| {
        endpoints
            .iter()
            .copied()
            .filter(|endpoint| position < *endpoint && *endpoint <= limit)
            .min()
            .map(EmacsBytePos::new)
    };
    let previous_expected = |position, limit| {
        endpoints
            .iter()
            .copied()
            .filter(|endpoint| limit <= *endpoint && *endpoint < position)
            .max()
            .map(EmacsBytePos::new)
    };
    let cold_next = queries.map(|(position, limit)| {
        let actual = index
            .next_boundary_after_no_alloc(EmacsBytePos::new(position), EmacsBytePos::new(limit));
        assert_eq!(
            actual,
            next_expected(position, limit),
            "cold next ({position}, {limit}]"
        );
        assert_eq!(
            actual,
            index.next_boundary_after(EmacsBytePos::new(position), EmacsBytePos::new(limit)),
            "canonical cold next ({position}, {limit}]"
        );
        assert!(
            index.published_endpoints().is_none(),
            "borrowed next query must not publish the allocating endpoint index"
        );
        actual
    });
    // Mirror every next-query position/limit pair. This includes equal and
    // reversed limits, plus a real endpoint exactly at the inclusive lower limit.
    let previous_queries = queries.map(|(position, limit)| (limit, position));
    let cold_previous = previous_queries.map(|(position, limit)| {
        let actual =
            index.previous_boundary_before(EmacsBytePos::new(position), EmacsBytePos::new(limit));
        assert_eq!(
            actual,
            previous_expected(position, limit),
            "cold previous [{limit}, {position})"
        );
        assert!(
            index.published_endpoints().is_none(),
            "borrowed previous query must not publish the allocating endpoint index"
        );
        actual
    });
    let _ = index.endpoint_index();
    assert!(index.published_endpoints().is_some());
    for (i, (position, limit)) in queries.into_iter().enumerate() {
        let actual = index
            .next_boundary_after_no_alloc(EmacsBytePos::new(position), EmacsBytePos::new(limit));
        assert_eq!(
            actual,
            next_expected(position, limit),
            "published next ({position}, {limit}]"
        );
        assert_eq!(
            actual, cold_next[i],
            "cold/published next ({position}, {limit}]"
        );
        assert_eq!(
            actual,
            index.next_boundary_after(EmacsBytePos::new(position), EmacsBytePos::new(limit)),
            "canonical published next ({position}, {limit}]"
        );
    }
    for (i, (position, limit)) in previous_queries.into_iter().enumerate() {
        let actual =
            index.previous_boundary_before(EmacsBytePos::new(position), EmacsBytePos::new(limit));
        assert_eq!(
            actual,
            previous_expected(position, limit),
            "published previous [{limit}, {position})"
        );
        assert_eq!(
            actual, cold_previous[i],
            "cold/published previous [{limit}, {position})"
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

#[test]
fn no_alloc_previous_boundary_keeps_unique_empty_overlay_at_lower_limit() {
    crate::test_utils::init_test_tracing();
    let mut index = OverlayIndex::new();
    for (start, end) in [(2, 12), (6, 6)] {
        assert!(index.attach(overlay(start, end), range(start, end)));
    }
    // No nonempty overlay contributes an endpoint at 6. In particular,
    // previous(7, 6) must return this empty overlay's endpoint at LIMIT;
    // previous(6, 6) and the reversed previous(6, 7) must return None.
    assert_cold_and_published_queries(&index, &[2, 12, 6, 6]);
}
