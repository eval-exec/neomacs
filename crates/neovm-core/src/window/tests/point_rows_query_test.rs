use crate::buffer::LispCharPos1;
use crate::window::{
    DisplayPointRole, DisplayPointRows, DisplayPointSnapshot, DisplayRowSnapshot,
    WindowDisplaySnapshot, WindowPart,
};

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
fn shared_and_flat_snapshot_queries_match_for_hidden_bidi_marker_and_end_slots() {
    let mut marker = point(5, 0, 2, 20, -2);
    marker.role = DisplayPointRole::OverlaidMarker;
    let mut end = point(10, 1, 3, 30, 12);
    end.width = 0;
    let points = canonical(vec![
        point(1, 0, 0, 15, -2),
        marker,
        point(5, 1, 0, 0, 12),
        point(7, 1, 2, 10, 12),
        point(6, 1, 1, 20, 12),
        end,
    ]);
    let flat = WindowDisplaySnapshot {
        points,
        rows: vec![
            DisplayRowSnapshot {
                row: 0,
                y: -2,
                height: 14,
                start_buffer_pos: Some(LispCharPos1::ONE),
                end_buffer_pos: Some(LispCharPos1::new(5)),
                ..DisplayRowSnapshot::default()
            },
            DisplayRowSnapshot {
                row: 1,
                y: 12,
                height: 12,
                start_buffer_pos: Some(LispCharPos1::new(5)),
                end_buffer_pos: Some(LispCharPos1::new(10)),
                ..DisplayRowSnapshot::default()
            },
        ],
        ..WindowDisplaySnapshot::default()
    };
    let mut shared = flat.clone();
    shared.point_rows = Some(DisplayPointRows::from_points(std::mem::take(
        &mut shared.points,
    )));
    assert_eq!(flat, shared);
    assert!(shared.points.is_empty());
    assert!(shared.has_points());
    assert_eq!(shared.point_count(), flat.points.len());
    for pos in 0..=11 {
        assert_eq!(
            shared.point_for_buffer_pos(LispCharPos1::new(pos)),
            flat.point_for_buffer_pos(LispCharPos1::new(pos))
        );
    }
    assert_eq!(
        shared
            .point_for_buffer_pos(LispCharPos1::new(5))
            .unwrap()
            .row,
        1
    );
    for y in [-2, 0, 11, 12, 23, 24, 40] {
        for x in [-5, 0, 10, 19, 20, 30, 90] {
            let at = WindowPart::Text.text_area_coordinate(x, y, 0).unwrap();
            assert_eq!(
                shared.point_at_coords(at),
                flat.point_at_coords(at),
                "({x},{y})"
            );
        }
    }
    assert_eq!(shared.materialize_points_mut(), &flat.points);
    assert!(shared.point_rows.is_none());
    assert_eq!(flat, shared);
}

#[test]
fn empty_rows_keep_coordinate_fallback_and_cold_replacement_invalidates_cells() {
    let flat = WindowDisplaySnapshot {
        rows: vec![DisplayRowSnapshot {
            row: 0,
            y: 0,
            height: 12,
            start_buffer_pos: Some(LispCharPos1::ONE),
            end_buffer_pos: Some(LispCharPos1::ONE),
            ..DisplayRowSnapshot::default()
        }],
        ..WindowDisplaySnapshot::default()
    };
    let mut shared = flat.clone();
    shared.point_rows = Some(DisplayPointRows::default());
    assert_eq!(shared, flat);
    assert!(!shared.has_points());
    for y in [0, 20] {
        let at = WindowPart::Text.text_area_coordinate(0, y, 0).unwrap();
        assert_eq!(shared.point_at_coords(at), flat.point_at_coords(at));
    }
    shared.set_points(vec![point(1, 0, 0, 0, 0)]);
    assert!(shared.point_rows.is_none());
    assert!(shared.has_points());
}
