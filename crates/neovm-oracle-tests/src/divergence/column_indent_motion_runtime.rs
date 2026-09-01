//! Column/indentation/line-motion parity: current-column with tabs and wide
//! (CJK) chars, move-to-column (+force), indent-to / indent-line-to /
//! current-indentation / back-to-indentation, forward-line return value,
//! beginning/end-of-line with arg + line-beginning/end-position, char-after/
//! before/following/preceding at boundaries, count-lines edges.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn back_to_indentation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (7 10)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "   \t  text here")
  (back-to-indentation)
  (list (point) (current-column)))"##,
        expect,
    );
}

#[test]
fn beginning_end_line_arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (12 7 19 18)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "line1\nline2\nline3\n")
  (goto-char 1)
  (list (progn (end-of-line 2) (point))
        (progn (beginning-of-line 1) (point))
        (line-beginning-position 3) (line-end-position 2)))"##,
        expect,
    );
}

#[test]
fn char_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((97 nil 97 0) (nil 99))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "abc")
  (list (progn (goto-char 1) (list (char-after) (char-before) (following-char) (preceding-char)))
        (progn (goto-char (point-max)) (list (char-after) (char-before)))))"##,
        expect,
    );
}

#[test]
fn count_lines_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 3 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "a\nb\nc")
  (list (count-lines (point-min) (point-max))
        (progn (insert "\n") (count-lines (point-min) (point-max)))
        (count-lines 1 1)))"##,
        expect,
    );
}

#[test]
fn current_column_tabs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (17 8 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (setq tab-width 8)
  (insert "a\tb\tc")
  (goto-char (point-max))
  (list (current-column)
        (progn (goto-char 3) (current-column))
        (progn (goto-char 2) (current-column))))"##,
        expect,
    );
}

#[test]
fn current_column_wide() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (5 2 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "日本x")
  (goto-char (point-max))
  (list (current-column) (progn (goto-char 2) (current-column))
        (progn (goto-char 3) (current-column))))"##,
        expect,
    );
}

#[test]
fn forward_line_return() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0 3 4 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "a\nb\nc")
  (goto-char (point-min))
  (list (forward-line 2) (line-number-at-pos)
        (forward-line 5) (line-number-at-pos)))"##,
        expect,
    );
}

#[test]
fn vertical_motion_narrow_window_moves_one_logical_line_with_treemacs_like_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (20 20 14 2 1 43 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (let ((buf (current-buffer)))
    (delete-other-windows)
    (let ((w (split-window (selected-window) 20 'left)))
      (select-window (next-window w))
      (setq w (selected-window))
      (switch-to-buffer buf)
      (setq truncate-lines nil)
      (insert (propertize "	neomacs ↑1" 'display '(raise 0.0)) "\n")
      (insert (propertize "" 'display '(raise 0.1))
              "	"
              (propertize "" 'display '(raise 0.0))
              "	"
              (propertize ".agent-shell/transcripts"
                          :collapsed '(1 "a" "b"))
              "\n")
      (insert (propertize "" 'display '(raise 0.1))
              "	"
              (propertize "" 'display '(raise 0.0))
              "	.cargo\n")
      (goto-char (point-min))
      (forward-line 1)
      (list (window-width w)
            (window-body-width w)
            (point)
            (line-number-at-pos)
            (vertical-motion 1 w)
            (point)
            (line-number-at-pos)))))"##,
        expect,
    );
}

#[test]
fn indent_line_to() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (8 \"\thello\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "    hello")
  (goto-char (point-min))
  (indent-line-to 8)
  (list (current-indentation) (buffer-string)))"##,
        expect,
    );
}

#[test]
fn indent_to_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (10 \"x\t  \")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "x")
  (indent-to 10)
  (list (current-column) (buffer-string)))"##,
        expect,
    );
}

#[test]
fn move_to_column_force() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (8 8 20 20)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (setq tab-width 8)
  (insert "ab\tcd")
  (goto-char (point-min))
  (list (move-to-column 5) (current-column)
        (progn (goto-char (point-min)) (move-to-column 20 t)) (current-column)))"##,
        expect,
    );
}
