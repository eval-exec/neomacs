use super::*;

fn point(pos: i64, row: i64, col: i64, x: i64, y: i64) -> DisplayPointSnapshot {
    DisplayPointSnapshot {
        buffer_pos: LispCharPos1::new(pos),
        role: DisplayPointRole::Glyph,
        x,
        y,
        width: 10,
        height: 12,
        row,
        col,
    }
}

fn canonical(mut points: Vec<DisplayPointSnapshot>) -> Vec<DisplayPointSnapshot> {
    points.sort_by_key(|point| (point.buffer_pos, point.row, point.col, point.x));
    points
}

#[test]
fn checked_compact_cells_preserve_geometry_and_independent_source_x_orders() {
    let mut terminator = point(103, 7, 3, 45, 30);
    terminator.width = 0;
    terminator.height = 29;
    let original = vec![
        point(102, 7, 0, 0, 30),
        point(101, 7, 2, 20, 31),
        terminator,
    ];
    let row = DisplayPointRow::from_points(original.clone());
    assert_eq!(std::mem::size_of::<PointCell>(), 16);
    assert!(row.is_compact());
    assert_eq!(
        row.points().collect::<Vec<_>>(),
        canonical(original.clone())
    );
    let mut visual = original;
    visual.sort_by_key(|point| (point.x, point.col, point.buffer_pos));
    assert_eq!(row.points_x_order().collect::<Vec<_>>(), visual);
    assert_eq!(row.max_height(), 29);
    assert_eq!(row.min_x(), Some(0));
    assert_eq!(row.max_x(), Some(46));
}

#[test]
fn wide_fallback_preserves_every_i64_and_rejects_placement_overflow() {
    let mut left = point(i64::MIN, i64::MAX, i64::MIN, i64::MIN, i64::MIN);
    left.width = i64::MIN;
    left.height = i64::MAX;
    left.role = DisplayPointRole::OverlaidMarker;
    let mut right = point(i64::MAX, i64::MAX, i64::MAX, i64::MAX, i64::MAX);
    right.width = i64::MAX;
    right.height = i64::MIN;
    let original = vec![right, left];
    let row = DisplayPointRow::from_points(original.clone());
    assert!(!row.is_compact());
    assert_eq!(row.points().collect::<Vec<_>>(), canonical(original));
    assert!(row.try_replaced_placement(0, row.y(), 1).is_none());
    assert!(row.try_replaced_placement(0, i64::MIN, 0).is_none());
    let unchanged = row.try_replaced_placement(0, row.y(), 0).unwrap();
    assert!(row.shares_cells_with(&unchanged));
}

#[test]
fn every_compact_field_has_a_lossless_wide_fallback() {
    let ordinary = point(1, 0, 0, 0, 0);
    let mut cases = Vec::new();
    for value in [i64::MIN, i64::MAX] {
        let mut p = ordinary.clone();
        p.x = value;
        cases.push(p);
        let mut p = ordinary.clone();
        p.col = value;
        cases.push(p);
        let mut p = ordinary.clone();
        p.width = value;
        cases.push(p);
        let mut p = ordinary.clone();
        p.height = value;
        cases.push(p);
        let mut p = ordinary.clone();
        p.y = value;
        cases.push(p);
        let mut p = ordinary.clone();
        p.buffer_pos = LispCharPos1::new(value);
        cases.push(p);
    }
    for exceptional in cases {
        let original = vec![ordinary.clone(), exceptional];
        let row = DisplayPointRow::from_points(original.clone());
        assert!(!row.is_compact());
        assert_eq!(row.points().collect::<Vec<_>>(), canonical(original));
    }
}

#[test]
fn relocation_shares_cells_and_preserves_offsets_after_repeated_moves() {
    for width in [10, i64::MAX] {
        let mut first = point(101, 4, 0, -3, 30);
        first.width = width;
        let row = DisplayPointRow::from_points(vec![first.clone(), point(102, 4, 1, 10, 31)]);
        let moved = row.replaced_placement(7, 45, 20);
        assert!(row.shares_cells_with(&moved));
        let mut expected = first;
        expected.row = 7;
        expected.y = 45;
        expected.buffer_pos = LispCharPos1::new(121);
        assert_eq!(moved.points().next(), Some(expected));
        let moved_again = moved.replaced_placement(-3, -40, -10);
        assert!(row.shares_cells_with(&moved_again));
        assert_eq!(
            moved_again.min_buffer_position(),
            Some(LispCharPos1::new(111))
        );
        assert_eq!(
            moved_again.max_buffer_position(),
            Some(LispCharPos1::new(112))
        );
        assert_eq!(moved_again.points().next().unwrap().y, -40);
        assert_eq!(moved_again.points().next_back().unwrap().y, -39);
    }
}

#[test]
fn window_merge_keeps_source_row_column_x_order_and_duplicate_walk_ties() {
    let mut marker = point(5, 0, 2, 20, 0);
    marker.role = DisplayPointRole::OverlaidMarker;
    let original = vec![
        point(6, 1, 0, 0, 12),
        point(5, 1, 1, 10, 12),
        marker.clone(),
        point(5, 0, 1, 10, 0),
        marker,
        point(1, 0, 0, 0, 0),
    ];
    let rows = DisplayPointRows::from_points(original.clone());
    assert_eq!(rows.iter_points().collect::<Vec<_>>(), canonical(original));
    assert_eq!(rows.point_count(), 6);
    assert_eq!(rows.rows.len(), 2);
}
