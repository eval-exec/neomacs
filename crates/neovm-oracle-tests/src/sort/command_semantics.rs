//! Oracle parity tests for GNU sorting and line-reordering commands.
//!
//! GNU implements these in `lisp/sort.el`, centered on `sort-subr`.
//! The tests compare final buffer text and return values for region
//! narrowing, field keys, numeric bases, line reversal, and duplicate removal.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_sort_lines_respects_region_and_fold_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'sort)
  (with-temp-buffer
    (insert "keep\nbeta\nAlpha\nalpha\nzeta\nkeep2\n")
    (let ((sort-fold-case t))
      (sort-lines nil
                  (save-excursion (goto-char (point-min)) (forward-line 1) (point))
                  (save-excursion (goto-char (point-min)) (forward-line 5) (point))))
    (buffer-string)))
"#;

    let expect =
        expect_test::expect![[r#""OK \"keep\\nAlpha\\nalpha\\nbeta\\nzeta\\nkeep2\\n\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_sort_fields_positive_and_negative_field_numbers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'sort)
  (let (first second)
    (with-temp-buffer
      (insert "b 02 z\nc 01 a\na 03 m\n")
      (sort-fields 2 (point-min) (point-max))
      (setq first (buffer-string)))
    (with-temp-buffer
      (insert "b 02 z\nc 01 a\na 03 m\n")
      (sort-fields -1 (point-min) (point-max))
      (setq second (buffer-string)))
    (list first second)))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"c 01 a\\nb 02 z\\na 03 m\\n\" \"c 01 a\\na 03 m\\nb 02 z\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_sort_numeric_fields_base_detection_and_blank_lines() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'sort)
  (with-temp-buffer
    (insert "hex 0x10\nblank   \noctal 010\ndecimal 9\n")
    (let ((sort-numeric-base 10))
      (sort-numeric-fields 2 (point-min) (point-max)))
    (buffer-string)))
"#;

    let expect = expect_test::expect![[r#""ERR (error \"Line has too few fields: blank   \")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_reverse_region_uses_only_full_lines() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'sort)
  (with-temp-buffer
    (insert "outer-a\none\ntwo\nthree\nouter-b\n")
    (reverse-region
     (save-excursion (goto-char (point-min)) (forward-line 1) (forward-char 1) (point))
     (save-excursion (goto-char (point-min)) (forward-line 4) (forward-char 2) (point)))
    (buffer-string)))
"#;

    let expect = expect_test::expect![[r#""OK \"outer-a\\none\\nthree\\ntwo\\nouter-b\\n\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_delete_duplicate_lines_modes_and_return_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'sort)
  (let (normal reverse adjacent keep-blanks)
    (with-temp-buffer
      (insert "a\nb\na\n\n\nb\n")
      (setq normal (list (delete-duplicate-lines (point-min) (point-max))
                         (buffer-string))))
    (with-temp-buffer
      (insert "a\nb\na\n\n\nb\n")
      (setq reverse (list (delete-duplicate-lines (point-min) (point-max) t)
                          (buffer-string))))
    (with-temp-buffer
      (insert "a\na\nb\na\nb\nb\n")
      (setq adjacent (list (delete-duplicate-lines (point-min) (point-max) nil t)
                           (buffer-string))))
    (with-temp-buffer
      (insert "a\n\n\nb\na\n")
      (setq keep-blanks (list (delete-duplicate-lines (point-min) (point-max) nil nil t)
                              (buffer-string))))
    (list normal reverse adjacent keep-blanks)))
"#;

    let expect = expect_test::expect![[
        r#""OK ((3 \"a\\nb\\n\\n\") (3 \"a\\n\\nb\\n\") (2 \"a\\nb\\na\\nb\\n\") (1 \"a\\n\\n\\nb\\n\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
