//! Strict combo oracle probes, batch 200: mapping over mixed sequence types.
//! mapcar/mapc over strings, vectors, lists; mapconcat over strings and lists;
//! mapcan/mapcar with multiple sequences; and seq-map over vector/string.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_mapcar_mapc_over_string_vector_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (mapcar #'identity "abc")
      (mapcar #'1+ [1 2 3])
      (mapcar #'symbol-name '(a b c))
      (mapcar #'char-to-string "abc")
      (mapcar #'list '(1 2 3) '(a b c))
      (let ((acc nil))
        (mapc (lambda (x) (push x acc)) '(1 2 3))
        (nreverse acc))
      (mapcar (lambda (x) (* x x)) [1 2 3 4]))
"##;
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments mapcar 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_mapconcat_mapcan_multiple_sequences() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (mapconcat #'identity '("a" "b" "c") "-")
      (mapconcat #'char-to-string "abcdef" "-")
      (mapconcat #'number-to-string '(1 2 3 4) ", ")
      (mapcan #'list '(1 2 3) '(a b c))
      (mapcan (lambda (x) (list x x)) '(1 2 3))
      (mapcar #'cons '(a b c) '(1 2 3))
      (mapconcat #'identity '("only") "-")
      (mapconcat #'identity '() "-"))
"##;
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments mapcan 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_seq_map_map_index_filter_string_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'seq)
(list (seq-map #'1+ [1 2 3])
      (seq-map #'identity "abc")
      (seq-map-indexed #'cons '(a b c d))
      (seq-filter #'cl-oddp '(1 2 3 4 5 6))
      (seq-remove #'cl-oddp '(1 2 3 4 5 6))
      (seq-map #'char-to-string "XYZ")
      (seq-elt [10 20 30] 1)
      (seq-into '(1 2 3) 'vector)
      (seq-into [a b c] 'list)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function cl-oddp)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
