//! GNU `partial_line_height`'s probe row must start at the PUBLISHED window
//! end, not at the last drawn glyph of the completed walk.
//!
//! The visible span's `end` is inclusive (and a right-truncated row reports
//! the skipped line terminator), so a probe started there begins inside a
//! row the window already displayed and its one-line advance never reaches
//! the following row: the typed `ProbeRow` silently degrades to a no-op. The
//! engine now feeds the probe the same `TextWindowRedisplayPositions` value
//! it publishes as `window-end`; these rows fail if that source regresses to
//! span arithmetic.

use super::super::fontification_probe_row;
use super::*;

/// A TTY frame whose selected window has a body of `rows` one-cell rows.
///
/// `set_window_layout_text_size` counts the minibuffer line, so pass
/// `rows + 1`; the displayed buffer's mode/header/tab line formats are nil,
/// which leaves the whole root window as body rows.
fn tty_probe_frame(
    text: &str,
    cols: i64,
    rows: i64,
) -> (
    Context,
    neovm_core::window::FrameId,
    BufferId,
    neovm_core::window::WindowId,
) {
    let (mut eval, frame_id, buffer_id, window_id) =
        incr_editing_frame(text, cols as u32, (rows + 1) as u32);
    eval.set_variable("noninteractive", Value::NIL);
    {
        let buffer = eval.buffer_manager_mut().get_mut(buffer_id).unwrap();
        buffer.set_buffer_local("mode-line-format", Value::NIL);
        buffer.set_buffer_local("header-line-format", Value::NIL);
        buffer.set_buffer_local("tab-line-format", Value::NIL);
    }
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).unwrap();
        frame.initial = false;
        frame.char_width = 1.0;
        frame.char_height = 1.0;
        frame.font_pixel_size = 1.0;
        frame.set_window_system(None);
        // Frame::new used GUI metrics; realize the live TTY minibuffer as
        // one cell before the text-size calculation counts it.
        if let Some(mini) = frame.minibuffer_leaf.as_mut() {
            let mut bounds = *mini.bounds();
            bounds.height = 1.0;
            mini.set_bounds(bounds);
        }
        frame.set_window_layout_text_size(cols, rows + 1);
    }
    (eval, frame_id, buffer_id, window_id)
}

fn walk_and_published_end(
    eval: &mut Context,
    frame_id: neovm_core::window::FrameId,
    buffer_id: BufferId,
    window_id: neovm_core::window::WindowId,
) -> neovm_core::buffer::LispCharPos1 {
    let mut engine = LayoutEngine::new_without_font_metrics();
    engine.layout_frame_rust(eval, frame_id);
    let buffer_z = eval
        .buffer_manager()
        .get(buffer_id)
        .expect("buffer")
        .point_max_lisp_char_pos();
    let window_end = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .find_window(window_id)
        .expect("selected window")
        .window_end_charpos(buffer_z)
        .expect("leaf window");
    assert_eq!(
        eval.frame_manager()
            .get(frame_id)
            .expect("frame")
            .find_window(window_id)
            .expect("selected window")
            .window_end_valid(),
        Some(true),
        "the walk must publish a current window-end record"
    );
    window_end
}

fn probe_params(window_id: neovm_core::window::WindowId, buffer_id: BufferId) -> WindowParams {
    let mut params = test_window_params();
    params.window_id = window_id.0 as i64;
    params.buffer_id = buffer_id.0;
    params.window_start = 1;
    params.point = 1;
    params.buffer_begv = 1;
    params.wrap_mode = crate::types::LineWrapMode::Wrap;
    params
}

/// 60 lines of 150 `a`s: with a 160-column, 37-row body the walk ends at
/// 5588 (the first position of line 38), and GNU's probe row is exactly that
/// 150-character line. Its exclusive end is 5739 (the newline is 5738, the
/// last stop GNU enters). The old inclusive-span start (5587) advanced to
/// 5588 and added nothing, which is the regression this row catches.
#[test]
fn probe_row_starts_at_the_published_window_end() {
    let text = "a".repeat(150) + "\n";
    let text = text.repeat(60);
    let (mut eval, frame_id, buffer_id, window_id) = tty_probe_frame(&text, 160, 37);
    let window_end = walk_and_published_end(&mut eval, frame_id, buffer_id, window_id);
    assert_eq!(
        window_end.as_i64(),
        5588,
        "37 body rows of 151-character lines end at the first position of line 38"
    );
    let params = probe_params(window_id, buffer_id);
    let probe = fontification_probe_row(
        &mut eval, &params, frame_id, window_id, buffer_id, window_end,
    );
    assert_eq!(
        probe,
        neovm_core::emacs_core::xdisp::ProbeRow::Through(5739),
        "the probe row is line 38, whose exclusive end is the start of line 39"
    );
}

/// Install a declining fontification function that records every position it
/// is entered at.
///
/// The unit runtime has no `make-local-variable`, so the hook value is built
/// from special forms and core subrs (`list`, `lambda`, `cons`) and stored
/// from Rust.
fn install_recording_fontification_hook(eval: &mut Context, variable: &str) {
    eval.eval_str(&format!("(defvar {variable} nil)"))
        .expect("defvar the call log");
    eval.eval_str(&format!("(setq {variable} nil)"))
        .expect("reset the call log");
    let hook = eval
        .eval_str(&format!(
            "(list (lambda (pos) (setq {variable} (cons pos {variable})) nil))"
        ))
        .expect("build the declining hook");
    eval.store_variable_by_name("fontification-functions", hook);
}

/// The hook's recorded positions in entry order (the hook conses, so the list
/// reads newest-first; reverse it in Rust rather than with `reverse`).
fn recorded_fontification_positions(eval: &mut Context, variable: &str) -> Vec<i64> {
    let value = eval.eval_str(variable).expect("read the call log");
    let mut positions: Vec<i64> = neovm_core::emacs_core::value::list_to_vec(&value)
        .expect("the call log is a list")
        .into_iter()
        .map(|entry| entry.as_int().expect("fixnum position"))
        .collect();
    positions.reverse();
    positions
}

/// GNU's probe row follows `current_matrix_up_to_date_p`, not Neo's reuse
/// choice.
///
/// `Fset_window_start` with NOFORCE never sets `optional_new_start`
/// (src/window.c:1961-1972); it only moves the marker, clears
/// `w->window_end_valid` and asks for redisplay.  The predicate is therefore
/// false at entry, the same-start branch skips
/// `try_window_reusing_current_matrix` and calls
/// `try_window (..., TRY_WINDOW_CHECK_MARGINS)` (src/xdisp.c:21041-21052,
/// predicate :20557-20586), and `partial_line_height` probes.  Neo may still
/// reuse the retained rows internally; that is an optimization and must not
/// hide the hook calls Lisp sees.
#[test]
fn probe_row_survives_row_reuse_after_noforce_set_window_start() {
    let text = ("a".repeat(150) + "\n").repeat(60);
    let (mut eval, frame_id, buffer_id, window_id) = tty_probe_frame(&text, 160, 37);
    eval.edit.buffers.set_current(buffer_id);
    // An interval tree makes a declining hook advance by GNU's stops; the
    // untracked property has no display effect.
    for index in 0..60i64 {
        let start = index * 151 + 1;
        neovm_edit::emacs_core::textprop::builtin_put_text_property(
            &mut eval,
            vec![
                Value::fixnum(start),
                Value::fixnum(start + 150),
                Value::symbol("p640"),
                Value::fixnum(index),
            ],
        )
        .expect("put the p640 text property");
    }
    install_recording_fontification_hook(&mut eval, "p640-probe-reuse-calls");
    let mut engine = LayoutEngine::new_without_font_metrics();
    engine.layout_frame_rust(&mut eval, frame_id);
    let buffer_z = eval
        .buffer_manager()
        .get(buffer_id)
        .expect("buffer")
        .point_max_lisp_char_pos();
    let window_end = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .find_window(window_id)
        .expect("selected window")
        .window_end_charpos(buffer_z)
        .expect("leaf window");
    assert_eq!(
        window_end.as_i64(),
        5588,
        "the first walk publishes the standard window end"
    );
    // Count only the second walk's hook entries.
    eval.eval_str("(setq p640-probe-reuse-calls nil)")
        .expect("reset the call log");
    // Same start, unchanged buffer, NOFORCE: GNU clears the end record.
    eval.eval_str("(set-window-start nil 1 t)")
        .expect("NOFORCE set-window-start");
    assert_eq!(
        eval.frame_manager()
            .get(frame_id)
            .expect("frame")
            .find_window(window_id)
            .expect("selected window")
            .window_end_valid(),
        Some(false),
        "set-window-start invalidates the published window end without moving the start"
    );
    engine.layout_frame_rust(&mut eval, frame_id);
    let stats = engine.last_layout_stats().clone();
    assert!(
        stats.reused_rows + stats.reused_shifted_rows > 0,
        "the fixture must take a Neo row-reuse path: {stats:?}"
    );
    let positions = recorded_fontification_positions(&mut eval, "p640-probe-reuse-calls");
    assert!(
        positions.iter().any(|pos| *pos >= window_end.as_i64()),
        "the reused walk must still enter the probe row past window-end {}: {positions:?}",
        window_end.as_i64()
    );
}

/// When the walk ends at ZV, GNU's `move_it_to` reports `IT_EOB` and
/// `partial_line_height` never scans a following row, so the request must
/// carry no probe.
#[test]
fn probe_row_is_absent_when_the_walk_ends_at_zv() {
    let text = "abcdefghij\n".repeat(5);
    let (mut eval, frame_id, buffer_id, window_id) = tty_probe_frame(&text, 160, 37);
    let window_end = walk_and_published_end(&mut eval, frame_id, buffer_id, window_id);
    let point_max = eval
        .buffer_manager()
        .get(buffer_id)
        .expect("buffer")
        .point_max_lisp_char_pos();
    assert_eq!(
        window_end, point_max,
        "a buffer shorter than the window publishes ZV as window-end"
    );
    let params = probe_params(window_id, buffer_id);
    let probe = fontification_probe_row(
        &mut eval, &params, frame_id, window_id, buffer_id, window_end,
    );
    assert_eq!(
        probe,
        neovm_core::emacs_core::xdisp::ProbeRow::None,
        "an EOB walk has no following screen row to probe"
    );
}
