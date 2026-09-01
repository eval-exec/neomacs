//! Strict combo oracle probes, batch 155: cl-loop full menagerie. parallel
//! for-clauses, for = ... then, with-bindings, aggregate accumulators
//! (sum/count/maximize/append/nconc/concat), thereis/while/until, being the
//! elements of vector, and finally (return ...) collecting accumulated
//! variables.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_cl_loop_for_across_parallel_then() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (cl-loop for x in '(1 2 3 4 5) sum x)
      (cl-loop for x across [10 20 30 40] maximize x)
      (cl-loop for i below 3 for j from 10 collect (list i j))
      (cl-loop for x in '(1 2 3) for y = (* x 2) collect (cons x y))
      (cl-loop for c = "x" then (concat c "x") repeat 4 collect c)
      (cl-loop for x in '((1 . a) (2 . b) (3 . c)) collect (cdr x))
      (cl-loop for x being the elements of [5 6 7] collect (* x 10)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_loop_aggregate_into_finally() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (cl-loop for x in '(a b c d) counting (eq x 'b) into n finally (return n))
      (cl-loop for x in '(1 2 3 4) thereis (and (> x 2) x))
      (cl-loop for x in '(1 2 3) append (list x x))
      (cl-loop for x in '(1 2 3) nconc (list (list x)))
      (cl-loop with a = 10 with b = 20
               for x from 1 to 3 sum (+ a b x) into s
               finally (return s))
      (cl-loop for i from 1 to 10
               when (zerop (% i 2)) collect i into evens
               else collect i into odds
               end
               finally (return (list evens odds)))
      (cl-loop for c in '(?a ?b ?c) concat (char-to-string c))
      (cl-loop for x in '(5 3 8 1 9 2)
               minimize x into mn maximize x into mx
               finally (return (list mn mx))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_loop_while_until_conditionals_named() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (cl-loop named outer for i below 5 do (when (= i 3) (cl-return-from outer i)))
      (cl-loop for x in '(1 2 3 4 5 6) while (< x 4) collect x)
      (cl-loop for x in '(1 2 3 4 5 6) until (= x 4) collect x)
      (cl-loop for x from 1 for y = (* x x) while (< y 20) collect y)
      (cl-loop for x in '(1 2 3 4 5)
               if (cl-oddp x) collect x into odds
               else collect x into evens
               end
               finally (return (list odds evens)))
      (cl-loop for a in '(1 2 3) as b in '(10 20 30) sum (* a b)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
