//! Strict combo oracle probes, batch 382: cl-loop hash-table iteration deep +
//! for-as variants. for being the hash-keys/-values with :using, for as =,
//! and loop with multiple accumulators over hash tables.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_cl_loop_hash_keys_values_using() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(let ((h (make-hash-table :test 'equal)))
  (puthash 'apple 1 h)
  (puthash 'banana 2 h)
  (puthash 'cherry 3 h)
  (list (sort (cl-loop for k being the hash-keys of h collect k)
              (lambda (a b) (string< (symbol-name a) (symbol-name b))))
        (sort (cl-loop for v being the hash-values of h collect v) #'<)
        (sort (cl-loop for k being the hash-keys of h using (hash-values v)
                       collect (cons k v))
              (lambda (a b) (string< (symbol-name (car a)) (symbol-name (car b)))))))
"##;
    let expect = expect_test::expect![[
        r#""OK ((apple banana cherry) (1 2 3) ((apple . 1) (banana . 2) (cherry . 3)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_loop_for_as_then_repeat_sum() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(list (cl-loop for x = 1 then (* x 2) repeat 5 collect x)
      (cl-loop for x = 0 then (+ x 1) while (< x 5) sum x)
      (cl-loop for a in '(1 2 3) for b = (* a 10) for c = (* b 10) collect (list a b c))
      (cl-loop for x in '(1 2 3 4 5) sum (* x x) into squares
               sum x into total
               finally (return (list squares total))))
"##;
    let expect = expect_test::expect![[
        r#""OK ((1 2 4 8 16) 10 ((1 10 100) (2 20 200) (3 30 300)) (55 15))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_loop_nested_dolist_accumulate_conditional() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(list (cl-loop for x in '((1 2) (3 4) (5 6))
               append (cl-loop for y in x collect (* y y)))
      (cl-loop for i below 3
               nconc (list i (* i 10) (* i 100)))
      (cl-loop for x in '(1 2 3 4 5 6)
               when (cl-evenp x)
                 collect x into evens
                 and count t into even-count
               end
               finally (return (list evens even-count))))
"##;
    let expect =
        expect_test::expect![[r#""OK ((1 4 9 16 25 36) (0 0 0 1 10 100 2 20 200) ((2 4 6) 3))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
