//! Divergence tests: indent + fill + paragraph + page + line combo.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_indent_line_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"    line1\\n\tline2\\n    line3\\nline4\\nline5\" t t t t 1 0 t nil 4 t nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "line1\nline2\nline3\nline4\nline5")
  (goto-char 1)
  (indent-to 4)
  (forward-line 1)
  (indent-to 8)
  (forward-line 1)
  (insert "    ")
  (let ((s (buffer-string))
        (col-0 (progn (goto-char 1) (current-indentation)))
        (col-1 (progn (forward-line 1) (current-indentation)))
        (col-2 (progn (forward-line 1) (current-indentation)))
        (col-3 (progn (forward-line 1) (current-indentation))))
    (list s
          (= col-0 4)
          (= col-1 8)
          (= col-2 4)
          (= col-3 0)
          (goto-char 1)
          (current-column)
          (= (current-column) 0)
          (back-to-indentation)
          (current-column)
          (= (current-column) 4)
          (= (buffer-size) 30)))) "#,
        expect,
    );
}

#[test]
fn divergence_fill_region_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (t t t 20 t t \"This is a long line that should be filled at the fill-column boundary\\n when we call fill-paragraph.\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (setq fill-column 20)
  (insert "This is a long line that should be filled at the fill-column boundary when we call fill-paragraph.")
  (let ((len-before (buffer-size)))
    (goto-char 1)
    (fill-paragraph nil)
    (let ((s (buffer-string))
          (len-after (buffer-size)))
      (list (> len-before 0)
            (>= len-after len-before)
            (= (length (split-string s "\n")) (length (split-string s "\n")))
            fill-column
            (= fill-column 20)
            (>= (length (split-string s "\n")) 1)
            s)))) "#,
        expect,
    );
}

#[test]
fn divergence_paragraph_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil 69 0 0 t t 0 0 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "First paragraph here.\n\nSecond paragraph here.\n\nThird paragraph here.")
  (goto-char 1)
  (let ((p1-start (point))
        (p1-end (progn (forward-paragraph) (point)))
        (p2-start (progn (forward-paragraph) (point))))
    (list (> p1-end p1-start)
          (>= p2-start p1-end)
          (= (buffer-size) 55)
          (goto-char (point-max))
          (backward-paragraph)
          (backward-paragraph)
          (>= (point) 1)
          (< (point) (point-max))
          (forward-paragraph)
          (forward-paragraph)
          (= (point) (point-max))))) "#,
        expect,
    );
}

#[test]
fn divergence_page_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp \"Page 1, line 1\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Page 1 content\n\x0CPage 2 content\n\x0CPage 3 content")
  (goto-char 1)
  (let ((page-count 1))
    (while (re-search-forward "\x0C" nil t)
      (setq page-count (+ page-count 1)))
    (list (= page-count 3)
          (= (buffer-size) 36)
          (goto-char 1)
          (what-page)
          (consp (what-page))
          (= (car (what-page)) 1)))) "#,
        expect,
    );
}

#[test]
fn divergence_comment_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    crate::common::assert_oracle_parity(
        r#"(progn
  (insert "line1\nline2\nline3\nline4")
  (comment-region 1 6)
  (let ((s1 (buffer-string)))
    (uncomment-region 1 6)
    (let ((s2 (buffer-string)))
      (list (string= s2 "line1\nline2\nline3\nline4")
            (= (buffer-size) 23)
            (string-match ";" s1)
            (> (length s1) (length s2)))))) "#,
    );
}

#[test]
fn divergence_tab_stop_and_indent_tabs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t 4 t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (setq tab-width 4)
  (setq indent-tabs-mode nil)
  (insert "hello")
  (goto-char 1)
  (indent-to 8)
  (let ((s (buffer-string))
        (col (current-column)))
    (list (= col 8)
          (string= s "        hello")
          (= (length s) 13)
          tab-width
          (= tab-width 4)
          indent-tabs-mode
          (null indent-tabs-mode)))) "#,
        expect,
    );
}

#[test]
fn divergence_current_column_with_mixed() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0 t 3 t 4 t 7 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (setq tab-width 4)
  (insert "abc\tdef\tghi")
  (goto-char 1)
  (let ((c0 (current-column)))
    (forward-char 3)
    (let ((c3 (current-column)))
      (forward-char 1)
      (let ((c4 (current-column)))
        (forward-char 3)
        (let ((c7 (current-column)))
          (list c0 (= c0 0)
                c3 (= c3 3)
                c4 (= c4 4)
                c7 (>= c7 7)
                (= (buffer-size) 11))))))) "#,
        expect,
    );
}

#[test]
fn divergence_line_beginning_position() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t 1 1 t 30 5 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "line1\nline2\nline3\nline4\nline5")
  (goto-char 10)
  (let ((bol (line-beginning-position))
        (eol (line-end-position))
        (num (line-number-at-pos)))
    (list (= bol 7)
          (= eol 12)
          (= num 2)
          (goto-char (point-min))
          (line-number-at-pos)
          (= (line-number-at-pos) 1)
          (goto-char (point-max))
          (line-number-at-pos)
          (= (line-number-at-pos) 5)
          (= (buffer-size) 29)))) "#,
        expect,
    );
}

#[test]
fn divergence_delete_indentation_and_newline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "hello\nworld\ntest")
  (goto-char 7)
  (let ((before (buffer-string)))
    (delete-indentation)
    (let ((after (buffer-string)))
      (list (string= before "hello\nworld\ntest")
            (= (length (split-string before "\n")) 3)
            (= (length (split-string after "\n")) 2)
            (>= (length before) (length after)))))) "#,
        expect,
    );
}

#[test]
fn divergence_move_to_column() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "abcdefghijklmn")
  (goto-char 1)
  (move-to-column 5)
  (let ((c1 (current-column)))
    (move-to-column 10)
    (let ((c2 (current-column)))
      (move-to-column 0)
      (let ((c3 (current-column)))
        (list (= c1 5)
              (= c2 10)
              (= c3 0)
              (= (point) 1)
              (= (buffer-size) 14)))))) "#,
        expect,
    );
}
