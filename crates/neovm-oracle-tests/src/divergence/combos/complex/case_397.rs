//! Complex combo batch 397 — `electric-pair`/`electric-indent`/`electric-
//! quote`/`electric-layout`/`repeat-mode`/`display-line-numbers`/
//! `visual-line`/`which-func`/`so-long` availability and behavior.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx397_electric_modes_availability_full() {
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
    )
}

#[test]
fn div_cx397_electric_pair_pairs_table_query() {
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
    )
}

#[test]
fn div_cx397_electric_indent_rules_query() {
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
    )
}

#[test]
fn div_cx397_subword_mode_word_navigation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 7 7)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "pre myCamelCaseVar snake_case_var rest end")
      (goto-char 5)
      (let ((default-pos (point)))
        (subword-mode 1)
        (forward-word 1)
        (let ((sub-pos (point)))
          (subword-mode -1)
          (list default-pos sub-pos (point)))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx397_repeat_mode_availability() {
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
    )
}

#[test]
fn div_cx397_display_line_numbers_availability() {
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
    )
}

#[test]
fn div_cx397_visual_line_mode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (fboundp 'visual-line-mode)
      (boundp 'visual-line-fringe-indicators)
      (boundp 'word-wrap))
"##,
        expect,
    )
}

#[test]
fn div_cx397_so_long_mode_availability() {
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
    )
}

#[test]
fn div_cx397_which_function_mode_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'which-func)
      (list (fboundp 'which-function-mode)
            (boundp 'which-func-modes)
            (boundp 'which-func-format)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx397_electric_with_marker_overlay_undo_narrow_mega() {
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
    (goto-char 5)
    (forward-word 1)
    (let ((state (list (point)
                       (boundp 'electric-indent-mode)
                       (boundp 'electric-pair-mode)
                      (fboundp 'display-line-numbers-mode)
                      (fboundp 'visual-line-mode)
                      (fboundp 'so-long)
                      (buffer-string)
                      (marker-position m)
                      (overlay-start ov) (overlay-end ov)
                      (text-properties-at 1))))
      (undo)
      (widen()
      (list state (buffer-string) (marker-position m)
            (overlay-start ov) (overlay-end ov)
            (text-properties-at 1)))))))
"##,
        expect,
    )
}
