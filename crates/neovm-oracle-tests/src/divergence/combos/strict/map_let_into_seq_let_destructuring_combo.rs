//! Strict combo oracle probes, batch 288: map.el + seq.el deep. map-let
//! destructuring, map-into, seq-let, and seq-min-by/max-by with key.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_map_let_destructuring_into() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'map)
(let ((m (list (cons 'a 1) (cons 'b 2) (cons 'c 3))))
  (list (map-let ((:a a) (:b b)) (list (cons :a 10) (cons :b 20)) (list a b))
        (let ((v (vector 1 2 3)))
          (map-into v (lambda (x) (* x 10)))
          v)
        (map-let ((?a ?b)) (string ?a ?b) (list))
        (map-length m)
        (map-copy m)))
"##;
    let expect = expect_test::expect![[
        r#""ERR (cl-no-applicable-method map-into [1 2 3] (closure (t) (x) (* x 10)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_seq_let_destructuring_min_max_by() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'seq)
(require 'cl-lib)
(list (seq-let [a b c] [1 2 3] (list a b c))
      (seq-let (a b &rest rest) '(1 2 3 4 5) (list a b rest))
      (seq-min-by '((3 . c) (1 . a) (2 . b)) #'< :key #'car)
      (seq-max-by '((3 . c) (1 . a) (2 . b)) #'> :key #'car)
      (seq-sort-by '("bbb" "a" "cc") #'length #'<)
      (seq-uniq '("a" "b" "a" "c" "b"))
      (seq-reverse '(1 2 3))
      (seq-group-by #'cl-evenp '(1 2 3 4 5 6)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function seq-min-by)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_seq_zip_difference_intersection_sorted() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'seq)
(list (seq-mapn #'list '(1 2 3) '(a b c))
      (seq-mapn #'cons [1 2] [3 4])
      (seq-union '(1 2 3) '(3 4 5))
      (seq-intersection '(1 2 3 4) '(3 4 5 6))
      (seq-difference '(1 2 3 4) '(3 4))
      (seq-sort '(3 1 2) #'<)
      (seq-sort '(3 1 2) #'>)
      (seq-contains-pos '(a b c d) 'c))
"##;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument sequencep <)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
