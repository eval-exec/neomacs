//! Oracle parity tests for GNU region and mark semantics.
//!
//! GNU implements `region-beginning`, `region-end`, and `mark-marker` in
//! `src/editfns.c`; Lisp-level `mark`, `set-mark`, and `push-mark` live in
//! `lisp/simple.el`.  These tests focus on narrowing clipping and active mark
//! behavior.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_region_bounds_clip_mark_to_current_narrowing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  (insert "0123456789")
  (let ((transient-mark-mode nil))
    (goto-char 6)
    (set-marker (mark-marker) 2 (current-buffer))
    (narrow-to-region 4 8)
    (let ((left (list (region-beginning) (region-end))))
      (widen)
      (goto-char 4)
      (set-marker (mark-marker) 10 (current-buffer))
      (narrow-to-region 5 8)
      (list left (region-beginning) (region-end)))))
"#;

    let expect = expect_test::expect![[r#""OK ((4 6) 5 8)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_mark_respects_transient_mark_mode_and_force() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  (insert "abc")
  (let ((transient-mark-mode t)
        (mark-even-if-inactive nil))
    (set-marker (mark-marker) 2 (current-buffer))
    (setq mark-active nil)
    (list
     (condition-case err
         (mark)
       (error (list (car err) (cdr err))))
     (mark t)
     (let ((mark-even-if-inactive t))
       (mark))
     (progn
       (setq mark-active t)
       (mark)))))
"#;

    let expect = expect_test::expect![[r#""OK ((mark-inactive nil) 2 2 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_set_mark_nil_clears_mark_and_region_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  (insert "abc")
  (set-mark 2)
  (let ((before (list (mark t) mark-active (marker-position (mark-marker)))))
    (set-mark nil)
    (list
     before
     mark-active
     (mark t)
     (marker-position (mark-marker))
     (condition-case err
         (region-beginning)
       (error (list (car err) (cdr err)))))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((2 t 2) nil nil nil (error (\"The mark is not set now, so there is no region\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_push_mark_return_value_and_ring_side_effects() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  (insert "abcdef")
  (let ((mark-ring nil)
        (global-mark-ring nil)
        (transient-mark-mode t))
    (goto-char 3)
    (let ((first (push-mark nil t nil))
          (first-mark (mark t))
          (first-active mark-active))
      (goto-char 5)
      (let ((second (push-mark nil t t)))
        (list
         first first-mark first-active
         second (mark t) mark-active
         (mapcar #'marker-position mark-ring)
         (mapcar #'marker-position global-mark-ring))))))
"#;

    let expect = expect_test::expect![[r#""OK (nil 3 nil nil 5 t (3) (3))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
