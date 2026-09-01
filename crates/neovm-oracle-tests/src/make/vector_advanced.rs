//! Advanced oracle parity tests for vector construction and manipulation.
//!
//! Tests make-vector with various init values, vector function, vconcat
//! merging, vector growth simulation, vector as bitmap, vector-based
//! binary heap, and vector-based disjoint set (union-find).

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// make-vector with various init values
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_make_vector_various_init() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test make-vector with different types of init values:
    // nil, t, integer, float, string, cons, lambda, keyword, symbol.
    let form = r#"(let ((v-nil    (make-vector 3 nil))
                        (v-t      (make-vector 3 t))
                        (v-int    (make-vector 4 42))
                        (v-float  (make-vector 2 3.14))
                        (v-str    (make-vector 2 "hello"))
                        (v-cons   (make-vector 3 '(1 . 2)))
                        (v-kw     (make-vector 2 :test))
                        (v-sym    (make-vector 2 'foo))
                        (v-zero   (make-vector 0 999)))
                    (list
                      ;; Lengths
                      (length v-nil) (length v-t) (length v-int) (length v-float)
                      (length v-str) (length v-cons) (length v-kw) (length v-sym)
                      (length v-zero)
                      ;; Values at index 0 (or empty)
                      (aref v-nil 0) (aref v-t 0) (aref v-int 0) (aref v-float 0)
                      (aref v-str 0) (aref v-cons 0) (aref v-kw 0) (aref v-sym 0)
                      ;; All elements are eq (shared identity for cons)
                      (eq (aref v-cons 0) (aref v-cons 1))
                      (eq (aref v-cons 0) (aref v-cons 2))
                      (eq (aref v-str 0) (aref v-str 1))
                      ;; Mutating through one ref affects the other
                      (progn (setcar (aref v-cons 0) 99)
                             (car (aref v-cons 2)))))"#;
    let expect = expect_test::expect![[
        r#""OK (3 3 4 2 2 3 2 2 0 nil t 42 3.14 \"hello\" (99 . 2) :test foo t t t 99)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// vector function with multiple arguments
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vector_function_multi_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // The `vector` function creates a vector from its arguments.
    let form = r#"(list
                    (vector)
                    (vector 1)
                    (vector 1 2 3)
                    (vector 'a 'b 'c 'd 'e)
                    (vector "hello" nil t 42 3.14 :key '(1 2))
                    ;; Nested vectors
                    (vector (vector 1 2) (vector 3 4))
                    ;; Length checks
                    (length (vector))
                    (length (vector 1 2 3 4 5 6 7 8 9 10))
                    ;; Access
                    (aref (vector 'x 'y 'z) 1)
                    ;; Conversion to list
                    (append (vector 10 20 30) nil))"#;
    let expect = expect_test::expect![[
        r#""OK ([] [1] [1 2 3] [a b c d e] [\"hello\" nil t 42 3.14 :key (1 2)] [[1 2] [3 4]] 0 10 y (10 20 30))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// vconcat merging different sequence types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vconcat_sequence_merge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // vconcat can merge vectors, lists, and strings (as char sequences).
    // Test chaining, empty inputs, and type mixing.
    let form = r#"(list
                    ;; Basic merges
                    (vconcat [1 2] '(3 4) [5 6])
                    (vconcat "abc" [100 101])
                    (vconcat '(a b c) '(d e f))
                    ;; Empty sequences
                    (vconcat [] [] [])
                    (vconcat nil nil nil)
                    (vconcat [1] nil [2] nil [3])
                    ;; String chars become integers
                    (vconcat "AB")
                    ;; Single sequence
                    (vconcat [10 20 30])
                    ;; Many sequences chained
                    (vconcat [1] '(2) "3" [4] '(5) "6")
                    ;; Result types
                    (vectorp (vconcat [1] '(2)))
                    (length (vconcat [1 2] '(3 4 5) "ab")))"#;
    let expect = expect_test::expect![[
        r#""OK ([1 2 3 4 5 6] [97 98 99 100 101] [a b c d e f] [] [] [1 2 3] [65 66] [10 20 30] [1 2 51 4 5 54] t 7)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Vector growth simulation (copy to larger vector)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vector_growth_simulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate a dynamic array: start small, grow by doubling when full.
    let form = "(let ((buf (make-vector 2 nil))
                      (len 0)
                      (cap 2))
                  (let ((dyn-push
                         (lambda (val)
                           ;; Grow if needed
                           (when (= len cap)
                             (let ((new-cap (* cap 2)))
                               (let ((new-buf (make-vector new-cap nil)))
                                 (dotimes (i len)
                                   (aset new-buf i (aref buf i)))
                                 (setq buf new-buf cap new-cap))))
                           (aset buf len val)
                           (setq len (1+ len))))
                        (dyn-get
                         (lambda (i) (aref buf i)))
                        (dyn-contents
                         (lambda ()
                           (let ((result nil))
                             (let ((i (1- len)))
                               (while (>= i 0)
                                 (setq result (cons (aref buf i) result))
                                 (setq i (1- i))))
                             result))))
                    ;; Push 10 elements into initial capacity 2
                    (dotimes (i 10)
                      (funcall dyn-push (* i i)))
                    (list
                      ;; Length and capacity
                      len cap
                      ;; Contents
                      (funcall dyn-contents)
                      ;; Random access
                      (funcall dyn-get 0)
                      (funcall dyn-get 5)
                      (funcall dyn-get 9)
                      ;; Capacity should be 16 (2->4->8->16)
                      (length buf))))";
    let expect = expect_test::expect![[r#""OK (10 16 (0 1 4 9 16 25 36 49 64 81) 0 25 81 16)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Vector as bitmap (bit manipulation via aref/aset)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vector_bitmap_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use a vector of integers as a bitmap. Each integer stores 30 bits
    // (safe for Elisp fixnums). Implement set-bit, clear-bit, test-bit.
    let form = "(let ((bits-per-word 30)
                      (bitmap (make-vector 4 0)))
                  (let ((set-bit
                         (lambda (n)
                           (let ((word (/ n bits-per-word))
                                 (bit (% n bits-per-word)))
                             (aset bitmap word
                                   (logior (aref bitmap word)
                                           (ash 1 bit))))))
                        (clear-bit
                         (lambda (n)
                           (let ((word (/ n bits-per-word))
                                 (bit (% n bits-per-word)))
                             (aset bitmap word
                                   (logand (aref bitmap word)
                                           (lognot (ash 1 bit)))))))
                        (test-bit
                         (lambda (n)
                           (let ((word (/ n bits-per-word))
                                 (bit (% n bits-per-word)))
                             (not (= 0 (logand (aref bitmap word)
                                               (ash 1 bit))))))))
                    ;; Set bits 0, 5, 29, 30, 60, 90
                    (funcall set-bit 0)
                    (funcall set-bit 5)
                    (funcall set-bit 29)
                    (funcall set-bit 30)
                    (funcall set-bit 60)
                    (funcall set-bit 90)
                    (let ((results-before
                           (list
                             (funcall test-bit 0)
                             (funcall test-bit 5)
                             (funcall test-bit 6)
                             (funcall test-bit 29)
                             (funcall test-bit 30)
                             (funcall test-bit 31)
                             (funcall test-bit 60)
                             (funcall test-bit 90))))
                      ;; Clear bits 5 and 30
                      (funcall clear-bit 5)
                      (funcall clear-bit 30)
                      (let ((results-after
                             (list
                               (funcall test-bit 5)
                               (funcall test-bit 30)
                               ;; Neighbors unaffected
                               (funcall test-bit 0)
                               (funcall test-bit 29)
                               (funcall test-bit 60)
                               (funcall test-bit 90))))
                        (list results-before results-after))))";
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: vector-based binary heap (min-heap)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vector_binary_heap() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Min-heap stored in a flat vector. heap[0] = min.
    // parent(i) = (i-1)/2, left(i) = 2*i+1, right(i) = 2*i+2.
    let form = "(let ((heap (make-vector 20 nil))
                      (size 0))
                  (let ((swap
                         (lambda (i j)
                           (let ((tmp (aref heap i)))
                             (aset heap i (aref heap j))
                             (aset heap j tmp))))
                        (sift-up
                         (lambda (i)
                           (while (> i 0)
                             (let ((parent (/ (1- i) 2)))
                               (if (< (aref heap i) (aref heap parent))
                                   (progn
                                     (let ((tmp (aref heap i)))
                                       (aset heap i (aref heap parent))
                                       (aset heap parent tmp))
                                     (setq i parent))
                                 (setq i 0))))))
                        (sift-down
                         (lambda (i)
                           (let ((done nil))
                             (while (not done)
                               (let ((smallest i)
                                     (left (1+ (* 2 i)))
                                     (right (+ 2 (* 2 i))))
                                 (when (and (< left size)
                                            (< (aref heap left) (aref heap smallest)))
                                   (setq smallest left))
                                 (when (and (< right size)
                                            (< (aref heap right) (aref heap smallest)))
                                   (setq smallest right))
                                 (if (= smallest i)
                                     (setq done t)
                                   (let ((tmp (aref heap i)))
                                     (aset heap i (aref heap smallest))
                                     (aset heap smallest tmp))
                                   (setq i smallest)))))))
                        (heap-push
                         (lambda (val)
                           (aset heap size val)
                           (setq size (1+ size))
                           ;; sift-up
                           (let ((i (1- size)))
                             (while (> i 0)
                               (let ((parent (/ (1- i) 2)))
                                 (if (< (aref heap i) (aref heap parent))
                                     (progn
                                       (let ((tmp (aref heap i)))
                                         (aset heap i (aref heap parent))
                                         (aset heap parent tmp))
                                       (setq i parent))
                                   (setq i 0)))))))
                        (heap-pop
                         (lambda ()
                           (let ((min-val (aref heap 0)))
                             (setq size (1- size))
                             (aset heap 0 (aref heap size))
                             ;; sift-down
                             (let ((i 0) (done nil))
                               (while (not done)
                                 (let ((smallest i)
                                       (left (1+ (* 2 i)))
                                       (right (+ 2 (* 2 i))))
                                   (when (and (< left size)
                                              (< (aref heap left) (aref heap smallest)))
                                     (setq smallest left))
                                   (when (and (< right size)
                                              (< (aref heap right) (aref heap smallest)))
                                     (setq smallest right))
                                   (if (= smallest i)
                                       (setq done t)
                                     (let ((tmp (aref heap i)))
                                       (aset heap i (aref heap smallest))
                                       (aset heap smallest tmp))
                                     (setq i smallest)))))
                             min-val))))
                    ;; Insert values in random order
                    (funcall heap-push 15)
                    (funcall heap-push 3)
                    (funcall heap-push 22)
                    (funcall heap-push 1)
                    (funcall heap-push 8)
                    (funcall heap-push 47)
                    (funcall heap-push 5)
                    (funcall heap-push 12)
                    ;; Extract all in sorted order
                    (let ((sorted nil))
                      (dotimes (_ 8)
                        (setq sorted (cons (funcall heap-pop) sorted)))
                      (nreverse sorted))))";
    let expect = expect_test::expect![[r#""OK (1 3 5 8 12 15 22 47)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(1 3 5 8 12 15 22 47)", &o, &n);
}

// ---------------------------------------------------------------------------
// Complex: vector-based disjoint set (union-find)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vector_union_find() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Union-Find with path compression and union by rank.
    // parent[i] and rank[i] stored in vectors.
    let form = "(let ((n 10)
                      (parent (make-vector 10 0))
                      (rank (make-vector 10 0)))
                  ;; Initialize: each element is its own parent
                  (dotimes (i n)
                    (aset parent i i))
                  (let ((find
                         (lambda (x)
                           (let ((root x))
                             ;; Find root
                             (while (/= (aref parent root) root)
                               (setq root (aref parent root)))
                             ;; Path compression
                             (while (/= (aref parent x) root)
                               (let ((next (aref parent x)))
                                 (aset parent x root)
                                 (setq x next)))
                             root)))
                        (union-sets
                         (lambda (a b)
                           (let ((ra (funcall find a))
                                 (rb (funcall find b)))
                             (when (/= ra rb)
                               (cond
                                 ((< (aref rank ra) (aref rank rb))
                                  (aset parent ra rb))
                                 ((> (aref rank ra) (aref rank rb))
                                  (aset parent rb ra))
                                 (t
                                  (aset parent rb ra)
                                  (aset rank ra (1+ (aref rank ra))))))))))
                    ;; Build groups: {0,1,2,3}, {4,5,6}, {7,8}, {9}
                    (funcall union-sets 0 1)
                    (funcall union-sets 1 2)
                    (funcall union-sets 2 3)
                    (funcall union-sets 4 5)
                    (funcall union-sets 5 6)
                    (funcall union-sets 7 8)
                    ;; Test connectivity
                    (list
                      ;; Same group
                      (= (funcall find 0) (funcall find 3))
                      (= (funcall find 4) (funcall find 6))
                      (= (funcall find 7) (funcall find 8))
                      ;; Different groups
                      (= (funcall find 0) (funcall find 4))
                      (= (funcall find 0) (funcall find 9))
                      (= (funcall find 4) (funcall find 7))
                      ;; Count distinct groups
                      (let ((groups (make-hash-table)))
                        (dotimes (i n)
                          (puthash (funcall find i) t groups))
                        (hash-table-count groups))
                      ;; Merge two groups and recount
                      (progn
                        (funcall union-sets 0 4)
                        (let ((groups (make-hash-table)))
                          (dotimes (i n)
                            (puthash (funcall find i) t groups))
                          (hash-table-count groups))))))";
    let expect = expect_test::expect![[r#""ERR (void-variable find)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
