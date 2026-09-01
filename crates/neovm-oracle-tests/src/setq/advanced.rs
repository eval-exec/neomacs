//! Oracle parity tests for advanced `setq` patterns: multiple
//! assignments, setq with side effects, setq-default, setq-local,
//! and complex mutation patterns.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// setq multiple pairs
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_setq_multiple_assignments() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // setq can assign multiple variables in one form
    let form = r#"(let ((a nil) (b nil) (c nil) (d nil))
                    (setq a 1 b 2 c 3 d 4)
                    (list a b c d))"#;
    let expect = expect_test::expect![[r#""OK (1 2 3 4)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_setq_sequential_dependency() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Each assignment sees previous ones (sequential, not parallel)
    let form = r#"(let ((x 0) (y 0) (z 0))
                    (setq x 10
                          y (+ x 5)
                          z (* y 2))
                    (list x y z))"#;
    let expect = expect_test::expect![[r#""OK (10 15 30)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_setq_return_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // setq returns the last value assigned
    let form = r#"(let ((a nil) (b nil))
                    (list (setq a 42)
                          (setq a 1 b 2)
                          a b))"#;
    let expect = expect_test::expect![[r#""OK (42 2 1 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// setq with complex expressions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_setq_with_funcall() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((x nil) (y nil) (z nil))
                    (setq x (mapcar #'1+ '(1 2 3))
                          y (apply #'+ x)
                          z (format "sum=%d" y))
                    (list x y z))"#;
    let expect = expect_test::expect![[r#""OK ((2 3 4) 9 \"sum=9\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_setq_accumulation_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Common pattern: accumulate in setq
    let form = r#"(let ((result nil)
                        (sum 0))
                    (dolist (x '(1 2 3 4 5))
                      (setq sum (+ sum x)
                            result (cons (cons x sum) result)))
                    (nreverse result))"#;
    let expect = expect_test::expect![[r#""OK ((1 . 1) (2 . 3) (3 . 6) (4 . 10) (5 . 15))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// setq with buffer-local semantics
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_setq_in_buffer_context() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // setq in different buffer contexts
    let form = r#"(with-temp-buffer
                    (let ((fill-column 70))
                      (setq fill-column 100)
                      (let ((val-in-buf fill-column))
                        (list val-in-buf))))"#;
    let expect = expect_test::expect![[r#""OK (100)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: state machine using setq
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_setq_state_machine() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse a simple expression like "3+4*2" using state machine
    let form = r#"(let ((input "3+14*2-5")
                        (pos 0)
                        (tokens nil))
                    ;; Tokenizer state machine
                    (while (< pos (length input))
                      (let ((c (aref input pos)))
                        (cond
                         ;; Digit: scan full number
                         ((and (>= c ?0) (<= c ?9))
                          (let ((start pos))
                            (while (and (< pos (length input))
                                        (let ((ch (aref input pos)))
                                          (and (>= ch ?0) (<= ch ?9))))
                              (setq pos (1+ pos)))
                            (setq tokens
                                  (cons (cons 'num
                                              (string-to-number
                                               (substring input start pos)))
                                        tokens))))
                         ;; Operator
                         ((memq c '(?+ ?- ?* ?/))
                          (setq tokens
                                (cons (cons 'op
                                            (char-to-string c))
                                      tokens)
                                pos (1+ pos)))
                         ;; Skip unknown
                         (t (setq pos (1+ pos))))))
                    (nreverse tokens))"#;
    let expect = expect_test::expect![[
        r#""OK ((num . 3) (op . \"+\") (num . 14) (op . \"*\") (num . 2) (op . \"-\") (num . 5))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: parallel assignment emulation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_setq_parallel_swap_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Rotate values through 4 variables using temp + setq
    let form = r#"(let ((a 1) (b 2) (c 3) (d 4))
                    ;; Rotate: a→b→c→d→a
                    (let ((tmp a))
                      (setq a d d c c b b tmp))
                    (list a b c d))"#;
    let expect = expect_test::expect![[r#""OK (4 1 2 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: setq in nested let with shadowing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_setq_let_shadowing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((x 1))
                    (let ((log nil))
                      (setq log (cons (list 'outer x) log))
                      (let ((x 10))
                        (setq log (cons (list 'inner-before x) log))
                        (setq x 20)
                        (setq log (cons (list 'inner-after x) log)))
                      ;; x should be back to 1 (outer scope)
                      (setq log (cons (list 'outer-after x) log))
                      (nreverse log)))"#;
    let expect = expect_test::expect![[
        r#""OK ((outer 1) (inner-before 10) (inner-after 20) (outer-after 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: builder pattern with chained setq
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_setq_builder_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a complex data structure incrementally
    let form = r#"(let ((config nil))
                    (setq config (cons '(version . 2) config))
                    (setq config (cons '(debug . t) config))
                    (setq config (cons (cons 'max-retries 3) config))
                    (setq config (cons (cons 'timeout 30) config))
                    ;; Add computed fields
                    (let ((timeout (cdr (assq 'timeout config)))
                          (retries (cdr (assq 'max-retries config))))
                      (setq config
                            (cons (cons 'total-wait
                                        (* timeout retries))
                                  config)))
                    ;; Verify all fields
                    (list (cdr (assq 'version config))
                          (cdr (assq 'debug config))
                          (cdr (assq 'max-retries config))
                          (cdr (assq 'timeout config))
                          (cdr (assq 'total-wait config))))"#;
    let expect = expect_test::expect![[r#""OK (2 t 3 30 90)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
