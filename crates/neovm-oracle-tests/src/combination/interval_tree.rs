//! Oracle parity tests for an interval tree implemented in Elisp:
//! interval representation as (low high data), BST-based insert keyed on low,
//! augmented max tracking, point query (find all intervals containing a point),
//! overlap query (find all intervals overlapping a given interval),
//! merge overlapping intervals, and calendar/scheduling with interval tree.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Interval tree core: insert and augmented max
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interval_tree_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Node: (low high data max-high left right) or nil
    // max-high = max of this interval's high and children's max-high
    let form = r#"(progn
  (fset 'neovm--itree-low    (lambda (n) (nth 0 n)))
  (fset 'neovm--itree-high   (lambda (n) (nth 1 n)))
  (fset 'neovm--itree-data   (lambda (n) (nth 2 n)))
  (fset 'neovm--itree-max    (lambda (n) (if n (nth 3 n) -1)))
  (fset 'neovm--itree-left   (lambda (n) (nth 4 n)))
  (fset 'neovm--itree-right  (lambda (n) (nth 5 n)))

  (fset 'neovm--itree-node
    (lambda (low high data left right)
      (let ((m high))
        (when left  (setq m (max m (funcall 'neovm--itree-max left))))
        (when right (setq m (max m (funcall 'neovm--itree-max right))))
        (list low high data m left right))))

  (fset 'neovm--itree-insert
    (lambda (tree low high data)
      (if (null tree)
          (funcall 'neovm--itree-node low high data nil nil)
        (if (< low (funcall 'neovm--itree-low tree))
            (funcall 'neovm--itree-node
                     (funcall 'neovm--itree-low tree)
                     (funcall 'neovm--itree-high tree)
                     (funcall 'neovm--itree-data tree)
                     (funcall 'neovm--itree-insert
                              (funcall 'neovm--itree-left tree) low high data)
                     (funcall 'neovm--itree-right tree))
          (funcall 'neovm--itree-node
                   (funcall 'neovm--itree-low tree)
                   (funcall 'neovm--itree-high tree)
                   (funcall 'neovm--itree-data tree)
                   (funcall 'neovm--itree-left tree)
                   (funcall 'neovm--itree-insert
                            (funcall 'neovm--itree-right tree) low high data))))))

  ;; In-order traversal: collect intervals as (low high data)
  (fset 'neovm--itree-inorder
    (lambda (tree)
      (if (null tree) nil
        (append
         (funcall 'neovm--itree-inorder (funcall 'neovm--itree-left tree))
         (list (list (funcall 'neovm--itree-low tree)
                     (funcall 'neovm--itree-high tree)
                     (funcall 'neovm--itree-data tree)))
         (funcall 'neovm--itree-inorder (funcall 'neovm--itree-right tree))))))

  (unwind-protect
      (let* ((tree nil)
             (tree (funcall 'neovm--itree-insert tree 15 20 "A"))
             (tree (funcall 'neovm--itree-insert tree 10 30 "B"))
             (tree (funcall 'neovm--itree-insert tree 17 19 "C"))
             (tree (funcall 'neovm--itree-insert tree 5 20 "D"))
             (tree (funcall 'neovm--itree-insert tree 12 15 "E"))
             (tree (funcall 'neovm--itree-insert tree 30 40 "F")))
        (list
         ;; In-order traversal (sorted by low endpoint)
         (funcall 'neovm--itree-inorder tree)
         ;; Root node's max should be 40
         (funcall 'neovm--itree-max tree)
         ;; Root interval
         (list (funcall 'neovm--itree-low tree)
               (funcall 'neovm--itree-high tree)
               (funcall 'neovm--itree-data tree))
         ;; Single insert
         (funcall 'neovm--itree-inorder
                  (funcall 'neovm--itree-insert nil 1 5 "solo"))
         ;; Count intervals
         (length (funcall 'neovm--itree-inorder tree))))
    (fmakunbound 'neovm--itree-low)
    (fmakunbound 'neovm--itree-high)
    (fmakunbound 'neovm--itree-data)
    (fmakunbound 'neovm--itree-max)
    (fmakunbound 'neovm--itree-left)
    (fmakunbound 'neovm--itree-right)
    (fmakunbound 'neovm--itree-node)
    (fmakunbound 'neovm--itree-insert)
    (fmakunbound 'neovm--itree-inorder)))"#;
    let expect = expect_test::expect![[
        r#""OK (((5 20 \"D\") (10 30 \"B\") (12 15 \"E\") (15 20 \"A\") (17 19 \"C\") (30 40 \"F\")) 40 (15 20 \"A\") ((1 5 \"solo\")) 6)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Point query: find all intervals containing a point
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interval_tree_point_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--itree-low    (lambda (n) (nth 0 n)))
  (fset 'neovm--itree-high   (lambda (n) (nth 1 n)))
  (fset 'neovm--itree-data   (lambda (n) (nth 2 n)))
  (fset 'neovm--itree-max    (lambda (n) (if n (nth 3 n) -1)))
  (fset 'neovm--itree-left   (lambda (n) (nth 4 n)))
  (fset 'neovm--itree-right  (lambda (n) (nth 5 n)))

  (fset 'neovm--itree-node
    (lambda (low high data left right)
      (let ((m high))
        (when left  (setq m (max m (funcall 'neovm--itree-max left))))
        (when right (setq m (max m (funcall 'neovm--itree-max right))))
        (list low high data m left right))))

  (fset 'neovm--itree-insert
    (lambda (tree low high data)
      (if (null tree)
          (funcall 'neovm--itree-node low high data nil nil)
        (if (< low (funcall 'neovm--itree-low tree))
            (funcall 'neovm--itree-node
                     (funcall 'neovm--itree-low tree)
                     (funcall 'neovm--itree-high tree)
                     (funcall 'neovm--itree-data tree)
                     (funcall 'neovm--itree-insert
                              (funcall 'neovm--itree-left tree) low high data)
                     (funcall 'neovm--itree-right tree))
          (funcall 'neovm--itree-node
                   (funcall 'neovm--itree-low tree)
                   (funcall 'neovm--itree-high tree)
                   (funcall 'neovm--itree-data tree)
                   (funcall 'neovm--itree-left tree)
                   (funcall 'neovm--itree-insert
                            (funcall 'neovm--itree-right tree) low high data))))))

  ;; Point query: find all intervals where low <= point <= high
  (fset 'neovm--itree-point-query
    (lambda (tree point)
      (if (null tree) nil
        (let ((result nil))
          ;; Check current node
          (when (and (<= (funcall 'neovm--itree-low tree) point)
                     (<= point (funcall 'neovm--itree-high tree)))
            (setq result (list (list (funcall 'neovm--itree-low tree)
                                     (funcall 'neovm--itree-high tree)
                                     (funcall 'neovm--itree-data tree)))))
          ;; Search left subtree if its max >= point
          (when (and (funcall 'neovm--itree-left tree)
                     (>= (funcall 'neovm--itree-max
                                  (funcall 'neovm--itree-left tree))
                         point))
            (setq result (append result
                                 (funcall 'neovm--itree-point-query
                                          (funcall 'neovm--itree-left tree) point))))
          ;; Search right subtree if point >= low of current
          (when (funcall 'neovm--itree-right tree)
            (setq result (append result
                                 (funcall 'neovm--itree-point-query
                                          (funcall 'neovm--itree-right tree) point))))
          result))))

  (unwind-protect
      (let* ((tree nil)
             (tree (funcall 'neovm--itree-insert tree 1 10 "A"))
             (tree (funcall 'neovm--itree-insert tree 5 15 "B"))
             (tree (funcall 'neovm--itree-insert tree 12 20 "C"))
             (tree (funcall 'neovm--itree-insert tree 25 30 "D"))
             (tree (funcall 'neovm--itree-insert tree 8 18 "E")))
        (list
         ;; Point 7: contained in A(1-10), B(5-15), but not C, D, E(8-18 no, 7<8)
         (sort (funcall 'neovm--itree-point-query tree 7)
               (lambda (a b) (< (car a) (car b))))
         ;; Point 13: contained in B(5-15), C(12-20), E(8-18)
         (sort (funcall 'neovm--itree-point-query tree 13)
               (lambda (a b) (< (car a) (car b))))
         ;; Point 0: no intervals contain 0
         (funcall 'neovm--itree-point-query tree 0)
         ;; Point 25: only D(25-30)
         (funcall 'neovm--itree-point-query tree 25)
         ;; Point 10: A(1-10), B(5-15), E(8-18)
         (sort (funcall 'neovm--itree-point-query tree 10)
               (lambda (a b) (< (car a) (car b))))
         ;; Point 50: nothing
         (funcall 'neovm--itree-point-query tree 50)
         ;; Empty tree
         (funcall 'neovm--itree-point-query nil 5)))
    (fmakunbound 'neovm--itree-low)
    (fmakunbound 'neovm--itree-high)
    (fmakunbound 'neovm--itree-data)
    (fmakunbound 'neovm--itree-max)
    (fmakunbound 'neovm--itree-left)
    (fmakunbound 'neovm--itree-right)
    (fmakunbound 'neovm--itree-node)
    (fmakunbound 'neovm--itree-insert)
    (fmakunbound 'neovm--itree-point-query)))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 10 \"A\") (5 15 \"B\")) ((5 15 \"B\") (8 18 \"E\") (12 20 \"C\")) nil ((25 30 \"D\")) ((1 10 \"A\") (5 15 \"B\") (8 18 \"E\")) nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Overlap query: find all intervals overlapping a given interval
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interval_tree_overlap_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--itree-low    (lambda (n) (nth 0 n)))
  (fset 'neovm--itree-high   (lambda (n) (nth 1 n)))
  (fset 'neovm--itree-data   (lambda (n) (nth 2 n)))
  (fset 'neovm--itree-max    (lambda (n) (if n (nth 3 n) -1)))
  (fset 'neovm--itree-left   (lambda (n) (nth 4 n)))
  (fset 'neovm--itree-right  (lambda (n) (nth 5 n)))

  (fset 'neovm--itree-node
    (lambda (low high data left right)
      (let ((m high))
        (when left  (setq m (max m (funcall 'neovm--itree-max left))))
        (when right (setq m (max m (funcall 'neovm--itree-max right))))
        (list low high data m left right))))

  (fset 'neovm--itree-insert
    (lambda (tree low high data)
      (if (null tree)
          (funcall 'neovm--itree-node low high data nil nil)
        (if (< low (funcall 'neovm--itree-low tree))
            (funcall 'neovm--itree-node
                     (funcall 'neovm--itree-low tree)
                     (funcall 'neovm--itree-high tree)
                     (funcall 'neovm--itree-data tree)
                     (funcall 'neovm--itree-insert
                              (funcall 'neovm--itree-left tree) low high data)
                     (funcall 'neovm--itree-right tree))
          (funcall 'neovm--itree-node
                   (funcall 'neovm--itree-low tree)
                   (funcall 'neovm--itree-high tree)
                   (funcall 'neovm--itree-data tree)
                   (funcall 'neovm--itree-left tree)
                   (funcall 'neovm--itree-insert
                            (funcall 'neovm--itree-right tree) low high data))))))

  ;; Two intervals [a,b] and [c,d] overlap iff a <= d AND c <= b
  (fset 'neovm--itree-overlaps-p
    (lambda (lo1 hi1 lo2 hi2)
      (and (<= lo1 hi2) (<= lo2 hi1))))

  ;; Find all intervals overlapping [qlo, qhi]
  (fset 'neovm--itree-overlap-query
    (lambda (tree qlo qhi)
      (if (null tree) nil
        (let ((result nil))
          ;; Check current node
          (when (funcall 'neovm--itree-overlaps-p
                         (funcall 'neovm--itree-low tree)
                         (funcall 'neovm--itree-high tree)
                         qlo qhi)
            (setq result (list (list (funcall 'neovm--itree-low tree)
                                     (funcall 'neovm--itree-high tree)
                                     (funcall 'neovm--itree-data tree)))))
          ;; Search left if left max >= qlo
          (when (and (funcall 'neovm--itree-left tree)
                     (>= (funcall 'neovm--itree-max
                                  (funcall 'neovm--itree-left tree))
                         qlo))
            (setq result (append result
                                 (funcall 'neovm--itree-overlap-query
                                          (funcall 'neovm--itree-left tree) qlo qhi))))
          ;; Search right if right subtree could have overlapping intervals
          (when (funcall 'neovm--itree-right tree)
            (setq result (append result
                                 (funcall 'neovm--itree-overlap-query
                                          (funcall 'neovm--itree-right tree) qlo qhi))))
          result))))

  (unwind-protect
      (let* ((tree nil)
             (tree (funcall 'neovm--itree-insert tree 0 5 "A"))
             (tree (funcall 'neovm--itree-insert tree 3 8 "B"))
             (tree (funcall 'neovm--itree-insert tree 6 10 "C"))
             (tree (funcall 'neovm--itree-insert tree 15 20 "D"))
             (tree (funcall 'neovm--itree-insert tree 18 25 "E")))
        (list
         ;; Query [4, 7]: overlaps A(0-5), B(3-8), C(6-10)
         (sort (funcall 'neovm--itree-overlap-query tree 4 7)
               (lambda (a b) (< (car a) (car b))))
         ;; Query [11, 14]: no overlaps (gap between C and D)
         (funcall 'neovm--itree-overlap-query tree 11 14)
         ;; Query [19, 22]: overlaps D(15-20), E(18-25)
         (sort (funcall 'neovm--itree-overlap-query tree 19 22)
               (lambda (a b) (< (car a) (car b))))
         ;; Query [0, 100]: all intervals
         (sort (funcall 'neovm--itree-overlap-query tree 0 100)
               (lambda (a b) (< (car a) (car b))))
         ;; Query [5, 5]: single point, overlaps A(0-5), B(3-8)
         (sort (funcall 'neovm--itree-overlap-query tree 5 5)
               (lambda (a b) (< (car a) (car b))))
         ;; Query [30, 40]: nothing
         (funcall 'neovm--itree-overlap-query tree 30 40)
         ;; Empty tree
         (funcall 'neovm--itree-overlap-query nil 0 10)))
    (fmakunbound 'neovm--itree-low)
    (fmakunbound 'neovm--itree-high)
    (fmakunbound 'neovm--itree-data)
    (fmakunbound 'neovm--itree-max)
    (fmakunbound 'neovm--itree-left)
    (fmakunbound 'neovm--itree-right)
    (fmakunbound 'neovm--itree-node)
    (fmakunbound 'neovm--itree-insert)
    (fmakunbound 'neovm--itree-overlaps-p)
    (fmakunbound 'neovm--itree-overlap-query)))"#;
    let expect = expect_test::expect![[
        r#""OK (((0 5 \"A\") (3 8 \"B\") (6 10 \"C\")) nil ((15 20 \"D\") (18 25 \"E\")) ((0 5 \"A\") (3 8 \"B\") (6 10 \"C\") (15 20 \"D\") (18 25 \"E\")) ((0 5 \"A\") (3 8 \"B\")) nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: merge overlapping intervals
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interval_tree_merge_overlapping() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Merge overlapping intervals from a flat list
  ;; Input: list of (low high) pairs, possibly unsorted and overlapping
  ;; Output: sorted, merged list of non-overlapping (low high) pairs
  (fset 'neovm--interval-merge
    (lambda (intervals)
      (if (null intervals) nil
        ;; Sort by low endpoint
        (let* ((sorted (sort (copy-sequence intervals)
                             (lambda (a b) (< (car a) (car b)))))
               (result (list (car sorted)))
               (rest (cdr sorted)))
          (dolist (iv rest)
            (let ((last-merged (car (last result))))
              (if (<= (car iv) (cadr last-merged))
                  ;; Overlapping: extend the last merged interval
                  (setcar (cdr last-merged)
                          (max (cadr last-merged) (cadr iv)))
                ;; No overlap: add new interval
                (setq result (append result (list iv))))))
          result))))

  ;; Check if a set of intervals covers a complete range [lo, hi]
  (fset 'neovm--interval-covers-p
    (lambda (intervals lo hi)
      (let ((merged (funcall 'neovm--interval-merge intervals)))
        ;; Check if any single merged interval covers [lo, hi]
        (let ((found nil))
          (dolist (iv merged)
            (when (and (<= (car iv) lo) (>= (cadr iv) hi))
              (setq found t)))
          found))))

  ;; Compute gaps between merged intervals within a range
  (fset 'neovm--interval-gaps
    (lambda (intervals lo hi)
      (let ((merged (funcall 'neovm--interval-merge intervals))
            (gaps nil)
            (current lo))
        (dolist (iv merged)
          (when (and (> (car iv) current) (<= current hi))
            (setq gaps (cons (list current (min (car iv) hi)) gaps)))
          (setq current (max current (cadr iv))))
        (when (< current hi)
          (setq gaps (cons (list current hi) gaps)))
        (nreverse gaps))))

  (unwind-protect
      (list
       ;; Basic merge: non-overlapping
       (funcall 'neovm--interval-merge '((1 3) (5 7) (9 11)))
       ;; Overlapping merge
       (funcall 'neovm--interval-merge '((1 5) (3 8) (6 10)))
       ;; All overlapping into one
       (funcall 'neovm--interval-merge '((1 10) (2 5) (3 7) (4 6)))
       ;; Adjacent intervals (touching)
       (funcall 'neovm--interval-merge '((1 3) (3 5) (5 7)))
       ;; Unsorted input
       (funcall 'neovm--interval-merge '((5 10) (1 3) (8 15) (2 4)))
       ;; Single interval
       (funcall 'neovm--interval-merge '((1 5)))
       ;; Empty
       (funcall 'neovm--interval-merge nil)
       ;; Coverage check
       (funcall 'neovm--interval-covers-p '((1 5) (3 8) (6 10)) 1 10)
       (funcall 'neovm--interval-covers-p '((1 5) (7 10)) 1 10)
       ;; Gap detection
       (funcall 'neovm--interval-gaps '((1 3) (5 7) (10 12)) 0 15)
       (funcall 'neovm--interval-gaps '((0 15)) 0 15)
       (funcall 'neovm--interval-gaps '((2 4) (6 8)) 0 10))
    (fmakunbound 'neovm--interval-merge)
    (fmakunbound 'neovm--interval-covers-p)
    (fmakunbound 'neovm--interval-gaps)))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 3) (5 7) (9 11)) ((1 10)) ((1 10)) ((1 7)) ((1 4) (5 15)) ((1 5)) nil t nil ((0 1) (3 5) (7 10) (12 15)) nil ((0 2) (4 6) (8 10)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: calendar/scheduling with interval tree
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interval_tree_scheduling() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Simple scheduling system using intervals (times as integers for simplicity)
  ;; Each event: (start end title)

  (fset 'neovm--sched-make (lambda () nil))

  (fset 'neovm--sched-add-event
    (lambda (schedule start end title)
      (cons (list start end title) schedule)))

  (fset 'neovm--sched-events-at
    (lambda (schedule time)
      (let ((result nil))
        (dolist (ev schedule)
          (when (and (<= (car ev) time) (< time (cadr ev)))
            (setq result (cons ev result))))
        (sort result (lambda (a b) (< (car a) (car b)))))))

  (fset 'neovm--sched-conflicts
    (lambda (schedule)
      ;; Find all pairs of overlapping events
      (let ((conflicts nil)
            (sorted (sort (copy-sequence schedule)
                          (lambda (a b) (< (car a) (car b))))))
        (let ((i 0))
          (while (< i (length sorted))
            (let ((ev1 (nth i sorted))
                  (j (1+ i)))
              (while (< j (length sorted))
                (let ((ev2 (nth j sorted)))
                  ;; ev2 starts before ev1 ends => conflict
                  (when (< (car ev2) (cadr ev1))
                    (setq conflicts
                          (cons (list (caddr ev1) (caddr ev2)) conflicts))))
                (setq j (1+ j))))
            (setq i (1+ i))))
        (nreverse conflicts))))

  (fset 'neovm--sched-free-slots
    (lambda (schedule day-start day-end)
      (let ((busy (sort (mapcar (lambda (ev) (list (car ev) (cadr ev)))
                                schedule)
                        (lambda (a b) (< (car a) (car b)))))
            (free nil)
            (current day-start))
        (dolist (ev busy)
          (when (> (car ev) current)
            (setq free (cons (list current (car ev)) free)))
          (setq current (max current (cadr ev))))
        (when (< current day-end)
          (setq free (cons (list current day-end) free)))
        (nreverse free))))

  (fset 'neovm--sched-total-busy
    (lambda (schedule)
      ;; Merge overlapping events and sum durations
      (if (null schedule) 0
        (let* ((sorted (sort (mapcar (lambda (ev) (list (car ev) (cadr ev)))
                                    schedule)
                             (lambda (a b) (< (car a) (car b)))))
               (merged (list (car sorted)))
               (rest (cdr sorted)))
          (dolist (iv rest)
            (let ((last-m (car (last merged))))
              (if (<= (car iv) (cadr last-m))
                  (setcar (cdr last-m) (max (cadr last-m) (cadr iv)))
                (setq merged (append merged (list iv))))))
          (let ((total 0))
            (dolist (iv merged)
              (setq total (+ total (- (cadr iv) (car iv)))))
            total)))))

  (unwind-protect
      (let* ((s (funcall 'neovm--sched-make))
             (s (funcall 'neovm--sched-add-event s 900 1000 "standup"))
             (s (funcall 'neovm--sched-add-event s 1000 1130 "design-review"))
             (s (funcall 'neovm--sched-add-event s 1100 1200 "1on1"))
             (s (funcall 'neovm--sched-add-event s 1400 1500 "sprint-planning"))
             (s (funcall 'neovm--sched-add-event s 1600 1700 "retro")))
        (list
         ;; Events at a specific time
         (funcall 'neovm--sched-events-at s 930)
         (funcall 'neovm--sched-events-at s 1100)
         (funcall 'neovm--sched-events-at s 1300)
         ;; Conflicts
         (funcall 'neovm--sched-conflicts s)
         ;; Free slots in a 9-17 day
         (funcall 'neovm--sched-free-slots s 900 1700)
         ;; Total busy time
         (funcall 'neovm--sched-total-busy s)
         ;; Edge: empty schedule
         (funcall 'neovm--sched-free-slots nil 900 1700)
         (funcall 'neovm--sched-total-busy nil)))
    (fmakunbound 'neovm--sched-make)
    (fmakunbound 'neovm--sched-add-event)
    (fmakunbound 'neovm--sched-events-at)
    (fmakunbound 'neovm--sched-conflicts)
    (fmakunbound 'neovm--sched-free-slots)
    (fmakunbound 'neovm--sched-total-busy)))"#;
    let expect = expect_test::expect![[
        r#""OK (((900 1000 \"standup\")) ((1000 1130 \"design-review\") (1100 1200 \"1on1\")) nil ((\"design-review\" \"1on1\")) ((1200 1400) (1500 1600)) 500 ((900 1700)) 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Interval tree: count and size queries
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interval_tree_count_and_size() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Utility functions operating on flat interval lists
  (fset 'neovm--iv-total-span
    (lambda (intervals)
      ;; Total span covered by merged intervals
      (let* ((sorted (sort (copy-sequence intervals)
                           (lambda (a b) (< (car a) (car b)))))
             (merged nil)
             (total 0))
        (dolist (iv sorted)
          (if (and merged (<= (car iv) (cadr (car (last merged)))))
              (setcar (cdr (car (last merged)))
                      (max (cadr (car (last merged))) (cadr iv)))
            (setq merged (append merged (list (list (car iv) (cadr iv)))))))
        (dolist (iv merged)
          (setq total (+ total (- (cadr iv) (car iv)))))
        total)))

  (fset 'neovm--iv-max-overlap
    (lambda (intervals)
      ;; Maximum number of overlapping intervals at any point
      ;; Uses sweep line algorithm with events
      (let ((events nil))
        (dolist (iv intervals)
          (setq events (cons (cons (car iv) 1) events))    ;; start
          (setq events (cons (cons (cadr iv) -1) events))) ;; end
        (setq events (sort events
                           (lambda (a b)
                             (or (< (car a) (car b))
                                 (and (= (car a) (car b))
                                      (< (cdr a) (cdr b)))))))
        (let ((max-depth 0) (current 0))
          (dolist (ev events)
            (setq current (+ current (cdr ev)))
            (setq max-depth (max max-depth current)))
          max-depth))))

  (fset 'neovm--iv-contains-p
    (lambda (intervals lo hi)
      ;; Check if any single interval fully contains [lo, hi]
      (let ((found nil))
        (dolist (iv intervals)
          (when (and (<= (car iv) lo) (>= (cadr iv) hi))
            (setq found t)))
        found)))

  (unwind-protect
      (let ((ivs '((1 5) (3 8) (6 10) (15 20) (18 25) (30 35))))
        (list
         ;; Total span covered (merged)
         (funcall 'neovm--iv-total-span ivs)
         ;; Max overlap depth
         (funcall 'neovm--iv-max-overlap ivs)
         ;; Contains check
         (funcall 'neovm--iv-contains-p ivs 2 4)   ;; (1 5) contains it
         (funcall 'neovm--iv-contains-p ivs 1 10)   ;; no single interval
         (funcall 'neovm--iv-contains-p ivs 15 20)  ;; exact match
         (funcall 'neovm--iv-contains-p ivs 31 34)  ;; (30 35) contains it
         ;; Edge cases
         (funcall 'neovm--iv-total-span '((0 0)))
         (funcall 'neovm--iv-total-span nil)
         (funcall 'neovm--iv-max-overlap '((1 10)))
         ;; Non-overlapping set
         (funcall 'neovm--iv-max-overlap '((1 3) (5 7) (9 11)))
         ;; All overlapping
         (funcall 'neovm--iv-max-overlap '((1 10) (2 9) (3 8) (4 7)))))
    (fmakunbound 'neovm--iv-total-span)
    (fmakunbound 'neovm--iv-max-overlap)
    (fmakunbound 'neovm--iv-contains-p)))"#;
    let expect = expect_test::expect![[r#""OK (24 2 t nil t t 0 0 1 1 4)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
