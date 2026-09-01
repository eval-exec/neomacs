//! Oracle parity tests for set operation algorithms:
//! union, intersection, difference, symmetric difference, power set,
//! subset checking, set equality, multi-set (bag) operations,
//! and Jaccard similarity coefficient.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Set union, intersection, difference using sorted lists
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_setops_union_intersection_difference() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-set-union
    (lambda (a b)
      "Union of two sorted lists (no duplicates)."
      (cond
        ((null a) b)
        ((null b) a)
        ((< (car a) (car b))
         (cons (car a) (funcall 'neovm--test-set-union (cdr a) b)))
        ((> (car a) (car b))
         (cons (car b) (funcall 'neovm--test-set-union a (cdr b))))
        (t ; equal
         (cons (car a) (funcall 'neovm--test-set-union (cdr a) (cdr b)))))))

  (fset 'neovm--test-set-intersect
    (lambda (a b)
      "Intersection of two sorted lists."
      (cond
        ((or (null a) (null b)) nil)
        ((< (car a) (car b))
         (funcall 'neovm--test-set-intersect (cdr a) b))
        ((> (car a) (car b))
         (funcall 'neovm--test-set-intersect a (cdr b)))
        (t
         (cons (car a) (funcall 'neovm--test-set-intersect (cdr a) (cdr b)))))))

  (fset 'neovm--test-set-diff
    (lambda (a b)
      "Set difference A - B for sorted lists."
      (cond
        ((null a) nil)
        ((null b) a)
        ((< (car a) (car b))
         (cons (car a) (funcall 'neovm--test-set-diff (cdr a) b)))
        ((> (car a) (car b))
         (funcall 'neovm--test-set-diff a (cdr b)))
        (t
         (funcall 'neovm--test-set-diff (cdr a) (cdr b))))))

  (unwind-protect
      (let ((a '(1 3 5 7 9 11 13))
            (b '(2 3 5 8 11 14)))
        (list
          (funcall 'neovm--test-set-union a b)
          (funcall 'neovm--test-set-intersect a b)
          (funcall 'neovm--test-set-diff a b)
          (funcall 'neovm--test-set-diff b a)
          ;; Edge cases
          (funcall 'neovm--test-set-union nil a)
          (funcall 'neovm--test-set-intersect nil b)
          (funcall 'neovm--test-set-diff a nil)
          (funcall 'neovm--test-set-diff nil a)))
    (fmakunbound 'neovm--test-set-union)
    (fmakunbound 'neovm--test-set-intersect)
    (fmakunbound 'neovm--test-set-diff)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 5 7 8 9 11 13 14) (3 5 11) (1 7 9 13) (2 8 14) (1 3 5 7 9 11 13) nil (1 3 5 7 9 11 13) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Symmetric difference
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_setops_symmetric_difference() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-set-symdiff
    (lambda (a b)
      "Symmetric difference: elements in exactly one of A or B."
      (let ((result nil)
            (aa (copy-sequence a))
            (bb (copy-sequence b)))
        ;; Sort both
        (setq aa (sort aa #'<))
        (setq bb (sort bb #'<))
        ;; Merge-walk
        (while (or aa bb)
          (cond
            ((null aa)
             (setq result (append (nreverse bb) result))
             (setq bb nil))
            ((null bb)
             (setq result (append (nreverse aa) result))
             (setq aa nil))
            ((< (car aa) (car bb))
             (setq result (cons (car aa) result))
             (setq aa (cdr aa)))
            ((> (car aa) (car bb))
             (setq result (cons (car bb) result))
             (setq bb (cdr bb)))
            (t ; equal — skip both
             (setq aa (cdr aa))
             (setq bb (cdr bb)))))
        (sort result #'<))))

  (unwind-protect
      (list
        (funcall 'neovm--test-set-symdiff '(1 2 3 4 5) '(3 4 5 6 7))
        (funcall 'neovm--test-set-symdiff '(10 20 30) '(10 20 30))
        (funcall 'neovm--test-set-symdiff '(1 2 3) nil)
        (funcall 'neovm--test-set-symdiff nil '(4 5 6))
        ;; Verify: symdiff = union - intersection
        (let ((a '(1 3 5 7 9))
              (b '(2 3 6 7 10)))
          (equal (funcall 'neovm--test-set-symdiff a b)
                 (let ((union-ab nil) (inter-ab nil))
                   ;; Naive union and intersection
                   (dolist (x a) (unless (memq x union-ab) (setq union-ab (cons x union-ab))))
                   (dolist (x b) (unless (memq x union-ab) (setq union-ab (cons x union-ab))))
                   (dolist (x a) (when (memq x b) (setq inter-ab (cons x inter-ab))))
                   ;; union - intersection
                   (sort (let ((r nil))
                           (dolist (x union-ab)
                             (unless (memq x inter-ab) (setq r (cons x r))))
                           r)
                         #'<)))))
    (fmakunbound 'neovm--test-set-symdiff)))"#;
    let expect = expect_test::expect![[r#""OK ((1 2 6 7) nil (1 2 3) (4 5 6) t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Power set generation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_setops_power_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-power-set
    (lambda (s)
      "Generate power set of list S using binary counting."
      (let* ((n (length s))
             (total (let ((p 1)) (dotimes (_ n) (setq p (* p 2))) p))
             (result nil))
        (dotimes (mask total)
          (let ((subset nil)
                (bit 0)
                (rest s))
            (while rest
              (when (/= 0 (logand mask (ash 1 bit)))
                (setq subset (cons (car rest) subset)))
              (setq rest (cdr rest))
              (setq bit (1+ bit)))
            (setq result (cons (nreverse subset) result))))
        (nreverse result))))

  (unwind-protect
      (let ((ps3 (funcall 'neovm--test-power-set '(a b c)))
            (ps0 (funcall 'neovm--test-power-set nil))
            (ps1 (funcall 'neovm--test-power-set '(x))))
        (list
          ;; |P({a,b,c})| = 8
          (length ps3)
          ;; Empty set is in power set
          (if (member nil ps3) t nil)
          ;; Full set is in power set
          (if (member '(a b c) ps3) t nil)
          ;; |P({})| = 1
          (length ps0)
          ;; P({}) = (nil)
          ps0
          ;; |P({x})| = 2
          (length ps1)
          ps1))
    (fmakunbound 'neovm--test-power-set)))"#;
    let expect = expect_test::expect![[r#""OK (8 t t 1 (nil) 2 (nil (x)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Subset checking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_setops_subset_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-subsetp
    (lambda (a b)
      "Check if sorted list A is a subset of sorted list B."
      (cond
        ((null a) t)
        ((null b) nil)
        ((< (car a) (car b)) nil)
        ((> (car a) (car b))
         (funcall 'neovm--test-subsetp a (cdr b)))
        (t ; equal
         (funcall 'neovm--test-subsetp (cdr a) (cdr b))))))

  (fset 'neovm--test-proper-subsetp
    (lambda (a b)
      "A is a proper subset of B: A ⊂ B and A ≠ B."
      (and (funcall 'neovm--test-subsetp a b)
           (not (= (length a) (length b))))))

  (unwind-protect
      (list
        ;; {1,3,5} ⊆ {1,2,3,4,5} → t
        (funcall 'neovm--test-subsetp '(1 3 5) '(1 2 3 4 5))
        ;; {1,3,6} ⊆ {1,2,3,4,5} → nil (6 not in B)
        (funcall 'neovm--test-subsetp '(1 3 6) '(1 2 3 4 5))
        ;; {} ⊆ anything → t
        (funcall 'neovm--test-subsetp nil '(1 2 3))
        ;; anything ⊆ {} → nil (unless also empty)
        (funcall 'neovm--test-subsetp '(1) nil)
        ;; {} ⊆ {} → t
        (funcall 'neovm--test-subsetp nil nil)
        ;; A ⊆ A → t
        (funcall 'neovm--test-subsetp '(2 4 6) '(2 4 6))
        ;; Proper subset: {1,3} ⊂ {1,2,3} → t
        (funcall 'neovm--test-proper-subsetp '(1 3) '(1 2 3))
        ;; Not proper: {1,2,3} ⊂ {1,2,3} → nil
        (funcall 'neovm--test-proper-subsetp '(1 2 3) '(1 2 3)))
    (fmakunbound 'neovm--test-subsetp)
    (fmakunbound 'neovm--test-proper-subsetp)))"#;
    let expect = expect_test::expect![[r#""OK (t nil t nil t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Set equality (order-independent)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_setops_set_equality() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-set-equal
    (lambda (a b)
      "True if A and B contain the same elements (as sets, no duplicates)."
      (let ((sa (sort (copy-sequence a) #'<))
            (sb (sort (copy-sequence b) #'<)))
        (equal sa sb))))

  (fset 'neovm--test-set-equal-sym
    (lambda (a b)
      "Set equality via subset in both directions."
      (let ((sa (sort (copy-sequence a) #'<))
            (sb (sort (copy-sequence b) #'<)))
        (and (let ((result t) (aa sa))
               (while (and result aa)
                 (unless (member (car aa) sb)
                   (setq result nil))
                 (setq aa (cdr aa)))
               result)
             (let ((result t) (bb sb))
               (while (and result bb)
                 (unless (member (car bb) sa)
                   (setq result nil))
                 (setq bb (cdr bb)))
               result)))))

  (unwind-protect
      (list
        ;; Same elements, same order
        (funcall 'neovm--test-set-equal '(1 2 3) '(1 2 3))
        ;; Same elements, different order
        (funcall 'neovm--test-set-equal '(3 1 2) '(2 3 1))
        ;; Different elements
        (funcall 'neovm--test-set-equal '(1 2 3) '(1 2 4))
        ;; Different sizes
        (funcall 'neovm--test-set-equal '(1 2) '(1 2 3))
        ;; Both empty
        (funcall 'neovm--test-set-equal nil nil)
        ;; Verify both methods agree
        (let ((pairs '(((1 2 3) . (3 2 1))
                       ((5 10) . (10 5 15))
                       (nil . nil)
                       ((7) . (7)))))
          (let ((all-agree t))
            (dolist (p pairs)
              (unless (eq (funcall 'neovm--test-set-equal (car p) (cdr p))
                          (funcall 'neovm--test-set-equal-sym (car p) (cdr p)))
                (setq all-agree nil)))
            all-agree)))
    (fmakunbound 'neovm--test-set-equal)
    (fmakunbound 'neovm--test-set-equal-sym)))"#;
    let expect = expect_test::expect![[r#""OK (t t nil nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multi-set (bag) operations with element counts
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_setops_multiset_bag_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-bag-from-list
    (lambda (lst)
      "Build a bag (hash-table of element → count) from LST."
      (let ((bag (make-hash-table :test 'equal)))
        (dolist (x lst)
          (puthash x (1+ (gethash x bag 0)) bag))
        bag)))

  (fset 'neovm--test-bag-to-sorted-alist
    (lambda (bag)
      "Convert bag hash-table to sorted alist for comparison."
      (let ((result nil))
        (maphash (lambda (k v) (setq result (cons (cons k v) result))) bag)
        (sort result (lambda (a b) (< (car a) (car b)))))))

  (fset 'neovm--test-bag-union
    (lambda (a b)
      "Multi-set union: max count of each element."
      (let ((result (make-hash-table :test 'equal)))
        (maphash (lambda (k v) (puthash k v result)) a)
        (maphash (lambda (k v)
                   (puthash k (max v (gethash k result 0)) result))
                 b)
        result)))

  (fset 'neovm--test-bag-intersect
    (lambda (a b)
      "Multi-set intersection: min count of each shared element."
      (let ((result (make-hash-table :test 'equal)))
        (maphash (lambda (k va)
                   (let ((vb (gethash k b 0)))
                     (when (> vb 0)
                       (puthash k (min va vb) result))))
                 a)
        result)))

  (fset 'neovm--test-bag-sum
    (lambda (a b)
      "Multi-set sum: add counts."
      (let ((result (make-hash-table :test 'equal)))
        (maphash (lambda (k v) (puthash k v result)) a)
        (maphash (lambda (k v)
                   (puthash k (+ v (gethash k result 0)) result))
                 b)
        result)))

  (unwind-protect
      (let* ((lst-a '(1 1 2 3 3 3 4))
             (lst-b '(1 2 2 3 5))
             (bag-a (funcall 'neovm--test-bag-from-list lst-a))
             (bag-b (funcall 'neovm--test-bag-from-list lst-b))
             (union (funcall 'neovm--test-bag-union bag-a bag-b))
             (inter (funcall 'neovm--test-bag-intersect bag-a bag-b))
             (bsum  (funcall 'neovm--test-bag-sum bag-a bag-b)))
        (list
          (funcall 'neovm--test-bag-to-sorted-alist bag-a)
          (funcall 'neovm--test-bag-to-sorted-alist bag-b)
          (funcall 'neovm--test-bag-to-sorted-alist union)
          (funcall 'neovm--test-bag-to-sorted-alist inter)
          (funcall 'neovm--test-bag-to-sorted-alist bsum)))
    (fmakunbound 'neovm--test-bag-from-list)
    (fmakunbound 'neovm--test-bag-to-sorted-alist)
    (fmakunbound 'neovm--test-bag-union)
    (fmakunbound 'neovm--test-bag-intersect)
    (fmakunbound 'neovm--test-bag-sum)))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 . 2) (2 . 1) (3 . 3) (4 . 1)) ((1 . 1) (2 . 2) (3 . 1) (5 . 1)) ((1 . 2) (2 . 2) (3 . 3) (4 . 1) (5 . 1)) ((1 . 1) (2 . 1) (3 . 1)) ((1 . 3) (2 . 3) (3 . 4) (4 . 1) (5 . 1)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Jaccard similarity coefficient
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_setops_jaccard_similarity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-jaccard
    (lambda (a b)
      "Jaccard similarity: |A∩B| / |A∪B|. Returns float. 0.0 if both empty."
      (let ((set-a (let ((h (make-hash-table :test 'equal)))
                     (dolist (x a) (puthash x t h)) h))
            (set-b (let ((h (make-hash-table :test 'equal)))
                     (dolist (x b) (puthash x t h)) h)))
        (let ((inter-count 0)
              (union-count 0))
          ;; Count union by merging both into a single hash table
          (let ((all (make-hash-table :test 'equal)))
            (maphash (lambda (k _) (puthash k t all)) set-a)
            (maphash (lambda (k _) (puthash k t all)) set-b)
            (setq union-count (hash-table-count all)))
          ;; Count intersection
          (maphash (lambda (k _)
                     (when (gethash k set-b)
                       (setq inter-count (1+ inter-count))))
                   set-a)
          (if (= union-count 0) 0.0
            (/ (float inter-count) (float union-count)))))))

  (unwind-protect
      (list
        ;; Identical sets → 1.0
        (funcall 'neovm--test-jaccard '(1 2 3) '(1 2 3))
        ;; Disjoint sets → 0.0
        (funcall 'neovm--test-jaccard '(1 2 3) '(4 5 6))
        ;; Partial overlap: {1,2,3} ∩ {2,3,4} = {2,3}, union = {1,2,3,4} → 0.5
        (funcall 'neovm--test-jaccard '(1 2 3) '(2 3 4))
        ;; One element shared out of 5 total: 1/5 = 0.2
        (funcall 'neovm--test-jaccard '(1 2 3) '(3 4 5))
        ;; Both empty → 0.0
        (funcall 'neovm--test-jaccard nil nil)
        ;; One empty → 0.0
        (funcall 'neovm--test-jaccard '(1 2) nil)
        ;; Verify symmetry: J(A,B) = J(B,A)
        (let ((a '(10 20 30 40))
              (b '(30 40 50 60 70)))
          (= (funcall 'neovm--test-jaccard a b)
             (funcall 'neovm--test-jaccard b a)))
        ;; Verify subset: J(A, A∪B) = |A|/|A∪B|
        (let ((a '(1 2 3))
              (ab '(1 2 3 4 5)))
          (funcall 'neovm--test-jaccard a ab)))
    (fmakunbound 'neovm--test-jaccard)))"#;
    let expect = expect_test::expect![[r#""OK (1.0 0.0 0.5 0.2 0.0 0.0 t 0.6)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: k-nearest neighbors by Jaccard distance on feature sets
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_setops_knn_jaccard() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-jaccard-dist
    (lambda (a b)
      "Jaccard distance = 1 - Jaccard similarity."
      (let ((set-a (make-hash-table :test 'equal))
            (set-b (make-hash-table :test 'equal)))
        (dolist (x a) (puthash x t set-a))
        (dolist (x b) (puthash x t set-b))
        (let ((inter 0) (union-h (make-hash-table :test 'equal)))
          (maphash (lambda (k _) (puthash k t union-h)) set-a)
          (maphash (lambda (k _) (puthash k t union-h)) set-b)
          (maphash (lambda (k _)
                     (when (gethash k set-b) (setq inter (1+ inter))))
                   set-a)
          (let ((u (hash-table-count union-h)))
            (if (= u 0) 0.0
              (- 1.0 (/ (float inter) (float u)))))))))

  (fset 'neovm--test-knn
    (lambda (query points k)
      "Find K nearest neighbors of QUERY among POINTS (each is (label . features))."
      (let ((with-dist (mapcar (lambda (p)
                                 (cons (funcall 'neovm--test-jaccard-dist
                                                query (cdr p))
                                       (car p)))
                               points)))
        ;; Sort by distance (ascending)
        (setq with-dist (sort with-dist (lambda (a b) (< (car a) (car b)))))
        ;; Take first K
        (let ((result nil) (i 0))
          (while (and with-dist (< i k))
            (setq result (cons (cdar with-dist) result))
            (setq with-dist (cdr with-dist))
            (setq i (1+ i)))
          (nreverse result)))))

  (unwind-protect
      (let ((points '((cat . (furry small meow indoor))
                       (dog . (furry medium bark outdoor))
                       (fish . (scales small swim indoor))
                       (bird . (feathers small fly outdoor))
                       (hamster . (furry small squeak indoor)))))
        (list
          ;; Query: (furry small indoor) → closest should be cat/hamster
          (funcall 'neovm--test-knn '(furry small indoor) points 2)
          ;; Query: (feathers fly outdoor) → closest should be bird
          (funcall 'neovm--test-knn '(feathers fly outdoor) points 1)
          ;; Query: (scales swim) → closest should be fish
          (funcall 'neovm--test-knn '(scales swim) points 1)
          ;; All distances from (furry small indoor)
          (let ((dists nil))
            (dolist (p points)
              (setq dists (cons (cons (car p)
                                      (funcall 'neovm--test-jaccard-dist
                                               '(furry small indoor) (cdr p)))
                                dists)))
            (sort dists (lambda (a b) (< (cdr a) (cdr b)))))))
    (fmakunbound 'neovm--test-jaccard-dist)
    (fmakunbound 'neovm--test-knn)))"#;
    let expect = expect_test::expect![[
        r#""OK ((cat hamster) (bird) (fish) ((hamster . 0.25) (cat . 0.25) (fish . 0.6) (bird . 0.8333333333333334) (dog . 0.8333333333333334)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
