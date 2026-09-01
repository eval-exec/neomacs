//! Oracle parity tests for a segment tree implementation in Elisp:
//! build from array, range sum queries, point updates, range minimum queries,
//! lazy propagation for range updates, and merge operations.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Build segment tree from array and range sum queries
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_segment_tree_build_and_range_sum() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Segment tree stored as a vector. For n elements, we need 4n space.
  ;; tree[1] = root, tree[2i] = left child, tree[2i+1] = right child.

  (fset 'neovm--st-build
    (lambda (arr)
      "Build a segment tree for sum queries from vector ARR.
       Returns (tree . n) where tree is the internal vector."
      (let* ((n (length arr))
             (size (* 4 n))
             (tree (make-vector size 0)))
        (fset 'neovm--st-build-rec
          (lambda (node lo hi)
            (if (= lo hi)
                (aset tree node (aref arr lo))
              (let ((mid (/ (+ lo hi) 2)))
                (funcall 'neovm--st-build-rec (* 2 node) lo mid)
                (funcall 'neovm--st-build-rec (1+ (* 2 node)) (1+ mid) hi)
                (aset tree node (+ (aref tree (* 2 node))
                                    (aref tree (1+ (* 2 node)))))))))
        (when (> n 0)
          (funcall 'neovm--st-build-rec 1 0 (1- n)))
        (fmakunbound 'neovm--st-build-rec)
        (cons tree n))))

  (fset 'neovm--st-query-sum
    (lambda (st l r)
      "Query sum of elements in range [l, r] (0-indexed)."
      (let ((tree (car st))
            (n (cdr st)))
        (fset 'neovm--st-query-rec
          (lambda (node lo hi l r)
            (cond
              ((or (> l hi) (< r lo)) 0)
              ((and (<= l lo) (>= r hi)) (aref tree node))
              (t (let ((mid (/ (+ lo hi) 2)))
                   (+ (funcall 'neovm--st-query-rec (* 2 node) lo mid l r)
                      (funcall 'neovm--st-query-rec (1+ (* 2 node)) (1+ mid) hi l r)))))))
        (let ((result (funcall 'neovm--st-query-rec 1 0 (1- n) l r)))
          (fmakunbound 'neovm--st-query-rec)
          result))))

  (unwind-protect
      (let* ((arr [1 3 5 7 9 11 13 15])
             (st (funcall 'neovm--st-build arr)))
        (list
          ;; Full range sum: 1+3+5+7+9+11+13+15 = 64
          (funcall 'neovm--st-query-sum st 0 7)
          ;; Single element
          (funcall 'neovm--st-query-sum st 0 0)
          (funcall 'neovm--st-query-sum st 3 3)
          (funcall 'neovm--st-query-sum st 7 7)
          ;; Various ranges
          (funcall 'neovm--st-query-sum st 0 3)   ;; 1+3+5+7 = 16
          (funcall 'neovm--st-query-sum st 4 7)   ;; 9+11+13+15 = 48
          (funcall 'neovm--st-query-sum st 2 5)   ;; 5+7+9+11 = 32
          (funcall 'neovm--st-query-sum st 1 6)   ;; 3+5+7+9+11+13 = 48
          ;; Adjacent pairs
          (funcall 'neovm--st-query-sum st 0 1)   ;; 4
          (funcall 'neovm--st-query-sum st 6 7))) ;; 28
    (fmakunbound 'neovm--st-build)
    (fmakunbound 'neovm--st-query-sum)))"#;
    let expect = expect_test::expect![[r#""OK (64 1 7 15 16 48 32 48 4 28)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Point updates on segment tree
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_segment_tree_point_update() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--st2-build
    (lambda (arr)
      (let* ((n (length arr))
             (size (* 4 n))
             (tree (make-vector size 0)))
        (fset 'neovm--st2-build-r
          (lambda (nd lo hi)
            (if (= lo hi)
                (aset tree nd (aref arr lo))
              (let ((mid (/ (+ lo hi) 2)))
                (funcall 'neovm--st2-build-r (* 2 nd) lo mid)
                (funcall 'neovm--st2-build-r (1+ (* 2 nd)) (1+ mid) hi)
                (aset tree nd (+ (aref tree (* 2 nd))
                                  (aref tree (1+ (* 2 nd)))))))))
        (when (> n 0) (funcall 'neovm--st2-build-r 1 0 (1- n)))
        (fmakunbound 'neovm--st2-build-r)
        (cons tree n))))

  (fset 'neovm--st2-update
    (lambda (st idx val)
      "Set element at IDX to VAL and update tree."
      (let ((tree (car st))
            (n (cdr st)))
        (fset 'neovm--st2-update-r
          (lambda (nd lo hi idx val)
            (if (= lo hi)
                (aset tree nd val)
              (let ((mid (/ (+ lo hi) 2)))
                (if (<= idx mid)
                    (funcall 'neovm--st2-update-r (* 2 nd) lo mid idx val)
                  (funcall 'neovm--st2-update-r (1+ (* 2 nd)) (1+ mid) hi idx val))
                (aset tree nd (+ (aref tree (* 2 nd))
                                  (aref tree (1+ (* 2 nd)))))))))
        (funcall 'neovm--st2-update-r 1 0 (1- n) idx val)
        (fmakunbound 'neovm--st2-update-r))))

  (fset 'neovm--st2-query
    (lambda (st l r)
      (let ((tree (car st)) (n (cdr st)))
        (fset 'neovm--st2-q-r
          (lambda (nd lo hi l r)
            (cond
              ((or (> l hi) (< r lo)) 0)
              ((and (<= l lo) (>= r hi)) (aref tree nd))
              (t (let ((mid (/ (+ lo hi) 2)))
                   (+ (funcall 'neovm--st2-q-r (* 2 nd) lo mid l r)
                      (funcall 'neovm--st2-q-r (1+ (* 2 nd)) (1+ mid) hi l r)))))))
        (let ((res (funcall 'neovm--st2-q-r 1 0 (1- n) l r)))
          (fmakunbound 'neovm--st2-q-r)
          res))))

  (unwind-protect
      (let* ((arr [10 20 30 40 50])
             (st (funcall 'neovm--st2-build arr)))
        (let ((results nil))
          ;; Initial sum
          (setq results (cons (funcall 'neovm--st2-query st 0 4) results))  ;; 150
          ;; Update index 2: 30 -> 100
          (funcall 'neovm--st2-update st 2 100)
          (setq results (cons (funcall 'neovm--st2-query st 0 4) results))  ;; 220
          (setq results (cons (funcall 'neovm--st2-query st 2 2) results))  ;; 100
          (setq results (cons (funcall 'neovm--st2-query st 0 2) results))  ;; 130
          ;; Update index 0: 10 -> 0
          (funcall 'neovm--st2-update st 0 0)
          (setq results (cons (funcall 'neovm--st2-query st 0 4) results))  ;; 210
          (setq results (cons (funcall 'neovm--st2-query st 0 0) results))  ;; 0
          ;; Update index 4: 50 -> 1
          (funcall 'neovm--st2-update st 4 1)
          (setq results (cons (funcall 'neovm--st2-query st 0 4) results))  ;; 161
          (setq results (cons (funcall 'neovm--st2-query st 3 4) results))  ;; 41
          ;; Multiple updates then query
          (funcall 'neovm--st2-update st 1 1)
          (funcall 'neovm--st2-update st 3 1)
          (setq results (cons (funcall 'neovm--st2-query st 0 4) results))  ;; 103
          (nreverse results)))
    (fmakunbound 'neovm--st2-build)
    (fmakunbound 'neovm--st2-update)
    (fmakunbound 'neovm--st2-query)))"#;
    let expect = expect_test::expect![[r#""OK (150 220 100 130 210 0 161 41 103)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Range minimum query segment tree
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_segment_tree_range_minimum() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--stmin-build
    (lambda (arr)
      "Build segment tree for range minimum queries."
      (let* ((n (length arr))
             (size (* 4 n))
             (tree (make-vector size most-positive-fixnum)))
        (fset 'neovm--stmin-build-r
          (lambda (nd lo hi)
            (if (= lo hi)
                (aset tree nd (aref arr lo))
              (let ((mid (/ (+ lo hi) 2)))
                (funcall 'neovm--stmin-build-r (* 2 nd) lo mid)
                (funcall 'neovm--stmin-build-r (1+ (* 2 nd)) (1+ mid) hi)
                (aset tree nd (min (aref tree (* 2 nd))
                                    (aref tree (1+ (* 2 nd)))))))))
        (when (> n 0) (funcall 'neovm--stmin-build-r 1 0 (1- n)))
        (fmakunbound 'neovm--stmin-build-r)
        (cons tree n))))

  (fset 'neovm--stmin-query
    (lambda (st l r)
      "Query minimum in range [l, r]."
      (let ((tree (car st)) (n (cdr st)))
        (fset 'neovm--stmin-q-r
          (lambda (nd lo hi l r)
            (cond
              ((or (> l hi) (< r lo)) most-positive-fixnum)
              ((and (<= l lo) (>= r hi)) (aref tree nd))
              (t (let ((mid (/ (+ lo hi) 2)))
                   (min (funcall 'neovm--stmin-q-r (* 2 nd) lo mid l r)
                        (funcall 'neovm--stmin-q-r (1+ (* 2 nd)) (1+ mid) hi l r)))))))
        (let ((res (funcall 'neovm--stmin-q-r 1 0 (1- n) l r)))
          (fmakunbound 'neovm--stmin-q-r)
          res))))

  (fset 'neovm--stmin-update
    (lambda (st idx val)
      "Point update: set index IDX to VAL."
      (let ((tree (car st)) (n (cdr st)))
        (fset 'neovm--stmin-u-r
          (lambda (nd lo hi idx val)
            (if (= lo hi)
                (aset tree nd val)
              (let ((mid (/ (+ lo hi) 2)))
                (if (<= idx mid)
                    (funcall 'neovm--stmin-u-r (* 2 nd) lo mid idx val)
                  (funcall 'neovm--stmin-u-r (1+ (* 2 nd)) (1+ mid) hi idx val))
                (aset tree nd (min (aref tree (* 2 nd))
                                    (aref tree (1+ (* 2 nd)))))))))
        (funcall 'neovm--stmin-u-r 1 0 (1- n) idx val)
        (fmakunbound 'neovm--stmin-u-r))))

  (unwind-protect
      (let* ((arr [5 2 8 1 9 3 7 4 6])
             (st (funcall 'neovm--stmin-build arr)))
        (list
          ;; Full range min: 1
          (funcall 'neovm--stmin-query st 0 8)
          ;; Single elements
          (funcall 'neovm--stmin-query st 0 0)   ;; 5
          (funcall 'neovm--stmin-query st 3 3)   ;; 1
          ;; Various ranges
          (funcall 'neovm--stmin-query st 0 3)   ;; min(5,2,8,1) = 1
          (funcall 'neovm--stmin-query st 4 8)   ;; min(9,3,7,4,6) = 3
          (funcall 'neovm--stmin-query st 0 1)   ;; min(5,2) = 2
          (funcall 'neovm--stmin-query st 5 7)   ;; min(3,7,4) = 3
          (funcall 'neovm--stmin-query st 2 6)   ;; min(8,1,9,3,7) = 1
          ;; Update: change min element
          (progn (funcall 'neovm--stmin-update st 3 10)  ;; 1 -> 10
                 (funcall 'neovm--stmin-query st 0 8))   ;; now min is 2
          (funcall 'neovm--stmin-query st 0 3)   ;; min(5,2,8,10) = 2
          (funcall 'neovm--stmin-query st 3 5)   ;; min(10,9,3) = 3
          ;; Update to create new minimum
          (progn (funcall 'neovm--stmin-update st 6 0)
                 (funcall 'neovm--stmin-query st 0 8))   ;; 0
          (funcall 'neovm--stmin-query st 5 7))) ;; min(3,0,4) = 0
    (fmakunbound 'neovm--stmin-build)
    (fmakunbound 'neovm--stmin-query)
    (fmakunbound 'neovm--stmin-update)))"#;
    let expect = expect_test::expect![[r#""OK (1 5 1 1 3 2 3 1 2 2 3 0 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Lazy propagation for range updates (range add + range sum query)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_segment_tree_lazy_propagation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Lazy segment tree: supports range-add and range-sum.
  ;; tree = sum values, lazy = pending additions.

  (fset 'neovm--stlazy-build
    (lambda (arr)
      (let* ((n (length arr))
             (size (* 4 n))
             (tree (make-vector size 0))
             (lazy (make-vector size 0)))
        (fset 'neovm--stlazy-b-r
          (lambda (nd lo hi)
            (if (= lo hi)
                (aset tree nd (aref arr lo))
              (let ((mid (/ (+ lo hi) 2)))
                (funcall 'neovm--stlazy-b-r (* 2 nd) lo mid)
                (funcall 'neovm--stlazy-b-r (1+ (* 2 nd)) (1+ mid) hi)
                (aset tree nd (+ (aref tree (* 2 nd))
                                  (aref tree (1+ (* 2 nd)))))))))
        (when (> n 0) (funcall 'neovm--stlazy-b-r 1 0 (1- n)))
        (fmakunbound 'neovm--stlazy-b-r)
        (list tree lazy n))))

  (fset 'neovm--stlazy-push
    (lambda (st nd lo hi)
      "Push lazy values down to children."
      (let ((tree (nth 0 st))
            (lazy (nth 1 st)))
        (when (/= (aref lazy nd) 0)
          (let ((mid (/ (+ lo hi) 2))
                (val (aref lazy nd)))
            ;; Update children's tree values
            (aset tree (* 2 nd)
                  (+ (aref tree (* 2 nd)) (* val (1+ (- mid lo)))))
            (aset tree (1+ (* 2 nd))
                  (+ (aref tree (1+ (* 2 nd))) (* val (- hi mid))))
            ;; Propagate lazy to children
            (aset lazy (* 2 nd) (+ (aref lazy (* 2 nd)) val))
            (aset lazy (1+ (* 2 nd)) (+ (aref lazy (1+ (* 2 nd))) val))
            ;; Clear current lazy
            (aset lazy nd 0))))))

  (fset 'neovm--stlazy-range-add
    (lambda (st l r val)
      "Add VAL to all elements in [l, r]."
      (let ((tree (nth 0 st))
            (lazy (nth 1 st))
            (n (nth 2 st)))
        (fset 'neovm--stlazy-ra-r
          (lambda (nd lo hi l r val)
            (cond
              ((or (> l hi) (< r lo)) nil)
              ((and (<= l lo) (>= r hi))
               (aset tree nd (+ (aref tree nd) (* val (1+ (- hi lo)))))
               (aset lazy nd (+ (aref lazy nd) val)))
              (t
               (funcall 'neovm--stlazy-push st nd lo hi)
               (let ((mid (/ (+ lo hi) 2)))
                 (funcall 'neovm--stlazy-ra-r (* 2 nd) lo mid l r val)
                 (funcall 'neovm--stlazy-ra-r (1+ (* 2 nd)) (1+ mid) hi l r val)
                 (aset tree nd (+ (aref tree (* 2 nd))
                                   (aref tree (1+ (* 2 nd))))))))))
        (funcall 'neovm--stlazy-ra-r 1 0 (1- n) l r val)
        (fmakunbound 'neovm--stlazy-ra-r))))

  (fset 'neovm--stlazy-query-sum
    (lambda (st l r)
      "Query sum of [l, r] with lazy propagation."
      (let ((tree (nth 0 st))
            (n (nth 2 st)))
        (fset 'neovm--stlazy-qs-r
          (lambda (nd lo hi l r)
            (cond
              ((or (> l hi) (< r lo)) 0)
              ((and (<= l lo) (>= r hi)) (aref tree nd))
              (t
               (funcall 'neovm--stlazy-push st nd lo hi)
               (let ((mid (/ (+ lo hi) 2)))
                 (+ (funcall 'neovm--stlazy-qs-r (* 2 nd) lo mid l r)
                    (funcall 'neovm--stlazy-qs-r (1+ (* 2 nd)) (1+ mid) hi l r)))))))
        (let ((res (funcall 'neovm--stlazy-qs-r 1 0 (1- n) l r)))
          (fmakunbound 'neovm--stlazy-qs-r)
          res))))

  (unwind-protect
      (let* ((arr [1 2 3 4 5 6 7 8])
             (st (funcall 'neovm--stlazy-build arr)))
        (let ((results nil))
          ;; Initial sum: 1+2+3+4+5+6+7+8 = 36
          (setq results (cons (funcall 'neovm--stlazy-query-sum st 0 7) results))
          ;; Add 10 to range [2, 5]: array becomes [1,2,13,14,15,16,7,8]
          (funcall 'neovm--stlazy-range-add st 2 5 10)
          (setq results (cons (funcall 'neovm--stlazy-query-sum st 0 7) results)) ;; 76
          (setq results (cons (funcall 'neovm--stlazy-query-sum st 2 5) results)) ;; 58
          (setq results (cons (funcall 'neovm--stlazy-query-sum st 0 1) results)) ;; 3
          (setq results (cons (funcall 'neovm--stlazy-query-sum st 6 7) results)) ;; 15
          ;; Add 5 to range [0, 7]: all elements +5
          (funcall 'neovm--stlazy-range-add st 0 7 5)
          (setq results (cons (funcall 'neovm--stlazy-query-sum st 0 7) results)) ;; 116
          ;; Add -3 to range [3, 4]
          (funcall 'neovm--stlazy-range-add st 3 4 -3)
          (setq results (cons (funcall 'neovm--stlazy-query-sum st 3 4) results)) ;; 31
          (setq results (cons (funcall 'neovm--stlazy-query-sum st 0 7) results)) ;; 110
          ;; Single element query after lazy updates
          (setq results (cons (funcall 'neovm--stlazy-query-sum st 0 0) results)) ;; 6
          (setq results (cons (funcall 'neovm--stlazy-query-sum st 7 7) results)) ;; 13
          (nreverse results)))
    (fmakunbound 'neovm--stlazy-build)
    (fmakunbound 'neovm--stlazy-push)
    (fmakunbound 'neovm--stlazy-range-add)
    (fmakunbound 'neovm--stlazy-query-sum)))"#;
    let expect = expect_test::expect![[r#""OK (36 76 58 3 15 116 33 110 6 13)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Segment tree merge: combine two segment trees
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_segment_tree_merge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Merge two segment trees by summing corresponding elements
    let form = r#"(progn
  (fset 'neovm--stm-build
    (lambda (arr)
      (let* ((n (length arr))
             (size (* 4 n))
             (tree (make-vector size 0)))
        (fset 'neovm--stm-b-r
          (lambda (nd lo hi)
            (if (= lo hi)
                (aset tree nd (aref arr lo))
              (let ((mid (/ (+ lo hi) 2)))
                (funcall 'neovm--stm-b-r (* 2 nd) lo mid)
                (funcall 'neovm--stm-b-r (1+ (* 2 nd)) (1+ mid) hi)
                (aset tree nd (+ (aref tree (* 2 nd))
                                  (aref tree (1+ (* 2 nd)))))))))
        (when (> n 0) (funcall 'neovm--stm-b-r 1 0 (1- n)))
        (fmakunbound 'neovm--stm-b-r)
        (cons tree n))))

  (fset 'neovm--stm-query
    (lambda (st l r)
      (let ((tree (car st)) (n (cdr st)))
        (fset 'neovm--stm-q-r
          (lambda (nd lo hi l r)
            (cond
              ((or (> l hi) (< r lo)) 0)
              ((and (<= l lo) (>= r hi)) (aref tree nd))
              (t (let ((mid (/ (+ lo hi) 2)))
                   (+ (funcall 'neovm--stm-q-r (* 2 nd) lo mid l r)
                      (funcall 'neovm--stm-q-r (1+ (* 2 nd)) (1+ mid) hi l r)))))))
        (let ((res (funcall 'neovm--stm-q-r 1 0 (1- n) l r)))
          (fmakunbound 'neovm--stm-q-r)
          res))))

  (fset 'neovm--stm-merge
    (lambda (st1 st2)
      "Merge two segment trees of same size by summing values."
      (let* ((n (cdr st1))
             (size (* 4 n))
             (t1 (car st1))
             (t2 (car st2))
             (merged (make-vector size 0)))
        ;; Element-wise merge: add corresponding tree nodes
        (dotimes (i size)
          (aset merged i (+ (aref t1 i) (aref t2 i))))
        (cons merged n))))

  (unwind-protect
      (let* ((arr1 [1 2 3 4 5])
             (arr2 [10 20 30 40 50])
             (st1 (funcall 'neovm--stm-build arr1))
             (st2 (funcall 'neovm--stm-build arr2)))
        (let ((merged (funcall 'neovm--stm-merge st1 st2)))
          (list
            ;; Original trees
            (funcall 'neovm--stm-query st1 0 4)   ;; 15
            (funcall 'neovm--stm-query st2 0 4)   ;; 150
            ;; Merged tree: sum of both
            (funcall 'neovm--stm-query merged 0 4)  ;; 165
            ;; Range queries on merged
            (funcall 'neovm--stm-query merged 0 0)   ;; 11
            (funcall 'neovm--stm-query merged 2 2)   ;; 33
            (funcall 'neovm--stm-query merged 0 2)   ;; 11+22+33 = 66
            (funcall 'neovm--stm-query merged 3 4)   ;; 44+55 = 99
            (funcall 'neovm--stm-query merged 1 3)   ;; 22+33+44 = 99
            ;; Merge of two identical trees = 2x
            (let ((doubled (funcall 'neovm--stm-merge st1 st1)))
              (list
                (funcall 'neovm--stm-query doubled 0 4)  ;; 30
                (funcall 'neovm--stm-query doubled 0 0)  ;; 2
                (funcall 'neovm--stm-query doubled 2 4))))))  ;; 24
    (fmakunbound 'neovm--stm-build)
    (fmakunbound 'neovm--stm-query)
    (fmakunbound 'neovm--stm-merge)))"#;
    let expect = expect_test::expect![[r#""OK (15 150 165 11 33 66 99 99 (30 2 24))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Range max query with point updates
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_segment_tree_range_max_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--stmax-build
    (lambda (arr)
      (let* ((n (length arr))
             (size (* 4 n))
             (tree (make-vector size most-negative-fixnum)))
        (fset 'neovm--stmax-b-r
          (lambda (nd lo hi)
            (if (= lo hi)
                (aset tree nd (aref arr lo))
              (let ((mid (/ (+ lo hi) 2)))
                (funcall 'neovm--stmax-b-r (* 2 nd) lo mid)
                (funcall 'neovm--stmax-b-r (1+ (* 2 nd)) (1+ mid) hi)
                (aset tree nd (max (aref tree (* 2 nd))
                                    (aref tree (1+ (* 2 nd)))))))))
        (when (> n 0) (funcall 'neovm--stmax-b-r 1 0 (1- n)))
        (fmakunbound 'neovm--stmax-b-r)
        (cons tree n))))

  (fset 'neovm--stmax-query
    (lambda (st l r)
      (let ((tree (car st)) (n (cdr st)))
        (fset 'neovm--stmax-q-r
          (lambda (nd lo hi l r)
            (cond
              ((or (> l hi) (< r lo)) most-negative-fixnum)
              ((and (<= l lo) (>= r hi)) (aref tree nd))
              (t (let ((mid (/ (+ lo hi) 2)))
                   (max (funcall 'neovm--stmax-q-r (* 2 nd) lo mid l r)
                        (funcall 'neovm--stmax-q-r (1+ (* 2 nd)) (1+ mid) hi l r)))))))
        (let ((res (funcall 'neovm--stmax-q-r 1 0 (1- n) l r)))
          (fmakunbound 'neovm--stmax-q-r)
          res))))

  (fset 'neovm--stmax-update
    (lambda (st idx val)
      (let ((tree (car st)) (n (cdr st)))
        (fset 'neovm--stmax-u-r
          (lambda (nd lo hi idx val)
            (if (= lo hi)
                (aset tree nd val)
              (let ((mid (/ (+ lo hi) 2)))
                (if (<= idx mid)
                    (funcall 'neovm--stmax-u-r (* 2 nd) lo mid idx val)
                  (funcall 'neovm--stmax-u-r (1+ (* 2 nd)) (1+ mid) hi idx val))
                (aset tree nd (max (aref tree (* 2 nd))
                                    (aref tree (1+ (* 2 nd)))))))))
        (funcall 'neovm--stmax-u-r 1 0 (1- n) idx val)
        (fmakunbound 'neovm--stmax-u-r))))

  (unwind-protect
      (let* ((arr [3 1 4 1 5 9 2 6 5 3])
             (st (funcall 'neovm--stmax-build arr)))
        (let ((results nil))
          ;; Full range max
          (setq results (cons (funcall 'neovm--stmax-query st 0 9) results))  ;; 9
          ;; Various ranges
          (setq results (cons (funcall 'neovm--stmax-query st 0 4) results))  ;; 5
          (setq results (cons (funcall 'neovm--stmax-query st 5 9) results))  ;; 9
          (setq results (cons (funcall 'neovm--stmax-query st 2 7) results))  ;; 9
          (setq results (cons (funcall 'neovm--stmax-query st 0 0) results))  ;; 3
          (setq results (cons (funcall 'neovm--stmax-query st 8 9) results))  ;; 5
          ;; Update: set the max element to 0
          (funcall 'neovm--stmax-update st 5 0)  ;; 9 -> 0
          (setq results (cons (funcall 'neovm--stmax-query st 0 9) results))  ;; now 6
          (setq results (cons (funcall 'neovm--stmax-query st 4 6) results))  ;; max(5,0,2) = 5
          ;; Set a new global max
          (funcall 'neovm--stmax-update st 3 100)
          (setq results (cons (funcall 'neovm--stmax-query st 0 9) results))  ;; 100
          (setq results (cons (funcall 'neovm--stmax-query st 0 3) results))  ;; 100
          (setq results (cons (funcall 'neovm--stmax-query st 4 9) results))  ;; 6
          (nreverse results)))
    (fmakunbound 'neovm--stmax-build)
    (fmakunbound 'neovm--stmax-query)
    (fmakunbound 'neovm--stmax-update)))"#;
    let expect = expect_test::expect![[r#""OK (9 5 9 9 3 5 6 5 100 100 6)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Segment tree for count of elements in range (frequency counting)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_segment_tree_frequency_counting() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use segment tree as a frequency array: index = value, tree stores count
    let form = r#"(progn
  (fset 'neovm--stf-create
    (lambda (max-val)
      "Create frequency segment tree for values 0..MAX-VAL."
      (let* ((n (1+ max-val))
             (size (* 4 n))
             (tree (make-vector size 0)))
        (cons tree n))))

  (fset 'neovm--stf-add
    (lambda (st val)
      "Increment frequency of VAL by 1."
      (let ((tree (car st)) (n (cdr st)))
        (fset 'neovm--stf-a-r
          (lambda (nd lo hi idx)
            (if (= lo hi)
                (aset tree nd (1+ (aref tree nd)))
              (let ((mid (/ (+ lo hi) 2)))
                (if (<= idx mid)
                    (funcall 'neovm--stf-a-r (* 2 nd) lo mid idx)
                  (funcall 'neovm--stf-a-r (1+ (* 2 nd)) (1+ mid) hi idx))
                (aset tree nd (+ (aref tree (* 2 nd))
                                  (aref tree (1+ (* 2 nd)))))))))
        (funcall 'neovm--stf-a-r 1 0 (1- n) val)
        (fmakunbound 'neovm--stf-a-r))))

  (fset 'neovm--stf-count-range
    (lambda (st l r)
      "Count elements with value in [l, r]."
      (let ((tree (car st)) (n (cdr st)))
        (fset 'neovm--stf-cr-r
          (lambda (nd lo hi l r)
            (cond
              ((or (> l hi) (< r lo)) 0)
              ((and (<= l lo) (>= r hi)) (aref tree nd))
              (t (let ((mid (/ (+ lo hi) 2)))
                   (+ (funcall 'neovm--stf-cr-r (* 2 nd) lo mid l r)
                      (funcall 'neovm--stf-cr-r (1+ (* 2 nd)) (1+ mid) hi l r)))))))
        (let ((res (funcall 'neovm--stf-cr-r 1 0 (1- n) l r)))
          (fmakunbound 'neovm--stf-cr-r)
          res))))

  (unwind-protect
      (let ((st (funcall 'neovm--stf-create 20))
            (data '(3 7 2 5 3 8 1 5 9 3 7 2 15 18 3)))
        ;; Insert all data points
        (dolist (v data)
          (funcall 'neovm--stf-add st v))
        (list
          ;; Count of all elements (0-20)
          (funcall 'neovm--stf-count-range st 0 20)
          ;; Count of 3s (should be 4)
          (funcall 'neovm--stf-count-range st 3 3)
          ;; Count in range [1, 5]
          (funcall 'neovm--stf-count-range st 1 5)
          ;; Count in range [6, 10]
          (funcall 'neovm--stf-count-range st 6 10)
          ;; Count of values >= 10
          (funcall 'neovm--stf-count-range st 10 20)
          ;; Count of values < 5
          (funcall 'neovm--stf-count-range st 0 4)
          ;; No elements with value 0
          (funcall 'neovm--stf-count-range st 0 0)
          ;; Elements with value 4 (none)
          (funcall 'neovm--stf-count-range st 4 4)
          ;; Add more and recheck
          (progn
            (funcall 'neovm--stf-add st 3)
            (funcall 'neovm--stf-add st 0)
            (funcall 'neovm--stf-add st 20)
            (list
              (funcall 'neovm--stf-count-range st 3 3)   ;; 5
              (funcall 'neovm--stf-count-range st 0 0)   ;; 1
              (funcall 'neovm--stf-count-range st 20 20) ;; 1
              (funcall 'neovm--stf-count-range st 0 20)))))  ;; 18
    (fmakunbound 'neovm--stf-create)
    (fmakunbound 'neovm--stf-add)
    (fmakunbound 'neovm--stf-count-range)))"#;
    let expect = expect_test::expect![[r#""OK (15 4 9 4 2 7 0 0 (5 1 1 18))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
