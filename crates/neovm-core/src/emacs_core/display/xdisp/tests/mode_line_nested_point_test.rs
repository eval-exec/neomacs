//! Live-point nesting and restriction history, with GNU31.1 oracle literals.
//! Collected through sandbox-run.sh by:
//! bash tmp/editpath-cont11/run-net-cg-nix-r1.sh python3 -B
//! tmp/editpath-cont16/collect-nested-r1.py /home/exec/.local/bin/emacs nested-gnu-r1
//! Actual values: tmp/editpath-cont16/nested-gnu-r1/values.el.
use super::*;
use crate::emacs_core::Context;
use crate::emacs_core::eval::save_scratch_gc_roots;

fn point_context() -> Context {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.set_variable("noninteractive", Value::NIL);
    eval.eval_str(
        r#"(progn
             (erase-buffer)
             (insert "alpha\nbravo words\ncharlie\ndelta\necho\n")
             (goto-char (point-min)))"#,
    )
    .expect("point fixture");
    let buffer = eval.buffers.current_buffer_id().unwrap();
    let frame = eval.frames.create_frame("nested-point", 120, 40, buffer);
    assert!(eval.frames.select_frame(frame));
    let window = eval.frames.selected_frame().unwrap().selected_window;
    eval.set_variable("d16-point-window", Value::make_window(window.0));
    let foreign = eval.buffers.create_buffer("d16-point-foreign");
    assert!(eval.buffers.switch_current_unrecorded(foreign));
    eval.eval_str(
        r#"(progn (insert "foreign\nsecond\n")
                  (goto-char (point-min)) (forward-line 1))"#,
    )
    .expect("foreign point fixture");
    assert!(eval.buffers.switch_current_unrecorded(buffer));
    eval.set_variable("d16-point-foreign", Value::make_buffer(foreign));
    eval
}

#[test]
fn mode_line_nested_point_counter_matches_observed_gnu() {
    assert!(
        mode_line_flow_policy::enabled(),
        "requires the normal Context entry"
    );
    for (name, nested, expected) in [
        (
            "same-window",
            r#"(format-mode-line "nested[%l/%c/%C]" nil d16-point-window)"#,
            "start[1/0/1]pre[3/2/3]nested[2/2/3]post[2/2/3]",
        ),
        (
            "foreign-buffer",
            r#"(format-mode-line "foreign[%l/%c/%C]" nil d16-point-window d16-point-foreign)"#,
            "start[1/0/1]pre[3/2/3]foreign[2/0/1]post[3/2/3]",
        ),
        (
            "foreign-literal",
            r#"(format-mode-line "foreign-literal" nil d16-point-window d16-point-foreign)"#,
            "start[1/0/1]pre[3/2/3]foreign-literalpost[3/2/3]",
        ),
        (
            "configuration",
            // Normal-path expansion of the observed save-window-excursion.
            // Plain let/progn avoids depending on loaded Lisp macro definitions.
            r#"(let ((saved (current-window-configuration)))
                 (let ((text (format-mode-line "nested[%l/%c/%C]" nil d16-point-window)))
                   (set-window-configuration saved) text))"#,
            "start[1/0/1]pre[3/2/3]nested[2/2/3]post[2/2/3]",
        ),
    ] {
        let mut eval = point_context();
        let original_buffer = eval.buffers.current_buffer_id();
        let original_window = eval.frames.selected_frame().unwrap().selected_window;
        let roots = save_scratch_gc_roots();
        let specpdl = eval.specpdl.len();
        let expression = format!(
            r#"(format-mode-line
                 '("start[%l/%c/%C]"
                   (:eval (progn
                            (goto-char (point-min)) (forward-line 1)
                            (narrow-to-region (point) (point-max))
                            (forward-line 1) (forward-char 2) ""))
                   "pre[%l/%c/%C]"
                   (:eval {nested})
                   "post[%l/%c/%C]")
                 nil d16-point-window)"#,
        );
        let result = eval.eval_str(&expression).expect(name);
        assert_eq!(result.as_utf8_str(), Some(expected), "{name}");
        assert_eq!(eval.buffers.current_buffer_id(), original_buffer, "{name}");
        assert_eq!(
            eval.frames.selected_frame().unwrap().selected_window,
            original_window,
            "{name}"
        );
        assert_eq!(eval.specpdl.len(), specpdl, "{name}");
        assert_eq!(save_scratch_gc_roots(), roots, "{name}");
    }
}

#[test]
fn mode_line_narrow_first_widen_matches_observed_gnu() {
    let mut eval = point_context();
    let result = eval
        .eval_str(
            r#"(progn
             (forward-line 2)
             (set-window-start d16-point-window (point) t)
             (forward-char 2)
             (format-mode-line
              '("column-only[%c/%C]"
                (:eval (progn
                         (save-excursion
                           (goto-char (point-min)) (forward-line 1)
                           (narrow-to-region (point) (point-max))) ""))
                "narrow[%l/%c/%C]"
                (:eval (progn (widen) ""))
                "widen[%l/%c/%C]")
              nil d16-point-window))"#,
        )
        .expect("narrow-first/widen");
    assert_eq!(
        result.as_utf8_str(),
        Some("column-only[2/3]narrow[2/2/3]widen[2/2/3]")
    );
}

#[test]
fn mode_line_clip_witness_survives_round_trip_until_accepted_ack() {
    let mut eval = point_context();
    let buffer = eval.buffers.current_buffer_mut().unwrap();
    buffer.reset_unchanged_region();
    assert!(!buffer.mode_line_clip_changed());
    buffer.widen();
    assert!(
        !buffer.mode_line_clip_changed(),
        "a no-op widen is not clip_changed"
    );
    let second_line = buffer.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(6));
    buffer.narrow_to_emacs_byte_range(crate::buffer::EmacsByteRange::new(
        second_line,
        buffer.point_max_emacs_byte_pos(),
    ));
    assert!(buffer.mode_line_clip_changed());
    buffer.widen();
    assert!(
        buffer.mode_line_clip_changed(),
        "entry bounds equality cannot erase restriction history"
    );
    buffer.reset_unchanged_region();
    assert!(
        !buffer.mode_line_clip_changed(),
        "accepted redisplay acknowledges the witness"
    );
}

#[test]
fn mode_line_clip_witness_is_separate_for_shared_indirect_text() {
    let mut eval = point_context();
    let base = eval.buffers.current_buffer_id().unwrap();
    let indirect = eval
        .buffers
        .create_indirect_buffer(base, "point-indirect", false)
        .expect("indirect buffer sharing text");
    for id in [base, indirect] {
        eval.buffers.get(id).unwrap().reset_unchanged_region();
    }
    let buffer = eval.buffers.get_mut(base).unwrap();
    let start = buffer.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(6));
    buffer.narrow_to_emacs_byte_range(EmacsByteRange::new(
        start,
        buffer.point_max_emacs_byte_pos(),
    ));
    assert!(eval.buffers.get(base).unwrap().mode_line_clip_changed());
    assert!(!eval.buffers.get(indirect).unwrap().mode_line_clip_changed());
    eval.buffers.get(indirect).unwrap().reset_unchanged_region();
    assert!(
        eval.buffers.get(base).unwrap().mode_line_clip_changed(),
        "another buffer's accepted display cannot acknowledge this restriction"
    );
}

#[test]
fn mode_line_buffer_install_clears_the_live_numeric_base() {
    let mut eval = point_context();
    for target in ["(current-buffer)", "d16-point-foreign"] {
        eval.frames
            .selected_frame_mut()
            .unwrap()
            .root_window_mut()
            .set_mode_line_base(Some(
                crate::window::ModeLineBase::new(CharPos0::new(6), 2)
                    .expect("positive fixture line"),
            ));
        // GNU window.c set_window_buffer -> wset_buffer -> adjust_window_count
        // clears base_line_pos for each install, including the same buffer.
        eval.eval_str(&format!("(set-window-buffer d16-point-window {target} t)"))
            .expect("actual Lisp window buffer installation");
        assert!(
            eval.frames
                .selected_frame()
                .unwrap()
                .root_window()
                .mode_line_base()
                .is_none()
        );
    }
}

// GNU31.1 collected through sandbox-run.sh by:
// bash tmp/editpath-cont11/run-net-cg-nix-r1.sh python3 -B
// tmp/editpath-cont16/collect-repeat-line-r3.py /home/exec/.local/bin/emacs repeat-line-gnu-r3
// Actual values: tmp/editpath-cont16/repeat-line-gnu-r3/values.el.
#[test]
fn mode_line_repeated_line_specs_reseat_like_observed_gnu() {
    for widened in [
        r#""widen[%l/%l/%l/%c/%C]""#,
        r#""widen[%l/" "%l/%l/%c/%C]""#,
    ] {
        let mut eval = point_context();
        let result = eval
            .eval_str(&format!(
                r#"(progn
                 (forward-line 2)
                 (set-window-start d16-point-window (point) t)
                 (forward-char 2)
                 (format-mode-line
                  '("column-only[%c/%C]"
                    (:eval (progn
                             (save-excursion
                               (goto-char (point-min)) (forward-line 1)
                               (narrow-to-region (point) (point-max))) ""))
                    "narrow[%l/%l/%l/%c/%C]"
                    (:eval (progn (widen) ""))
                    {widened})
                  nil d16-point-window))"#
            ))
            .expect("repeated-line GNU fixture");
        assert_eq!(
            result.as_utf8_str(),
            Some("column-only[2/3]narrow[2/2/2/2/3]widen[2/3/3/2/3]"),
            "{widened}"
        );
    }
}
