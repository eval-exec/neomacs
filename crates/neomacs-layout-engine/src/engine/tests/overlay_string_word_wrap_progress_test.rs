//! Issue #744: rewinding after an inserted string must not replay it forever.
//!
//! GNU 31.1 xdisp.c:26102/26405 saves/restores the complete display iterator,
//! including the overlay-string position (dispextern.h:339), rather than just
//! its buffer anchor. The paired GUI baseline reproduces repeated " hint "
//! rows and noncompletion in Neo, while GNU retains the buffer tail.

use super::*;
use neomacs_display_protocol::frame_glyphs::GlyphRowRole;
use neomacs_display_protocol::glyph_matrix::GlyphProvenance;

fn assert_overlay_wrap_progress(
    property: &str,
    word_wrap: bool,
    string: &str,
    tail_len: usize,
    buffer_space: bool,
) {
    let _ = assert_overlay_wrap_progress_with_insertions(
        property,
        word_wrap,
        string,
        tail_len,
        buffer_space,
        &[],
    );
}

fn assert_overlay_wrap_progress_with_insertions(
    property: &str,
    word_wrap: bool,
    string: &str,
    tail_len: usize,
    buffer_space: bool,
    additional: &[(i64, i64, &str, &str)],
) -> String {
    let prefix = if buffer_space { "see " } else { "see" };
    let text = format!("{}{}\n", prefix, "x".repeat(tail_len));
    // Thirty physical rows are ample for this 300-character tail at roughly
    // eighty columns, but bound the old failure's repeated rows. This test
    // never calls the unbounded motion retry which hung in the GUI baseline.
    let (mut eval, frame, _buffer, window) = incr_editing_frame(&text, 640, 480);
    {
        let frame = eval.frame_manager_mut().get_mut(frame).expect("frame");
        frame.window_system = Some(Value::symbol("neomacs"));
        frame.char_width = 8.0;
        frame.char_height = 16.0;
        frame.font_pixel_size = 16.0;
    }
    eval.eval_str(
        "(setq mode-line-format nil header-line-format nil tab-line-format nil
               truncate-lines nil)",
    )
    .expect("plain wrapped window");
    eval.eval_str(if word_wrap {
        "(setq word-wrap t)"
    } else {
        "(setq word-wrap nil)"
    })
    .expect("word-wrap control");
    // AFTER [3,4) and BEFORE [4,5) insert at the same one-based boundary 4.
    let (start, end) = match property {
        "after-string" => (3, 4),
        "before-string" => (4, 5),
        _ => panic!("fixture supports before/after strings only"),
    };
    eval.eval_form(Value::list(vec![
        Value::symbol("overlay-put"),
        Value::list(vec![
            Value::symbol("make-overlay"),
            Value::fixnum(start),
            Value::fixnum(end),
        ]),
        Value::list(vec![Value::symbol("quote"), Value::symbol(property)]),
        Value::string(string),
    ]))
    .expect("benign inserted string");
    for &(start, end, property, string) in additional {
        eval.eval_form(Value::list(vec![
            Value::symbol("overlay-put"),
            Value::list(vec![
                Value::symbol("make-overlay"),
                Value::fixnum(start),
                Value::fixnum(end),
            ]),
            Value::list(vec![Value::symbol("quote"), Value::symbol(property)]),
            Value::string(string),
        ]))
        .expect("additional benign insertion");
    }
    eval.eval_str("(goto-char (point-min))").expect("point");

    let mut engine = LayoutEngine::new_without_font_metrics();
    engine.layout_frame_rust(&mut eval, frame);
    let matrix = &engine
        .last_frame_display_state
        .as_ref()
        .expect("display state")
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == window.0 as i64)
        .expect("selected window")
        .matrix;
    let rows: Vec<_> = matrix
        .rows
        .iter()
        .filter(|row| row.enabled && row.role == GlyphRowRole::Text)
        .collect();
    let row_text: Vec<_> = rows
        .iter()
        .map(|row| glyphs_logical_text(&row.glyphs[1]))
        .collect();
    let displayed = row_text.join("\n");
    let case = format!("{property}, word-wrap={word_wrap}, tail={tail_len}, space={buffer_space}");
    for &(_, _, _, inserted) in additional {
        assert_eq!(
            displayed.matches(inserted).count(),
            1,
            "{case}: additional insertion {inserted:?} is emitted once: {row_text:?}"
        );
    }
    assert_eq!(
        displayed.matches("hint").count(),
        1,
        "{case}: the inserted hint is emitted once: {row_text:?}"
    );
    assert_eq!(
        displayed.chars().filter(|ch| *ch == 'x').count(),
        tail_len,
        "{case}: the complete following buffer tail remains visible: {row_text:?}"
    );

    // Check source progress independently of the glyph text: every ASCII
    // tail position must occur once in order, not be dropped or replayed.
    let tail_start = prefix.chars().count();
    let tail_end = tail_start + tail_len;
    let tail_positions: Vec<_> = rows
        .iter()
        .flat_map(|row| row.glyphs[1].iter())
        .filter(|glyph| !glyph.padding)
        .filter_map(|glyph| match glyph.provenance {
            GlyphProvenance::Buffer { charpos } if (tail_start..tail_end).contains(&charpos) => {
                Some(charpos)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        tail_positions,
        (tail_start..tail_end).collect::<Vec<_>>(),
        "{case}: buffer source positions advance through the whole tail"
    );
    let snapshot = eval
        .frame_manager()
        .get(frame)
        .expect("frame")
        .redisplay_snapshot(window)
        .expect("selected window snapshot");
    let row_ends: Vec<_> = snapshot
        .rows
        .iter()
        .filter_map(|row| row.end_buffer_pos.map(|position| position.as_i64()))
        .collect();
    assert!(
        row_ends.windows(2).all(|pair| pair[0] <= pair[1]),
        "{case}: source row ends do not regress: {row_ends:?}"
    );
    assert!(
        row_ends.iter().any(|end| *end > tail_end as i64),
        "{case}: finite viewport reaches the end of the tail: {row_ends:?}"
    );
    displayed
}

#[test]
fn word_wrap_after_string_retains_tail_and_advances_source() {
    assert_overlay_wrap_progress("after-string", true, " hint ", 300, false);
}

#[test]
fn word_wrap_before_string_retains_tail_and_advances_source() {
    assert_overlay_wrap_progress("before-string", true, " hint ", 300, false);
}

#[test]
fn word_wrap_overlay_no_space_control() {
    assert_overlay_wrap_progress("after-string", true, "[hint]", 300, false);
}

#[test]
fn word_wrap_overlay_disabled_control() {
    assert_overlay_wrap_progress("after-string", false, " hint ", 300, false);
}

#[test]
fn word_wrap_overlay_short_tail_control() {
    assert_overlay_wrap_progress("after-string", true, " hint ", 20, false);
}

#[test]
fn word_wrap_overlay_buffer_space_control() {
    assert_overlay_wrap_progress("after-string", true, " hint ", 300, true);
}

#[test]
fn word_wrap_restores_consumed_anchor_after_visiting_later_overlay() {
    // The x following A's trailing space records the wrap checkpoint at 4.
    // B is reached before the first overflow, but contains no whitespace so
    // it cannot replace that checkpoint. Rollback keeps A and replays B with
    // the discarded word: retaining only the last visited anchor is unsafe.
    let _ = assert_overlay_wrap_progress_with_insertions(
        "after-string",
        true,
        " hint ",
        300,
        false,
        &[(50, 51, "before-string", "tag")],
    );
}

#[test]
fn word_wrap_preserves_ordered_anchor_strings_and_replays_later_overlay_once() {
    let displayed = assert_overlay_wrap_progress_with_insertions(
        "after-string",
        true,
        " hint ",
        300,
        false,
        &[
            (4, 5, "before-string", " note "),
            (50, 51, "before-string", "tag"),
        ],
    );
    // Different overlays at the same anchor: GNU compare_overlay_entries
    // (xdisp.c:7048) orders the after-string before the before-string.
    // Both fit on the first row, so this does not pin a wrapping column.
    assert!(
        displayed.contains(" hint  note "),
        "the ordered strings remain together exactly once: {displayed:?}"
    );
}
