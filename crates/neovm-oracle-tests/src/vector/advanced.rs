//! Advanced oracle parity tests for vector operations.
//!
//! Tests vconcat with multiple argument types, nested vectors (2D arrays),
//! fillarray differences, sorting with predicates, vector-as-stack,
//! circular buffer simulation, and seq-* operations on vectors.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// vconcat with multiple heterogeneous arguments
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vconcat_multi_heterogeneous() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Combine vectors, lists, strings, and nil in a single vconcat call
    let form = r#"(let ((v1 [1 2 3])
                        (l1 '(4 5))
                        (s1 "AB")
                        (v2 [10 20]))
                    (list
                      (vconcat v1 l1 s1 v2)
                      (vconcat nil v1 nil)
                      (vconcat '() [99] "Z" '(100))
                      (vconcat v1 v1 v1)))"#;
    let expect = expect_test::expect![[
        r#""OK ([1 2 3 4 5 65 66 10 20] [1 2 3] [99 90 100] [1 2 3 1 2 3 1 2 3])""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// make-vector: edge cases and large sizes
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_make_vector_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Various init values including lists, vectors, strings
    let form = r#"(let ((v1 (make-vector 3 '(a b)))
                        (v2 (make-vector 4 [1 2]))
                        (v3 (make-vector 2 "hello"))
                        (v4 (make-vector 0 99))
                        (v5 (make-vector 5 t)))
                    (list
                      (length v1) (aref v1 0) (aref v1 2)
                      ;; All elements should be eq (same object)
                      (eq (aref v1 0) (aref v1 1))
                      (length v2) (aref v2 3)
                      (length v3) (aref v3 1)
                      (length v4)
                      v5))"#;
    let expect =
        expect_test::expect![[r#""OK (3 (a b) (a b) t 4 [1 2] 2 \"hello\" 0 [t t t t t])""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested vectors: 2D array simulation with aref/aset
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nested_vectors_2d_array() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a 3x3 matrix, set values, compute row sums
    let form = "(let ((matrix (make-vector 3 nil)))
                  ;; Initialize each row as a separate vector
                  (dotimes (i 3)
                    (aset matrix i (make-vector 3 0)))
                  ;; Set values: identity matrix * (i+1)
                  (dotimes (i 3)
                    (dotimes (j 3)
                      (aset (aref matrix i) j
                            (if (= i j) (* (1+ i) 10) (+ i j)))))
                  ;; Read back all values + compute row sums
                  (let ((sums nil))
                    (dotimes (i 3)
                      (let ((row-sum 0))
                        (dotimes (j 3)
                          (setq row-sum (+ row-sum (aref (aref matrix i) j))))
                        (setq sums (cons row-sum sums))))
                    (list (aref (aref matrix 0) 0)   ;; 10
                          (aref (aref matrix 1) 1)   ;; 20
                          (aref (aref matrix 2) 2)   ;; 30
                          (aref (aref matrix 0) 1)   ;; 0+1=1
                          (aref (aref matrix 2) 0)   ;; 2+0=2
                          (nreverse sums))))";
    let expect = expect_test::expect![[r#""OK (10 20 30 1 2 (13 24 35))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// fillarray: vector vs string differences
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_fillarray_vector_vs_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // fillarray on vector fills with any value; on string fills with char code
    let form = r#"(let ((v (vector 1 2 3 4 5))
                        (s (copy-sequence "abcde")))
                    ;; Fill vector with symbol
                    (fillarray v 'x)
                    ;; Fill string with char
                    (fillarray s ?Z)
                    (let ((v2 (make-vector 4 0)))
                      (fillarray v2 42)
                      (list v s v2
                            ;; Verify all elements changed
                            (aref v 0) (aref v 4)
                            (aref s 0) (aref s 4)
                            ;; fillarray returns the array itself
                            (eq (fillarray (make-vector 2 0) 1)
                                (fillarray (make-vector 2 0) 1)))))"#;
    let expect =
        expect_test::expect![[r#""OK ([x x x x x] \"ZZZZZ\" [42 42 42 42] x x 90 90 nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Vector sorting with custom predicate
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vector_sort_custom_predicate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // sort is destructive; test with various predicates
    let form = r#"(let ((v1 (sort (vector 5 3 1 4 2) #'<))
                        (v2 (sort (vector 5 3 1 4 2) #'>))
                        ;; Sort strings by length
                        (v3 (sort (vector "hello" "a" "foo" "hi" "world!")
                                  (lambda (a b)
                                    (< (length a) (length b)))))
                        ;; Sort by absolute value
                        (v4 (sort (vector -3 1 -5 2 -1 4)
                                  (lambda (a b)
                                    (< (abs a) (abs b)))))
                        ;; Stable-ish: sort already sorted
                        (v5 (sort (vector 1 2 3 4 5) #'<)))
                    (list v1 v2
                          (append v3 nil)  ;; convert to list for easier comparison
                          v4 v5))"#;
    let expect = expect_test::expect![[
        r#""OK ([1 2 3 4 5] [5 4 3 2 1] (\"a\" \"hi\" \"foo\" \"hello\" \"world!\") [1 -1 2 -3 4 -5] [1 2 3 4 5])""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Vector as stack (push/pop via aref/aset + length tracking)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vector_as_stack() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate a fixed-size stack with a vector and a top pointer
    let form = "(let ((stack (make-vector 10 nil))
                      (top 0))
                  ;; Push helper: returns new top
                  (let ((push-fn (lambda (val)
                                   (aset stack top val)
                                   (setq top (1+ top))))
                        (pop-fn (lambda ()
                                  (setq top (1- top))
                                  (aref stack top)))
                        (peek-fn (lambda ()
                                   (aref stack (1- top)))))
                    ;; Push several values
                    (funcall push-fn 'a)
                    (funcall push-fn 'b)
                    (funcall push-fn 'c)
                    (funcall push-fn 'd)
                    (let ((size-after-push top)
                          (peek-val (funcall peek-fn)))
                      ;; Pop two
                      (let ((pop1 (funcall pop-fn))
                            (pop2 (funcall pop-fn)))
                        ;; Push one more
                        (funcall push-fn 'e)
                        (list size-after-push
                              peek-val
                              pop1 pop2
                              top
                              (funcall peek-fn)
                              ;; Drain remaining
                              (funcall pop-fn)
                              (funcall pop-fn)
                              (funcall pop-fn))))))";
    let expect = expect_test::expect![[r#""OK (4 d d c 3 e e b a)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Circular buffer with wrap-around
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vector_circular_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Ring buffer: fixed size, head/tail pointers, wrap-around
    let form = "(let ((buf (make-vector 4 nil))
                      (head 0)
                      (tail 0)
                      (count 0)
                      (capacity 4))
                  (let ((enqueue (lambda (val)
                                   (when (< count capacity)
                                     (aset buf tail val)
                                     (setq tail (% (1+ tail) capacity))
                                     (setq count (1+ count)))))
                        (dequeue (lambda ()
                                   (when (> count 0)
                                     (let ((val (aref buf head)))
                                       (setq head (% (1+ head) capacity))
                                       (setq count (1- count))
                                       val))))
                        (contents (lambda ()
                                    (let ((result nil) (i 0) (pos head))
                                      (while (< i count)
                                        (setq result (cons (aref buf pos) result))
                                        (setq pos (% (1+ pos) capacity))
                                        (setq i (1+ i)))
                                      (nreverse result)))))
                    ;; Fill the buffer
                    (funcall enqueue 10)
                    (funcall enqueue 20)
                    (funcall enqueue 30)
                    (funcall enqueue 40)
                    (let ((full-contents (funcall contents))
                          ;; Try to enqueue when full (should be no-op)
                          (_ (funcall enqueue 50))
                          (still-full (funcall contents)))
                      ;; Dequeue two, causing wrap-around
                      (let ((d1 (funcall dequeue))
                            (d2 (funcall dequeue)))
                        ;; Enqueue two more (these wrap around)
                        (funcall enqueue 50)
                        (funcall enqueue 60)
                        (list full-contents
                              still-full
                              d1 d2
                              count
                              (funcall contents)
                              ;; Dequeue all
                              (funcall dequeue)
                              (funcall dequeue)
                              (funcall dequeue)
                              (funcall dequeue)
                              count)))))";
    let expect = expect_test::expect![[
        r#""OK ((10 20 30 40) (10 20 30 40) 10 20 4 (30 40 50 60) 30 40 50 60 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// seq-* operations on vectors
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_seq_operations_on_vectors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // seq-map, seq-filter, seq-reduce, seq-find, seq-every-p, seq-some on vectors
    let form = "(let ((v [1 2 3 4 5 6 7 8 9 10]))
                  (list
                    ;; seq-map: square each element
                    (seq-map (lambda (x) (* x x)) v)
                    ;; seq-filter: keep evens
                    (seq-filter (lambda (x) (= 0 (% x 2))) v)
                    ;; seq-reduce: sum
                    (seq-reduce #'+ v 0)
                    ;; seq-find: first > 5
                    (seq-find (lambda (x) (> x 5)) v)
                    ;; seq-every-p: all positive?
                    (seq-every-p (lambda (x) (> x 0)) v)
                    ;; seq-every-p: all > 5?
                    (seq-every-p (lambda (x) (> x 5)) v)
                    ;; seq-some: any > 8?
                    (seq-some (lambda (x) (> x 8)) v)
                    ;; seq-count: count odds
                    (seq-count (lambda (x) (= 1 (% x 2))) v)
                    ;; seq-uniq
                    (seq-uniq [1 2 2 3 3 3 4 4 4 4])))";
    let expect = expect_test::expect![[
        r#""OK ((1 4 9 16 25 36 49 64 81 100) (2 4 6 8 10) 55 6 t nil t 5 (1 2 3 4))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Vector transposition (2D matrix transpose)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vector_matrix_transpose() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Transpose a 3x4 matrix stored as vector-of-vectors into a 4x3 matrix
    let form = "(let ((rows 3) (cols 4))
                  ;; Build 3x4 matrix with values i*10+j
                  (let ((mat (make-vector rows nil)))
                    (dotimes (i rows)
                      (aset mat i (make-vector cols 0))
                      (dotimes (j cols)
                        (aset (aref mat i) j (+ (* i 10) j))))
                    ;; Transpose to 4x3
                    (let ((trans (make-vector cols nil)))
                      (dotimes (j cols)
                        (aset trans j (make-vector rows 0))
                        (dotimes (i rows)
                          (aset (aref trans j) i (aref (aref mat i) j))))
                      ;; Verify: trans[j][i] == mat[i][j]
                      (list
                        ;; Original: mat[0] = [0 1 2 3], mat[1] = [10 11 12 13], mat[2] = [20 21 22 23]
                        (aref mat 0) (aref mat 1) (aref mat 2)
                        ;; Transposed: trans[0] = [0 10 20], trans[1] = [1 11 21], etc.
                        (aref trans 0) (aref trans 1) (aref trans 2) (aref trans 3)
                        ;; Cross-check specific cells
                        (= (aref (aref mat 1) 2) (aref (aref trans 2) 1))
                        (= (aref (aref mat 2) 0) (aref (aref trans 0) 2))))))";
    let expect = expect_test::expect![[
        r#""OK ([0 1 2 3] [10 11 12 13] [20 21 22 23] [0 10 20] [1 11 21] [2 12 22] [3 13 23] t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
