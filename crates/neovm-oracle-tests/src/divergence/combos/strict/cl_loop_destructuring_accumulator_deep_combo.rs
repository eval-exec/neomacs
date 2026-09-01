//! Strict combo oracle probes, batch 358: cl-loop with destructuring +
//! accumulators deep. for with destructuring, sum/count/collect/append with
//! multiple accumulators, and minimize/maximize into.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_cl_loop_destructuring_pairs_accumulate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(list (cl-loop for (k . v) in '((a . 1) (b . 2) (c . 3))
               sum v into total
               collect k into keys
               finally (return (list total keys)))
      (cl-loop for (a b) in '((1 10) (2 20) (3 30))
               sum (* a b))
      (cl-loop for x in '(3 1 4 1 5 9 2 6)
               maximize x into mx minimize x into mn
               count (cl-evenp x) into evens
               finally (return (list mx mn evens))))
"##;
    let expect = expect_test::expect![[r#""OK ((6 (a b c)) 140 (9 1 3))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_loop_append_concat_nconc_accumulate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(list (cl-loop for x in '((1 2) (3 4) (5 6)) append x)
      (cl-loop for s in '("ab" "cd" "ef") concat s)
      (cl-loop for x in '((a) (b) (c)) nconc x into flat
               finally (return flat))
      (cl-loop for i below 5 collect (* i i)))
"##;
    let expect = expect_test::expect![[r#""OK ((1 2 3 4 5 6) \"abcdef\" (a b c) (0 1 4 9 16))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_loop_while_until_for_parallel_conditional() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(list (cl-loop for x in '(1 2 3 4 5 6) while (< x 4) collect x)
      (cl-loop for x in '(1 2 3 4 5) until (= x 4) collect x)
      (cl-loop for a in '(1 2 3) and b in '(10 20 30) collect (+ a b))
      (cl-loop for i from 1 to 10
               if (cl-evenp i) collect i into evens
               else collect i into odds
               end
               finally (return (list evens odds))))
"##;
    let expect =
        expect_test::expect![[r#""OK ((1 2 3) (1 2 3) (11 22 33) ((2 4 6 8 10) (1 3 5 7 9)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
