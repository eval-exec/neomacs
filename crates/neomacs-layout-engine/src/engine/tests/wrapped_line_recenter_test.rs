//! Point leaving the bottom of a window whose lines wrap, with the default
//! `scroll-conservatively` 0. GNU `redisplay_window` goes to `recenter:`,
//! which moves back from point by DISPLAY rows (`move_it_vertically_backward`,
//! src/xdisp.c:21128-21223): point lands in the middle of the window after one
//! `try_window`, and the following motions stay on screen.

use super::*;
use crate::engine::viewport_retry_depth_probe as probe;

/// Text rows of `window` in the last published snapshot.
fn text_rows(
    eval: &Context,
    frame: neovm_core::window::FrameId,
    window: neovm_core::window::WindowId,
) -> Vec<neovm_core::window::DisplayRowSnapshot> {
    eval.frame_manager()
        .get(frame)
        .unwrap()
        .redisplay_snapshot(window)
        .unwrap()
        .rows
        .iter()
        .filter(|row| row.start_buffer_pos.is_some())
        .cloned()
        .collect()
}

/// Row index of a point that sits at the start of a logical line.
fn row_starting_at(rows: &[neovm_core::window::DisplayRowSnapshot], point: i64) -> Option<usize> {
    rows.iter()
        .position(|row| row.start_buffer_pos.map(LispCharPos1::as_i64) == Some(point))
}

fn window_start(
    eval: &Context,
    frame: neovm_core::window::FrameId,
    window: neovm_core::window::WindowId,
) -> i64 {
    match eval
        .frame_manager()
        .get(frame)
        .unwrap()
        .find_window(window)
        .unwrap()
    {
        neovm_core::window::Window::Leaf { window_start, .. } => window_start.as_i64(),
        _ => unreachable!("selected window is a leaf"),
    }
}

/// Characters per text row, measured from the first (continued) row.
fn row_columns(rows: &[neovm_core::window::DisplayRowSnapshot]) -> i64 {
    let row = &rows[0];
    row.end_buffer_pos.unwrap().as_i64() - row.start_buffer_pos.unwrap().as_i64()
}

#[test]
fn recentering_below_wrapped_lines_takes_one_walk_and_centres_point() {
    const LINE_CHARS: usize = 520;
    let line = format!("{}\n", "x".repeat(LINE_CHARS));
    let (mut eval, frame, _buffer, window) = incr_editing_frame(&line.repeat(200), 640, 400);
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    engine.layout_frame_rust(&mut eval, frame);
    let rows = text_rows(&eval, frame, window);
    let window_rows = rows.len();
    let rows_per_line = LINE_CHARS.div_ceil(row_columns(&rows) as usize);
    assert!(
        rows_per_line >= 3 && window_rows >= 12,
        "fixture must wrap every line over several rows: {window_rows} rows, {rows_per_line} rows per line"
    );

    // A motion that stays on screen: the cost of a frame with no scroll.
    eval.eval_str("(goto-char (point-min)) (forward-line 1)")
        .unwrap();
    probe::reset();
    engine.layout_frame_rust(&mut eval, frame);
    let still_attempts = probe::attempts();

    // Point at the start of the first logical line that begins below the
    // last visible row.
    let line_len = line.len() as i64;
    let last_visible = rows.last().unwrap().end_buffer_pos.unwrap().as_i64();
    let point = ((last_visible - 1) / line_len + 1) * line_len + 1;
    eval.eval_str(&format!("(goto-char {point})")).unwrap();
    probe::reset();
    engine.layout_frame_rust(&mut eval, frame);
    let scroll_attempts = probe::attempts();

    let rows = text_rows(&eval, frame, window);
    let point_row = row_starting_at(&rows, point).unwrap_or_else(|| {
        panic!(
            "point {point} must be visible after redisplay; window-start {}",
            window_start(&eval, frame, window)
        )
    });
    assert!(
        scroll_attempts <= still_attempts + 1,
        "recentering must lay the window out at most once more than a frame \
         without scrolling: {scroll_attempts} walks vs {still_attempts} \
         (point on row {point_row} of {window_rows}, {rows_per_line} rows per line)"
    );
    assert!(
        point_row.abs_diff(window_rows / 2) <= 1,
        "recenter puts point half the window's DISPLAY rows down: row {point_row} of {window_rows}"
    );
}

#[test]
fn forward_line_through_wrapped_lines_recenters_by_display_rows() {
    const STEPS: usize = 200;
    let (mut eval, frame, buffer, window) = incr_editing_frame("x\n", 800, 1000);
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    engine.layout_frame_rust(&mut eval, frame);
    // Size the lines to wrap over exactly three rows of this window.
    eval.eval_str(&format!(
        "(progn (erase-buffer) (insert (make-string {} ?x) \"\\n\") (goto-char (point-min)))",
        8 * 1024
    ))
    .unwrap();
    engine.layout_frame_rust(&mut eval, frame);
    let columns = row_columns(&text_rows(&eval, frame, window));
    let line_chars = 2 * columns + columns / 2;
    eval.eval_str(&format!(
        "(let ((lines {}))
           (erase-buffer)
           (while (> lines 0)
             (insert (make-string {line_chars} ?x) \"\\n\")
             (setq lines (1- lines)))
           (goto-char (point-min)))",
        STEPS * 2
    ))
    .unwrap();
    scroll_window_to(&mut eval, frame, window, buffer, 1, 0);
    engine.layout_frame_rust(&mut eval, frame);
    let window_rows = text_rows(&eval, frame, window).len() as i64;
    assert!(
        window_rows >= 40,
        "fixture needs a tall window: {window_rows} rows"
    );

    let mut start = window_start(&eval, frame, window);
    let mut changes = 0usize;
    for step in 0..STEPS {
        eval.eval_str("(forward-line 1)").unwrap();
        engine.layout_frame_rust(&mut eval, frame);
        let point = eval.eval_str("(point)").unwrap().as_fixnum().unwrap();
        assert!(
            row_starting_at(&text_rows(&eval, frame, window), point).is_some(),
            "step {step}: point {point} must be visible"
        );
        let now = window_start(&eval, frame, window);
        if now != start {
            changes += 1;
            start = now;
        }
    }
    // Model of a display-row recenter (GNU `recenter:`): point lands on row
    // rows/2; each step moves it three rows, so it leaves the window again
    // after `steps_per_recenter` motions.
    let steps_per_recenter = ((window_rows - 1 - window_rows / 2) / 3 + 1) as usize;
    let model_changes = STEPS.div_ceil(steps_per_recenter);
    assert!(
        changes <= model_changes + 2,
        "{STEPS} forward-line steps changed window-start {changes} times; \
         recentering by display rows does it about {model_changes} times \
         ({window_rows} rows, 3 rows per line)"
    );
}

#[test]
fn recentering_below_unwrapped_lines_takes_one_walk_and_centres_point() {
    // Short lines: counting buffer lines is exact, so the retry needs no
    // display motion and the window is still laid out once more at most.
    let (mut eval, frame, _buffer, window) = incr_editing_frame(&"x\n".repeat(400), 640, 400);
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    engine.layout_frame_rust(&mut eval, frame);
    let rows = text_rows(&eval, frame, window);
    let window_rows = rows.len();
    assert!(
        window_rows >= 12,
        "fixture needs a tall window: {window_rows} rows"
    );

    eval.eval_str("(goto-char (point-min)) (forward-line 1)")
        .unwrap();
    probe::reset();
    engine.layout_frame_rust(&mut eval, frame);
    let still_attempts = probe::attempts();

    let last_visible = rows.last().unwrap().end_buffer_pos.unwrap().as_i64();
    // The start of the first line below the last visible row (2-char lines).
    let point = ((last_visible - 1) / 2 + 1) * 2 + 1;
    eval.eval_str(&format!("(goto-char {point})")).unwrap();
    probe::reset();
    engine.layout_frame_rust(&mut eval, frame);
    let scroll_attempts = probe::attempts();

    let rows = text_rows(&eval, frame, window);
    let point_row = row_starting_at(&rows, point).unwrap_or_else(|| {
        panic!(
            "point {point} must be visible after redisplay; window-start {}",
            window_start(&eval, frame, window)
        )
    });
    assert!(
        scroll_attempts <= still_attempts + 1,
        "{scroll_attempts} walks vs {still_attempts} (point on row {point_row} of {window_rows})"
    );
    assert!(
        point_row.abs_diff(window_rows / 2) <= 1,
        "recenter puts point half the window down: row {point_row} of {window_rows}"
    );
}

#[test]
fn forced_start_above_wrapped_lines_moves_point_into_the_window() {
    // GNU `redisplay_window`'s force_start branch keeps a start set with
    // `set-window-start` and moves POINT into the window instead.
    const LINE_CHARS: usize = 520;
    let line = format!("{}\n", "x".repeat(LINE_CHARS));
    let (mut eval, frame, _buffer, window) = incr_editing_frame(&line.repeat(200), 640, 400);
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    engine.layout_frame_rust(&mut eval, frame);
    let rows = text_rows(&eval, frame, window);
    let line_len = line.len() as i64;
    let last_visible = rows.last().unwrap().end_buffer_pos.unwrap().as_i64();
    // The start of the first logical line below the last visible row.
    let below = ((last_visible - 1) / line_len + 1) * line_len + 1;

    eval.eval_str(&format!("(goto-char {below}) (set-window-start nil 1)"))
        .unwrap();
    engine.layout_frame_rust(&mut eval, frame);

    let start = window_start(&eval, frame, window);
    let point = eval.eval_str("(point)").unwrap().as_fixnum().unwrap();
    let rows = text_rows(&eval, frame, window);
    let first = rows.first().unwrap().start_buffer_pos.unwrap().as_i64();
    let end = rows.last().unwrap().end_buffer_pos.unwrap().as_i64();
    assert_eq!(start, 1, "a forced window start is kept");
    // Point moves back to the end of the last text row
    // (`forced_start_point_target`), not further.
    assert!(
        (first..=end).contains(&point),
        "point moves back to the window {first}..={end}: point {point}, was {below}"
    );
}

#[test]
fn recentering_counts_leading_before_string_rows_as_display_rows() {
    // Rows produced only by a before-string at the window start occupy
    // screen space but offer no buffer position past the start. GNU
    // `recenter:` moves back from point by display height
    // (`move_it_vertically_backward`, src/xdisp.c:21219), so those rows
    // count like any other row.
    const STRING_ROWS: usize = 7;
    let (mut eval, frame, _buffer, window) = incr_editing_frame(&"x\n".repeat(400), 640, 400);
    eval.eval_str(&format!(
        "(progn (setq scroll-margin 3)
                (overlay-put (make-overlay 1 2) 'before-string {:?}))",
        "s\n".repeat(STRING_ROWS)
    ))
    .unwrap();
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    engine.layout_frame_rust(&mut eval, frame);
    let all_rows = |eval: &Context| {
        eval.frame_manager()
            .get(frame)
            .unwrap()
            .redisplay_snapshot(window)
            .unwrap()
            .rows
            .clone()
    };
    let rows = all_rows(&eval);
    let string_rows = rows
        .iter()
        .take_while(|row| {
            row.end_source == neovm_core::window::DisplayRowEndSource::OverlayBeforeString
        })
        .count();
    let window_rows = rows
        .iter()
        .filter(|row| row.start_buffer_pos.is_some())
        .count();
    assert!(
        string_rows == STRING_ROWS && window_rows >= 2 * STRING_ROWS,
        "fixture needs {STRING_ROWS} string rows above the first buffer line: \
         {string_rows} string rows of {window_rows}"
    );

    // Point inside the bottom scroll margin, on a row that was laid out.
    let point = rows[window_rows - 2].start_buffer_pos.unwrap().as_i64();
    assert!(
        point > 1,
        "point must sit on a buffer row, not a string row"
    );
    eval.eval_str(&format!("(goto-char {point})")).unwrap();
    engine.layout_frame_rust(&mut eval, frame);

    let rows = all_rows(&eval);
    let point_row = row_starting_at(&rows, point).unwrap_or_else(|| {
        panic!(
            "point {point} must be visible after redisplay; window-start {}",
            window_start(&eval, frame, window)
        )
    });
    assert!(
        point_row.abs_diff(window_rows / 2) <= 1,
        "recenter puts point half the window's DISPLAY rows down, string rows \
         included: row {point_row} of {window_rows}, window-start {}",
        window_start(&eval, frame, window)
    );
}
