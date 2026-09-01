//! Strict combo oracle probes, batch 56: DEEP characterization of the
//! screen-line wrapping divergence (vertical-motion / count-screen-lines).
//! Varies the wrapping-influencing configuration: truncate-lines t, word-wrap,
//! narrow window width, CJK wide chars, and truncate-partial-width-windows.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_l0_wrap_with_truncate_lines_t() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    // Characterization: with truncate-lines t, long lines do NOT wrap — so this
    // should agree (1 screen line) in both engines. Confirms the wrapping gap
    // is about the default (truncate-lines nil) case.
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert (make-string 200 ?x))
  (let ((truncate-lines t))
    (count-screen-lines (point-min) (point-max))))
"##,
        expect,
    );
}

#[test]
fn div_l0_word_wrap_at_spaces() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 3""#]];
    // Divergence surfaced 2026-06-27:
    // GNU Emacs: OK 3
    // Neomacs:   OK 1
    // a 201-char line with a space at column 100 is 3 screen lines in GNU and
    // stayed 1 in Neomacs.
    //
    // Ledger 195 corrects this comment's REASON while leaving its value alone.
    // GNU's 3 is the CHARACTER-wrap answer, not a word boundary: the expression
    // runs under `--batch`, and `Fvertical_motion` under `noninteractive` is
    // `vmotion' -> `compute_motion' (src/indent.c:2280-2286, :1963-1964,
    // :1253-1254), which has no word-wrap concept at all -- the identifier
    // `word_wrap' does not occur in src/indent.c. `word-wrap' is an input to
    // `init_iterator' (src/xdisp.c:3425-3426), which only the interactive arm
    // reaches. Measured, GNU Emacs 31.0.90, 80-column terminal:
    //
    //   emacs --batch        rows 1 80 159       count-screen-lines 3
    //   emacs -nw in a pty   rows 1 80 102 181   count-screen-lines 4
    //
    // So this pin is a BATCH pin and 4 is the right answer in a terminal.
    // Ledger 191 word-wrapped in both engines, which is what this caught.
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert (make-string 100 ?x) " " (make-string 100 ?x))
  (let ((word-wrap t))
    (count-screen-lines (point-min) (point-max))))
"##,
        expect,
    );
}

#[test]
fn div_l0_wrap_in_narrow_window() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (40 0 201)""#]];
    // Divergence surfaced 2026-06-27:
    // GNU Emacs: OK (40 0 201)
    // Neomacs:   OK (40 0 1)
    // In a 40-column window, vertical-motion 1 over a 200-char line moves to
    // char 201 in GNU (wraps across screen lines); Neomacs stays at 1.
    // (count-screen-lines reports 0 in both here — the window-body-width of 40
    // agrees, confirming the frame geometry matches; only the wrap differs.)
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((b (get-buffer-create " *probe-wrap-narrow*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer b)
        (with-current-buffer b (erase-buffer) (insert (make-string 200 ?x)))
        (let ((w2 (split-window nil 40 'right)))
          (select-window w2)
          (list (window-body-width)
                (count-screen-lines (point-min) (point-max))
                (progn (goto-char (point-min)) (vertical-motion 1) (point)))))
    (when (buffer-live-p b) (kill-buffer b))
    (delete-other-windows)))
"##,
        expect,
    );
}

#[test]
fn div_l0_wrap_with_cjk_wide_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (80 2 40)""#]];
    // Divergence surfaced 2026-06-27:
    // GNU Emacs: OK (80 2 40)
    // Neomacs:   OK (80 1 1)
    // 50 CJK chars (100 display columns on an 80-col body) wrap to 2 screen
    // lines in GNU with vertical-motion landing at char 40; Neomacs counts 1
    // line and stays at char 1.
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((b (get-buffer-create " *probe-wrap-cjk*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer b)
        (with-current-buffer b
          (erase-buffer)
          (insert (make-string 50 ?一)))
        (list (window-body-width)
              (count-screen-lines (point-min) (point-max))
              (progn (goto-char (point-min)) (vertical-motion 1) (point))))
    (when (buffer-live-p b) (kill-buffer b))
    (delete-other-windows)))
"##,
        expect,
    );
}

#[test]
fn div_l0_vertical_motion_three_steps() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 80 2 238 -1 159)""#]];
    // Divergence surfaced 2026-06-27:
    // GNU Emacs: OK (1 80 2 238 -1 159)
    // Neomacs:   OK (0 1 0 1 0 1)
    // vertical-motion never moves on a single logical line: it returns the
    // step count (1/2/-1) and advances point in GNU, but in Neomacs it returns
    // 0 and leaves point at 1 regardless of step count or direction.
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((b (get-buffer-create " *probe-vm3*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer b)
        (with-current-buffer b (erase-buffer) (insert (make-string 250 ?x)))
        (goto-char (point-min))
        (list (vertical-motion 1)
              (point)
              (vertical-motion 2)
              (point)
              (vertical-motion -1)
              (point)))
    (when (buffer-live-p b) (kill-buffer b))
    (delete-other-windows)))
"##,
        expect,
    );
}

#[test]
fn div_l0_truncate_partial_width_windows_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 50 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (boundp 'truncate-partial-width-windows)
      (default-value 'truncate-partial-width-windows)
      (boundp 'word-wrap-by-category))
"##,
        expect,
    );
}
