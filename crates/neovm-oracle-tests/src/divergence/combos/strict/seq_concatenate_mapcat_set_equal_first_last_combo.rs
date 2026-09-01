//! Strict combo oracle probes, batch 234: seq.el concatenation + set ops.
//! seq-concatenate (list/vector/string), seq-mapcat, seq-partition,
//! seq-contains, seq-set-equal-p, seq-first/last, and seq-into-sequence.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_seq_concatenate_mapcat_multiple_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'seq)
(list (seq-concatenate 'list '(1 2) '(3 4) '(5))
      (seq-concatenate 'vector [1 2] [3 4])
      (seq-concatenate 'string "ab" "cd")
      (seq-mapcat #'list '(1 2 3))
      (seq-mapcat (lambda (x) (list x x)) '(1 2))
      (seq-concatenate 'list nil '(1 2))
      (seq-concatenate 'string "" "x"))
"##;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5) [1 2 3 4] \"abcd\" (1 2 3) (1 1 2 2) (1 2) \"x\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_seq_contains_set_equal_first_last() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'seq)
(list (seq-contains '(1 2 3) 2)
      (seq-contains '(1 2 3) 9)
      (seq-contains "hello" ?l)
      (seq-contains [a b c] 'b)
      (seq-set-equal-p '(1 2 3) '(3 2 1))
      (seq-set-equal-p '(1 2 3) '(1 2 4))
      (seq-first '(1 2 3))
      (seq-first [10 20 30])
      (seq-last '(1 2 3))
      (seq-last "abc")))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function seq-last)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_seq_into_find_position_copy_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'seq)
(list (seq-into '(1 2 3) 'vector)
      (seq-into [a b c] 'list)
      (seq-into "abc" 'list)
      (seq-find (lambda (x) (> x 2)) '(1 2 3 4))
      (seq-find (lambda (x) (> x 9)) '(1 2 3))
      (seq-position '(a b c d) 'c)
      (seq-count (lambda (x) (cl-evenp x)) '(1 2 3 4 5 6))
      (seq-copy '(1 2 3))
      (eq (seq-copy '(1 2 3)) '(1 2 3))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function cl-evenp)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
