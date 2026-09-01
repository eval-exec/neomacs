//! Strict combo oracle probes, batch 326: char-fold deep. char-fold-to-regexp
//! over ASCII/latin-with-diacritics/CJK, char-fold-sparse, and search using
//! the folded regexp.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_char_fold_to_regexp_diacritic_cjk() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'char-fold)
(list (char-fold-to-regexp ?a)
      (char-fold-to-regexp ?é)
      (char-fold-to-regexp ?Ä)
      (char-fold-to-regexp ?日)
      (char-fold-to-regexp "abc")
      (length (char-fold-to-regexp ?e))
      (stringp (char-fold-to-regexp ?a)))
"##;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument sequencep 97)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_char_fold_search_match_diacritic_insensitive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'char-fold)
(with-temp-buffer
  (insert "café naïve résumé")
  (let ((re (char-fold-to-regexp "cafe")))
    (goto-char (point-min))
    (list (re-search-forward re nil t)
          (match-string 0)
          (let ((re2 (char-fold-to-regexp "naive")))
            (goto-char (point-min))
            (re-search-forward re2 nil t))
          (match-string 0))))
"##;
    let expect = expect_test::expect![[r#""OK (5 \"café\" 11 \"naïve\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_char_fold_spare_include_exclude_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'char-fold)
(list (char-table-p char-fold-table)
      (char-fold-sparse)
      (consp (char-fold-sparse ?a char-fold-table))
      (stringp (char-fold-to-regexp "test"))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function char-fold-sparse)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
