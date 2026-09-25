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
