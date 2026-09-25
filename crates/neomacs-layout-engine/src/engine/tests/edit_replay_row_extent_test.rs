//! An edit just past a line-final `display` string must relay the string's
//! row. The row's `end_charpos` is where its last glyph came from -- the
//! string's start -- so the edit replay took the row for clean, reused it
//! verbatim and walked the rest from the wrong matrix index: the typed
//! character vanished and every row below moved down one.

use super::*;

/// Lay out `text` with point at `point_char`, apply `forms` one per frame,
/// and compare each incremental frame with a fresh full layout.
fn assert_each_frame_matches_a_full_layout(text: &str, point_char: usize, forms: &[&str]) {
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

fn source() -> (String, usize) {
    let text: String = (0..60).map(|i| format!("(defun f{i} (a b)\n")).collect();
    let eol = text
        .split_inclusive('\n')
        .take(16)
        .map(|line| line.chars().count())
        .sum::<usize>()
        - 1;
    (text, eol)
}

#[test]
fn typing_after_a_line_final_display_string_relays_its_row() {
    let (text, eol) = source();
    assert_each_frame_matches_a_full_layout(
        &text,
        eol,
        &[
            "(progn (insert \" ;; word\") \
                    (put-text-property (- (point) 4) (point) 'display \"DISPLAYED\"))",
            "(insert \"\\t\")",
            "(insert \"中\")",
            "(insert \"\\n\")",
        ],
    );
}
