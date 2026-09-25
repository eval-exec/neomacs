//! P3.5 E1 (`NEOMACS_MODE_LINE_GATE=gnu`): an edit frame evaluates the mode
//! line exactly when GNU's optimization 1 would not apply.
//!
//! The expected counts are GNU's, measured with a counting `(:eval ...)` in
//! the mode line over typing cycles on a 40x120 TTY (tmp/d2rd/evalcount2.el
//! in the P3.5 lane worktree): at the end of a buffer GNU evaluates on the
//! insert only when jit-lock marks the line from its start, never on the
//! delete, and never at all with font-lock off. These pins reproduce each
//! clause in-process; the TUI pair test `mode_line_eval_count_oracle`
//! compares the live counts.

use super::*;
use crate::incremental_layout::mode_line_gate::{ModeLineGate, set_mode_line_gate_for_test};

struct GateGuard;

impl GateGuard {
    fn set(gate: ModeLineGate) -> Self {
        set_mode_line_gate_for_test(Some(gate));
        GateGuard
    }
}

impl Drop for GateGuard {
    fn drop(&mut self) {
        set_mode_line_gate_for_test(None);
    }
}

const LINE: &str = "(defun f (a b) (+ a b))\n";

/// A 10-line buffer (the end of the buffer is on screen) with a constant
/// mode line, point at `point_byte`, the modified star already settled, a
/// fixed paragraph direction (as every `prog-mode` buffer has; see
/// `a_buffer_with_an_automatic_paragraph_direction_always_evaluates`), and
/// one accepted frame.
fn settled_frame(
    format: &str,
    point_byte: usize,
) -> (
    Context,
    neovm_core::window::FrameId,
    BufferId,
    neovm_core::window::WindowId,
    LayoutEngine,
) {
    let (mut eval, frame_id, buf_id, window) = incr_editing_frame(&LINE.repeat(10), 800, 600);
    {
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.set_buffer_local("mode-line-format", Value::string(format));
        buf.set_buffer_local("bidi-paragraph-direction", Value::symbol("left-to-right"));
        buf.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(point_byte));
    }
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);
    activate_last_engine_presentation(&mut eval, &engine, frame_id);
    (eval, frame_id, buf_id, window, engine)
}

fn redisplay(
    eval: &mut Context,
    engine: &mut LayoutEngine,
    frame_id: neovm_core::window::FrameId,
) -> u32 {
    engine.layout_frame_rust(eval, frame_id);
    activate_last_engine_presentation(eval, engine, frame_id);
    crate::display_status_line::mode_line_eval_count()
}

fn end_of_buffer() -> usize {
    LINE.len() * 10
}

#[test]
fn typing_on_the_last_empty_line_keeps_the_mode_line_under_the_gnu_gate() {
    let _gate = GateGuard::set(ModeLineGate::Gnu);
    let (mut eval, frame_id, _buf, _window, mut engine) = settled_frame("ML", end_of_buffer());
    eval.eval_str("(insert \"x\")").expect("insert");
    assert_eq!(redisplay(&mut eval, &mut engine, frame_id), 0, "insert");
    assert_eq!(engine.last_layout_stats().edit_windows, 1, "edit replay");
    eval.eval_str("(delete-region (1- (point)) (point))")
        .expect("delete");
    assert_eq!(redisplay(&mut eval, &mut engine, frame_id), 0, "delete");
    assert_eq!(rendered_mode_line_text(&engine), "ML");
}

#[test]
fn the_legacy_gate_evaluates_where_gnu_does_not() {
    // The divergence the gate exists for: the box-topology lookbehind pulls
    // the row above an edit on an empty line into the walk, and the legacy
    // rule then refuses the skip.
    let _gate = GateGuard::set(ModeLineGate::Legacy);
    let (mut eval, frame_id, _buf, _window, mut engine) = settled_frame("ML", end_of_buffer());
    eval.eval_str("(insert \"x\")").expect("insert");
    assert_eq!(redisplay(&mut eval, &mut engine, frame_id), 1);
}

#[test]
fn a_property_change_from_the_line_start_evaluates_like_jit_lock_in_gnu() {
    let _gate = GateGuard::set(ModeLineGate::Gnu);
    let (mut eval, frame_id, _buf, _window, mut engine) = settled_frame("ML", end_of_buffer());
    // What `jit-lock-after-change` does after a keystroke: mark the whole
    // line unfontified. GNU records the property change one char early, so
    // it starts before the line and optimization 1 is refused.
    eval.eval_str(
        "(progn (insert \"x\") \
                (put-text-property (line-beginning-position) (point-max) 'fontified nil))",
    )
    .expect("insert + mark");
    assert_eq!(redisplay(&mut eval, &mut engine, frame_id), 1);
    // A deletion at the end of the buffer marks an empty region, which GNU's
    // `put-text-property` never records: the delete keeps the mode line.
    eval.eval_str(
        "(progn (delete-region (1- (point)) (point)) \
                (put-text-property (point) (point-max) 'fontified nil))",
    )
    .expect("delete + empty mark");
    assert_eq!(redisplay(&mut eval, &mut engine, frame_id), 0);
}

#[test]
fn a_property_change_inside_the_line_keeps_the_mode_line() {
    let _gate = GateGuard::set(ModeLineGate::Gnu);
    let mid_line = LINE.len() * 5 + 5;
    let (mut eval, frame_id, _buf, _window, mut engine) = settled_frame("ML", mid_line);
    eval.eval_str(
        "(progn (insert \"x\") \
                (put-text-property (1+ (line-beginning-position)) (line-end-position) \
                                   'fontified nil))",
    )
    .expect("insert + mark");
    assert_eq!(redisplay(&mut eval, &mut engine, frame_id), 0);
}

#[test]
fn an_inserted_newline_evaluates() {
    let _gate = GateGuard::set(ModeLineGate::Gnu);
    let mid_line = LINE.len() * 5 + 5;
    let (mut eval, frame_id, _buf, _window, mut engine) = settled_frame("L%l", mid_line);
    eval.eval_str("(insert \"\\n\")").expect("newline");
    assert_eq!(redisplay(&mut eval, &mut engine, frame_id), 1);
    assert_eq!(rendered_mode_line_text(&engine), "L7");
}

#[test]
fn a_line_that_wraps_after_the_edit_evaluates() {
    let _gate = GateGuard::set(ModeLineGate::Gnu);
    let mid_line = LINE.len() * 5 + 5;
    let (mut eval, frame_id, _buf, _window, mut engine) = settled_frame("ML", mid_line);
    // GNU's display_line result check: a continued line cancels.
    eval.eval_str("(insert (make-string 200 ?y))")
        .expect("long insert");
    assert_eq!(redisplay(&mut eval, &mut engine, frame_id), 1);
}

#[test]
fn an_edit_to_a_buffer_shown_twice_evaluates() {
    let _gate = GateGuard::set(ModeLineGate::Gnu);
    let mid_line = LINE.len() * 5 + 5;
    let (mut eval, frame_id, buf_id, window, _engine) = settled_frame("ML", mid_line);
    eval.frame_manager_mut()
        .split_window(
            frame_id,
            window,
            neovm_core::window::SplitDirection::Vertical,
            buf_id,
            None,
            neovm_core::window::SplitPlacement::AfterTarget,
        )
        .expect("split window onto the same buffer");
    let mut engine = LayoutEngine::new();
    redisplay(&mut eval, &mut engine, frame_id);
    eval.eval_str("(insert \"x\")").expect("insert");
    // `bset_redisplay`: a change to a buffer two windows show sets
    // `windows_or_buffers_changed`, which disables optimization 1.
    assert!(redisplay(&mut eval, &mut engine, frame_id) >= 1);
}

#[test]
fn the_modified_star_flip_still_evaluates_under_the_gnu_gate() {
    let _gate = GateGuard::set(ModeLineGate::Gnu);
    let (mut eval, frame_id, buf_id, _window, mut engine) = settled_frame("ML", end_of_buffer());
    eval.buffer_manager_mut()
        .get_mut(buf_id)
        .expect("buffer")
        .set_modified(false);
    redisplay(&mut eval, &mut engine, frame_id);
    eval.eval_str("(insert \"x\")").expect("insert");
    assert_eq!(redisplay(&mut eval, &mut engine, frame_id), 1, "star flip");
    eval.eval_str("(insert \"y\")").expect("insert");
    assert_eq!(redisplay(&mut eval, &mut engine, frame_id), 0, "settled");
}

#[test]
fn a_kept_mode_line_is_the_one_a_fresh_layout_draws() {
    let _gate = GateGuard::set(ModeLineGate::Gnu);
    let (mut eval, frame_id, _buf, _window, mut engine) = settled_frame("L%l", end_of_buffer());
    for step in ["x", "y", "z"] {
        eval.eval_str(&format!("(insert {step:?})"))
            .expect("insert");
        assert_eq!(redisplay(&mut eval, &mut engine, frame_id), 0, "{step}");
        let mut fresh = LayoutEngine::new();
        fresh.layout_frame_rust(&mut eval, frame_id);
        assert_eq!(
            rendered_mode_line_text(&engine),
            rendered_mode_line_text(&fresh),
            "{step}"
        );
    }
}

#[test]
fn a_buffer_with_an_automatic_paragraph_direction_always_evaluates() {
    // `text_outside_line_unchanged_p` gives up under bidi reordering with an
    // automatic paragraph direction (xdisp.c:17030-17035): in GNU every
    // keystroke in a fundamental-mode or text-mode buffer redraws the mode
    // line. `prog-mode` sets `bidi-paragraph-direction` to `left-to-right`.
    let _gate = GateGuard::set(ModeLineGate::Gnu);
    let (mut eval, frame_id, buf_id, _window, mut engine) = settled_frame("ML", end_of_buffer());
    eval.buffer_manager_mut()
        .get_mut(buf_id)
        .expect("buffer")
        .set_buffer_local("bidi-paragraph-direction", Value::NIL);
    eval.eval_str("(insert \"x\")").expect("insert");
    assert_eq!(redisplay(&mut eval, &mut engine, frame_id), 1);
}
