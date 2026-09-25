//! The approximate window geometry (`posn-at-point`, `pos-visible-in-window-p`
//! and `posn-at-x-y` without a presented layout) reads only the text the
//! window can show instead of copying the whole buffer (P3.5 H,
//! `NEOMACS_POSN_BOUNDED_TEXT=on`). These pins
//! compare every answer with the whole-text algorithm it replaced, over short
//! lines, lines longer than the text window, a buffer shorter than the
//! window, a window start mid-buffer, and coordinates below the window.

use super::*;

/// The approximations as they were, over the WHOLE buffer text.
mod whole_text {
    pub(super) fn nth_line_start(chars: &[char], mut start: usize, rows: i64) -> usize {
        start = start.min(chars.len());
        for _ in 0..rows.max(0) {
            if start >= chars.len() {
                return chars.len();
            }
            match chars[start..].iter().position(|ch| *ch == '\n') {
                Some(offset) => start += offset + 1,
                None => return chars.len(),
            }
        }
        start
    }

    pub(super) fn row_col(chars: &[char], start: usize, lisp_pos: usize, wrap: i64) -> (i64, i64) {
        let target = lisp_pos.saturating_sub(1).min(chars.len());
        let (mut row, mut col) = (0_i64, 0_i64);
        let wrap = wrap.max(1);
        let mut idx = start.min(chars.len());
        while idx < target {
            if chars[idx] == '\n' {
                row += 1;
                col = 0;
            } else {
                col += 1;
                if col >= wrap && idx + 1 < target {
                    row += 1;
                    col = 0;
                }
            }
            idx += 1;
        }
        (row, col)
    }

    /// `(point, dx, row, col)` for coordinates, as the old scanner answered.
    pub(super) fn point_at(
        chars: &[char],
        start: usize,
        query_row: i64,
        query_col: i64,
        wrap: i64,
    ) -> (usize, i64) {
        let mut row = 0_i64;
        let mut line_start = start.min(chars.len());
        loop {
            let line_end = chars[line_start..]
                .iter()
                .position(|ch| *ch == '\n')
                .map_or(chars.len(), |offset| line_start + offset);
            let line_len = (line_end - line_start) as i64;
            let visual_rows = ((line_len + wrap - 1) / wrap).max(1);
            if query_row < row + visual_rows {
                let segment_start = line_start + ((query_row - row) * wrap) as usize;
                let segment_len = line_end.saturating_sub(segment_start) as i64;
                let chosen = query_col.min(segment_len.min(wrap));
                let point = (segment_start + chosen as usize + 1).min(chars.len() + 1);
                return (point, chosen);
            }
            if line_end >= chars.len() {
                break;
            }
            row += visual_rows;
            line_start = line_end + 1;
        }
        (chars.len() + 1, 0)
    }
}

/// A frame whose window shows `text` from `window_start` (1-based).
fn window_over(text: &str, window_start: usize) -> (Context, FrameId, WindowId) {
    let mut eval = interactive_context();
    let buf_id = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id = eval
        .frames
        .create_frame("approx-window-text", 40 * 8, 12 * 16, buf_id);
    let window = eval.frames.get(frame_id).expect("frame").selected_window;
    eval.buffers.get_mut(buf_id).expect("buffer").insert(text);
    {
        let frame = eval.frames.get_mut(frame_id).expect("frame");
        if let Some(Window::Leaf {
            window_start: start,
            ..
        }) = frame.find_window_mut(window)
        {
            *start = LispCharPos1::from_one_based_usize(window_start);
        }
    }
    (eval, frame_id, window)
}

fn check(text: &str, window_start: usize) {
    set_bounded_window_text_for_test(Some(true));
    let (eval, frame_id, window) = window_over(text, window_start);
    let ctx = live_window_display_context_for(&eval.frames, &eval.buffers, frame_id, window)
        .expect("context")
        .expect("a live window");
    let chars: Vec<char> = text.chars().collect();
    let start = window_start.saturating_sub(1).min(chars.len());
    let wrap = approx_wrap_cols(&ctx);
    assert!(
        ctx.text.chars.len() <= chars.len(),
        "the context never holds more than the buffer"
    );

    // pos = t: the last visible line's start.
    assert_eq!(
        last_visible_row_start_lisp_pos(&ctx).to_one_based_usize(),
        (whole_text::nth_line_start(&chars, start, ctx.body_lines - 1) + 1).min(chars.len() + 1),
        "last visible line start (start {window_start})"
    );

    // Every position: visible or not, and where.
    for pos in 1..=chars.len() + 1 {
        let lisp = LispCharPos1::from_one_based_usize(pos);
        let expected = (pos >= window_start).then(|| {
            let (row, col) = whole_text::row_col(&chars, start, pos, wrap);
            (row < ctx.body_lines).then_some((row, col))
        });
        let got = approximate_pos_visible_metrics(&ctx, lisp)
            .map(|metrics| (metrics.vpos, metrics.x / ctx.char_width));
        assert_eq!(
            got,
            expected.flatten(),
            "position {pos} (start {window_start})"
        );
    }

    // Coordinates, including rows below the window.
    for query_row in 0..ctx.body_lines + 4 {
        for query_col in [0, 1, 5, wrap - 1, wrap, wrap + 3] {
            let x = query_col * ctx.char_width;
            let y = query_row * ctx.char_height;
            let answer = match approximate_point_at_coords(&ctx, x, y) {
                Some(ApproxPointAtCoords::Point(metrics)) => Some(metrics),
                Some(ApproxPointAtCoords::NeedsAllText) => {
                    let whole = live_window_display_context_with_all_text(
                        &eval.frames,
                        &eval.buffers,
                        frame_id,
                        window,
                    )
                    .expect("context")
                    .expect("a live window");
                    match approximate_point_at_coords(&whole, x, y) {
                        Some(ApproxPointAtCoords::Point(metrics)) => Some(metrics),
                        _ => None,
                    }
                }
                None => None,
            }
            .expect("coordinates inside the frame answer");
            let (point, chosen) = whole_text::point_at(&chars, start, query_row, query_col, wrap);
            assert_eq!(
                (answer.point.to_one_based_usize(), answer.dx),
                (point, x - chosen * ctx.char_width),
                "({query_col}, {query_row}) (start {window_start})"
            );
        }
    }
}

#[test]
fn short_lines_answer_like_the_whole_text() {
    let text: String = (0..200).map(|i| format!("line {i}\n")).collect();
    for start in [1, 2, 7, 500, 1200] {
        check(&text, start);
    }
}

#[test]
fn lines_longer_than_the_text_window_answer_like_the_whole_text() {
    let mut text = String::new();
    for i in 0..30 {
        text.push_str(&"x".repeat(if i % 3 == 0 { 2000 } else { 17 }));
        text.push('\n');
    }
    for start in [1, 30, 2001, 2020] {
        check(&text, start);
    }
}

#[test]
fn a_buffer_shorter_than_the_window_and_multibyte_text_answer_like_the_whole_text() {
    check("short\ntext\n", 1);
    check("", 1);
    let text: String = (0..80).map(|i| format!("中文 {i} été\n")).collect();
    for start in [1, 9, 300] {
        check(&text, start);
    }
}

#[test]
fn a_large_buffer_is_read_only_as_far_as_the_window_shows() {
    set_bounded_window_text_for_test(Some(true));
    let text: String = (0..20_000).map(|i| format!("line {i}\n")).collect();
    let (eval, frame_id, window) = window_over(&text, 1);
    let ctx = live_window_display_context_for(&eval.frames, &eval.buffers, frame_id, window)
        .expect("context")
        .expect("a live window");
    let budget = ((ctx.body_lines + 2) * (approx_wrap_cols(&ctx) + 1) + 1) as usize;
    assert!(
        ctx.text.chars.len() <= budget,
        "{} chars read",
        ctx.text.chars.len()
    );
    assert_eq!(ctx.total_chars, text.chars().count());

    // Off, the text runs to the end of the buffer, as before.
    set_bounded_window_text_for_test(Some(false));
    let ctx = live_window_display_context_for(&eval.frames, &eval.buffers, frame_id, window)
        .expect("context")
        .expect("a live window");
    assert_eq!(ctx.text.chars.len(), text.chars().count());
    set_bounded_window_text_for_test(None);
}
