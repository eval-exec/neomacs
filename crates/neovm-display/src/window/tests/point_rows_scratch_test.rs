//! Freezing must preserve stable indexes and immutable ownership across reuse.
use super::*;

fn point(pos: i64, col: i64, x: i64) -> DisplayPointSnapshot {
    DisplayPointSnapshot {
        buffer_pos: LispCharPos1::new(pos),
        role: DisplayPointRole::Glyph,
        x,
        y: 16,
        width: 8,
        height: 16,
        row: 1,
        col,
    }
}

fn source_order(mut points: Vec<DisplayPointSnapshot>) -> Vec<DisplayPointSnapshot> {
    points.sort_by_key(|point| (point.buffer_pos, point.row, point.col, point.x));
    points
}

fn x_order(mut points: Vec<DisplayPointSnapshot>) -> Vec<DisplayPointSnapshot> {
    points.sort_by_key(|point| (point.x, point.col, point.buffer_pos));
    points
}

#[test]
fn point_row_identity_orders_reuse_compact_scratch_without_aliasing_frozen_cells() {
    fn require_send_sync<T: Send + Sync + 'static>() {}
    require_send_sync::<DisplayPointRow>();
    for count in [0, 1, 50, 1_000, 100_000] {
        let mut scratch = Vec::with_capacity(count.max(1));
        scratch.extend(
            (0..count)
                .map(|index| point(index as i64 + 1, (index % 4096) as i64, index as i64 * 8)),
        );
        let expected = scratch.clone();
        let allocation = scratch.as_ptr();
        let capacity = scratch.capacity();
        let row = DisplayPointRow::from_points_reusing(&mut scratch);
        assert!(scratch.is_empty());
        assert_eq!(scratch.as_ptr(), allocation);
        assert_eq!(scratch.capacity(), capacity);
        assert!(row.is_compact());
        assert!(matches!(row.data.source_order, PointOrder::Identity { len } if len == count));
        assert!(matches!(row.data.x_order, PointOrder::Identity { len } if len == count));
        assert_eq!(row.points().collect::<Vec<_>>(), expected);
        assert_eq!(row.points_x_order().collect::<Vec<_>>(), expected);

        scratch.push(point(200_001, 0, 24));
        let next = DisplayPointRow::from_points_reusing(&mut scratch);
        assert_eq!(next.point_count(), 1);
        assert_eq!(row.points().collect::<Vec<_>>(), expected);
        let moved = row.replaced_placement(7, 32, 10);
        assert!(moved.shares_cells_with(&row));
        let mut expected_moved = expected;
        for point in &mut expected_moved {
            point.buffer_pos = LispCharPos1::new(point.buffer_pos.as_i64() + 10);
            point.row = 7;
            point.y += 16;
        }
        assert_eq!(moved.points().collect::<Vec<_>>(), expected_moved);
    }
}

#[test]
fn point_row_reused_scratch_keeps_independent_stable_source_and_x_orders() {
    let mut marker = point(2, 1, 8);
    marker.role = DisplayPointRole::OverlaidMarker;
    for original in [
        vec![point(1, 0, 16), point(2, 1, 8)],
        vec![point(2, 0, 0), point(1, 1, 8)],
        vec![point(3, 2, 24), marker, point(2, 1, 8), point(1, 0, 16)],
    ] {
        let mut scratch = original.clone();
        let row = DisplayPointRow::from_points_reusing(&mut scratch);
        assert!(scratch.is_empty());
        assert!(row.is_compact());
        assert_eq!(
            row.points().collect::<Vec<_>>(),
            source_order(original.clone())
        );
        assert_eq!(
            row.points_x_order().collect::<Vec<_>>(),
            x_order(original.clone())
        );
        assert_eq!(row.points_emission_order().collect::<Vec<_>>(), original);
        scratch.extend([point(99, 0, 0), point(98, 1, 8)]);
        let next = DisplayPointRow::from_points_reusing(&mut scratch);
        assert_eq!(next.point_count(), 2);
        assert_eq!(row.points().collect::<Vec<_>>(), source_order(original));
    }
}

#[test]
fn point_row_reused_scratch_wide_fallback_retains_geometry_after_producer_reuse() {
    let original = vec![point(2, 1, i64::MAX), point(1, 0, i64::MIN)];
    let mut scratch = original.clone();
    let row = DisplayPointRow::from_points_reusing(&mut scratch);
    assert!(!row.is_compact());
    assert!(scratch.is_empty());
    let reader = row.clone();
    let expected = source_order(original.clone());
    let read = std::thread::spawn(move || reader.points().collect::<Vec<_>>());
    scratch.extend([point(3, 0, 0), point(4, 1, 8)]);
    let compact = DisplayPointRow::from_points_reusing(&mut scratch);
    assert!(compact.is_compact());
    assert_eq!(read.join().unwrap(), expected);
    assert_eq!(row.points().collect::<Vec<_>>(), expected);
    assert_eq!(row.points_x_order().collect::<Vec<_>>(), x_order(original));
}
