//! Strict combo oracle probes, batch 329: char-fold extended variants —
//! compatibility decomposition folds (ﬁ→fi ligature, ①→1 circled, ½→1/2
//! fraction), case-fold + char-fold combined, and char-fold multi-char.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_char_fold_compatibility_ligature_circled_fraction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'char-fold)
(let ((pairs '(("fi" . "ﬁ") ("ff" . "ﬀ") ("1" . "①") ("2" . "②")
               ("1/2" . "½") ("A" . "Å"))))
  (mapcar (lambda (p)
            (let ((re (char-fold-to-regexp (car p))))
              (with-temp-buffer
                (insert (cdr p))
                (goto-char (point-min))
                (if (re-search-forward re nil t) 'match 'no-match))))
          pairs))
"##;
    let expect = expect_test::expect![[r#""OK (match match match match no-match match)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_char_fold_case_fold_combined() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'char-fold)
(let ((case-fold-search t))
  (let ((pairs '(("hello" . "HELLO") ("cafe" . "CAFÉ") ("abc" . "ÄBC"))))
    (mapcar (lambda (p)
              (let ((re (char-fold-to-regexp (car p))))
                (with-temp-buffer
                  (insert (cdr p))
                  (goto-char (point-min))
                  (if (re-search-forward re nil t) 'match 'no-match))))
            pairs)))
"##;
    let expect = expect_test::expect![[r#""OK (match match match)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_char_fold_table_sparse_inclusion_exclusion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'char-fold)
(list (char-table-p char-fold-table)
      (let ((entry (aref char-fold-table ?a)))
        (or (null entry) (stringp entry) (consp entry)))
      (let ((entry (aref char-fold-table ?i)))
        (or (null entry) (stringp entry) (consp entry)))
      (let ((entry (aref char-fold-table ?e)))
        (or (null entry) (stringp entry) (consp entry)))
      (length (char-fold-to-regexp "test")))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t 224)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
