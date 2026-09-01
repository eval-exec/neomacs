//! Advanced oracle parity tests for cl-lib-like patterns available in
//! vanilla Elisp (no `(require 'cl-lib)` needed).
//!
//! Covers: dotimes with result forms, dolist with accumulation,
//! nested iteration, manual cl-loop/reduce/mapcan/remove-if emulations.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// dotimes with complex result forms
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_adv_dotimes_result_accumulates_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a vector of factorials via dotimes result form that
    // returns the accumulated vector.
    let form = r#"(let ((v (make-vector 8 0))
                        (fact 1))
                    (dotimes (i 8 v)
                      (setq fact (if (= i 0) 1 (* fact i)))
                      (aset v i fact)))"#;
    let expect = expect_test::expect![r#""OK [1 1 2 6 24 120 720 5040]""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// dolist with result form returning accumulated value
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_adv_dolist_result_partition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Partition a list into (evens . odds) via dolist result form.
    let form = r#"(let ((evens nil) (odds nil))
                    (dolist (x '(1 2 3 4 5 6 7 8 9 10)
                              (cons (nreverse evens)
                                    (nreverse odds)))
                      (if (= (% x 2) 0)
                          (setq evens (cons x evens))
                        (setq odds (cons x odds)))))"#;
    let expect = expect_test::expect![r#""OK ((2 4 6 8 10) 1 3 5 7 9)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested dolist/dotimes combinations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_adv_nested_iteration_matrix_multiply() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // 2x3 times 3x2 matrix multiplication using nested dotimes.
    // A = [[1,2,3],[4,5,6]], B = [[7,8],[9,10],[11,12]]
    // Result should be [[58,64],[139,154]]
    let form = r#"(let* ((a-rows 2) (a-cols 3) (b-cols 2)
                         (a (vector (vector 1 2 3) (vector 4 5 6)))
                         (b (vector (vector 7 8) (vector 9 10) (vector 11 12)))
                         (result (make-vector a-rows nil)))
                    (dotimes (i a-rows)
                      (aset result i (make-vector b-cols 0))
                      (dotimes (j b-cols)
                        (let ((sum 0))
                          (dotimes (k a-cols)
                            (setq sum (+ sum (* (aref (aref a i) k)
                                                (aref (aref b k) j)))))
                          (aset (aref result i) j sum))))
                    (list (append (aref result 0) nil)
                          (append (aref result 1) nil)))"#;
    let expect = expect_test::expect![[r#""OK ((58 64) (139 154))""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("((58 64) (139 154))", &o, &n);
}

#[test]
fn oracle_prop_cl_adv_nested_dolist_cartesian_filter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Cartesian product of two lists filtered to pairs summing > 7.
    let form = r#"(let ((result nil))
                    (dolist (a '(1 3 5 7))
                      (dolist (b '(2 4 6 8))
                        (when (> (+ a b) 7)
                          (setq result (cons (cons a b) result)))))
                    (nreverse result))"#;
    let expect = expect_test::expect![
        r#""OK ((1 . 8) (3 . 6) (3 . 8) (5 . 4) (5 . 6) (5 . 8) (7 . 2) (7 . 4) (7 . 6) (7 . 8))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Manual cl-loop-like patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_adv_loop_collect_when_with_index() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Emulate (cl-loop for x in list for i from 0 when (pred x) collect (list i x))
    let form = r#"(let ((result nil)
                        (idx 0))
                    (dolist (x '(apple banana cherry date elderberry fig grape))
                      (when (> (length (symbol-name x)) 5)
                        (setq result (cons (list idx x) result)))
                      (setq idx (1+ idx)))
                    (nreverse result))"#;
    let expect = expect_test::expect![r#""OK ((1 banana) (2 cherry) (4 elderberry))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Manual cl-reduce with initial value
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_adv_reduce_nested_alist_merge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Reduce a list of alists into one by merging, later entries override.
    // This is the cl-reduce pattern: fold left with initial value.
    let form = r#"(let ((alists '(((a . 1) (b . 2))
                                   ((b . 20) (c . 3))
                                   ((a . 100) (d . 4))))
                        (merged nil))
                    (dolist (al alists)
                      (dolist (pair al)
                        (let ((existing (assq (car pair) merged)))
                          (if existing
                              (setcdr existing (cdr pair))
                            (setq merged (cons (cons (car pair) (cdr pair))
                                               merged))))))
                    ;; Sort by key name for deterministic output
                    (sort merged (lambda (x y)
                                   (string< (symbol-name (car x))
                                            (symbol-name (car y))))))"#;
    let expect = expect_test::expect![r#""OK ((a . 100) (b . 20) (c . 3) (d . 4))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// mapcan-like behavior with nconc+mapcar
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_adv_mapcan_flatten_and_transform() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // mapcan-like: for each element, produce 0-N outputs, flatten.
    // Here: expand each number n into (n n*10 n*100) if n > 2, else skip.
    let form = r#"(let ((result nil))
                    (dolist (n '(1 3 2 5 4))
                      (when (> n 2)
                        (setq result (nconc result
                                           (list n (* n 10) (* n 100))))))
                    result)"#;
    let expect = expect_test::expect![[r#""OK (3 30 300 5 50 500 4 40 400)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(3 30 300 5 50 500 4 40 400)", &o, &n);
}

// ---------------------------------------------------------------------------
// cl-remove-if / cl-remove-if-not emulation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cl_adv_remove_if_not_chained_filters() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Chain of filters: keep numbers that are positive, even, and < 20.
    // Implements remove-if-not three times in sequence.
    let form = r#"(let ((data '(-5 2 -3 4 6 15 18 22 -8 10 0 14 21 16)))
                    (let ((step1 nil))
                      ;; Keep positive
                      (dolist (x data)
                        (when (> x 0)
                          (setq step1 (cons x step1))))
                      (setq step1 (nreverse step1))
                      (let ((step2 nil))
                        ;; Keep even
                        (dolist (x step1)
                          (when (= (% x 2) 0)
                            (setq step2 (cons x step2))))
                        (setq step2 (nreverse step2))
                        (let ((step3 nil))
                          ;; Keep < 20
                          (dolist (x step2)
                            (when (< x 20)
                              (setq step3 (cons x step3))))
                          (nreverse step3)))))"#;
    let expect = expect_test::expect![[r#""OK (2 4 6 18 10 14 16)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(2 4 6 18 10 14 16)", &o, &n);
}
