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
