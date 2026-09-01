//! Oracle parity tests for `number-sequence` — generating numeric ranges
//! with optional step, including float sequences and edge cases.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Basic integer range
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_sequence_integer_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(number-sequence 1 10)"#;
    let expect = expect_test::expect![[r#""OK (1 2 3 4 5 6 7 8 9 10)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_number_sequence_negative_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(number-sequence -5 5)"#;
    let expect = expect_test::expect![[r#""OK (-5 -4 -3 -2 -1 0 1 2 3 4 5)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Custom step
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_sequence_custom_step() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0 5 10 15 20)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence 0 20 5)", expect);
    let expect = expect_test::expect![[r#""OK (1 4 7 10 13)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence 1 15 3)", expect);
    let expect = expect_test::expect![[r#""OK (2)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence 2 2 1)", expect);
}

// ---------------------------------------------------------------------------
// Negative step for descending
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_sequence_descending() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (10 9 8 7 6 5 4 3 2 1)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence 10 1 -1)", expect);
    let expect = expect_test::expect![[r#""OK (100 75 50 25 0)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence 100 0 -25)", expect);
    let expect = expect_test::expect![[r#""OK (5 2 -1 -4 -7 -10)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence 5 -10 -3)", expect);
}

// ---------------------------------------------------------------------------
// Float sequences
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_sequence_float() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0.0 0.25 0.5 0.75 1.0)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence 0.0 1.0 0.25)", expect);
    let expect = expect_test::expect![[r#""OK (1.0 1.5 2.0 2.5 3.0)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence 1.0 3.0 0.5)", expect);
    let expect = expect_test::expect![[r#""OK 11""#]];
    // Length check to sidestep float precision in element comparison
    crate::common::assert_oracle_parity_expect("(length (number-sequence 0.0 1.0 0.1))", expect);
}

// ---------------------------------------------------------------------------
// Edge: FROM > TO with positive step => nil
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_sequence_from_gt_to_positive_step() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence 10 1)", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence 10 1 2)", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence 5 3 1)", expect);
}

// ---------------------------------------------------------------------------
// Edge: FROM = TO => single element
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_sequence_single_element() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (7)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence 7 7)", expect);
    let expect = expect_test::expect![[r#""OK (0)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence 0 0)", expect);
    let expect = expect_test::expect![[r#""OK (-3)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence -3 -3)", expect);
    let expect = expect_test::expect![[r#""OK (42)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence 42 42 100)", expect);
}

// ---------------------------------------------------------------------------
// Complex: generate indices, then map over a vector
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_sequence_index_into_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use number-sequence to produce even indices, then extract those
    // elements from a vector to build a filtered list
    let form = r#"(let ((data [10 20 30 40 50 60 70 80 90 100])
                        (indices (number-sequence 0 9 2)))
                    (mapcar (lambda (i) (aref data i)) indices))"#;
    let expect = expect_test::expect![[r#""OK (10 30 50 70 90)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: use number-sequence to build multiplication table
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_sequence_multiplication_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a 5x5 multiplication table as a list of lists
    let form = r#"(let ((rows (number-sequence 1 5)))
                    (mapcar
                     (lambda (r)
                       (mapcar
                        (lambda (c) (* r c))
                        (number-sequence 1 5)))
                     rows))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5) (2 4 6 8 10) (3 6 9 12 15) (4 8 12 16 20) (5 10 15 20 25))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: triangular numbers via nested number-sequence
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_sequence_triangular_numbers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // T(n) = sum(1..n). Compute first 10 triangular numbers using
    // number-sequence + apply #'+
    let form = r#"(mapcar
                    (lambda (n)
                      (apply #'+ (number-sequence 1 n)))
                    (number-sequence 1 10))"#;
    let expect = expect_test::expect![[r#""OK (1 3 6 10 15 21 28 36 45 55)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: sieve of Eratosthenes using number-sequence
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_sequence_sieve_primes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Generate primes up to 50 using a simple sieve built on number-sequence
    let form = r#"(let ((limit 50)
                        (sieve (make-vector 51 t)))
                    ;; 0 and 1 are not prime
                    (aset sieve 0 nil)
                    (aset sieve 1 nil)
                    ;; Sieve: for each prime p, mark p*2, p*3, ... as composite
                    (dolist (p (number-sequence 2 (floor (sqrt 50))))
                      (when (aref sieve p)
                        (let ((multiples (number-sequence (* p 2) limit p)))
                          (dolist (m multiples)
                            (aset sieve m nil)))))
                    ;; Collect primes
                    (let ((primes nil))
                      (dolist (n (number-sequence 2 limit))
                        (when (aref sieve n)
                          (setq primes (cons n primes))))
                      (nreverse primes)))"#;
    let expect = expect_test::expect![[r#""OK (2 3 5 7 11 13 17 19 23 29 31 37 41 43 47)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
