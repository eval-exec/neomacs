//! Complex combo batch 216 — `electric-indent` / `electric-pair` /
//! `electric-quote` / `electric-layout` / `subword` / `superword` /
//! `which-func` / `repeat-mode` / `display-line-numbers` availability.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx216_electric_modes_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (boundp 'electric-indent-mode)
      (boundp 'electric-pair-mode)
      (boundp 'electric-quote-mode)
      (boundp 'electric-layout-mode)
      (fboundp 'electric-indent-local-mode)
      (fboundp 'electric-pair-local-mode))
"##,
        expect,
    );
}

#[test]
fn div_cx216_subword_superword_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 8 15 t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "myCamelCaseVar snake_case_var PascalCaseVar")
      (goto-char 1)
      (subword-mode 1)
      (forward-word 1)
      (let ((p1 (point)))
        (forward-word 1)
        (let ((p2 (point)))
          (subword-mode -1)
          (goto-char 1)
          (forward-word 1)
          (let ((p3 (point)))
            (list p1 p2 p3 (subword-mode 1) (subword-mode -1))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx216_repeat_mode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'repeat-mode)
          (boundp 'repeat-keep-prefix)
          (boundp 'repeat-exit-key)
          (boundp 'repeat-in-progress))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx216_display_line_numbers_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (fboundp 'display-line-numbers-mode)
      (fboundp 'global-display-line-numbers-mode)
      (boundp 'display-line-numbers-width)
      (boundp 'display-line-numbers-type))
"##,
        expect,
    );
}

#[test]
fn div_cx216_electric_pair_pairs_table_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable electric-pair-pairs)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (boundp 'electric-pair-pairs)
      (consp electric-pair-pairs)
      (boundp 'electric-pair-skip-whitespace)
      (boundp 'electric-pair-preserve-balance)
      (boundp 'electric-pair-inhibit-predicate))
"##,
        expect,
    );
}

#[test]
fn div_cx216_electric_indent_rules_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (boundp 'electric-indent-functions)
      (boundp 'electric-indent-chars)
      (boundp 'electric-indent-inhibit)
      (boundp 'electric-indent-just-newline))
"##,
        expect,
    );
}

#[test]
fn div_cx216_so_long_mode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'so-long)
      (list (fboundp 'so-long)
            (boundp 'so-long-threshold)
            (boundp 'so-long-max-lines)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx216_visual_line_mode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (fboundp 'visual-line-mode)
      (fboundp 'visual-fill-column-mode)
      (boundp 'visual-line-fringe-indicators)
      (boundp 'word-wrap))
"##,
        expect,
    );
}

#[test]
fn div_cx216_whitespace_mode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'whitespace)
      (list (fboundp 'whitespace-mode)
            (fboundp 'whitespace-toggle-options)
            (boundp 'whitespace-style)
            (boundp 'whitespace-space)
            (boundp 'whitespace-tab)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx216_electric_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "myCamelCaseVar snake_case_var test buffer content")
  (put-text-property 1 5 'face 'bold)
  (let ((m (set-marker (make-marker) 10))
        (ov (make-overlay 4 18)))
    (overlay-put ov 'face 'italic)
    (overlay-put ov 'evaporate t)
    (narrow-to-region 2 25)
    (goto-char 1)
    (forward-word 1)
    (let ((state (list (point)
                       (boundp 'electric-indent-mode)
                       (boundp 'electric-pair-mode)
                       (fboundp 'display-line-numbers-mode)
                       (buffer-string)
                       (marker-position m)
                       (overlay-start ov) (overlay-end ov)
                       (text-properties-at 1))))
      (undo)
      (widen)
      (list state (buffer-string) (marker-position m)
            (overlay-start ov) (overlay-end ov)
            (text-properties-at 1)))))
"##,
        expect,
    );
}
