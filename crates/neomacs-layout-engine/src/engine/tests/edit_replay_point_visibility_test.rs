//! An edit replay keeps window-start, so it cannot follow point. When the
//! edit leaves point below the rows it shows -- lines inserted above point
//! near the window bottom -- a full layout scrolls to point, and so must the
//! replay's frame: GNU's try_window_id gives up and lets redisplay_window
//! place the window (xdisp.c:23077-23110). The same goes for point on a
//! reused row the cursor cannot be put back on (the end of a line, an empty
//! line): before, the frame came out with no cursor at all.

use super::*;

fn assert_frames_match_a_full_layout(text: &str, point_char: usize, forms: &[&str]) {
    let (mut eval, frame_id, _buf, _window) = incr_editing_frame(text, 800, 600);
    eval.eval_str(&format!("(goto-char {})", point_char + 1))
        .expect("goto");
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);
    for form in forms {
        eval.eval_str(form)
            .unwrap_or_else(|error| panic!("{form}: {error:?}"));
        engine.layout_frame_rust(&mut eval, frame_id);
        let incremental = selected_window_layout_trace(&eval, &engine, frame_id);
        let mut fresh = LayoutEngine::new();
        fresh.layout_frame_rust(&mut eval, frame_id);
        let reference = selected_window_layout_trace(&eval, &fresh, frame_id);
        assert_eq!(incremental, reference, "{form}");
    }
}

/// 60 short lines; the 800x600 test frame shows about 35.
fn source() -> String {
    (0..60).map(|i| format!("(defun f{i} (a b)\n")).collect()
}

/// Char position of the end of line `line`.
fn end_of_line(text: &str, line: usize) -> usize {
    text.split_inclusive('\n')
        .take(line + 1)
        .map(|l| l.chars().count())
        .sum::<usize>()
        - 1
}

#[test]
fn lines_inserted_above_point_near_the_bottom_scroll_like_a_full_layout() {
    let text = source();
    assert_frames_match_a_full_layout(
        &text,
        end_of_line(&text, 33),
        &["(save-excursion (goto-char (point-min)) (forward-line 5) (insert \"\\n\\n\\n\"))"],
    );
}

#[test]
fn an_edit_that_carries_point_below_the_window_scrolls_like_a_full_layout() {
    let text = source();
    assert_frames_match_a_full_layout(
        &text,
        end_of_line(&text, 33),
        &["(insert \"ab\\n\\tcd\\n\\tef\\n\")"],
    );
}

/// Point at the end of a line the replay reuses (the edit is a property
/// change further down): the cursor cannot be put back from the row's
/// glyphs, so the frame is laid out in full rather than shown without it.
#[test]
fn point_at_the_end_of_a_reused_line_keeps_its_cursor() {
    let text = source();
    assert_frames_match_a_full_layout(
        &text,
        end_of_line(&text, 10),
        &["(put-text-property (+ (point) 100) (+ (point) 104) 'face 'bold)"],
    );
}

/// The same for point on an empty line the replay reuses.
#[test]
fn point_on_a_reused_empty_line_keeps_its_cursor() {
    let mut text = source();
    text.insert(end_of_line(&text, 10) + 1, '\n');
    assert_frames_match_a_full_layout(
        &text,
        end_of_line(&text, 10) + 1,
        &["(put-text-property (+ (point) 100) (+ (point) 104) 'face 'bold)"],
    );
}

/// A newline typed at the end of the last line wholly on screen puts point
/// on the partly visible row below it; the walk "finds" the cursor there,
/// clipped, and a full layout scrolls instead. (The 800x600 test frame shows
/// 35 whole rows.)
#[test]
fn a_newline_at_the_end_of_the_last_whole_row_scrolls_like_a_full_layout() {
    let text = source();
    assert_frames_match_a_full_layout(&text, end_of_line(&text, 34), &["(insert \"\\n\")"]);
}
