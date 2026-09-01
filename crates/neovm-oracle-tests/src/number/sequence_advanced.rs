//! Advanced oracle parity tests for `number-sequence` with ALL parameter
//! combinations: FROM only, FROM+TO, FROM+TO+INCR (positive, negative, float),
//! edge cases (FROM=TO, zero step, non-divisible ranges), and complex
//! compositions with mapcar and filtering.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// FROM only (single-argument): should produce list of one element
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_sequence_from_only() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (5)""#]];
    // When TO is nil, number-sequence returns a list containing just FROM
    crate::common::assert_oracle_parity_expect("(number-sequence 5)", expect);
    let expect = expect_test::expect![[r#""OK (0)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence 0)", expect);
    let expect = expect_test::expect![[r#""OK (-7)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence -7)", expect);
    let expect = expect_test::expect![[r#""OK (999)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence 999)", expect);
    let expect = expect_test::expect![[r#""OK (3.14)""#]];
    // Float FROM, no TO
    crate::common::assert_oracle_parity_expect("(number-sequence 3.14)", expect);
    let expect = expect_test::expect![[r#""OK (-2.5)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence -2.5)", expect);
}

// ---------------------------------------------------------------------------
// FROM and TO ascending range (default INCR = 1)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_sequence_from_to_ascending() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2 3 4 5)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence 1 5)", expect);
    let expect = expect_test::expect![[r#""OK (0 1 2 3 4 5 6 7 8 9 10)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence 0 10)", expect);
    let expect = expect_test::expect![[r#""OK (-3 -2 -1 0 1 2 3)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence -3 3)", expect);
    let expect = expect_test::expect![[r#""OK (-10 -9 -8 -7 -6 -5)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence -10 -5)", expect);
    let expect = expect_test::expect![[r#""OK 200""#]];
    // Large range
    crate::common::assert_oracle_parity_expect("(length (number-sequence 1 200))", expect);
    let expect = expect_test::expect![[r#""OK (50 75 26)""#]];
    // Verify first and last elements of a range
    crate::common::assert_oracle_parity_expect(
        "(let ((s (number-sequence 50 75)))
           (list (car s) (car (last s)) (length s)))",
        expect,
    );
}

// ---------------------------------------------------------------------------
// FROM, TO, and positive INCR
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_sequence_positive_incr() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0 5 10 15 20)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence 0 20 5)", expect);
    let expect = expect_test::expect![[r#""OK (1 4 7 10 13)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence 1 15 3)", expect);
    let expect = expect_test::expect![[r#""OK (10 20 30 40 50 60 70 80 90 100)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence 10 100 10)", expect);
    let expect = expect_test::expect![[r#""OK (-20 -13 -6 1 8 15)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence -20 20 7)", expect);
    let expect = expect_test::expect![[r#""OK (1)""#]];
    // INCR larger than range: only FROM included
    crate::common::assert_oracle_parity_expect("(number-sequence 1 5 100)", expect);
    let expect = expect_test::expect![[r#""OK (3 4 5 6 7 8)""#]];
    // INCR = 1 (same as default)
    crate::common::assert_oracle_parity_expect("(number-sequence 3 8 1)", expect);
    let expect = expect_test::expect![[r#""OK (0 2 4 6 8 10 12 14 16 18 20)""#]];
    // INCR = 2 (evens)
    crate::common::assert_oracle_parity_expect("(number-sequence 0 20 2)", expect);
}

// ---------------------------------------------------------------------------
// FROM > TO with negative INCR (descending)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_sequence_descending_negative_incr() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (10 9 8 7 6 5 4 3 2 1)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence 10 1 -1)", expect);
    let expect = expect_test::expect![[r#""OK (100 90 80 70 60 50 40 30 20 10 0)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence 100 0 -10)", expect);
    let expect = expect_test::expect![[r#""OK (50 25 0 -25 -50)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence 50 -50 -25)", expect);
    let expect = expect_test::expect![[r#""OK (5 2 -1 -4)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence 5 -5 -3)", expect);
    let expect = expect_test::expect![[r#""OK (0 -4 -8 -12 -16 -20)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence 0 -20 -4)", expect);
    let expect = expect_test::expect![[r#""OK (20 5 6)""#]];
    // Verify elements
    crate::common::assert_oracle_parity_expect(
        "(let ((s (number-sequence 20 5 -3)))
           (list (car s) (car (last s)) (length s)))",
        expect,
    );
}

// ---------------------------------------------------------------------------
// Float arguments: FROM, TO, INCR as floats
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_sequence_float_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0.0 0.25 0.5 0.75 1.0)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence 0.0 1.0 0.25)", expect);
    let expect = expect_test::expect![[r#""OK (1.0 1.5 2.0 2.5 3.0)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence 1.0 3.0 0.5)", expect);
    let expect = expect_test::expect![[r#""OK (-1.0 -0.5 0.0 0.5 1.0)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence -1.0 1.0 0.5)", expect);
    let expect = expect_test::expect![[r#""OK (0 0.2 0.4 0.6000000000000001 0.8 1.0)""#]];
    // Mixed int and float
    crate::common::assert_oracle_parity_expect("(number-sequence 0 1.0 0.2)", expect);
    let expect = expect_test::expect![[r#""OK (0.0 1.0 2.0 3.0 4.0 5.0)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence 0.0 5 1)", expect);
    let expect = expect_test::expect![[r#""OK (2.0 1.5 1.0 0.5 0.0)""#]];
    // Descending floats
    crate::common::assert_oracle_parity_expect("(number-sequence 2.0 0.0 -0.5)", expect);
    let expect = expect_test::expect![[r#""OK (1.0 0.75 0.5 0.25 0.0 -0.25 -0.5 -0.75 -1.0)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence 1.0 -1.0 -0.25)", expect);
    let expect = expect_test::expect![[r#""OK 101""#]];
    // Verify length to sidestep float precision
    crate::common::assert_oracle_parity_expect("(length (number-sequence 0.0 10.0 0.1))", expect);
    let expect = expect_test::expect![[r#""OK 4""#]];
    crate::common::assert_oracle_parity_expect("(length (number-sequence 0.0 1.0 0.3))", expect);
}

// ---------------------------------------------------------------------------
// INCR that doesn't evenly divide range
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_sequence_non_divisible_incr() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 4 7 10)""#]];
    // 1 to 10 by 3: 1, 4, 7, 10 (10 is included since 7+3=10)
    crate::common::assert_oracle_parity_expect("(number-sequence 1 10 3)", expect);
    let expect = expect_test::expect![[r#""OK (1 4 7 10)""#]];
    // 1 to 11 by 3: 1, 4, 7, 10 (10+3=13 > 11, so stops at 10)
    crate::common::assert_oracle_parity_expect("(number-sequence 1 11 3)", expect);
    let expect = expect_test::expect![[r#""OK (0 3 6)""#]];
    // 0 to 7 by 3: 0, 3, 6
    crate::common::assert_oracle_parity_expect("(number-sequence 0 7 3)", expect);
    let expect = expect_test::expect![[r#""OK (0 33 66 99)""#]];
    // Large step relative to range
    crate::common::assert_oracle_parity_expect("(number-sequence 0 100 33)", expect);
    let expect = expect_test::expect![[r#""OK (10 7 4 1)""#]];
    // Descending non-divisible
    crate::common::assert_oracle_parity_expect("(number-sequence 10 1 -3)", expect);
    let expect = expect_test::expect![[r#""OK (100 67 34 1)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence 100 0 -33)", expect);
    let expect = expect_test::expect![[r#""OK 4""#]];
    // Float non-divisible
    crate::common::assert_oracle_parity_expect("(length (number-sequence 0.0 1.0 0.3))", expect);
    let expect = expect_test::expect![[r#""OK 2""#]];
    crate::common::assert_oracle_parity_expect("(length (number-sequence 0.0 1.0 0.7))", expect);
}

// ---------------------------------------------------------------------------
// Edge cases: FROM=TO, INCR=0 (should signal error), negative numbers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_sequence_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (7)""#]];
    // FROM = TO: single element regardless of INCR
    crate::common::assert_oracle_parity_expect("(number-sequence 7 7)", expect);
    let expect = expect_test::expect![[r#""OK (0)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence 0 0)", expect);
    let expect = expect_test::expect![[r#""OK (-3)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence -3 -3)", expect);
    let expect = expect_test::expect![[r#""OK (42)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence 42 42 5)", expect);
    let expect = expect_test::expect![[r#""OK (42)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence 42 42 -5)", expect);
    let expect = expect_test::expect![[r#""OK (42)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence 42 42 0)", expect);

    let expect = expect_test::expect![[r#""OK (error error)""#]];
    // INCR = 0 with FROM != TO should signal an error
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        "(condition-case err
           (number-sequence 1 10 0)
           (error (list 'error (car err))))",
        expect,
    );
    assert_eq!(neovm, oracle);

    let expect = expect_test::expect![[r#""OK nil""#]];
    // FROM > TO with positive step: nil (empty)
    crate::common::assert_oracle_parity_expect("(number-sequence 10 1 2)", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence 5 3 1)", expect);

    let expect = expect_test::expect![[r#""OK nil""#]];
    // FROM < TO with negative step: nil (empty)
    crate::common::assert_oracle_parity_expect("(number-sequence 1 10 -1)", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence -5 5 -2)", expect);

    let expect = expect_test::expect![[r#""OK (-10 -9 -8 -7 -6 -5 -4 -3 -2 -1)""#]];
    // All negative numbers
    crate::common::assert_oracle_parity_expect("(number-sequence -10 -1)", expect);
    let expect = expect_test::expect![[r#""OK (-1 -2 -3 -4 -5 -6 -7 -8 -9 -10)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence -1 -10 -1)", expect);
    let expect = expect_test::expect![[r#""OK (-100 -93 -86 -79 -72 -65 -58 -51)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence -100 -50 7)", expect);
}

#[test]
fn oracle_number_sequence_zero_increment_error_payload() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU lisp/subr.el:number-sequence returns (FROM) before consulting INC
    // when TO is nil or numerically equal to FROM.  Only a non-equal endpoint
    // with zero INC signals this exact `error' payload.
    let form = r#"
(list
 (number-sequence 7 7 0)
 (number-sequence 7 nil 0)
 (condition-case err
     (number-sequence 1 10 0)
   (error (list (car err) (cdr err))))
 (condition-case err
     (number-sequence 10 1 0)
   (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((7) (7) (error (\"The increment can not be zero\")) (error (\"The increment can not be zero\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_number_sequence_fixnum_boundary_lengths() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (length (number-sequence (1- most-positive-fixnum) most-positive-fixnum))
 (length (number-sequence (1+ most-negative-fixnum) most-negative-fixnum -1))
 (number-sequence most-positive-fixnum most-positive-fixnum 0)
 (number-sequence most-negative-fixnum most-negative-fixnum 0))
"#;

    let expect = expect_test::expect![[r#""OK (2 2 (0) (0))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: arithmetic progressions with filtering
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_sequence_arithmetic_progressions_filtered() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Generate arithmetic progressions, then filter for various properties
    let form = r#"(let* ((seq1 (number-sequence 1 100 3))    ;; 1, 4, 7, ..., 100
                         (seq2 (number-sequence 2 100 5))    ;; 2, 7, 12, ..., 97
                         ;; Filter seq1 for elements also in seq2 (intersection)
                         (common nil))
                    (dolist (x seq1)
                      (when (memq x seq2)
                        (setq common (cons x common))))
                    (let* ((common-sorted (sort (nreverse common) #'<))
                           ;; Sum of elements in each progression
                           (sum1 (apply #'+ seq1))
                           (sum2 (apply #'+ seq2))
                           ;; Filter for primes in short range
                           (small (number-sequence 2 30))
                           (primes (let ((result nil))
                                     (dolist (n small)
                                       (let ((is-prime t) (d 2))
                                         (while (and is-prime (<= (* d d) n))
                                           (when (= 0 (% n d))
                                             (setq is-prime nil))
                                           (setq d (1+ d)))
                                         (when is-prime
                                           (setq result (cons n result)))))
                                     (nreverse result))))
                      (list
                        (length seq1)
                        (length seq2)
                        sum1
                        sum2
                        common-sorted
                        primes
                        ;; Squares of first 10 naturals
                        (mapcar (lambda (n) (* n n)) (number-sequence 1 10))
                        ;; Cubes of first 5 naturals
                        (mapcar (lambda (n) (* n n n)) (number-sequence 1 5)))))"#;
    let expect = expect_test::expect![[
        r#""OK (34 20 1717 990 (7 22 37 52 67 82 97) (2 3 5 7 11 13 17 19 23 29) (1 4 9 16 25 36 49 64 81 100) (1 8 27 64 125))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: combining number-sequence with mapcar for computed sequences
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_sequence_mapcar_compositions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let* (;; Fibonacci-like: use number-sequence as index, accumulate
                         (fib-indices (number-sequence 0 15))
                         (fibs (let ((memo (make-hash-table)))
                                 (puthash 0 0 memo)
                                 (puthash 1 1 memo)
                                 (mapcar
                                   (lambda (n)
                                     (or (gethash n memo)
                                         (let ((val (+ (gethash (- n 1) memo)
                                                       (gethash (- n 2) memo))))
                                           (puthash n val memo)
                                           val)))
                                   fib-indices)))
                         ;; Factorial via accumulation
                         (fact-seq (let ((acc 1)
                                         (result nil))
                                    (dolist (n (number-sequence 1 10))
                                      (setq acc (* acc n))
                                      (setq result (cons acc result)))
                                    (nreverse result)))
                         ;; Pascal's triangle row: C(n,k) for n=8
                         (pascal-row
                           (let ((n 8))
                             (mapcar
                               (lambda (k)
                                 (let ((num 1) (den 1) (i 0))
                                   (while (< i k)
                                     (setq num (* num (- n i)))
                                     (setq den (* den (1+ i)))
                                     (setq i (1+ i)))
                                   (/ num den)))
                               (number-sequence 0 n))))
                         ;; Partial sums: cumulative sum of 1..10
                         (partial-sums
                           (let ((sum 0) (result nil))
                             (dolist (n (number-sequence 1 10))
                               (setq sum (+ sum n))
                               (setq result (cons sum result)))
                             (nreverse result))))
                    (list
                      fibs
                      fact-seq
                      pascal-row
                      partial-sums
                      ;; Zip two sequences together
                      (let ((letters '("a" "b" "c" "d" "e"))
                            (nums (number-sequence 1 5)))
                        (mapcar (lambda (i)
                                  (cons (nth (1- i) letters) i))
                                nums))))"#;
    let expect = expect_test::expect![[
        r#""OK ((0 1 1 2 3 5 8 13 21 34 55 89 144 233 377 610) (1 2 6 24 120 720 5040 40320 362880 3628800) (1 8 28 56 70 56 28 8 1) (1 3 6 10 15 21 28 36 45 55) ((\"a\" . 1) (\"b\" . 2) (\"c\" . 3) (\"d\" . 4) (\"e\" . 5)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: number-sequence for matrix generation and manipulation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_sequence_matrix_generation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let* (;; Generate a 4x4 identity-like matrix using number-sequence
                         (rows (number-sequence 0 3))
                         (cols (number-sequence 0 3))
                         (identity-matrix
                           (mapcar (lambda (r)
                                     (mapcar (lambda (c)
                                               (if (= r c) 1 0))
                                             cols))
                                   rows))
                         ;; Generate a multiplication table 1..6 x 1..6
                         (mul-table
                           (mapcar (lambda (r)
                                     (mapcar (lambda (c) (* r c))
                                             (number-sequence 1 6)))
                                   (number-sequence 1 6)))
                         ;; Diagonal of multiplication table
                         (diagonal
                           (mapcar (lambda (i)
                                     (nth i (nth i mul-table)))
                                   (number-sequence 0 5)))
                         ;; Sum of each row
                         (row-sums
                           (mapcar (lambda (row) (apply #'+ row)) mul-table))
                         ;; Sum of each column via transpose
                         (col-sums
                           (mapcar (lambda (j)
                                     (apply #'+ (mapcar (lambda (row) (nth j row))
                                                        mul-table)))
                                   (number-sequence 0 5))))
                    (list identity-matrix
                          mul-table
                          diagonal
                          row-sums
                          col-sums))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 0 0 0) (0 1 0 0) (0 0 1 0) (0 0 0 1)) ((1 2 3 4 5 6) (2 4 6 8 10 12) (3 6 9 12 15 18) (4 8 12 16 20 24) (5 10 15 20 25 30) (6 12 18 24 30 36)) (1 4 9 16 25 36) (21 42 63 84 105 126) (21 42 63 84 105 126))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: number-sequence with reduce/fold patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_sequence_reduce_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--nsadv-fold-left
    (lambda (fn init seq)
      "Left fold: (fn (fn (fn init e1) e2) e3) ..."
      (let ((acc init))
        (dolist (x seq)
          (setq acc (funcall fn acc x)))
        acc)))

  (unwind-protect
      (let* (;; Sum via fold
             (sum (funcall 'neovm--nsadv-fold-left #'+ 0 (number-sequence 1 100)))
             ;; Product of 1..10 via fold
             (product (funcall 'neovm--nsadv-fold-left #'* 1 (number-sequence 1 10)))
             ;; Maximum via fold
             (maxval (funcall 'neovm--nsadv-fold-left #'max -999 (number-sequence -50 50 7)))
             ;; Build a reversed list via fold
             (reversed (funcall 'neovm--nsadv-fold-left
                                (lambda (acc x) (cons x acc))
                                nil
                                (number-sequence 1 8)))
             ;; Running maximum
             (running-max
               (let ((mx nil) (result nil))
                 (dolist (n (number-sequence 5 1 -1))
                   (setq mx (if mx (max mx n) n))
                   (setq result (cons mx result)))
                 (nreverse result)))
             ;; Alternating sum: 1 - 2 + 3 - 4 + ... + 9 - 10
             (alt-sum
               (let ((sum 0) (sign 1))
                 (dolist (n (number-sequence 1 10))
                   (setq sum (+ sum (* sign n)))
                   (setq sign (* sign -1)))
                 sum)))
        (list sum product maxval reversed running-max alt-sum))
    (fmakunbound 'neovm--nsadv-fold-left)))"#;
    let expect =
        expect_test::expect![[r#""OK (5050 3628800 48 (8 7 6 5 4 3 2 1) (5 5 5 5 5) -5)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
