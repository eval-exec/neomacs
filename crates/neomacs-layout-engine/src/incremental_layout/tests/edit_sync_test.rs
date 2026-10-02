//! The walker's synchronization test (`EditSyncStop::reached_at`) and the
//! placement of the synchronized rows (`EditSyncPlan::install`).

use super::*;
use neomacs_display_protocol::frame_glyphs::GlyphRowRole;

fn row(start: usize, y: f32) -> MatrixRow {
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    row.enabled = true;
    row.start_charpos = start;
    row.end_charpos = start + 5;
    row.pixel_y = y;
    row.height_px = 10.0;
    MatrixRow::new(row)
}

/// Three rows below an edit, at old indices 5..8 and y 50..80, whose first
/// row starts at 100 after the edit.
fn plan() -> EditSyncPlan {
    EditSyncPlan {
        stop_charpos: 100,
        first_unchanged_index: 5,
        first_unchanged_y: 50.0,
        rows: vec![
            (5, row(100, 50.0)),
            (6, row(106, 60.0)),
            (7, row(112, 70.0)),
        ],
        row_snapshots: Vec::new(),
        points: Vec::new(),
        point_rows: None,
    }
}

#[test]
fn the_walk_synchronizes_only_at_the_exact_stop_position() {
    let stop = plan().stop();
    assert!(stop.reached_at(LayoutCharPos0::new(99), 50.0, 5).is_none());
    assert!(stop.reached_at(LayoutCharPos0::new(101), 50.0, 5).is_none());
    assert_eq!(
        stop.reached_at(LayoutCharPos0::new(100), 60.0, 6),
        Some(EditSyncReached {
            display_row_index: 6,
            y: 60.0
        })
    );
}

#[test]
fn rows_that_would_move_up_are_left_to_the_walk() {
    let stop = plan().stop();
    assert!(stop.reached_at(LayoutCharPos0::new(100), 40.0, 4).is_none());
    // Rows keeping their place synchronize.
    assert!(stop.reached_at(LayoutCharPos0::new(100), 50.0, 5).is_some());
}

#[test]
fn installing_moves_every_row_and_drops_what_falls_off() {
    let placed = plan().install(
        EditSyncReached {
            display_row_index: 6,
            y: 60.0,
        },
        80.0,
        usize::MAX,
    );
    assert_eq!(placed.dy, 10.0);
    let placed_rows: Vec<(usize, f32)> = placed
        .rows
        .iter()
        .map(|(index, row)| (*index, row.pixel_y))
        .collect();
    assert_eq!(placed_rows, vec![(6, 60.0), (7, 70.0)]);
}

#[test]
fn installing_in_place_keeps_the_shared_rows() {
    let plan = plan();
    let original = plan.rows[0].1.clone();
    let placed = plan.install(
        EditSyncReached {
            display_row_index: 5,
            y: 50.0,
        },
        80.0,
        usize::MAX,
    );
    assert_eq!(placed.dy, 0.0);
    assert!(std::ptr::eq(placed.rows[0].1.as_ref(), original.as_ref()));
    assert_eq!(placed.rows.len(), 3);
}

#[test]
fn installing_respects_the_chrome_index_limit() {
    let placed = plan().install(
        EditSyncReached {
            display_row_index: 6,
            y: 60.0,
        },
        f32::INFINITY,
        8,
    );
    let indices: Vec<usize> = placed.rows.iter().map(|(index, _)| *index).collect();
    assert_eq!(indices, vec![6, 7]);
}

struct StillGuard;

impl StillGuard {
    fn on() -> Self {
        set_edit_sync_still_for_test(Some(true));
        Self
    }
}

impl Drop for StillGuard {
    fn drop(&mut self) {
        set_edit_sync_still_for_test(None);
    }
}

fn populated_plan() -> EditSyncPlan {
    let mut plan = plan();
    plan.row_snapshots = plan
        .rows
        .iter()
        .map(|(index, row)| DisplayRowSnapshot {
            row: *index as i64,
            y: row.pixel_y as i64,
            height: 10,
            start_buffer_pos: Some(LispCharPos1::from_one_based_usize(row.start_charpos + 1)),
            end_buffer_pos: Some(LispCharPos1::from_one_based_usize(row.end_charpos + 1)),
            end_x: 20,
            end_col: 2,
            ..DisplayRowSnapshot::default()
        })
        .collect();
    plan.points = plan
        .rows
        .iter()
        .flat_map(|(index, row)| {
            (0..2).map(move |col| DisplayPointSnapshot {
                buffer_pos: LispCharPos1::from_one_based_usize(row.start_charpos + col + 1),
                role: neovm_core::window::DisplayPointRole::Glyph,
                x: col as i64 * 10,
                y: row.pixel_y as i64,
                width: 10,
                height: 10,
                row: *index as i64,
                col: col as i64,
            })
        })
        .collect();
    plan.point_rows = Some(neovm_core::window::DisplayPointRows::from_points(
        plan.points.clone(),
    ));
    plan
}

#[test]
fn still_installation_keeps_geometry_storage_and_snapshot_answers() {
    let _still = StillGuard::on();
    let plan = populated_plan();
    let original = plan.clone();
    let row_storage = plan.rows.as_ptr();
    let snapshot_storage = plan.row_snapshots.as_ptr();
    let point_storage = plan.points.as_ptr();
    let point_row_storage = plan.point_rows.as_ref().unwrap().rows.as_ptr();
    let placed = plan.install(
        EditSyncReached {
            display_row_index: 5,
            y: 50.0,
        },
        80.0,
        usize::MAX,
    );
    assert_eq!(placed.dy, 0.0);
    assert_eq!(placed.rows.as_ptr(), row_storage);
    assert_eq!(placed.row_snapshots.as_ptr(), snapshot_storage);
    assert_eq!(placed.points.as_ptr(), point_storage);
    assert_eq!(
        placed.point_rows.as_ref().unwrap().rows.as_ptr(),
        point_row_storage
    );
    for ((index, row), (old_index, old_row)) in placed.rows.iter().zip(&original.rows) {
        assert_eq!(index, old_index);
        assert!(std::ptr::eq(row.as_ref(), old_row.as_ref()));
    }
    assert_eq!(placed.row_snapshots, original.row_snapshots);
    assert_eq!(placed.points, original.points);
    assert_eq!(placed.point_rows, original.point_rows);
}

#[test]
fn still_installation_clips_geometry_at_the_exact_visible_bottom() {
    let _still = StillGuard::on();
    let original = populated_plan();
    for (bottom, kept) in [(70.5, 2), (75.0, 3)] {
        let placed = original.clone().install(
            EditSyncReached {
                display_row_index: 5,
                y: 50.0,
            },
            bottom,
            usize::MAX,
        );
        assert_eq!(placed.rows.len(), kept);
        assert_eq!(placed.row_snapshots, original.row_snapshots[..kept]);
        assert_eq!(placed.points, original.points[..kept * 2]);
        let rows = placed.point_rows.as_ref().unwrap();
        assert_eq!(rows.rows.len(), kept);
        assert_eq!(rows.iter_points().collect::<Vec<_>>(), placed.points);
        assert_eq!(
            rows.rows.iter().map(|row| row.row()).collect::<Vec<_>>(),
            (5..5 + kept as i64).collect::<Vec<_>>()
        );
    }
}

#[test]
fn still_installation_clips_geometry_at_the_chrome_index() {
    let _still = StillGuard::on();
    let original = populated_plan();
    let placed = original.clone().install(
        EditSyncReached {
            display_row_index: 5,
            y: 50.0,
        },
        f32::INFINITY,
        7,
    );
    assert_eq!(placed.rows.len(), 2);
    assert_eq!(placed.row_snapshots, original.row_snapshots[..2]);
    assert_eq!(placed.points, original.points[..4]);
    let rows = placed.point_rows.as_ref().unwrap();
    assert_eq!(rows.rows.len(), 2);
    assert_eq!(rows.iter_points().collect::<Vec<_>>(), placed.points);
    assert_eq!(
        rows.rows.iter().map(|row| row.row()).collect::<Vec<_>>(),
        vec![5, 6]
    );
}

#[test]
fn still_installation_remaps_row_indices_even_when_y_stays_fixed() {
    let _still = StillGuard::on();
    let mut original = populated_plan();
    let placed = original.clone().install(
        EditSyncReached {
            display_row_index: 6,
            y: 50.0,
        },
        80.0,
        usize::MAX,
    );
    assert_eq!(placed.dy, 0.0);
    assert_eq!(
        placed
            .rows
            .iter()
            .map(|(index, _)| *index)
            .collect::<Vec<_>>(),
        vec![6, 7, 8]
    );
    for row in &mut original.row_snapshots {
        row.row += 1;
    }
    for point in &mut original.points {
        point.row += 1;
    }
    assert_eq!(placed.row_snapshots, original.row_snapshots);
    assert_eq!(placed.points, original.points);
    let rows = placed.point_rows.as_ref().unwrap();
    assert_eq!(rows.rows.len(), 3);
    assert_eq!(rows.iter_points().collect::<Vec<_>>(), placed.points);
    assert_eq!(
        rows.rows.iter().map(|row| row.row()).collect::<Vec<_>>(),
        vec![6, 7, 8]
    );
}
