//! VM tests for `vertical-motion` over display-resolved rows.
//!
//! Eval's unit runtime has no window geometry (`display::display_window.
//! motion_resolve` and `display::display_window.window_body_width` are
//! absent-layer slots there), so GNU's row-based motion cannot fold. The
//! rows moved here run against core's canonical display install. Test
//! names, bodies and expectations are unchanged; the plain-text tail of
//! `vm_motion_builtins_use_shared_current_buffer_state` (which stays in
//! eval) is preserved as its own test with the same assertion values.

use neovm_value::error::EvalError;
use neovm_value::value::Value;

fn vm_eval_str(src: &str) -> String {
    let mut eval = crate::test_utils::new_context();
    eval.set_lexical_binding(false);
    let result: Result<Value, EvalError> = eval.eval_str(src);
    crate::emacs_core::format_eval_result(&result)
}

#[test]
fn vm_vertical_motion_steps_rows_in_plain_text() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (insert "abc\ndef\nghi")
                 (goto-char 1)
                 (list (vertical-motion 1) (point)
                       (vertical-motion 1) (point)
                       (vertical-motion -1) (point)))"#
        ),
        "OK (1 5 1 9 -1 5)"
    );
}

#[test]
fn vm_vertical_motion_uses_invisible_text_boundaries_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (insert "aaaa\nbbbb\ncccc\n")
                 (put-text-property 5 10 'invisible t)
                 (goto-char 5)
                 (let ((forward (list (vertical-motion 1)
                                      (point)
                                      (line-number-at-pos)
                                      (current-column)
                                      (char-after))))
                   (goto-char 11)
                   (list forward
                         (list (vertical-motion -1)
                               (point)
                               (line-number-at-pos)
                               (current-column)
                               (char-after)))))"#
        ),
        "OK ((1 11 3 0 99) (-1 1 1 0 97))"
    );
}

#[test]
fn vm_vertical_motion_zero_backs_across_invisible_newlines_to_visual_row_start_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (insert
                  "order-17\n"
                  "  item: keyboard\n"
                  "  item: mouse\n"
                  "  total: 120\n"
                  "order-23\n"
                  "  status: shipped\n")
                 (setq buffer-invisibility-spec '(checkout-fold))
                 (let* ((fold-end (save-excursion
                                    (goto-char (point-min))
                                    (forward-line 4)
                                    (point)))
                        (fold (make-overlay
                               (save-excursion
                                 (goto-char (point-min))
                                 (line-end-position))
                               fold-end)))
                   (overlay-put fold 'invisible 'checkout-fold)
                   (goto-char fold-end)
                   (list (vertical-motion 0)
                         (point)
                         (line-number-at-pos))))"#
        ),
        "OK (0 1 1)"
    );
}
