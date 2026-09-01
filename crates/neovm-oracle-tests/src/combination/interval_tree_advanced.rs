//! Oracle parity tests for advanced interval tree operations:
//! insertion with max propagation, overlap query (all intervals overlapping
//! a point/interval), stabbing query optimization, interval union/intersection,
//! merge overlapping intervals, sweep-line for all intersections, interval
//! scheduling (max non-overlapping), gap finding, and range coverage computation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Interval tree with delete and rebalance (max propagation)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interval_tree_adv_insert_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Node: (lo hi data max-hi left right)
  (fset 'neovm--itadv-lo    (lambda (n) (nth 0 n)))
  (fset 'neovm--itadv-hi    (lambda (n) (nth 1 n)))
  (fset 'neovm--itadv-data  (lambda (n) (nth 2 n)))
  (fset 'neovm--itadv-max   (lambda (n) (if n (nth 3 n) most-negative-fixnum)))
  (fset 'neovm--itadv-left  (lambda (n) (nth 4 n)))
  (fset 'neovm--itadv-right (lambda (n) (nth 5 n)))

  (fset 'neovm--itadv-node
    (lambda (lo hi data left right)
      (let ((m hi))
        (when left  (setq m (max m (funcall 'neovm--itadv-max left))))
        (when right (setq m (max m (funcall 'neovm--itadv-max right))))
        (list lo hi data m left right))))

  (fset 'neovm--itadv-insert
    (lambda (tree lo hi data)
      (if (null tree)
          (funcall 'neovm--itadv-node lo hi data nil nil)
        (if (< lo (funcall 'neovm--itadv-lo tree))
            (funcall 'neovm--itadv-node
                     (funcall 'neovm--itadv-lo tree)
                     (funcall 'neovm--itadv-hi tree)
                     (funcall 'neovm--itadv-data tree)
                     (funcall 'neovm--itadv-insert
                              (funcall 'neovm--itadv-left tree) lo hi data)
                     (funcall 'neovm--itadv-right tree))
          (funcall 'neovm--itadv-node
                   (funcall 'neovm--itadv-lo tree)
                   (funcall 'neovm--itadv-hi tree)
                   (funcall 'neovm--itadv-data tree)
                   (funcall 'neovm--itadv-left tree)
                   (funcall 'neovm--itadv-insert
                            (funcall 'neovm--itadv-right tree) lo hi data))))))

  ;; In-order traversal
  (fset 'neovm--itadv-inorder
    (lambda (tree)
      (if (null tree) nil
        (append
         (funcall 'neovm--itadv-inorder (funcall 'neovm--itadv-left tree))
         (list (list (funcall 'neovm--itadv-lo tree)
                     (funcall 'neovm--itadv-hi tree)
                     (funcall 'neovm--itadv-data tree)))
         (funcall 'neovm--itadv-inorder (funcall 'neovm--itadv-right tree))))))

  ;; Find minimum node (leftmost)
  (fset 'neovm--itadv-min-node
    (lambda (tree)
      (if (null (funcall 'neovm--itadv-left tree))
          tree
        (funcall 'neovm--itadv-min-node (funcall 'neovm--itadv-left tree)))))

  ;; Delete by (lo, data) key
  (fset 'neovm--itadv-delete
    (lambda (tree lo data)
      (if (null tree) nil
        (cond
         ((< lo (funcall 'neovm--itadv-lo tree))
          (funcall 'neovm--itadv-node
                   (funcall 'neovm--itadv-lo tree)
                   (funcall 'neovm--itadv-hi tree)
                   (funcall 'neovm--itadv-data tree)
                   (funcall 'neovm--itadv-delete
                            (funcall 'neovm--itadv-left tree) lo data)
                   (funcall 'neovm--itadv-right tree)))
         ((> lo (funcall 'neovm--itadv-lo tree))
          (funcall 'neovm--itadv-node
                   (funcall 'neovm--itadv-lo tree)
                   (funcall 'neovm--itadv-hi tree)
                   (funcall 'neovm--itadv-data tree)
                   (funcall 'neovm--itadv-left tree)
                   (funcall 'neovm--itadv-delete
                            (funcall 'neovm--itadv-right tree) lo data)))
         ;; lo matches: check data
         ((equal data (funcall 'neovm--itadv-data tree))
          ;; Delete this node
          (cond
           ((null (funcall 'neovm--itadv-left tree))
            (funcall 'neovm--itadv-right tree))
           ((null (funcall 'neovm--itadv-right tree))
            (funcall 'neovm--itadv-left tree))
           (t ;; Two children: replace with inorder successor
            (let ((succ (funcall 'neovm--itadv-min-node
                                  (funcall 'neovm--itadv-right tree))))
              (funcall 'neovm--itadv-node
                       (funcall 'neovm--itadv-lo succ)
                       (funcall 'neovm--itadv-hi succ)
                       (funcall 'neovm--itadv-data succ)
                       (funcall 'neovm--itadv-left tree)
                       (funcall 'neovm--itadv-delete
                                (funcall 'neovm--itadv-right tree)
                                (funcall 'neovm--itadv-lo succ)
                                (funcall 'neovm--itadv-data succ)))))))
         ;; Same lo but different data: search right
         (t (funcall 'neovm--itadv-node
                     (funcall 'neovm--itadv-lo tree)
                     (funcall 'neovm--itadv-hi tree)
                     (funcall 'neovm--itadv-data tree)
                     (funcall 'neovm--itadv-left tree)
                     (funcall 'neovm--itadv-delete
                              (funcall 'neovm--itadv-right tree) lo data)))))))

  (unwind-protect
      (let* ((t1 nil)
             (t1 (funcall 'neovm--itadv-insert t1 5 15 "A"))
             (t1 (funcall 'neovm--itadv-insert t1 10 20 "B"))
             (t1 (funcall 'neovm--itadv-insert t1 3 8 "C"))
             (t1 (funcall 'neovm--itadv-insert t1 25 35 "D"))
             (t1 (funcall 'neovm--itadv-insert t1 1 4 "E")))
        (list
         ;; In-order before delete
         (funcall 'neovm--itadv-inorder t1)
         ;; Max of whole tree
         (funcall 'neovm--itadv-max t1)
         ;; Delete leaf "E"
         (funcall 'neovm--itadv-inorder
                  (funcall 'neovm--itadv-delete t1 1 "E"))
         ;; Delete internal node "C"
         (funcall 'neovm--itadv-inorder
                  (funcall 'neovm--itadv-delete t1 3 "C"))
         ;; Delete root "A" (has two children in some orders)
         (let ((t2 (funcall 'neovm--itadv-delete t1 5 "A")))
           (list (funcall 'neovm--itadv-inorder t2)
                 (funcall 'neovm--itadv-max t2)))
         ;; Delete nonexistent
         (funcall 'neovm--itadv-inorder
                  (funcall 'neovm--itadv-delete t1 99 "X"))
         ;; Count after deletions
         (length (funcall 'neovm--itadv-inorder t1))))
    (fmakunbound 'neovm--itadv-lo)
    (fmakunbound 'neovm--itadv-hi)
    (fmakunbound 'neovm--itadv-data)
    (fmakunbound 'neovm--itadv-max)
    (fmakunbound 'neovm--itadv-left)
    (fmakunbound 'neovm--itadv-right)
    (fmakunbound 'neovm--itadv-node)
    (fmakunbound 'neovm--itadv-insert)
    (fmakunbound 'neovm--itadv-inorder)
    (fmakunbound 'neovm--itadv-min-node)
    (fmakunbound 'neovm--itadv-delete)))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 4 \"E\") (3 8 \"C\") (5 15 \"A\") (10 20 \"B\") (25 35 \"D\")) 35 ((3 8 \"C\") (5 15 \"A\") (10 20 \"B\") (25 35 \"D\")) ((1 4 \"E\") (5 15 \"A\") (10 20 \"B\") (25 35 \"D\")) (((1 4 \"E\") (3 8 \"C\") (10 20 \"B\") (25 35 \"D\")) 35) ((1 4 \"E\") (3 8 \"C\") (5 15 \"A\") (10 20 \"B\") (25 35 \"D\")) 5)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Stabbing query: optimized point query with early termination
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interval_tree_adv_stabbing_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Flat list representation for simplicity (sorted by lo)
  ;; Stabbing query: find ALL intervals containing a point
  ;; Optimization: skip subtrees whose max < point

  (fset 'neovm--itadv-stab
    (lambda (intervals point)
      "Find all intervals from sorted list containing POINT."
      (let ((result nil))
        (dolist (iv intervals)
          (cond
           ((> (car iv) point) nil)  ;; past point, but list might wrap
           ((and (<= (car iv) point) (<= point (cadr iv)))
            (setq result (cons iv result)))))
        (nreverse result))))

  ;; Count query: how many intervals contain this point
  (fset 'neovm--itadv-stab-count
    (lambda (intervals point)
      (length (funcall 'neovm--itadv-stab intervals point))))

  ;; Find the point with maximum overlap depth (brute force sweep)
  (fset 'neovm--itadv-max-depth-point
    (lambda (intervals)
      (if (null intervals) (list 0 nil)
        (let ((events nil))
          (dolist (iv intervals)
            (setq events (cons (list (car iv) 1) events))
            (setq events (cons (list (cadr iv) -1) events)))
          (setq events (sort events (lambda (a b)
                                       (or (< (car a) (car b))
                                           (and (= (car a) (car b))
                                                (> (cadr a) (cadr b)))))))
          (let ((max-d 0) (cur 0) (max-pt nil))
            (dolist (ev events)
              (setq cur (+ cur (cadr ev)))
              (when (> cur max-d)
                (setq max-d cur)
                (setq max-pt (car ev))))
            (list max-d max-pt))))))

  (unwind-protect
      (let ((ivs '((1 5 "A") (3 8 "B") (6 12 "C") (10 15 "D") (14 20 "E")
                   (2 4 "F") (7 9 "G") (18 25 "H"))))
        (list
         ;; Stab at 3: A(1-5), B(3-8), F(2-4)
         (mapcar #'caddr (funcall 'neovm--itadv-stab ivs 3))
         ;; Stab at 7: B(3-8), C(6-12), G(7-9)
         (mapcar #'caddr (funcall 'neovm--itadv-stab ivs 7))
         ;; Stab at 14: D(10-15), E(14-20)
         (mapcar #'caddr (funcall 'neovm--itadv-stab ivs 14))
         ;; Stab at 0: nothing
         (funcall 'neovm--itadv-stab ivs 0)
         ;; Stab at 30: nothing
         (funcall 'neovm--itadv-stab ivs 30)
         ;; Counts
         (funcall 'neovm--itadv-stab-count ivs 3)
         (funcall 'neovm--itadv-stab-count ivs 7)
         (funcall 'neovm--itadv-stab-count ivs 25)
         ;; Max depth
         (funcall 'neovm--itadv-max-depth-point
                  (mapcar (lambda (iv) (list (car iv) (cadr iv))) ivs))
         ;; Edge: empty
         (funcall 'neovm--itadv-stab nil 5)
         ;; Edge: single point interval
         (funcall 'neovm--itadv-stab '((5 5 "pt")) 5)
         (funcall 'neovm--itadv-stab '((5 5 "pt")) 4)))
    (fmakunbound 'neovm--itadv-stab)
    (fmakunbound 'neovm--itadv-stab-count)
    (fmakunbound 'neovm--itadv-max-depth-point)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"A\" \"B\" \"F\") (\"B\" \"C\" \"G\") (\"D\" \"E\") nil nil 3 3 1 (3 3) nil ((5 5 \"pt\")) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Interval union and intersection operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interval_tree_adv_union_intersection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Merge overlapping intervals (union)
  (fset 'neovm--itadv-union
    (lambda (intervals)
      (if (null intervals) nil
        (let* ((sorted (sort (copy-sequence intervals)
                             (lambda (a b) (< (car a) (car b)))))
               (result (list (list (caar sorted) (cadar sorted))))
               (rest (cdr sorted)))
          (dolist (iv rest)
            (let ((last (car (last result))))
              (if (<= (car iv) (cadr last))
                  (setcar (cdr last) (max (cadr last) (cadr iv)))
                (setq result (append result (list (list (car iv) (cadr iv))))))))
          result))))

  ;; Intersection of two sets of intervals
  ;; Result: set of intervals that are in BOTH a and b
  (fset 'neovm--itadv-intersect
    (lambda (set-a set-b)
      (let ((a (funcall 'neovm--itadv-union set-a))
            (b (funcall 'neovm--itadv-union set-b))
            (result nil))
        (while (and a b)
          (let ((a-lo (caar a)) (a-hi (cadar a))
                (b-lo (caar b)) (b-hi (cadar b)))
            ;; Intersection of [a-lo,a-hi] and [b-lo,b-hi]
            (let ((lo (max a-lo b-lo))
                  (hi (min a-hi b-hi)))
              (when (<= lo hi)
                (setq result (cons (list lo hi) result))))
            ;; Advance the one that ends first
            (if (<= a-hi b-hi)
                (setq a (cdr a))
              (setq b (cdr b)))))
        (nreverse result))))

  ;; Complement: gaps within [lo, hi]
  (fset 'neovm--itadv-complement
    (lambda (intervals lo hi)
      (let ((merged (funcall 'neovm--itadv-union intervals))
            (gaps nil)
            (cur lo))
        (dolist (iv merged)
          (when (and (> (car iv) cur) (<= cur hi))
            (setq gaps (cons (list cur (min (car iv) hi)) gaps)))
          (setq cur (max cur (cadr iv))))
        (when (< cur hi)
          (setq gaps (cons (list cur hi) gaps)))
        (nreverse gaps))))

  (unwind-protect
      (list
       ;; Union: non-overlapping
       (funcall 'neovm--itadv-union '((1 3) (5 7) (9 11)))
       ;; Union: overlapping
       (funcall 'neovm--itadv-union '((1 5) (3 8) (7 10)))
       ;; Union: all one
       (funcall 'neovm--itadv-union '((1 10) (2 5) (3 7)))
       ;; Union: unsorted
       (funcall 'neovm--itadv-union '((5 10) (1 3) (8 15) (2 4)))
       ;; Union: empty
       (funcall 'neovm--itadv-union nil)

       ;; Intersection: overlapping sets
       (funcall 'neovm--itadv-intersect '((1 5) (8 12)) '((3 9)))
       ;; Intersection: no overlap
       (funcall 'neovm--itadv-intersect '((1 3)) '((5 7)))
       ;; Intersection: contained
       (funcall 'neovm--itadv-intersect '((1 10)) '((3 7)))
       ;; Intersection: multiple overlaps
       (funcall 'neovm--itadv-intersect '((1 5) (8 12) (15 20)) '((3 10) (18 25)))

       ;; Complement
       (funcall 'neovm--itadv-complement '((2 4) (7 9)) 0 12)
       (funcall 'neovm--itadv-complement '((0 12)) 0 12)
       (funcall 'neovm--itadv-complement nil 0 10))
    (fmakunbound 'neovm--itadv-union)
    (fmakunbound 'neovm--itadv-intersect)
    (fmakunbound 'neovm--itadv-complement)))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 3) (5 7) (9 11)) ((1 10)) ((1 10)) ((1 4) (5 15)) nil ((3 5) (8 9)) nil ((3 7)) ((3 5) (8 10) (18 20)) ((0 2) (4 7) (9 12)) nil ((0 10)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Sweep-line: find all pairwise intersections
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interval_tree_adv_sweep_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Sweep line to find all pairs of overlapping intervals
  ;; Sort endpoints; at each start, check against all active intervals
  (fset 'neovm--itadv-all-intersections
    (lambda (intervals)
      (let* ((sorted (sort (copy-sequence intervals)
                           (lambda (a b) (< (car a) (car b)))))
             (pairs nil))
        ;; Brute-force O(n^2) but correct
        (let ((i 0))
          (while (< i (length sorted))
            (let ((iv1 (nth i sorted))
                  (j (1+ i)))
              (while (< j (length sorted))
                (let ((iv2 (nth j sorted)))
                  ;; iv2 starts <= iv1 ends => overlap
                  (when (<= (car iv2) (cadr iv1))
                    (setq pairs (cons (list (caddr iv1) (caddr iv2)) pairs))))
                (setq j (1+ j))))
            (setq i (1+ i))))
        (nreverse pairs))))

  ;; Count total number of pairwise overlaps
  (fset 'neovm--itadv-overlap-count
    (lambda (intervals)
      (length (funcall 'neovm--itadv-all-intersections intervals))))

  (unwind-protect
      (let ((ivs '((1 5 "A") (3 8 "B") (6 10 "C") (12 15 "D") (14 20 "E"))))
        (list
         ;; All pairwise intersections
         (funcall 'neovm--itadv-all-intersections ivs)
         ;; Count
         (funcall 'neovm--itadv-overlap-count ivs)
         ;; Non-overlapping set
         (funcall 'neovm--itadv-all-intersections
                  '((1 3 "X") (5 7 "Y") (9 11 "Z")))
         ;; All overlapping
         (funcall 'neovm--itadv-all-intersections
                  '((1 10 "P") (2 9 "Q") (3 8 "R")))
         ;; Single interval
         (funcall 'neovm--itadv-all-intersections '((1 5 "S")))
         ;; Empty
         (funcall 'neovm--itadv-all-intersections nil)
         ;; Adjacent (touching at endpoints)
         (funcall 'neovm--itadv-all-intersections
                  '((1 3 "T1") (3 5 "T2") (5 7 "T3")))))
    (fmakunbound 'neovm--itadv-all-intersections)
    (fmakunbound 'neovm--itadv-overlap-count)))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"A\" \"B\") (\"B\" \"C\") (\"D\" \"E\")) 3 nil ((\"P\" \"Q\") (\"P\" \"R\") (\"Q\" \"R\")) nil nil ((\"T1\" \"T2\") (\"T2\" \"T3\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Interval scheduling: max non-overlapping subset
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interval_tree_adv_scheduling() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Greedy interval scheduling: select max non-overlapping intervals
  ;; Strategy: sort by end time, greedily pick earliest-finishing
  (fset 'neovm--itadv-schedule-max
    (lambda (intervals)
      "Select max non-overlapping intervals using greedy earliest-finish."
      (let* ((sorted (sort (copy-sequence intervals)
                           (lambda (a b) (< (cadr a) (cadr b)))))
             (selected nil)
             (last-end most-negative-fixnum))
        (dolist (iv sorted)
          (when (>= (car iv) last-end)
            (setq selected (cons iv selected))
            (setq last-end (cadr iv))))
        (nreverse selected))))

  ;; Minimum intervals to remove to eliminate all overlaps
  (fset 'neovm--itadv-min-removals
    (lambda (intervals)
      (- (length intervals)
         (length (funcall 'neovm--itadv-schedule-max intervals)))))

  ;; Weighted interval scheduling: each interval has a weight (profit)
  ;; Interval: (lo hi weight label)
  ;; Uses DP: sort by end, for each interval find latest non-conflicting
  (fset 'neovm--itadv-schedule-weighted
    (lambda (intervals)
      (if (null intervals) 0
        (let* ((sorted (sort (copy-sequence intervals)
                             (lambda (a b) (< (cadr a) (cadr b)))))
               (n (length sorted))
               ;; dp[i] = max weight using first i intervals
               (dp (make-vector (1+ n) 0)))
          ;; For each interval, find latest non-conflicting
          (dotimes (i n)
            (let ((iv (nth i sorted))
                  (weight (nth 2 (nth i sorted)))
                  (prev 0))
              ;; Find latest j < i where sorted[j].hi <= sorted[i].lo
              (let ((j (1- i)))
                (while (>= j 0)
                  (when (<= (cadr (nth j sorted)) (car iv))
                    (setq prev (1+ j))
                    (setq j -1))  ;; break
                  (setq j (1- j))))
              ;; dp[i+1] = max(dp[i], dp[prev] + weight)
              (aset dp (1+ i) (max (aref dp i) (+ (aref dp prev) weight)))))
          (aref dp n)))))

  (unwind-protect
      (let ((tasks '((1 3 "A") (2 5 "B") (4 7 "C") (6 9 "D") (8 10 "E"))))
        (list
         ;; Max non-overlapping
         (funcall 'neovm--itadv-schedule-max tasks)
         ;; Count of selected
         (length (funcall 'neovm--itadv-schedule-max tasks))
         ;; Min removals
         (funcall 'neovm--itadv-min-removals tasks)

         ;; All non-overlapping: all selected
         (funcall 'neovm--itadv-schedule-max
                  '((1 3 "X") (5 7 "Y") (9 11 "Z")))

         ;; All same interval: only one selected
         (funcall 'neovm--itadv-schedule-max
                  '((1 5 "P") (1 5 "Q") (1 5 "R")))

         ;; Single
         (funcall 'neovm--itadv-schedule-max '((1 10 "S")))

         ;; Empty
         (funcall 'neovm--itadv-schedule-max nil)

         ;; Weighted scheduling
         (funcall 'neovm--itadv-schedule-weighted
                  '((1 3 10) (2 5 5) (4 7 8) (6 9 12) (8 10 6)))
         ;; Weighted: single high-value vs many small
         (funcall 'neovm--itadv-schedule-weighted
                  '((1 10 100) (1 3 10) (4 6 10) (7 10 10)))))
    (fmakunbound 'neovm--itadv-schedule-max)
    (fmakunbound 'neovm--itadv-min-removals)
    (fmakunbound 'neovm--itadv-schedule-weighted)))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 3 \"A\") (4 7 \"C\") (8 10 \"E\")) 3 2 ((1 3 \"X\") (5 7 \"Y\") (9 11 \"Z\")) ((1 5 \"P\")) ((1 10 \"S\")) nil 24 100)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Gap finding: find all uncovered segments in a range
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interval_tree_adv_gap_finding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Find gaps (uncovered regions) within [range-lo, range-hi]
  (fset 'neovm--itadv-find-gaps
    (lambda (intervals range-lo range-hi)
      (let* ((sorted (sort (copy-sequence intervals)
                           (lambda (a b) (< (car a) (car b)))))
             (gaps nil)
             (cur range-lo))
        (dolist (iv sorted)
          (when (and (> (car iv) cur) (<= cur range-hi))
            (setq gaps (cons (list cur (min (car iv) range-hi)) gaps)))
          (setq cur (max cur (cadr iv))))
        (when (< cur range-hi)
          (setq gaps (cons (list cur range-hi) gaps)))
        (nreverse gaps))))

  ;; Total gap size
  (fset 'neovm--itadv-total-gap
    (lambda (intervals range-lo range-hi)
      (let ((gaps (funcall 'neovm--itadv-find-gaps intervals range-lo range-hi))
            (total 0))
        (dolist (g gaps)
          (setq total (+ total (- (cadr g) (car g)))))
        total)))

  ;; Coverage percentage (0-100)
  (fset 'neovm--itadv-coverage-pct
    (lambda (intervals range-lo range-hi)
      (let* ((range-size (- range-hi range-lo))
             (gap-size (funcall 'neovm--itadv-total-gap intervals range-lo range-hi)))
        (if (= range-size 0) 100
          (/ (* (- range-size gap-size) 100) range-size)))))

  ;; Largest gap
  (fset 'neovm--itadv-largest-gap
    (lambda (intervals range-lo range-hi)
      (let ((gaps (funcall 'neovm--itadv-find-gaps intervals range-lo range-hi))
            (largest nil)
            (max-size 0))
        (dolist (g gaps)
          (let ((size (- (cadr g) (car g))))
            (when (> size max-size)
              (setq max-size size)
              (setq largest g))))
        (list largest max-size))))

  (unwind-protect
      (let ((ivs '((2 5) (8 12) (15 20))))
        (list
         ;; Gaps in [0, 25]
         (funcall 'neovm--itadv-find-gaps ivs 0 25)
         ;; Total gap
         (funcall 'neovm--itadv-total-gap ivs 0 25)
         ;; Coverage
         (funcall 'neovm--itadv-coverage-pct ivs 0 25)
         ;; Largest gap
         (funcall 'neovm--itadv-largest-gap ivs 0 25)

         ;; Full coverage: no gaps
         (funcall 'neovm--itadv-find-gaps '((0 25)) 0 25)
         (funcall 'neovm--itadv-total-gap '((0 25)) 0 25)

         ;; No intervals: entire range is gap
         (funcall 'neovm--itadv-find-gaps nil 0 10)
         (funcall 'neovm--itadv-total-gap nil 0 10)

         ;; Overlapping intervals
         (funcall 'neovm--itadv-find-gaps '((1 5) (3 8) (7 12)) 0 15)

         ;; Range entirely within one interval
         (funcall 'neovm--itadv-find-gaps '((0 20)) 5 15)

         ;; Range outside all intervals
         (funcall 'neovm--itadv-find-gaps '((1 5)) 10 20)))
    (fmakunbound 'neovm--itadv-find-gaps)
    (fmakunbound 'neovm--itadv-total-gap)
    (fmakunbound 'neovm--itadv-coverage-pct)
    (fmakunbound 'neovm--itadv-largest-gap)))"#;
    let expect = expect_test::expect![[
        r#""OK (((0 2) (5 8) (12 15) (20 25)) 13 48 ((20 25) 5) nil 0 ((0 10)) 10 ((0 1) (12 15)) nil ((10 20)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Range coverage: total length covered by union of intervals
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interval_tree_adv_range_coverage() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Compute total coverage (length of union)
  (fset 'neovm--itadv-total-coverage
    (lambda (intervals)
      (if (null intervals) 0
        (let* ((sorted (sort (copy-sequence intervals)
                             (lambda (a b) (< (car a) (car b)))))
               (merged nil))
          (dolist (iv sorted)
            (if (and merged (<= (car iv) (cadr (car (last merged)))))
                (setcar (cdr (car (last merged)))
                        (max (cadr (car (last merged))) (cadr iv)))
              (setq merged (append merged (list (list (car iv) (cadr iv)))))))
          (let ((total 0))
            (dolist (iv merged)
              (setq total (+ total (- (cadr iv) (car iv)))))
            total)))))

  ;; Redundancy: how much extra coverage from overlaps
  (fset 'neovm--itadv-redundancy
    (lambda (intervals)
      (let ((raw-total 0)
            (merged-total (funcall 'neovm--itadv-total-coverage intervals)))
        (dolist (iv intervals)
          (setq raw-total (+ raw-total (- (cadr iv) (car iv)))))
        (- raw-total merged-total))))

  ;; Density: coverage / bounding-box
  (fset 'neovm--itadv-density
    (lambda (intervals)
      (if (null intervals) 0
        (let ((min-lo (caar intervals))
              (max-hi (cadar intervals)))
          (dolist (iv intervals)
            (setq min-lo (min min-lo (car iv)))
            (setq max-hi (max max-hi (cadr iv))))
          (let ((bbox (- max-hi min-lo)))
            (if (= bbox 0) 100
              (/ (* (funcall 'neovm--itadv-total-coverage intervals) 100) bbox)))))))

  (unwind-protect
      (list
       ;; Non-overlapping
       (funcall 'neovm--itadv-total-coverage '((1 3) (5 7) (9 11)))
       ;; Overlapping
       (funcall 'neovm--itadv-total-coverage '((1 5) (3 8) (7 10)))
       ;; Contained
       (funcall 'neovm--itadv-total-coverage '((1 10) (3 7) (2 5)))
       ;; Single
       (funcall 'neovm--itadv-total-coverage '((5 15)))
       ;; Empty
       (funcall 'neovm--itadv-total-coverage nil)
       ;; Point intervals
       (funcall 'neovm--itadv-total-coverage '((5 5) (5 5)))

       ;; Redundancy
       (funcall 'neovm--itadv-redundancy '((1 5) (3 8) (7 10)))
       (funcall 'neovm--itadv-redundancy '((1 3) (5 7) (9 11)))

       ;; Density
       (funcall 'neovm--itadv-density '((1 3) (5 7) (9 11)))
       (funcall 'neovm--itadv-density '((1 10)))
       (funcall 'neovm--itadv-density '((1 5) (3 8) (7 10))))
    (fmakunbound 'neovm--itadv-total-coverage)
    (fmakunbound 'neovm--itadv-redundancy)
    (fmakunbound 'neovm--itadv-density)))"#;
    let expect = expect_test::expect![[r#""OK (6 9 9 10 0 0 3 0 60 100 100)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Interval partitioning: minimum resources for all intervals
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interval_tree_adv_partitioning() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Interval partitioning: minimum number of "rooms" needed to schedule
    // all intervals without overlap within each room
    let form = r#"(progn
  ;; Min rooms = max overlap depth (sweep line)
  (fset 'neovm--itadv-min-rooms
    (lambda (intervals)
      (if (null intervals) 0
        (let ((events nil))
          (dolist (iv intervals)
            (setq events (cons (cons (car iv) 1) events))
            (setq events (cons (cons (cadr iv) -1) events)))
          (setq events (sort events
                             (lambda (a b)
                               (or (< (car a) (car b))
                                   (and (= (car a) (car b))
                                        (< (cdr a) (cdr b)))))))
          (let ((max-d 0) (cur 0))
            (dolist (ev events)
              (setq cur (+ cur (cdr ev)))
              (setq max-d (max max-d cur)))
            max-d)))))

  ;; Actually partition into rooms (greedy: assign to first available room)
  (fset 'neovm--itadv-partition
    (lambda (intervals)
      (let* ((sorted (sort (copy-sequence intervals)
                           (lambda (a b) (< (car a) (car b)))))
             (rooms nil))  ;; list of (last-end . assigned-intervals)
        (dolist (iv sorted)
          (let ((assigned nil))
            ;; Find first room where last interval ends <= iv start
            (let ((r rooms) (found nil))
              (while (and r (not found))
                (when (<= (caar r) (car iv))
                  (setcar (car r) (cadr iv))
                  (setcdr (car r) (append (cdar r) (list iv)))
                  (setq assigned t)
                  (setq found t))
                (setq r (cdr r))))
            (unless assigned
              ;; New room
              (setq rooms (append rooms (list (cons (cadr iv) (list iv))))))))
        ;; Return number of rooms and their assignments
        (list (length rooms)
              (mapcar (lambda (room) (cdr room)) rooms)))))

  (unwind-protect
      (list
       ;; Simple: non-overlapping
       (funcall 'neovm--itadv-min-rooms '((1 3) (5 7) (9 11)))
       ;; All overlapping: need 3 rooms
       (funcall 'neovm--itadv-min-rooms '((1 5) (2 6) (3 7)))
       ;; Partial overlap
       (funcall 'neovm--itadv-min-rooms '((1 4) (2 5) (5 8) (7 10)))
       ;; Single
       (funcall 'neovm--itadv-min-rooms '((1 10)))
       ;; Empty
       (funcall 'neovm--itadv-min-rooms nil)

       ;; Partition: verify assignments
       (funcall 'neovm--itadv-partition '((1 3) (2 5) (4 7) (6 9)))
       (funcall 'neovm--itadv-partition '((1 3) (5 7) (9 11)))
       (funcall 'neovm--itadv-partition '((1 5) (2 6) (3 7))))
    (fmakunbound 'neovm--itadv-min-rooms)
    (fmakunbound 'neovm--itadv-partition)))"#;
    let expect = expect_test::expect![[
        r#""OK (1 3 2 1 0 (2 (((1 3) (4 7)) ((2 5) (6 9)))) (1 (((1 3) (5 7) (9 11)))) (3 (((1 5)) ((2 6)) ((3 7)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Interval flattening: convert overlapping intervals to non-overlapping segments
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interval_tree_adv_flatten() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Break overlapping intervals into non-overlapping segments,
    // recording which original intervals cover each segment
    let form = r#"(progn
  (fset 'neovm--itadv-flatten
    (lambda (intervals)
      "Break into non-overlapping segments with coverage lists."
      (if (null intervals) nil
        ;; Collect all unique endpoints
        (let ((points nil))
          (dolist (iv intervals)
            (unless (member (car iv) points) (setq points (cons (car iv) points)))
            (unless (member (cadr iv) points) (setq points (cons (cadr iv) points))))
          (setq points (sort points #'<))
          ;; For each pair of adjacent points, find covering intervals
          (let ((segments nil)
                (rest points))
            (while (cdr rest)
              (let ((lo (car rest))
                    (hi (cadr rest))
                    (covering nil))
                (dolist (iv intervals)
                  (when (and (<= (car iv) lo) (>= (cadr iv) hi))
                    (setq covering (cons (caddr iv) covering))))
                (when covering
                  (setq segments (cons (list lo hi (nreverse covering)) segments))))
              (setq rest (cdr rest)))
            (nreverse segments))))))

  (unwind-protect
      (list
       ;; Two overlapping
       (funcall 'neovm--itadv-flatten '((1 5 "A") (3 8 "B")))
       ;; Three overlapping
       (funcall 'neovm--itadv-flatten '((1 6 "A") (3 8 "B") (5 10 "C")))
       ;; Non-overlapping
       (funcall 'neovm--itadv-flatten '((1 3 "X") (5 7 "Y")))
       ;; Contained
       (funcall 'neovm--itadv-flatten '((1 10 "O") (3 7 "I")))
       ;; Single
       (funcall 'neovm--itadv-flatten '((1 5 "S")))
       ;; Empty
       (funcall 'neovm--itadv-flatten nil)
       ;; Three stacked at same point
       (funcall 'neovm--itadv-flatten '((1 5 "A") (1 5 "B") (1 5 "C"))))
    (fmakunbound 'neovm--itadv-flatten)))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 3 (\"A\")) (3 5 (\"A\" \"B\")) (5 8 (\"B\"))) ((1 3 (\"A\")) (3 5 (\"A\" \"B\")) (5 6 (\"A\" \"B\" \"C\")) (6 8 (\"B\" \"C\")) (8 10 (\"C\"))) ((1 3 (\"X\")) (5 7 (\"Y\"))) ((1 3 (\"O\")) (3 7 (\"O\" \"I\")) (7 10 (\"O\"))) ((1 5 (\"S\"))) nil ((1 5 (\"A\" \"B\" \"C\"))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
