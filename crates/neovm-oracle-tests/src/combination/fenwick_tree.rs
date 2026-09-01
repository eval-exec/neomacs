//! Oracle parity tests for a Fenwick (Binary Indexed) Tree in Elisp:
//! point update, prefix sum, range sum queries, construction from array,
//! point queries, inverse (find index with given cumulative frequency),
//! and a 2D Fenwick tree variant.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Point update and prefix sum
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_fenwick_point_update_prefix_sum() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Fenwick tree: 1-indexed vector of size n+1.
  ;; bit[i] stores partial sums over ranges determined by lowest set bit.

  (fset 'neovm--fw-make
    (lambda (n)
      "Create a Fenwick tree of size N (all zeros)."
      (make-vector (1+ n) 0)))

  (fset 'neovm--fw-update
    (lambda (bit i delta)
      "Add DELTA to position I (1-indexed) in Fenwick tree BIT."
      (let ((n (1- (length bit))))
        (while (<= i n)
          (aset bit i (+ (aref bit i) delta))
          (setq i (+ i (logand i (- i))))))))

  (fset 'neovm--fw-prefix-sum
    (lambda (bit i)
      "Compute prefix sum from index 1 to I (inclusive)."
      (let ((s 0))
        (while (> i 0)
          (setq s (+ s (aref bit i)))
          (setq i (- i (logand i (- i)))))
        s)))

  (unwind-protect
      (let ((bit (funcall 'neovm--fw-make 8)))
        ;; Insert values: position -> value
        ;; Array: [3, 2, -1, 6, 5, 4, -3, 7]
        (funcall 'neovm--fw-update bit 1 3)
        (funcall 'neovm--fw-update bit 2 2)
        (funcall 'neovm--fw-update bit 3 -1)
        (funcall 'neovm--fw-update bit 4 6)
        (funcall 'neovm--fw-update bit 5 5)
        (funcall 'neovm--fw-update bit 6 4)
        (funcall 'neovm--fw-update bit 7 -3)
        (funcall 'neovm--fw-update bit 8 7)
        (list
         ;; Prefix sums
         (funcall 'neovm--fw-prefix-sum bit 1)   ;; 3
         (funcall 'neovm--fw-prefix-sum bit 2)   ;; 3+2=5
         (funcall 'neovm--fw-prefix-sum bit 3)   ;; 5+(-1)=4
         (funcall 'neovm--fw-prefix-sum bit 4)   ;; 4+6=10
         (funcall 'neovm--fw-prefix-sum bit 5)   ;; 10+5=15
         (funcall 'neovm--fw-prefix-sum bit 8)   ;; 3+2-1+6+5+4-3+7=23
         ;; Update: add 10 to position 3
         (progn (funcall 'neovm--fw-update bit 3 10) nil)
         (funcall 'neovm--fw-prefix-sum bit 3)   ;; 5+(-1+10)=14
         (funcall 'neovm--fw-prefix-sum bit 8)   ;; 23+10=33
         ;; Prefix sum of 0 should be 0
         (funcall 'neovm--fw-prefix-sum bit 0)))
    (fmakunbound 'neovm--fw-make)
    (fmakunbound 'neovm--fw-update)
    (fmakunbound 'neovm--fw-prefix-sum)))"#;
    let expect = expect_test::expect![[r#""OK (3 5 4 10 15 23 nil 14 33 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Range sum queries
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_fenwick_range_sum() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--fw2-make
    (lambda (n) (make-vector (1+ n) 0)))

  (fset 'neovm--fw2-update
    (lambda (bit i delta)
      (let ((n (1- (length bit))))
        (while (<= i n)
          (aset bit i (+ (aref bit i) delta))
          (setq i (+ i (logand i (- i))))))))

  (fset 'neovm--fw2-prefix
    (lambda (bit i)
      (let ((s 0))
        (while (> i 0)
          (setq s (+ s (aref bit i)))
          (setq i (- i (logand i (- i)))))
        s)))

  (fset 'neovm--fw2-range-sum
    (lambda (bit l r)
      "Sum of elements from index L to R (1-indexed, inclusive)."
      (- (funcall 'neovm--fw2-prefix bit r)
         (funcall 'neovm--fw2-prefix bit (1- l)))))

  (unwind-protect
      (let ((bit (funcall 'neovm--fw2-make 10)))
        ;; Array: [1, 3, 5, 7, 9, 11, 13, 15, 17, 19]
        (let ((i 1))
          (while (<= i 10)
            (funcall 'neovm--fw2-update bit i (1- (* 2 i)))
            (setq i (1+ i))))
        (list
         ;; Range [1,1] = 1
         (funcall 'neovm--fw2-range-sum bit 1 1)
         ;; Range [1,5] = 1+3+5+7+9 = 25
         (funcall 'neovm--fw2-range-sum bit 1 5)
         ;; Range [3,7] = 5+7+9+11+13 = 45
         (funcall 'neovm--fw2-range-sum bit 3 7)
         ;; Range [6,10] = 11+13+15+17+19 = 75
         (funcall 'neovm--fw2-range-sum bit 6 10)
         ;; Range [1,10] = 100
         (funcall 'neovm--fw2-range-sum bit 1 10)
         ;; Range [5,5] = 9
         (funcall 'neovm--fw2-range-sum bit 5 5)
         ;; After update: add 100 to position 5
         (progn (funcall 'neovm--fw2-update bit 5 100) nil)
         ;; Range [4,6] was 7+9+11=27, now 7+109+11=127
         (funcall 'neovm--fw2-range-sum bit 4 6)
         ;; Range [1,10] was 100, now 200
         (funcall 'neovm--fw2-range-sum bit 1 10)))
    (fmakunbound 'neovm--fw2-make)
    (fmakunbound 'neovm--fw2-update)
    (fmakunbound 'neovm--fw2-prefix)
    (fmakunbound 'neovm--fw2-range-sum)))"#;
    let expect = expect_test::expect![[r#""OK (1 25 45 75 100 9 nil 127 200)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Construction from array
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_fenwick_build_from_array() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--fw3-update
    (lambda (bit i delta)
      (let ((n (1- (length bit))))
        (while (<= i n)
          (aset bit i (+ (aref bit i) delta))
          (setq i (+ i (logand i (- i))))))))

  (fset 'neovm--fw3-prefix
    (lambda (bit i)
      (let ((s 0))
        (while (> i 0)
          (setq s (+ s (aref bit i)))
          (setq i (- i (logand i (- i)))))
        s)))

  (fset 'neovm--fw3-build
    (lambda (arr)
      "Build a Fenwick tree from vector ARR (0-indexed input).
       Returns a 1-indexed Fenwick tree."
      (let* ((n (length arr))
             (bit (make-vector (1+ n) 0)))
        ;; O(n log n) construction via repeated updates
        (let ((i 0))
          (while (< i n)
            (funcall 'neovm--fw3-update bit (1+ i) (aref arr i))
            (setq i (1+ i))))
        bit)))

  (fset 'neovm--fw3-range
    (lambda (bit l r)
      (- (funcall 'neovm--fw3-prefix bit r)
         (funcall 'neovm--fw3-prefix bit (1- l)))))

  (unwind-protect
      (list
       ;; Build from [10, 20, 30, 40, 50]
       (let ((bit (funcall 'neovm--fw3-build [10 20 30 40 50])))
         (list
          (funcall 'neovm--fw3-prefix bit 1)  ;; 10
          (funcall 'neovm--fw3-prefix bit 3)  ;; 60
          (funcall 'neovm--fw3-prefix bit 5)  ;; 150
          (funcall 'neovm--fw3-range bit 2 4)))  ;; 90

       ;; Build from single element
       (let ((bit (funcall 'neovm--fw3-build [42])))
         (list (funcall 'neovm--fw3-prefix bit 1)))

       ;; Build from [1, 1, 1, 1, 1, 1, 1, 1]
       (let ((bit (funcall 'neovm--fw3-build [1 1 1 1 1 1 1 1])))
         (list
          (funcall 'neovm--fw3-prefix bit 4)  ;; 4
          (funcall 'neovm--fw3-prefix bit 8)  ;; 8
          (funcall 'neovm--fw3-range bit 3 6)))  ;; 4

       ;; Build from negative values
       (let ((bit (funcall 'neovm--fw3-build [-5 10 -3 8 -1])))
         (list
          (funcall 'neovm--fw3-prefix bit 5)    ;; 9
          (funcall 'neovm--fw3-range bit 1 3)   ;; 2
          (funcall 'neovm--fw3-range bit 2 4)))) ;; 15
    (fmakunbound 'neovm--fw3-update)
    (fmakunbound 'neovm--fw3-prefix)
    (fmakunbound 'neovm--fw3-build)
    (fmakunbound 'neovm--fw3-range)))"#;
    let expect = expect_test::expect![[r#""OK ((10 60 150 90) (42) (4 8 4) (9 2 15))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Point queries (get single element value)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_fenwick_point_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--fw4-update
    (lambda (bit i delta)
      (let ((n (1- (length bit))))
        (while (<= i n)
          (aset bit i (+ (aref bit i) delta))
          (setq i (+ i (logand i (- i))))))))

  (fset 'neovm--fw4-prefix
    (lambda (bit i)
      (let ((s 0))
        (while (> i 0)
          (setq s (+ s (aref bit i)))
          (setq i (- i (logand i (- i)))))
        s)))

  (fset 'neovm--fw4-point-query
    (lambda (bit i)
      "Get the value of element at index I (1-indexed).
       This is prefix(i) - prefix(i-1)."
      (- (funcall 'neovm--fw4-prefix bit i)
         (funcall 'neovm--fw4-prefix bit (1- i)))))

  (unwind-protect
      (let ((bit (make-vector 9 0)))
        ;; Array: [5, 3, 7, 2, 8, 1, 4, 6]
        (funcall 'neovm--fw4-update bit 1 5)
        (funcall 'neovm--fw4-update bit 2 3)
        (funcall 'neovm--fw4-update bit 3 7)
        (funcall 'neovm--fw4-update bit 4 2)
        (funcall 'neovm--fw4-update bit 5 8)
        (funcall 'neovm--fw4-update bit 6 1)
        (funcall 'neovm--fw4-update bit 7 4)
        (funcall 'neovm--fw4-update bit 8 6)
        (list
         ;; Point queries: get individual values back
         (funcall 'neovm--fw4-point-query bit 1)   ;; 5
         (funcall 'neovm--fw4-point-query bit 2)   ;; 3
         (funcall 'neovm--fw4-point-query bit 3)   ;; 7
         (funcall 'neovm--fw4-point-query bit 5)   ;; 8
         (funcall 'neovm--fw4-point-query bit 8)   ;; 6
         ;; After update: add 10 to position 3
         (progn (funcall 'neovm--fw4-update bit 3 10) nil)
         (funcall 'neovm--fw4-point-query bit 3)   ;; 17
         ;; Other positions unchanged
         (funcall 'neovm--fw4-point-query bit 2)   ;; 3
         (funcall 'neovm--fw4-point-query bit 4))) ;; 2
    (fmakunbound 'neovm--fw4-update)
    (fmakunbound 'neovm--fw4-prefix)
    (fmakunbound 'neovm--fw4-point-query)))"#;
    let expect = expect_test::expect![[r#""OK (5 3 7 8 6 nil 17 3 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Inverse: find index with given cumulative frequency
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_fenwick_find_index_by_cumulative() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--fw5-update
    (lambda (bit i delta)
      (let ((n (1- (length bit))))
        (while (<= i n)
          (aset bit i (+ (aref bit i) delta))
          (setq i (+ i (logand i (- i))))))))

  (fset 'neovm--fw5-prefix
    (lambda (bit i)
      (let ((s 0))
        (while (> i 0)
          (setq s (+ s (aref bit i)))
          (setq i (- i (logand i (- i)))))
        s)))

  (fset 'neovm--fw5-find
    (lambda (bit target)
      "Find the smallest index I such that prefix-sum(I) >= TARGET.
       Uses binary lifting technique. Returns nil if no such index.
       Assumes all values are non-negative."
      (let* ((n (1- (length bit)))
             (pos 0)
             (remaining target)
             ;; Find highest power of 2 <= n
             (bit-mask 1))
        (while (<= (* bit-mask 2) n)
          (setq bit-mask (* bit-mask 2)))
        ;; Binary lifting
        (while (> bit-mask 0)
          (let ((next (+ pos bit-mask)))
            (when (and (<= next n)
                       (< (aref bit next) remaining))
              (setq remaining (- remaining (aref bit next)))
              (setq pos next)))
          (setq bit-mask (/ bit-mask 2)))
        (let ((result (1+ pos)))
          (if (<= result n) result nil)))))

  (unwind-protect
      (let ((bit (make-vector 9 0)))
        ;; Cumulative frequencies: [2, 1, 3, 2, 5, 1, 4, 2]
        ;; Prefix sums: [2, 3, 6, 8, 13, 14, 18, 20]
        (funcall 'neovm--fw5-update bit 1 2)
        (funcall 'neovm--fw5-update bit 2 1)
        (funcall 'neovm--fw5-update bit 3 3)
        (funcall 'neovm--fw5-update bit 4 2)
        (funcall 'neovm--fw5-update bit 5 5)
        (funcall 'neovm--fw5-update bit 6 1)
        (funcall 'neovm--fw5-update bit 7 4)
        (funcall 'neovm--fw5-update bit 8 2)
        (list
         ;; Find index where cumulative >= target
         (funcall 'neovm--fw5-find bit 1)    ;; 1 (prefix[1]=2 >= 1)
         (funcall 'neovm--fw5-find bit 2)    ;; 1 (prefix[1]=2 >= 2)
         (funcall 'neovm--fw5-find bit 3)    ;; 2 (prefix[2]=3 >= 3)
         (funcall 'neovm--fw5-find bit 6)    ;; 3 (prefix[3]=6 >= 6)
         (funcall 'neovm--fw5-find bit 7)    ;; 4 (prefix[4]=8 >= 7)
         (funcall 'neovm--fw5-find bit 13)   ;; 5 (prefix[5]=13 >= 13)
         (funcall 'neovm--fw5-find bit 14)   ;; 6 (prefix[6]=14 >= 14)
         (funcall 'neovm--fw5-find bit 20)   ;; 8 (prefix[8]=20 >= 20)
         (funcall 'neovm--fw5-find bit 21))) ;; nil (no index)
    (fmakunbound 'neovm--fw5-update)
    (fmakunbound 'neovm--fw5-prefix)
    (fmakunbound 'neovm--fw5-find)))"#;
    let expect = expect_test::expect![[r#""OK (1 1 2 3 4 5 6 8 nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 2D Fenwick tree
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_fenwick_2d() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; 2D Fenwick tree: vector of vectors, 1-indexed.
  ;; Supports point update (x, y, delta) and prefix sum (1,1)...(x,y).

  (fset 'neovm--fw2d-make
    (lambda (rows cols)
      "Create a 2D Fenwick tree with ROWS x COLS."
      (let ((tree (make-vector (1+ rows) nil)))
        (let ((i 0))
          (while (<= i rows)
            (aset tree i (make-vector (1+ cols) 0))
            (setq i (1+ i))))
        (cons (cons rows cols) tree))))

  (fset 'neovm--fw2d-dims
    (lambda (fw) (car fw)))

  (fset 'neovm--fw2d-tree
    (lambda (fw) (cdr fw)))

  (fset 'neovm--fw2d-update
    (lambda (fw x y delta)
      "Add DELTA to position (X, Y) in 2D Fenwick tree."
      (let ((rows (car (funcall 'neovm--fw2d-dims fw)))
            (cols (cdr (funcall 'neovm--fw2d-dims fw)))
            (tree (funcall 'neovm--fw2d-tree fw)))
        (let ((i x))
          (while (<= i rows)
            (let ((j y))
              (while (<= j cols)
                (let ((row (aref tree i)))
                  (aset row j (+ (aref row j) delta)))
                (setq j (+ j (logand j (- j))))))
            (setq i (+ i (logand i (- i)))))))))

  (fset 'neovm--fw2d-prefix
    (lambda (fw x y)
      "Compute prefix sum from (1,1) to (X,Y)."
      (let ((tree (funcall 'neovm--fw2d-tree fw))
            (s 0))
        (let ((i x))
          (while (> i 0)
            (let ((j y))
              (while (> j 0)
                (setq s (+ s (aref (aref tree i) j)))
                (setq j (- j (logand j (- j))))))
            (setq i (- i (logand i (- i))))))
        s)))

  (fset 'neovm--fw2d-range
    (lambda (fw x1 y1 x2 y2)
      "Sum of elements in rectangle (X1,Y1) to (X2,Y2)."
      (- (+ (funcall 'neovm--fw2d-prefix fw x2 y2)
            (funcall 'neovm--fw2d-prefix fw (1- x1) (1- y1)))
         (funcall 'neovm--fw2d-prefix fw x2 (1- y1))
         (funcall 'neovm--fw2d-prefix fw (1- x1) y2))))

  (unwind-protect
      (let ((fw (funcall 'neovm--fw2d-make 4 4)))
        ;; Fill a 4x4 grid:
        ;; [1 2 3 4]
        ;; [5 6 7 8]
        ;; [9 10 11 12]
        ;; [13 14 15 16]
        (let ((r 1))
          (while (<= r 4)
            (let ((c 1))
              (while (<= c 4)
                (funcall 'neovm--fw2d-update fw r c
                         (+ (* (1- r) 4) c))
                (setq c (1+ c))))
            (setq r (1+ r))))
        (list
         ;; Prefix sum (1,1) = 1
         (funcall 'neovm--fw2d-prefix fw 1 1)
         ;; Prefix sum (2,2) = 1+2+5+6 = 14
         (funcall 'neovm--fw2d-prefix fw 2 2)
         ;; Prefix sum (4,4) = sum of all = 136
         (funcall 'neovm--fw2d-prefix fw 4 4)
         ;; Range (2,2) to (3,3) = 6+7+10+11 = 34
         (funcall 'neovm--fw2d-range fw 2 2 3 3)
         ;; Range (1,1) to (1,4) = 1+2+3+4 = 10
         (funcall 'neovm--fw2d-range fw 1 1 1 4)
         ;; Range (3,3) to (4,4) = 11+12+15+16 = 54
         (funcall 'neovm--fw2d-range fw 3 3 4 4)
         ;; Single element (2,3) = 7
         (funcall 'neovm--fw2d-range fw 2 3 2 3)
         ;; Update: add 100 to (2,2)
         (progn (funcall 'neovm--fw2d-update fw 2 2 100) nil)
         ;; Range (2,2) to (3,3) should now be 134
         (funcall 'neovm--fw2d-range fw 2 2 3 3)
         ;; Prefix (4,4) should now be 236
         (funcall 'neovm--fw2d-prefix fw 4 4)))
    (fmakunbound 'neovm--fw2d-make)
    (fmakunbound 'neovm--fw2d-dims)
    (fmakunbound 'neovm--fw2d-tree)
    (fmakunbound 'neovm--fw2d-update)
    (fmakunbound 'neovm--fw2d-prefix)
    (fmakunbound 'neovm--fw2d-range)))"#;
    let expect = expect_test::expect![[r#""OK (1 14 136 34 10 54 7 nil 134 236)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
