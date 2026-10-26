//! Unit tests for the row lookup display motion answers from.

use super::*;

const BUFFER_END: i64 = 302;

fn row(index: i64, start: i64, end: i64) -> DisplayRowSnapshot {
    DisplayRowSnapshot {
        row: index,
        start_buffer_pos: Some(LispCharPos1::new(start)),
        end_buffer_pos: Some(LispCharPos1::new(end)),
        ..Default::default()
    }
}

/// The rows a real redisplay publishes for 300 `x` characters followed by a
/// newline, in a 160-column window with `truncate-lines` on.
///
/// Read from a live TTY session: the truncated row stops where the drawing
/// stopped (GNU makes `maxpos` `it->current.pos` for "Line is truncated on
/// right", src/xdisp.c:25269), and the empty end-of-buffer row follows at the
/// line's end.  No row covers positions 160 through 301.
fn truncated_line_rows() -> Vec<DisplayRowSnapshot> {
    vec![row(0, 1, 159), row(1, BUFFER_END, BUFFER_END)]
}

fn lookup_ending_at(rows: &[DisplayRowSnapshot], pos: i64, end: i64) -> Option<usize> {
    let rows: Vec<&DisplayRowSnapshot> = rows.iter().collect();
    snapshot_row_index_for_pos_or_truncated_line(
        &rows,
        LispCharPos1::new(pos),
        LispCharPos1::new(end),
    )
}

fn lookup(rows: &[DisplayRowSnapshot], pos: i64) -> Option<usize> {
    lookup_ending_at(rows, pos, BUFFER_END)
}

#[test]
fn a_position_inside_a_row_keeps_using_that_row() {
    let rows = truncated_line_rows();
    assert_eq!(lookup(&rows, 1), Some(0));
    assert_eq!(lookup(&rows, 159), Some(0));
}

/// GNU's `Fvertical_motion` walks forward from the start of the origin's line
/// and backtracks one line when that walk overshoots a line truncated on the
/// right (src/indent.c:2393-2400).  That lands on the truncated row itself, so
/// a point past the right margin resolves to the row it is drawn on -- the
/// `C-e` case, which otherwise leaves point unable to reach the line's end.
#[test]
fn a_position_past_the_right_margin_belongs_to_the_truncated_row() {
    let rows = truncated_line_rows();
    assert_eq!(lookup(&rows, 160), Some(0));
    assert_eq!(lookup(&rows, 300), Some(0));
    assert_eq!(lookup(&rows, 301), Some(0));
}

/// The row below starts where the truncated line ends, and it keeps every
/// position from there on: the overflow rule must not reach past the line the
/// origin is on.
#[test]
fn a_position_from_the_next_row_start_belongs_to_the_row_below() {
    let rows = truncated_line_rows();
    assert_eq!(lookup(&rows, BUFFER_END), Some(1));
}

/// A gap between one row's end and the next row's start is text that row
/// stopped drawing without ending its line, so it belongs to the row above --
/// the same widening the overflow rule makes for the truncated case.
#[test]
fn a_gap_belongs_to_the_row_above_it() {
    let rows = vec![row(0, 10, 20), row(1, 30, 40)];
    assert_eq!(lookup(&rows, 25), Some(0));
}

/// A position no row starts at or before is still nothing to answer with: the
/// rule only widens a row to the end of its own line, it does not invent one.
#[test]
fn a_position_before_every_row_has_no_row() {
    let rows = vec![row(0, 10, 20), row(1, 30, 40)];
    assert_eq!(lookup(&rows, 5), None);
}

/// The last row owns the buffer's end position too, since no row follows it to
/// take it.  Measured against live GNU: for 300 characters and no trailing
/// newline in a truncated window, `(vertical-motion -1)` at the end answers the
/// line's start with 0 lines moved -- the row's own start -- not a failure to
/// resolve the origin.
#[test]
fn the_last_row_owns_the_buffer_end() {
    // 300 characters, no trailing newline: the buffer ends at 301 and only the
    // truncated row is published.
    let mut rows = vec![row(0, 1, 159)];
    rows[0].truncated_end_buffer_pos = Some(LispCharPos1::new(301));
    assert_eq!(lookup_ending_at(&rows, 301, 301), Some(0));
}

#[test]
fn a_bounded_last_row_does_not_own_unmeasured_source() {
    let rows = vec![row(0, 1, 20), row(1, 21, 40)];
    assert_eq!(lookup(&rows, 90), None);
    let mut truncated = vec![row(0, 1, 20)];
    truncated[0].truncated_end_buffer_pos = Some(LispCharPos1::new(50));
    assert_eq!(lookup(&truncated, 50), Some(0));
    assert_eq!(lookup(&truncated, 51), None);
}

fn eob_overlay_snapshot(two_lines: bool, extra_rows: usize) -> WindowDisplaySnapshot {
    use crate::window::DisplayRowEndSource;

    let end = if two_lines { 8 } else { 4 };
    let mut rows = Vec::new();
    if two_lines {
        rows.push(row(0, 1, 4));
    }
    let mut source = row(rows.len() as i64, if two_lines { 5 } else { 1 }, end);
    source.end_source = DisplayRowEndSource::OverlayAfterString;
    rows.push(source);
    for _ in 0..extra_rows {
        let mut inserted = row(rows.len() as i64, end, end);
        inserted.end_source = DisplayRowEndSource::OverlayAfterString;
        rows.push(inserted);
    }
    rows.push(DisplayRowSnapshot {
        row: rows.len() as i64,
        ..Default::default()
    });
    WindowDisplaySnapshot {
        rows,
        ..Default::default()
    }
}

fn overlay_motion(
    snapshot: &WindowDisplaySnapshot,
    end: i64,
    origin: i64,
    lines: i64,
) -> RowMotion {
    use crate::buffer::{CharLen, CharPos0, CharRange};

    vertical_motion_on_rows(
        snapshot,
        MotionRequest {
            buffer: BufferId(1),
            window: None,
            origin: LispCharPos1::new(origin),
            rows: lines,
            goal_column: None,
        },
        AccessibleCharRange::new(CharRange::from_start_len(
            CharPos0::new(0),
            CharLen::new((end - 1) as usize),
        )),
        LineWrap::WindowWrap,
    )
}

#[test]
fn eob_overlay_motion_stops_at_the_source_boundary_without_counting_virtual_rows() {
    for extra_rows in 0..3 {
        let snapshot = eob_overlay_snapshot(false, extra_rows);
        for origin in [1, 4] {
            for lines in [1, 2, 10] {
                let motion = overlay_motion(&snapshot, 4, origin, lines)
                    .settled()
                    .expect("EOB is covered even without a later buffer-source row");
                assert_eq!((motion.moved, motion.target.as_i64()), (0, 4));
            }
            for lines in [0, -1] {
                let motion = overlay_motion(&snapshot, 4, origin, lines)
                    .settled()
                    .expect("virtual EOB rows must not affect backward or zero motion");
                assert_eq!((motion.moved, motion.target.as_i64()), (0, 1));
            }
        }
    }
}

#[test]
fn a_source_row_ending_in_an_eob_overlay_keeps_its_line_start() {
    for extra_rows in 0..3 {
        let snapshot = eob_overlay_snapshot(true, extra_rows);
        for (origin, lines, expected) in [
            (1, 1, (1, 5)),
            (1, 2, (1, 8)),
            (8, 1, (0, 8)),
            (8, -1, (-1, 1)),
            (8, 0, (0, 5)),
        ] {
            let motion = overlay_motion(&snapshot, 8, origin, lines)
                .settled()
                .expect("motion resolves the source row before its virtual tail");
            assert_eq!((motion.moved, motion.target.as_i64()), expected);
        }
    }
}

#[test]
fn an_overlay_at_a_partial_query_boundary_still_requires_more_source_rows() {
    let snapshot = eob_overlay_snapshot(false, 2);
    assert!(overlay_motion(&snapshot, 8, 1, 1).settled().is_none());
    assert!(overlay_motion(&snapshot, 8, 7, 1).settled().is_none());
}
