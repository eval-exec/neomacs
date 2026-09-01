//! Strict combo oracle probes, batch 360: mixed-type mapping. mapcar/mapcan/
//! mapconcat over hash-table-keys/values + vectors + strings, and seq-map
//! across mixed types.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_mixed_type_mapcar_can_concat_strings() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(list (mapcar #'1+ '(1 2 3))
      (mapcar #'1+ [10 20 30])
      (mapcar #'char-to-string "abc")
      (mapcan #'list '(1 2 3) '(a b c))
      (mapconcat #'identity '("a" "b" "c") "-")
      (mapconcat #'char-to-string "hello" ""))
"##;
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments mapcan 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_hash_table_map_keys_values_sorted() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((h (make-hash-table :test 'equal)))
  (puthash 'one 1 h)
  (puthash 'two 2 h)
  (puthash 'three 3 h)
  (let ((keys (sort (hash-table-keys h)
                    (lambda (a b) (string< (symbol-name a) (symbol-name b)))))
        (vals (sort (hash-table-values h) #'<)))
    (list keys vals (hash-table-count h))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function hash-table-keys)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_seq_map_filter_over_mixed_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'seq)
(require 'cl-lib)
(list (seq-map #'1+ [1 2 3])
      (seq-map #'char-to-string "xyz")
      (seq-filter #'cl-evenp '(1 2 3 4 5 6))
      (seq-map-indexed #'cons '(a b c))
      (seq-reduce #'+ '(1 2 3 4) 0))
"##;
    let expect = expect_test::expect![[
        r#""OK ((2 3 4) (\"x\" \"y\" \"z\") (2 4 6) ((a . 0) (b . 1) (c . 2)) 10)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
