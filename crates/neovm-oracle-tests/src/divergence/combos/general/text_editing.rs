//! Divergence tests: complex text editing combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_kill_yank_rectangle_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"line1\\nline1-B\\nline1-C\" \"line1\\nline1-B\\nline1-C-A\" \"line1\\nline1-B\\nline1-C-A\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"line1-A\\nline1-B\\nline1-C\")
  (goto-char 1)
  (let ((start (point)))
    (undo-boundary)
    (kill-rectangle 6 8)
    (let ((s1 (buffer-string)))
      (goto-char (point-max))
      (undo-boundary)
      (yank-rectangle)
      (let ((s2 (buffer-string)))
        (list s1 s2 (buffer-string)))))) ",
        expect,
    );
}

#[test]
fn divergence_transpose_regions_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"-DDD-BBBB-CCCCAAAD\"""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"AAA-BBBB-CCCC-DDDD\")
  (transpose-regions 1 4 14 18)
  (buffer-string)) ",
        expect,
    );
}

#[test]
fn divergence_format_region_insert_delete_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"item-000\" \"item-001\" \"item-002\" \"item-003\" \"item-004\") 1 nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (dotimes (i 5) (insert (format \"item-%03d\\n\" i)))
  (goto-char 1)
  (let ((lines nil))
    (while (not (eobp))
      (push (buffer-substring (line-beginning-position) (line-end-position)) lines)
      (forward-line 1))
    (list (nreverse lines)
          (length (nreverse lines))
          (string= (nth 0 (nreverse lines)) \"item-000\")))) ",
        expect,
    );
}

#[test]
fn divergence_kill_ring_save_excursion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"FIRST PAR\" \"FIRST PART\" \" SECOND PART THIRD PARTFIRST PART\" 34)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"FIRST PART SECOND PART THIRD PART\")
  (let ((p1 (save-excursion
               (goto-char 1)
               (buffer-substring 1 10))))
    (kill-region 1 11)
    (let ((killed-text (current-kill 0)))
      (goto-char (point-max))
      (yank)
      (list p1
            (string-trim killed-text)
            (buffer-string)
            (point))))) ",
        expect,
    );
}

#[test]
fn divergence_replace_regex_with_backref_transform() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK \"name:Alice age:31 city:NYC\\nname:Bob age:26 city:LA\"""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"name:Alice age:30 city:NYC\\nname:Bob age:25 city:LA\")
  (goto-char 1)
  (while (re-search-forward \"age:\\\\([0-9]+\\\\)\" nil t)
    (let ((age (string-to-number (match-string 1))))
      (replace-match (format \"age:%d\" (+ age 1)) t)))
  (buffer-string)) ",
        expect,
    );
}

#[test]
fn divergence_fill_region_paragraphs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"This is a very long line that should be filled at the fill column\\n boundary.\" t t 2)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (setq fill-column 20)
  (insert \"This is a very long line that should be filled at the fill column boundary.\")
  (goto-char 1)
  (fill-paragraph nil)
  (list (buffer-string)
        (>= (length (buffer-string)) 50)
        (<= (length (buffer-string)) 200)
        (length (split-string (buffer-string) \"\\n\")))) ",
        expect,
    );
}

#[test]
fn divergence_sort_lines_in_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"apple\\nbanana\\ncherry\\ndate\\nelm\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"cherry\\napple\\nbanana\\ndate\\nelm\")
  (sort-lines nil 1 (point-max))
  (list (buffer-string)
        (string= (buffer-string) \"apple\\nbanana\\ncherry\\ndate\\nelm\"))) ",
        expect,
    );
}

#[test]
fn divergence_delete_duplicate_lines() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"aaa\\nbbb\\nccc\\nddd\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"aaa\\nbbb\\naaa\\nccc\\nbbb\\nddd\")
  (delete-duplicate-lines 1 (point-max))
  (list (buffer-string)
        (string= (buffer-string) \"aaa\\nbbb\\nccc\\nddd\"))) ",
        expect,
    );
}

#[test]
fn divergence_align_regex() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (error \"No match for subexpression 1\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"name:Alice\\nage:30\\ncity:NYC\")
  (align-regexp 1 (point-max) \":\" 1 1 t)
  (list (buffer-string)
        (string-match \" +:\" (buffer-string)))) ",
        expect,
    );
}

#[test]
fn divergence_indent_rigidly() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"    line1\\n      line2\\n\tline3\" \"  line1\\n    line2\\n      line3\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"line1\\n  line2\\n    line3\")
  (indent-rigidly 1 (point-max) 4)
  (let ((s1 (buffer-string)))
    (indent-rigidly 1 (point-max) -2)
    (list s1 (buffer-string)))) ",
        expect,
    );
}
