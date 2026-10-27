//! Batch terminal controls for the initial graphical-frame lifecycle.
//!
//! Expectations were captured from the pinned GNU batch executable; these
//! controls must keep working while GUI frame creation moves into Lisp.
use crate::common;

#[test]
fn batch_startup_retains_one_live_visible_terminal_frame() {
    common::return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(let ((frame (selected-frame)))
  (list (and noninteractive t)
        (length (frame-list))
        (eq frame terminal-frame)
        (framep frame)
        (frame-live-p frame)
        (display-graphic-p frame)
        (frame-visible-p frame)))"#,
        expect_test::expect![[r#""OK (t 1 t t t nil t)""#]],
    );
}

#[test]
fn sole_terminal_frame_deletion_and_invisibility_match_gnu() {
    common::return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(let ((frame (selected-frame)) (results nil))
  (dolist (operation '((delete-frame nil) (delete-frame t)
                       (make-frame-invisible nil) (make-frame-invisible t)))
    (push (list (car operation) (cadr operation)
                (condition-case data
                    (funcall (car operation) frame (cadr operation))
                  (error (list (car data) (cadr data)))))
          results))
  (list (nreverse results)
        (eq frame (selected-frame))
        (length (frame-list))
        (frame-live-p frame)
        (frame-visible-p frame)))"#,
        expect_test::expect![[
            r#""OK (((delete-frame nil (error \"Attempt to delete the sole visible or iconified frame\")) (delete-frame t (error \"Attempt to delete the only frame\")) (make-frame-invisible nil (error \"Attempt to make invisible the sole visible or iconified frame\")) (make-frame-invisible t nil)) t 1 t t)""#
        ]],
    );
}

#[test]
fn batch_frame_initialize_preserves_terminal_without_creation_hooks() {
    common::return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(let* ((frame (selected-frame))
        (events nil)
        (before-make-frame-hook
         (list (lambda () (push 'before events))))
        (after-make-frame-functions
         (list (lambda (created) (push (framep created) events)))))
  (frame-initialize)
  (list (eq frame (selected-frame))
        (eq frame terminal-frame)
        (length (frame-list))
        events
        (framep frame)
        (display-graphic-p frame)))"#,
        expect_test::expect![[r#""OK (t t 1 nil t nil)""#]],
    );
}
