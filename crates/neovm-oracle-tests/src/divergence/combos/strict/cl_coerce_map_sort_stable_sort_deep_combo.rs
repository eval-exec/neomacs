//! Strict combo oracle probes, batch 287: cl sequence deep. cl-coerce variants,
//! cl-map, cl-stable-sort / cl-sort stability, cl-merge, and cl-position-if /
//! cl-find-if.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_cl_map_stable_sort_merge_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(list (cl-map 'list (lambda (x) (* x x)) [1 2 3])
      (cl-map 'vector #'1+ '(1 2 3))
      (cl-map 'string #'char-to-string "abc")
      (cl-stable-sort (copy-sequence '((1 . a) (1 . b) (2 . c) (1 . d)))
                      (lambda (a b) (< (car a) (car b))))
      (cl-merge '(1 3 5) '(2 4 6) #'<)
      (cl-merge '(1 1) '(1 1) #'<)
      (cl-sort (copy-sequence [3 1 4 1 5 9 2 6]) #'<)
      (cl-coerce '(?a ?b ?c) 'string))
"##;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument characterp \"a\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_position_if_find_if_count_if() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(list (cl-position-if #'cl-evenp '(1 3 5 6 7))
      (cl-position-if-not #'cl-evenp '(2 4 6 7))
      (cl-find-if #'cl-oddp '(2 4 5 6))
      (cl-find-if-not #'cl-evenp '(2 4 5 6))
      (cl-count-if #'cl-oddp '(1 2 3 4 5))
      (cl-remove-if #'cl-oddp '(1 2 3 4 5))
      (cl-remove-duplicates '(1 2 1 3 2 4) :from-end t)
      (cl-substitute-if 0 #'cl-evenp '(1 2 3 4)))
"##;
    let expect = expect_test::expect![[r#""OK (3 3 5 5 3 (2 4) (1 2 3 4) (1 0 3 0))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_loop_for_in_across_hash_collect_sum() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(let ((h (make-hash-table :test 'equal)))
  (puthash 'a 1 h)
  (puthash 'b 2 h)
  (puthash 'c 3 h)
  (list (sort (cl-loop for k being the hash-keys of h collect k)
              (lambda (a b) (string< (symbol-name a) (symbol-name b))))
        (cl-loop for v being the hash-values of h sum v)
        (cl-loop for x in '(1 2 3 4) when (cl-evenp x) collect (* x x))
        (cl-loop for i from 1 to 5 collect (* i i))
        (cl-loop for c across "abc" collect c)
        (cl-loop for (k . v) in '((a . 1) (b . 2)) collect (list k v))))
"##;
    let expect =
        expect_test::expect![[r#""OK ((a b c) 6 (4 16) (1 4 9 16 25) (97 98 99) ((a 1) (b 2)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
