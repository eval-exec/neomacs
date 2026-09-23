//! Unit tests for the empty-line fringe fill request policy. The end-to-end
//! "filler rows below ZV carry the empty-line bitmap" behavior is covered by the
//! integration test in `engine_test.rs`
//! (`layout_frame_rust_fills_empty_line_fringe_below_buffer_end`); here we lock
//! down the side/suppression policy without standing up a full frame.

use super::*;

#[test]
fn off_when_indicate_empty_lines_disabled() {
    assert!(
        EndOfBufferRowsFillRequest::from_parts(0, false)
            .side()
            .is_none()
    );
}

#[test]
fn left_fringe_for_value_one() {
    assert_eq!(
        EndOfBufferRowsFillRequest::from_parts(1, false).side(),
        Some(EmptyLineFringeSide::Left),
        "indicate-empty-lines = 1 (t/left) draws in the left fringe"
    );
}

#[test]
fn right_fringe_for_value_two() {
    assert_eq!(
        EndOfBufferRowsFillRequest::from_parts(2, false).side(),
        Some(EmptyLineFringeSide::Right),
        "indicate-empty-lines = 2 (right) draws in the right fringe"
    );
}

#[test]
fn never_in_minibuffer() {
    // GNU `!MINI_WINDOW_P (it->w)` — the echo area never shows empty-line marks.
    assert!(
        EndOfBufferRowsFillRequest::from_parts(1, true)
            .side()
            .is_none(),
        "empty-line indicator is suppressed in the mini-window even when on"
    );
}
