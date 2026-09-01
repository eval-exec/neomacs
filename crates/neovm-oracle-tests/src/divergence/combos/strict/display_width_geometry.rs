//! Strict combo oracle probes, batch 13: string-width under display text
//! properties, window internal metric defaults, distinct display-geometry
//! observables (count-screen-lines, pos-visible, move-to-window-line), and
//! buffer justification/fill-column queries.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_e8_string_width_with_display_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 1 2 0 3 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-width "abc")
      (string-width (propertize "x" 'display "longer-text"))
      (string-width (propertize "ab" 'display "ZZZ"))
      (string-width (propertize "" 'display "shown"))
      (string-width (propertize "abc" 'display '(space :width 5)))
      (string-width (propertize "abc" 'display '(space :align-to 10))))
"##,
        expect,
    );
}

#[test]
fn div_e8_window_internal_metrics() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((nil) (0 0 nil nil) (nil 0 t nil 0 t nil) 0 80 80)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((b (get-buffer-create " *probe-wim*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer b)
        (list (window-margins)
              (window-fringes)
              (window-scroll-bars)
              (window-vscroll)
              (window-body-width)
              (window-total-width)))
    (when (buffer-live-p b) (kill-buffer b))
    (delete-other-windows)))
"##,
        expect,
    );
}

#[test]
fn div_e8_justification_fill_column() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (70 0 left 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abc")
  (list (current-fill-column)
        (current-left-margin)
        (current-justification)
        (current-indentation)))
"##,
        expect,
    );
}

#[test]
fn div_e8_geometry_count_screen_lines_wrapped() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 80 23)""#]];
    // Divergence surfaced 2026-06-27:
    // GNU Emacs: OK (3 80 23)
    // Neomacs:   OK (1 80 23)
    // count-screen-lines does not wrap long lines in Neomacs: a 200-char line
    // on an 80-column body (window-body-width agrees at 80) is 3 screen lines
    // in GNU but Neomacs counts it as a single screen line.
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((b (get-buffer-create " *probe-csl*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (with-current-buffer b (erase-buffer) (insert (make-string 200 ?x)))
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

#[test]
fn div_e8_geometry_pos_visible_in_window() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil 1 411)""#]];
    // Divergence surfaced 2026-06-27:
    // GNU Emacs: OK (nil nil 1 411)
    // Neomacs:   OK (nil nil 1 152)
    // window-end (with update) reports a far smaller position in Neomacs; the
    // visible-region geometry for a many-line buffer diverges from GNU.
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((b (get-buffer-create " *probe-pvw*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (with-current-buffer b
          (erase-buffer)
          (dotimes (i 60) (insert (format "line%d\n" i))))
        (switch-to-buffer b)
        (goto-char (point-min))
        (list (pos-visible-in-window-p (point-min))
              (pos-visible-in-window-p (point-max))
              (window-start)
              (window-end nil t)))
    (when (buffer-live-p b) (kill-buffer b))
    (delete-other-windows)))
"##,
        expect,
    );
}

#[test]
fn div_e8_geometry_move_to_window_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 80 1 80)""#]];
    // Divergence surfaced 2026-06-27:
    // GNU Emacs: OK (1 80 1 80)
    // Neomacs:   OK (1 1 1 80)
    // vertical-motion across a long unwrapped line: GNU advances by one screen
    // line (to the body width, char 80); Neomacs does not wrap and stays at
    // point 1.
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((b (get-buffer-create " *probe-mtwl*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (with-current-buffer b
          (erase-buffer)
          (insert (make-string 200 ?x)))
        (switch-to-buffer b)
        (goto-char (point-min))
        (let ((p0 (point)))
          (vertical-motion 1)
          (let ((p1 (point)))
            (move-to-window-line 0)
            (list p0 p1 (point) (window-body-width)))))
    (when (buffer-live-p b) (kill-buffer b))
    (delete-other-windows)))
"##,
        expect,
    );
}

#[test]
fn div_e8_format_control_char_strings() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"\u{7}\u{7}\u{7}\" \"\\\"\u{7}\u{7}\u{7}\\\"\" \"\\\"\u{7}\u{8}\\\"\" \"\t\\n\\r\" \"\\\"\t\\n\\r\\\"\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (format "%s" (make-string 3 7))
      (prin1-to-string (make-string 3 7))
      (format "%S" "\a\b")
      (format "%s" (string 9 10 13))
      (prin1-to-string (string 9 10 13)))
"##,
        expect,
    );
}
