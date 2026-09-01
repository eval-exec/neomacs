//! Strict combo oracle probes, batch 297: char-table + category-table
//! behavioral. make-char-table range set/query, make-category-table +
//! modify-category-entry + char-category-set, and category-doc-string.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_char_table_range_query_uniform_spans() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((ct (make-char-table 'syntax-table nil)))
  (aset ct ?a 'word)
  (set-char-table-range ct '(?A . ?Z) 'upper)
  (set-char-table-range ct ?! 'punct)
  (list (char-table-range ct ?a)
        (char-table-range ct ?g)
        (char-table-range ct ?Z)
        (char-table-range ct ?0)
        (char-table-range ct '(?A . ?Z))
        (char-table-range ct '(?a . ?z))))
"##;
    let expect = expect_test::expect![[r#""OK (word nil upper nil upper word)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_category_table_modify_doc_string_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((cat (make-category-table)))
  (define-category ?x "probe category X" cat)
  (modify-category-entry ?a ?x cat)
  (modify-category-entry ?b ?x cat)
  (list (category-doc-string ?x cat)
        (char-category-set ?a cat)
        (category-set-mnemonics (char-category-set ?a cat))
        (char-category-set ?z cat)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function category-doc-string)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_char_table_extra_slot_parent_inherit_behavioral() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let* ((ct (make-char-table 'foo nil))
       (child (make-char-table 'foo 'child-default)))
  (aset ct ?a 'parent-a)
  (set-char-table-range ct '(?A . ?Z) 'parent-upper)
  (set-char-table-parent child ct)
  (aset child ?a 'child-a)
  (list (char-table-range child ?a)
        (char-table-range child ?z)
        (char-table-range child ?B)
        (char-table-range child ?5)
        (eq (char-table-parent child) ct)))
"##;
    let expect =
        expect_test::expect![[r#""OK (child-a child-default child-default child-default t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
