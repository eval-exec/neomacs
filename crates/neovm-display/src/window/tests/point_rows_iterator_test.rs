//! Iterator diets preserve the full legacy decoded stream and exact lengths.

use super::*;
use std::iter::FusedIterator;

fn point(pos: i64, row: i64, col: i64, x: i64, role: DisplayPointRole) -> DisplayPointSnapshot {
    DisplayPointSnapshot {
        buffer_pos: LispCharPos1::new(pos),
        role,
        x,
        y: row * 16,
        width: 8,
        height: 16,
        row,
        col,
    }
}

fn fixture(input: &[Vec<DisplayPointSnapshot>]) -> DisplayPointRows {
    DisplayPointRows {
        rows: input
            .iter()
            .cloned()
            .map(DisplayPointRow::from_points)
            .collect(),
    }
}

/// Independently sort producer records by the existing heap's vector-row tie
/// order and each row's stable source key. Numeric row IDs are not a new
/// ordering precondition when callers replace the public descriptor vector.
fn expected(input: &[Vec<DisplayPointSnapshot>]) -> Vec<DisplayPointSnapshot> {
    let mut points: Vec<_> = input
        .iter()
        .enumerate()
        .flat_map(|(row_index, row)| row.iter().cloned().map(move |point| (row_index, point)))
        .collect();
    points.sort_by_key(|(row_index, point)| (point.buffer_pos, *row_index, point.col, point.x));
    points.into_iter().map(|(_, point)| point).collect()
}

fn assert_stream(
    mut iter: impl ExactSizeIterator<Item = DisplayPointSnapshot> + FusedIterator,
    expected: &[DisplayPointSnapshot],
) {
    for (index, point) in expected.iter().enumerate() {
        let remaining = expected.len() - index;
        assert_eq!(iter.len(), remaining);
        assert_eq!(iter.size_hint(), (remaining, Some(remaining)));
        assert_eq!(iter.next(), Some(point.clone()));
    }
    for _ in 0..3 {
        assert_eq!(iter.next(), None);
        assert_eq!(iter.len(), 0);
        assert_eq!(iter.size_hint(), (0, Some(0)));
    }
}

fn assert_modes(rows: &DisplayPointRows, expected: &[DisplayPointSnapshot]) {
    for mode in [PointRowIterMode::Off, PointRowIterMode::On] {
        assert_stream(rows.iter_points_with_mode(mode), expected);
        assert_stream(
            WindowDisplayPointIter::Rows(rows.iter_points_with_mode(mode)),
            expected,
        );
    }
    assert_stream(WindowDisplayPointIter::new(expected, None), expected);
}

#[test]
fn mode_defaults_on_and_accepts_boolean_aliases_without_process_state() {
    assert_eq!(PointRowIterMode::from_setting(None), PointRowIterMode::On);
    for setting in [
        Some(""),
        Some("off"),
        Some("0"),
        Some("false"),
        Some("no"),
        Some("unknown"),
    ] {
        assert_eq!(
            PointRowIterMode::from_setting(setting),
            PointRowIterMode::Off
        );
    }
    for setting in ["on", "1", "true", "yes", " ON ", "True"] {
        assert_eq!(
            PointRowIterMode::from_setting(Some(setting)),
            PointRowIterMode::On
        );
    }
}

#[test]
fn boundary_ties_and_bidi_keep_the_entire_legacy_stream() {
    let input = vec![
        vec![
            point(3, 0, 2, 0, DisplayPointRole::Glyph),
            point(1, 0, 0, 24, DisplayPointRole::Glyph),
            point(3, 0, 1, 8, DisplayPointRole::OverlaidMarker),
            point(3, 0, 1, 8, DisplayPointRole::Glyph),
        ],
        vec![
            point(5, 1, 1, 0, DisplayPointRole::Glyph),
            point(3, 1, 0, 8, DisplayPointRole::Glyph),
        ],
    ];
    let rows = fixture(&input);
    assert!(matches!(
        rows.iter_points_with_mode(PointRowIterMode::On).order,
        PointRowsIterOrder::Concat { .. }
    ));
    assert_modes(&rows, &expected(&input));
}

#[test]
fn empty_descriptors_never_change_bounds_or_remaining_length() {
    for input in [
        vec![],
        vec![vec![], vec![]],
        vec![
            vec![],
            vec![point(2, 1, 0, 0, DisplayPointRole::Glyph)],
            vec![],
        ],
        vec![
            vec![point(2, 1, 0, 0, DisplayPointRole::Glyph)],
            vec![],
            vec![point(2, 2, 0, 0, DisplayPointRole::OverlaidMarker)],
        ],
    ] {
        assert_modes(&fixture(&input), &expected(&input));
    }
}

#[test]
fn overlapping_or_reversed_ranges_use_the_legacy_heap() {
    let first = vec![
        point(1, 0, 0, 0, DisplayPointRole::Glyph),
        point(4, 0, 1, 8, DisplayPointRole::OverlaidMarker),
    ];
    let second = vec![
        point(2, 1, 0, 8, DisplayPointRole::Glyph),
        point(5, 1, 1, 0, DisplayPointRole::Glyph),
    ];
    for input in [vec![first.clone(), second.clone()], vec![second, first]] {
        let rows = fixture(&input);
        assert!(matches!(
            rows.iter_points_with_mode(PointRowIterMode::On).order,
            PointRowsIterOrder::Merge { .. }
        ));
        assert_modes(&rows, &expected(&input));
    }
}

#[test]
fn wide_and_shifted_endpoints_preserve_every_decoded_field() {
    let mut wide = point(i64::MAX - 20, 2, -1, i64::MAX, DisplayPointRole::Glyph);
    wide.y = i64::MIN + 20;
    wide.width = i64::MAX;
    let mut last = wide.clone();
    last.buffer_pos = LispCharPos1::new(i64::MAX - 10);
    last.col = 1;
    last.role = DisplayPointRole::OverlaidMarker;
    let mut input = vec![
        vec![point(2, 0, 0, -12, DisplayPointRole::Glyph)],
        vec![last, wide],
    ];
    let mut rows = fixture(&input);
    assert!(rows.rows[0].is_compact());
    rows.rows[0] = rows.rows[0].replaced_placement(7, -5, 10);
    input[0][0].buffer_pos = LispCharPos1::new(12);
    input[0][0].row = 7;
    input[0][0].y = -5;
    assert!(!rows.rows[1].is_compact());
    rows.rows[1] = rows.rows[1]
        .try_replaced_placement(5, i64::MIN + 120, -1)
        .expect("shifted bounds remain representable");
    for point in &mut input[1] {
        point.buffer_pos = LispCharPos1::new(point.buffer_pos.as_i64() - 1);
        point.row = 5;
        point.y += 100;
    }
    assert_modes(&rows, &expected(&input));
}

#[test]
fn public_vector_mutations_recompute_proof_and_keep_vector_index_ties() {
    let mut input = vec![
        vec![point(2, 8, 7, 24, DisplayPointRole::OverlaidMarker)],
        vec![point(2, 8, 0, 0, DisplayPointRole::Glyph)],
        vec![point(5, 1, 0, -8, DisplayPointRole::Glyph)],
    ];
    let mut rows = fixture(&input);
    // Duplicate/out-of-order numeric row IDs retain the existing vector tie.
    assert_modes(&rows, &expected(&input));
    input.swap(0, 1);
    rows.rows.swap(0, 1);
    assert_modes(&rows, &expected(&input));
    input.swap(0, 2);
    rows.rows.swap(0, 2);
    assert_modes(&rows, &expected(&input));
    input.remove(0);
    rows.rows.remove(0);
    input.insert(1, Vec::new());
    rows.rows
        .insert(1, DisplayPointRow::from_points(Vec::new()));
    assert_modes(&rows, &expected(&input));
    input[2][0].buffer_pos = LispCharPos1::new(1);
    rows.rows[2] = rows.rows[2].replaced_placement(rows.rows[2].row(), rows.rows[2].y(), -1);
    assert_modes(&rows, &expected(&input));
}

#[test]
fn partial_consumers_keep_exact_lengths_and_fused_exhaustion() {
    let input = vec![
        vec![point(1, 0, 0, 0, DisplayPointRole::Glyph)],
        vec![],
        vec![
            point(2, 1, 0, 8, DisplayPointRole::Glyph),
            point(3, 1, 1, 0, DisplayPointRole::OverlaidMarker),
        ],
    ];
    let rows = fixture(&input);
    let expected = expected(&input);
    for mode in [PointRowIterMode::Off, PointRowIterMode::On] {
        let mut iter = rows.iter_points_with_mode(mode);
        assert_eq!(iter.nth(1), Some(expected[1].clone()));
        assert_stream(iter, &expected[2..]);
        assert_eq!(rows.iter_points_with_mode(mode).count(), expected.len());
        assert_eq!(
            rows.iter_points_with_mode(mode).last(),
            expected.last().cloned()
        );
        assert_eq!(
            rows.iter_points_with_mode(mode).take(2).collect::<Vec<_>>(),
            expected[..2]
        );
        let mut iter = rows.iter_points_with_mode(mode);
        assert_eq!(iter.nth(expected.len()), None);
        assert_stream(iter, &[]);
    }
}

#[test]
fn concurrent_borrowed_readers_have_independent_numeric_cursors() {
    let input = vec![
        vec![point(1, 0, 0, 8, DisplayPointRole::Glyph)],
        vec![
            point(3, 1, 0, 8, DisplayPointRole::Glyph),
            point(2, 1, 1, 0, DisplayPointRole::OverlaidMarker),
        ],
    ];
    let rows = fixture(&input);
    let expected = expected(&input);
    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| assert_modes(&rows, &expected));
        }
    });
}
