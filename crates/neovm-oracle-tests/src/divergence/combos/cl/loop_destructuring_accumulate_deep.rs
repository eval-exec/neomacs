//! Deep combo: cl-loop + destructuring + multiple accumulators + into + hash.
//! Tests complex iteration patterns with collection building.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_cl_loop_for_in_with_destructuring() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (cl-loop for (a b c) in '((1 2 3) (4 5 6) (7 8 9))\n\
         collect (+ a (* b c))))",
        expect,
    );
}

#[test]
fn deficiency_cl_loop_for_on_with_destructuring_accumulate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (cl-loop for (first . rest) on '(a b c d e f)\n\
         while rest\n\
         collect (list first (length rest))))",
        expect,
    );
}

#[test]
fn deficiency_cl_loop_with_hash_table_iteration_and_sum() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((ht (make-hash-table :test 'eql)))\n\
         (dotimes (i 10) (puthash i (* i i) ht))\n\
         (list (cl-loop for v being the hash-values of ht sum v)\n\
         (cl-loop for k being the hash-keys of ht\n\
         when (cl-oddp k)\n\
         collect k)\n\
         (cl-loop for k being the hash-keys of ht\n\
         for v being the hash-values of ht\n\
         maximize v))))",
        expect,
    );
}

#[test]
fn deficiency_cl_loop_with_multiple_accumulate_clauses() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (cl-loop for i from 1 to 20\n\
         count (cl-evenp i) into evens\n\
         count (cl-oddp i) into odds\n\
         sum i into total\n\
         maximize i into max-val\n\
         minimize i into min-val\n\
         finally (return (list evens odds total max-val min-val))))",
        expect,
    );
}

#[test]
fn deficiency_cl_loop_for_vector_with_index() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((v [10 20 30 40 50]))\n\
         (cl-loop for val across v\n\
         for i from 0\n\
         collect (list i val (* val 2)))))",
        expect,
    );
}

#[test]
fn deficiency_cl_loop_with_initially_and_finally() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((log nil))\n\
         (cl-loop initially (push 'start log)\n\
         for i from 1 to 5\n\
         do (push i log)\n\
         finally (push 'end log)\n\
         (return (nreverse log)))))",
        expect,
    );
}

#[test]
fn deficiency_cl_loop_with_while_and_until_combined() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((lst '(1 2 3 4 5 6 7 8 9 10 11 12)))\n\
         (cl-loop for x in lst\n\
         while (< x 8)\n\
         until (> x 6)\n\
         collect (* x x))))",
        expect,
    );
}

#[test]
fn deficiency_cl_loop_with_reducing_multiply() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (cl-loop for i from 1 to 8\n\
         reduce (* acc val) :initial-value 1\n\
         :into result\n\
         finally (return result)))",
        expect,
    );
}

#[test]
fn deficiency_cl_loop_nested_for_with_vector_and_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((matrix [[1 2 3] [4 5 6] [7 8 9]]))\n\
         (cl-loop for row across matrix\n\
         collect (cl-loop for val across row\n\
         when (cl-oddp val)\n\
         collect val))))",
        expect,
    );
}

#[test]
fn deficiency_cl_loop_for_string_with_collect_and_append() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (cl-loop for c across \"Hello World\"\n\
         for i from 0\n\
         if (memq c '(?H ?W))\n\
         collect (list i c) into specials\n\
         else\n\
         count t into normal-count\n\
         end\n\
         finally (return (list specials normal-count))))",
        expect,
    );
}
