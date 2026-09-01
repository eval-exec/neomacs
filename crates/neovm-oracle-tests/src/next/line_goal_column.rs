//! Oracle parity tests for GNU vertical-motion goal-column semantics.
//!
//! These tests cover the `next-line`/`previous-line` command behavior that
//! keeps a temporary goal column while moving across lines.  The GUI frontend
//! ultimately depends on the same Elisp command semantics: when point starts
//! in column N, `C-n`/`C-p` should try to keep point in column N on the target
//! line, clamping only while the target line is shorter.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_next_line_preserves_column_across_equal_width_lines() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  (insert "abcdef\nuvwxyz\n123456\n")
  (setq goal-column nil)
  (setq temporary-goal-column nil)
  (goto-char (point-min))
  (forward-char 4)
  (let ((before (list (point) (current-column) goal-column temporary-goal-column)))
    (next-line 1)
    (let ((after-one (list (point) (current-column) goal-column temporary-goal-column)))
      (next-line 1)
      (list before
            after-one
            (list (point) (current-column) goal-column temporary-goal-column)
            (buffer-substring-no-properties (line-beginning-position)
                                            (line-end-position))))))
"#;

    let expect =
        expect_test::expect![[r#""OK ((5 4 nil nil) (12 4 nil 4) (19 4 nil 4) \"123456\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_next_line_restores_temporary_goal_after_short_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  (insert "abcdef\nxy\n1234567\n")
  (setq goal-column nil)
  (setq temporary-goal-column nil)
  (goto-char (point-min))
  (forward-char 4)
  (let ((start (list (point) (current-column) goal-column temporary-goal-column)))
    (next-line 1)
    (let ((short-line (list (point) (current-column) goal-column temporary-goal-column)))
      (next-line 1)
      (list start
            short-line
            (list (point) (current-column) goal-column temporary-goal-column)
            (buffer-substring-no-properties (line-beginning-position)
                                            (line-end-position))))))
"#;

    let expect =
        expect_test::expect![[r#""OK ((5 4 nil nil) (10 2 nil 4) (13 2 nil 2) \"1234567\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_previous_line_uses_existing_goal_column() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  (insert "wide-line\nxy\nabcdefg\n")
  (setq goal-column nil)
  (setq temporary-goal-column nil)
  (goto-char (point-min))
  (forward-line 2)
  (forward-char 5)
  (let ((start (list (point) (current-column) goal-column temporary-goal-column)))
    (previous-line 1)
    (let ((short-line (list (point) (current-column) goal-column temporary-goal-column)))
      (previous-line 1)
      (list start
            short-line
            (list (point) (current-column) goal-column temporary-goal-column)
            (buffer-substring-no-properties (line-beginning-position)
                                            (line-end-position))))))
"#;

    let expect =
        expect_test::expect![[r#""OK ((19 5 nil nil) (13 2 nil 5) (3 2 nil 2) \"wide-line\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_next_line_honors_explicit_goal_column() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  (insert "abcdef\nuvwxyz\n123456\n")
  (setq temporary-goal-column nil)
  (let ((goal-column 2))
    (goto-char (point-min))
    (forward-char 5)
    (let ((start (list (point) (current-column) goal-column temporary-goal-column)))
      (next-line 1)
      (let ((after-one (list (point) (current-column) goal-column temporary-goal-column)))
        (next-line 1)
        (list start
              after-one
              (list (point) (current-column) goal-column temporary-goal-column)
              (buffer-substring-no-properties (line-beginning-position)
                                              (line-end-position)))))))
"#;

    let expect = expect_test::expect![[r#""OK ((6 5 2 nil) (10 2 2 5) (17 2 2 2) \"123456\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
