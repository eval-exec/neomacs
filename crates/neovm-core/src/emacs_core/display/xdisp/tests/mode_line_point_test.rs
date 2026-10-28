//! The DISPLAY entry must read the same live coordinates as format-mode-line.

use super::*;
use crate::emacs_core::Context;

#[test]
fn mode_line_display_point_tracks_eval_moves_and_restrictions() {
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
    let buffer = eval.buffers.current_buffer_id().expect("current buffer");
    let frame = eval.frames.create_frame("point-percent", 120, 40, buffer);
    assert!(eval.frames.select_frame(frame));
    let format = eval
        .eval_str(
            r#"'("start[%l/%c/%C]"
             (:eval (progn (forward-line 2) (forward-char 2) ""))
             "forward[%l/%c/%C][%l/%c/%C]"
             (:eval (progn (forward-line -1) (forward-char 4) ""))
             "backward[%l/%c/%C]"
             (:eval (progn (goto-char (point-min)) (forward-line 1)
                           (narrow-to-region (point) (point-max))
                           (forward-line 1) (forward-char 2) ""))
             "narrow[%l/%c/%C]"
             (:eval (progn (widen) ""))
             "widen[%l/%c/%C]"
             (:eval (save-excursion (goto-char (point-max)) ""))
             "excursion[%l/%c/%C]"
             (:eval (progn (goto-char (point-min)) ""))
             "restored[%l/%c/%C]")"#,
        )
        .expect("point format");
    let value = format_mode_line_for_display(&mut eval, format, Value::NIL, Value::NIL, 120);
    // Literal from interactive GNU31.1, same fields in mode_line_point_probe.el:
    // sandbox-run.sh <python> -B tmp/editpath-cont16/point-pty.py
    //   $NEOVM_FORCE_ORACLE_PATH -nw -Q -no-comp-spawn
    //   --eval="(setq native-comp-jit-compilation nil native-comp-async-report-warnings-errors 'silent)"
    //   -l crates/neomacs-tui-tests/tests/mode_line_point_probe.el
    // Original authority: tmp/editpath-cont16/point-gnu-r1/values.el.
    assert_eq!(
        value.as_utf8_str(),
        Some(
            "start[1/0/1]forward[3/2/3][3/2/3]backward[2/4/5]narrow[3/2/3]widen[3/2/3]excursion[3/2/3]restored[1/0/1]"
        )
    );
}
