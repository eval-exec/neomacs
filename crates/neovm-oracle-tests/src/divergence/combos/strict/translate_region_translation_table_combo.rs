//! Strict combo oracle probes, batch 194: char-table translation. make-char-
//! table translation-table, aset mappings, translate-region over ASCII and
//! multibyte, make-translation-table from alist, and translation-table-from-
//! vector.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_translate_region_chartable_ascii() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((table (make-char-table 'translation-table nil)))
  (aset table ?a ?A)
  (aset table ?b ?B)
  (aset table ?c ?C)
  (with-temp-buffer
    (insert "abcdefg")
    (translate-region 1 4 table)
    (buffer-string)))
"##;
    let expect = expect_test::expect![[r#""OK \"ABCdefg\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_make_translation_table_alist_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((table (make-translation-table '((?a . ?@) (?o . ?0) (?e . ?3)))))
  (with-temp-buffer
    (insert "hello world")
    (translate-region 1 12 table)
    (buffer-string)))
"##;
    let expect = expect_test::expect![[r#""OK \"h3ll0 w0rld\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_translate_region_full_span_ranges() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((table (make-char-table 'translation-table nil)))
  (set-char-table-range table '(?a . ?z) ?*)
  (with-temp-buffer
    (insert "Hello World 123")
    (let ((before (buffer-string)))
      (translate-region 1 (point-max) table)
      (list before (buffer-string)))))
"##;
    let expect = expect_test::expect![[r#""OK (\"Hello World 123\" \"H**** W**** 123\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
