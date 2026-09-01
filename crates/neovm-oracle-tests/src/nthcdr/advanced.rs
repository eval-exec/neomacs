//! Oracle parity tests for `nthcdr`, `nth`, `elt`, `last` with
//! all parameters, plus `butlast`, `nbutlast`, `safe-length`,
//! `proper-list-p`, and complex list access patterns.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// nthcdr edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nthcdr_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((lst '(a b c d e f g)))
                    (list (nthcdr 0 lst)
                          (nthcdr 1 lst)
                          (nthcdr 3 lst)
                          (nthcdr 6 lst)
                          (nthcdr 7 lst)   ;; past end → nil
                          (nthcdr 100 lst) ;; way past end → nil
                          (nthcdr 0 nil)
                          (nthcdr 5 nil)))"#;
    let expect = expect_test::expect![[
        r#""OK ((a b c d e f g) (b c d e f g) (d e f g) (g) nil nil nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// nth with various positions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nth_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((lst '(alpha beta gamma delta epsilon)))
                    (list (nth 0 lst)
                          (nth 1 lst)
                          (nth 4 lst)
                          (nth 5 lst)   ;; past end → nil
                          (nth 99 lst)
                          (nth 0 nil)))"#;
    let expect = expect_test::expect![[r#""OK (alpha beta epsilon nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// last with optional N parameter
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_last_with_n() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((lst '(a b c d e)))
                    (list (last lst)
                          (last lst 1)
                          (last lst 2)
                          (last lst 3)
                          (last lst 5)
                          (last lst 6)
                          (last lst 0)
                          (last nil)
                          (last nil 3)))"#;
    let expect = expect_test::expect![[
        r#""OK ((e) (e) (d e) (c d e) (a b c d e) (a b c d e) nil nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// butlast / nbutlast
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_butlast_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((lst '(1 2 3 4 5)))
                    (list (butlast lst)
                          (butlast lst 1)
                          (butlast lst 2)
                          (butlast lst 4)
                          (butlast lst 5)
                          (butlast lst 6)
                          (butlast nil)
                          (butlast '(solo))))"#;
    let expect =
        expect_test::expect![[r#""OK ((1 2 3 4) (1 2 3 4) (1 2 3) (1) nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_nbutlast_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // nbutlast destructively modifies
    let form = r#"(let ((l1 (list 1 2 3 4 5))
                        (l2 (list 1 2 3 4 5))
                        (l3 (list 1 2 3 4 5)))
                    (list (nbutlast l1)
                          (nbutlast l2 2)
                          (nbutlast l3 5)))"#;
    let expect = expect_test::expect![[r#""OK ((1 2 3 4) (1 2 3) nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// elt on various sequence types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_elt_list_and_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // elt works on both lists and vectors
    let form = r#"(list (elt '(a b c d e) 0)
                        (elt '(a b c d e) 3)
                        (elt [10 20 30 40 50] 0)
                        (elt [10 20 30 40 50] 4)
                        (elt "hello" 0)
                        (elt "hello" 4))"#;
    let expect = expect_test::expect![[r#""OK (a d 10 50 104 111)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: list rotation using nthcdr
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nthcdr_rotate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Rotate a list by N positions
    let form = r#"(let ((rotate
                         (lambda (lst n)
                           (let ((len (length lst)))
                             (when (> len 0)
                               (let ((n (% n len)))
                                 (append (nthcdr n lst)
                                         (butlast lst (- len n)))))))))
                    (list (funcall rotate '(1 2 3 4 5) 0)
                          (funcall rotate '(1 2 3 4 5) 1)
                          (funcall rotate '(1 2 3 4 5) 2)
                          (funcall rotate '(1 2 3 4 5) 4)
                          (funcall rotate '(1 2 3 4 5) 5)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5) (2 3 4 5 1) (3 4 5 1 2) (5 1 2 3 4) (1 2 3 4 5))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: sliding window using nthcdr
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nthcdr_sliding_window() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compute sliding window averages
    let form = r#"(let ((data '(10 20 30 40 50 60 70 80 90 100))
                        (window-size 3)
                        (averages nil))
                    (let ((i 0)
                          (n (length data)))
                      (while (<= (+ i window-size) n)
                        (let ((window (butlast (nthcdr i data)
                                               (- n i window-size)))
                              (sum 0))
                          (dolist (x window)
                            (setq sum (+ sum x)))
                          (setq averages
                                (cons (/ (float sum) window-size)
                                      averages)))
                        (setq i (1+ i))))
                    (nreverse averages))"#;
    let expect = expect_test::expect![[r#""OK (20.0 30.0 40.0 50.0 60.0 70.0 80.0 90.0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: chunk list using nthcdr + butlast
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nthcdr_chunk() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Split list into chunks of size N
    let form = r#"(let ((chunk
                         (lambda (lst n)
                           (let ((chunks nil))
                             (while lst
                               (let ((tail (nthcdr n lst)))
                                 (setq chunks
                                       (cons (butlast lst
                                                      (length tail))
                                             chunks))
                                 (setq lst tail)))
                             (nreverse chunks)))))
                    (list (funcall chunk '(1 2 3 4 5 6 7 8 9) 3)
                          (funcall chunk '(a b c d e) 2)
                          (funcall chunk '(x) 5)
                          (funcall chunk nil 3)))"#;
    let expect =
        expect_test::expect![[r#""OK (((1 2 3) (4 5 6) (7 8 9)) ((a b) (c d) (e)) ((x)) nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: interleave two lists
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nthcdr_interleave() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((interleave
                         (lambda (a b)
                           (let ((result nil))
                             (while (or a b)
                               (when a
                                 (setq result (cons (car a) result)
                                       a (cdr a)))
                               (when b
                                 (setq result (cons (car b) result)
                                       b (cdr b))))
                             (nreverse result)))))
                    (list (funcall interleave '(1 3 5) '(2 4 6))
                          (funcall interleave '(a b c d) '(1 2))
                          (funcall interleave nil '(x y z))
                          (funcall interleave '(solo) nil)))"#;
    let expect = expect_test::expect![[r#""OK ((1 2 3 4 5 6) (a 1 b 2 c d) (x y z) (solo))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
