//! Strict combo oracle probes, batch 16: redisplay/screen-engine motion —
//! negative vertical-motion over wrapped lines, visual line-move, partial
//! pos-visible-in-window-p coordinates, compute-motion, current/move-to
//! column over wide lines, and count-screen-lines with explicit width.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_f1_vertical_motion_negative_wrapped() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (159 80 80)""#]];
    // Divergence surfaced 2026-06-27:
    // GNU Emacs: OK (159 80 80)
    // Neomacs:   OK (1 1 80)
    // vertical-motion does not wrap long lines: GNU advances 2 screen lines
    // (to ~char 160) then 1 back (to 80); Neomacs never leaves point 1.
    // (line-move visual, compute-motion and move-to-column DO wrap — only
    // vertical-motion and count-screen-lines lack wrapping.)
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((b (get-buffer-create " *probe-vmn*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (with-current-buffer b (erase-buffer) (insert (make-string 300 ?x)))
        (switch-to-buffer b)
        (goto-char 1)
        (vertical-motion 2)
        (let ((p1 (point)))
          (vertical-motion -1)
          (list p1 (point) (window-body-width))))
    (when (buffer-live-p b) (kill-buffer b))
    (delete-other-windows)))
"##,
        expect,
    );
}

#[test]
fn div_f1_line_move_visual_wrapped() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (end-of-buffer)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((b (get-buffer-create " *probe-lmv*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (with-current-buffer b (erase-buffer) (insert (make-string 300 ?x)))
        (switch-to-buffer b)
        (goto-char 1)
        (let ((line-move-visual t)) (line-move 1))
        (list (point) (window-body-width)))
    (when (buffer-live-p b) (kill-buffer b))
    (delete-other-windows)))
"##,
        expect,
    );
}

#[test]
fn div_f1_pos_visible_partial_coordinates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil 1 911)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((b (get-buffer-create " *probe-pvp2*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (with-current-buffer b
          (erase-buffer)
          (dotimes (i 10) (insert (make-string 90 ?x) "\n")))
        (switch-to-buffer b)
        (goto-char (point-min))
        (list (pos-visible-in-window-p 5)
              (pos-visible-in-window-p 250)
              (window-start)
              (window-end nil t)))
    (when (buffer-live-p b) (kill-buffer b))
    (delete-other-windows)))
"##,
        expect,
    );
}

#[test]
fn div_f1_compute_motion_wide_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument consp 23)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((b (get-buffer-create " *probe-cm*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (with-current-buffer b (erase-buffer) (insert (make-string 200 ?x)))
        (switch-to-buffer b)
        (compute-motion 1 '(0 . 0) (point-max) (window-body-height)
                        (window-body-width) 0 nil))
    (when (buffer-live-p b) (kill-buffer b))
    (delete-other-windows)))
"##,
        expect,
    );
}

#[test]
fn div_f1_column_over_wide_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 100 50)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((b (get-buffer-create " *probe-col*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (with-current-buffer b (erase-buffer) (insert (make-string 200 ?x)))
        (switch-to-buffer b)
        (goto-char 1)
        (list (current-column)
              (progn (move-to-column 100) (current-column))
              (progn (move-to-column 50) (current-column))))
    (when (buffer-live-p b) (kill-buffer b))
    (delete-other-windows)))
"##,
        expect,
    );
}

#[test]
fn div_f1_count_screen_lines_various_widths() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (6 80 23)""#]];
    // Divergence surfaced 2026-06-27:
    // GNU Emacs: OK (6 80 23)
    // Neomacs:   OK (3 80 23)
    // count-screen-lines does not wrap the 250-char line into multiple
    // screen lines; GNU counts 6 (wrapped), Neomacs counts 3.
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((b (get-buffer-create " *probe-csw*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (with-current-buffer b
          (erase-buffer)
          (insert "short\n")
          (insert (make-string 250 ?x))
          (insert "\nmore\n"))
        (switch-to-buffer b)
        (list (count-screen-lines (point-min) (point-max))
              (window-body-width)
              (window-text-height)))
    (when (buffer-live-p b) (kill-buffer b))
    (delete-other-windows)))
"##,
        expect,
    );
}
