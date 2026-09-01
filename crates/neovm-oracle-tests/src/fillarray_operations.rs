//! Oracle parity tests for `fillarray` — fills arrays (vectors, strings,
//! bool-vectors) with a value in place.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Basic fillarray on vectors
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_fillarray_vector_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Fill a fresh vector with a single value and verify every element
    let form = r#"(let ((v (make-vector 6 0)))
                    (fillarray v 42)
                    (list (aref v 0) (aref v 1) (aref v 2)
                          (aref v 3) (aref v 4) (aref v 5)))"#;
    let expect = expect_test::expect![[r#""OK (42 42 42 42 42 42)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Fill a string with a character
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_fillarray_string_with_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((s (copy-sequence "abcdef")))
                    (fillarray s ?z)
                    s)"#;
    let expect = expect_test::expect![[r#""OK \"zzzzzz\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Fill a bool-vector with t/nil
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_fillarray_bool_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Fill with t then check, fill with nil then check
    let form = r#"(let ((bv (make-bool-vector 8 nil)))
                    (fillarray bv t)
                    (let ((all-true (let ((ok t))
                                      (dotimes (i 8)
                                        (unless (aref bv i)
                                          (setq ok nil)))
                                      ok)))
                      (fillarray bv nil)
                      (let ((all-false (let ((ok t))
                                         (dotimes (i 8)
                                           (when (aref bv i)
                                             (setq ok nil)))
                                         ok)))
                        (list all-true all-false))))"#;
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Overwrite existing content
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_fillarray_overwrite_existing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Vector already has diverse content; fillarray overwrites all
    let form = r#"(let ((v (vector 'a 'b 'c 100 200 nil t "hello")))
                    (fillarray v 'replaced)
                    (let ((result nil))
                      (dotimes (i (length v))
                        (setq result (cons (aref v i) result)))
                      (nreverse result)))"#;
    let expect = expect_test::expect![[
        r#""OK (replaced replaced replaced replaced replaced replaced replaced replaced)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// fillarray returns the SAME array (identity, not copy)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_fillarray_returns_same_object() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // `eq` must be t: fillarray returns the very same object it mutated
    let form = r#"(let ((v (make-vector 4 0)))
                    (let ((result (fillarray v 99)))
                      (list (eq v result)
                            (aref v 0)
                            (aref result 3))))"#;
    let expect = expect_test::expect![[r#""OK (t 99 99)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: selective fill using aset loop then fillarray for reset
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_fillarray_selective_then_reset() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a histogram in a vector, read it, then fillarray to zero it out
    let form = r#"(let ((histogram (make-vector 10 0))
                        (data '(3 1 4 1 5 9 2 6 5 3 5 0 0 7 8 9 9)))
                    ;; Accumulate counts
                    (dolist (d data)
                      (aset histogram d (1+ (aref histogram d))))
                    ;; Snapshot the counts before reset
                    (let ((snapshot (copy-sequence histogram)))
                      ;; Reset via fillarray
                      (fillarray histogram 0)
                      ;; Verify reset is all zeros and snapshot preserved
                      (let ((all-zero t))
                        (dotimes (i 10)
                          (unless (= (aref histogram i) 0)
                            (setq all-zero nil)))
                        (list all-zero
                              (append snapshot nil)
                              (aref snapshot 3)
                              (aref snapshot 5)))))"#;
    let expect = expect_test::expect![[r#""OK (t (2 2 1 2 1 3 1 1 1 3) 2 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Fill vector with nil to simulate clearing a sparse table
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_fillarray_clear_sparse_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a sparse association table in a vector (using cons cells),
    // then clear it with fillarray
    let form = r#"(let ((table (make-vector 8 nil)))
                    ;; Populate some slots (simple hash: mod 8)
                    (dolist (pair '((0 . "zero") (3 . "three") (5 . "five") (7 . "seven")))
                      (let ((idx (% (car pair) 8)))
                        (aset table idx (cons pair (aref table idx)))))
                    ;; Read before clearing
                    (let ((before-3 (aref table 3))
                          (before-5 (aref table 5)))
                      ;; Clear entire table
                      (fillarray table nil)
                      (let ((after-3 (aref table 3))
                            (after-5 (aref table 5)))
                        (list before-3 before-5 after-3 after-5))))"#;
    let expect = expect_test::expect![[r#""OK (((3 . \"three\")) ((5 . \"five\")) nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Fill string with multibyte character
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_fillarray_string_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((s (make-string 5 ?a)))
                    (fillarray s ?x)
                    (list s (length s) (aref s 0) (aref s 4)))"#;
    let expect = expect_test::expect![[r#""OK (\"xxxxx\" 5 120 120)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
