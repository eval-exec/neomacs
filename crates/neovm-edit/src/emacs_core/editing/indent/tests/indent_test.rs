use neovm_value::error::FlowKind;

use super::*;

use std::fs;

/// GNU reaches a `vertical-motion` goal column at `first_x + to_x`
/// (`src/indent.c:2540`), `first_x` being `it.first_visible_x` (`:2321`), i.e.
/// the hscroll.  The scanner in `indent.rs` counts columns from the LINE start,
/// so the window-relative goal has to be moved into that space.
///
/// The table is GNU Emacs 31.0.90's own, 80x24 pty, `truncate-lines' t, a line
/// starting at 202 (`scripts/l216-hscroll-origin-probe.el`, PARTA): at hscroll
/// 5 the goals 0 / 10 / 40 / 79 answer 207 / 217 / 247 / 286, every one
/// `202 + hscroll + goal`.
#[test]
fn a_goal_column_is_window_relative_and_the_walk_is_line_relative() {
    let unscrolled = ScreenLineExtent {
        first_visible_col: 0,
        last_visible_col: 79,
    };
    let scrolled = ScreenLineExtent {
        first_visible_col: 5,
        last_visible_col: 84,
    };
    for goal in [0, 10, 40, 79] {
        assert_eq!(unscrolled.goal_col_in_line_space(goal), goal);
        assert_eq!(scrolled.goal_col_in_line_space(goal), goal + 5);
    }
    // GNU clamps a negative COLS to the row start, not to a negative column.
    assert_eq!(scrolled.goal_col_in_line_space(-3), 5);
    // The edge is the OTHER term and is not the goal's business: a goal past
    // it stays past it, and the walk clamps.
    assert_eq!(scrolled.goal_col_in_line_space(200), 205);
}

/// GNU's WORD_WRAP goal backtrack lives in `move_it_in_display_line`
/// (`src/xdisp.c:10859-10888`), the CALLER of the walk, and applies only to a
/// row the walk left by continuation.  Measured, GNU Emacs 31.0.90, 80x24 pty,
/// one 300-character line of `x' with no wrap opportunity in it
/// (`tmp/l216/wrapgoal-gnu-cold.txt`): goal 80 answers 80 with `word-wrap' nil
/// and 79 with it t, while goal 79 answers 80 under both.
#[test]
fn only_word_wrap_backs_a_goal_off_the_row_edge() {
    assert!(LineWrap::Truncate.goal_stops_at_row_edge());
    assert!(LineWrap::WindowWrap.goal_stops_at_row_edge());
    assert!(!LineWrap::WordWrap.goal_stops_at_row_edge());
}

#[cfg(test)]
#[path = "gdm_columns.rs"]
mod gdm_columns;
