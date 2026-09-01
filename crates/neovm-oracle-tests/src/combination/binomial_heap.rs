//! Oracle parity tests for a binomial heap implemented in pure Elisp.
//! Binomial tree: (rank key children). Operations: merge, insert,
//! find-minimum, delete-minimum. Complex tests: priority queue with
//! insert-many/extract-min sequence, and heap sort.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Binomial heap core: merge two heaps, insert, find-min
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_binomial_heap_core() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A binomial heap is a list of binomial trees in increasing rank order.
    // A binomial tree of rank k: (rank key children)
    //   where children is a list of binomial trees of ranks k-1, k-2, ..., 0
    // Merge is analogous to binary addition with carry.
    let form = r#"(progn
  ;; Node: (rank key children)
  (fset 'neovm--bh-rank (lambda (node) (car node)))
  (fset 'neovm--bh-key (lambda (node) (cadr node)))
  (fset 'neovm--bh-children (lambda (node) (caddr node)))

  ;; Make a singleton tree (rank 0)
  (fset 'neovm--bh-singleton
    (lambda (key) (list 0 key nil)))

  ;; Link two trees of the same rank: smaller key becomes root
  (fset 'neovm--bh-link
    (lambda (t1 t2)
      (if (<= (funcall 'neovm--bh-key t1) (funcall 'neovm--bh-key t2))
          (list (1+ (funcall 'neovm--bh-rank t1))
                (funcall 'neovm--bh-key t1)
                (cons t2 (funcall 'neovm--bh-children t1)))
        (list (1+ (funcall 'neovm--bh-rank t2))
              (funcall 'neovm--bh-key t2)
              (cons t1 (funcall 'neovm--bh-children t2))))))

  ;; Insert a tree into a heap (list of trees sorted by rank)
  (fset 'neovm--bh-ins-tree
    (lambda (tree heap)
      (if (null heap)
          (list tree)
        (if (< (funcall 'neovm--bh-rank tree) (funcall 'neovm--bh-rank (car heap)))
            (cons tree heap)
          (funcall 'neovm--bh-ins-tree
                   (funcall 'neovm--bh-link tree (car heap))
                   (cdr heap))))))

  ;; Merge two heaps
  (fset 'neovm--bh-merge
    (lambda (h1 h2)
      (cond
       ((null h1) h2)
       ((null h2) h1)
       (t (let ((t1 (car h1)) (t2 (car h2)))
            (cond
             ((< (funcall 'neovm--bh-rank t1) (funcall 'neovm--bh-rank t2))
              (cons t1 (funcall 'neovm--bh-merge (cdr h1) h2)))
             ((> (funcall 'neovm--bh-rank t1) (funcall 'neovm--bh-rank t2))
              (cons t2 (funcall 'neovm--bh-merge h1 (cdr h2))))
             (t (funcall 'neovm--bh-ins-tree
                         (funcall 'neovm--bh-link t1 t2)
                         (funcall 'neovm--bh-merge (cdr h1) (cdr h2))))))))))

  ;; Insert a key into a heap
  (fset 'neovm--bh-insert
    (lambda (key heap)
      (funcall 'neovm--bh-ins-tree (funcall 'neovm--bh-singleton key) heap)))

  ;; Find minimum key in heap
  (fset 'neovm--bh-find-min
    (lambda (heap)
      (if (null heap)
          nil
        (let ((min-key (funcall 'neovm--bh-key (car heap)))
              (rest (cdr heap)))
          (while rest
            (let ((k (funcall 'neovm--bh-key (car rest))))
              (when (< k min-key)
                (setq min-key k)))
            (setq rest (cdr rest)))
          min-key))))

  (unwind-protect
      (let ((h nil))
        ;; Insert some values
        (setq h (funcall 'neovm--bh-insert 5 h))
        (setq h (funcall 'neovm--bh-insert 3 h))
        (setq h (funcall 'neovm--bh-insert 8 h))
        (setq h (funcall 'neovm--bh-insert 1 h))
        (setq h (funcall 'neovm--bh-insert 7 h))
        (list
         ;; Find min should be 1
         (funcall 'neovm--bh-find-min h)
         ;; Number of trees in heap
         (length h)
         ;; Ranks of trees
         (mapcar (lambda (t) (funcall 'neovm--bh-rank t)) h)
         ;; Merge with another heap
         (let ((h2 nil))
           (setq h2 (funcall 'neovm--bh-insert 2 h2))
           (setq h2 (funcall 'neovm--bh-insert 6 h2))
           (let ((merged (funcall 'neovm--bh-merge h h2)))
             (list (funcall 'neovm--bh-find-min merged)
                   (length merged))))))
    (fmakunbound 'neovm--bh-rank)
    (fmakunbound 'neovm--bh-key)
    (fmakunbound 'neovm--bh-children)
    (fmakunbound 'neovm--bh-singleton)
    (fmakunbound 'neovm--bh-link)
    (fmakunbound 'neovm--bh-ins-tree)
    (fmakunbound 'neovm--bh-merge)
    (fmakunbound 'neovm--bh-insert)
    (fmakunbound 'neovm--bh-find-min)))"#;
    let expect = expect_test::expect![[r#""OK (1 2 (0 2) (1 3))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Delete-minimum: reverse children, merge back
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_binomial_heap_delete_min() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--bhd-rank (lambda (node) (car node)))
  (fset 'neovm--bhd-key (lambda (node) (cadr node)))
  (fset 'neovm--bhd-children (lambda (node) (caddr node)))
  (fset 'neovm--bhd-singleton (lambda (key) (list 0 key nil)))

  (fset 'neovm--bhd-link
    (lambda (t1 t2)
      (if (<= (funcall 'neovm--bhd-key t1) (funcall 'neovm--bhd-key t2))
          (list (1+ (funcall 'neovm--bhd-rank t1))
                (funcall 'neovm--bhd-key t1)
                (cons t2 (funcall 'neovm--bhd-children t1)))
        (list (1+ (funcall 'neovm--bhd-rank t2))
              (funcall 'neovm--bhd-key t2)
              (cons t1 (funcall 'neovm--bhd-children t2))))))

  (fset 'neovm--bhd-ins-tree
    (lambda (tree heap)
      (if (null heap) (list tree)
        (if (< (funcall 'neovm--bhd-rank tree) (funcall 'neovm--bhd-rank (car heap)))
            (cons tree heap)
          (funcall 'neovm--bhd-ins-tree
                   (funcall 'neovm--bhd-link tree (car heap))
                   (cdr heap))))))

  (fset 'neovm--bhd-merge
    (lambda (h1 h2)
      (cond
       ((null h1) h2)
       ((null h2) h1)
       (t (let ((t1 (car h1)) (t2 (car h2)))
            (cond
             ((< (funcall 'neovm--bhd-rank t1) (funcall 'neovm--bhd-rank t2))
              (cons t1 (funcall 'neovm--bhd-merge (cdr h1) h2)))
             ((> (funcall 'neovm--bhd-rank t1) (funcall 'neovm--bhd-rank t2))
              (cons t2 (funcall 'neovm--bhd-merge h1 (cdr h2))))
             (t (funcall 'neovm--bhd-ins-tree
                         (funcall 'neovm--bhd-link t1 t2)
                         (funcall 'neovm--bhd-merge (cdr h1) (cdr h2))))))))))

  (fset 'neovm--bhd-insert
    (lambda (key heap)
      (funcall 'neovm--bhd-ins-tree (funcall 'neovm--bhd-singleton key) heap)))

  (fset 'neovm--bhd-find-min
    (lambda (heap)
      (if (null heap) nil
        (let ((min-key (funcall 'neovm--bhd-key (car heap)))
              (rest (cdr heap)))
          (while rest
            (let ((k (funcall 'neovm--bhd-key (car rest))))
              (when (< k min-key) (setq min-key k)))
            (setq rest (cdr rest)))
          min-key))))

  ;; Delete minimum: find min tree, remove from heap, reverse its children, merge
  (fset 'neovm--bhd-remove-min-tree
    (lambda (heap)
      "Return (min-tree . rest-of-heap)."
      (if (null (cdr heap))
          (cons (car heap) nil)
        (let* ((rest-result (funcall 'neovm--bhd-remove-min-tree (cdr heap)))
               (min-rest (car rest-result))
               (heap-rest (cdr rest-result)))
          (if (<= (funcall 'neovm--bhd-key (car heap))
                  (funcall 'neovm--bhd-key min-rest))
              (cons (car heap) (cdr heap))
            (cons min-rest (cons (car heap) heap-rest)))))))

  (fset 'neovm--bhd-delete-min
    (lambda (heap)
      "Delete minimum and return (min-key . new-heap)."
      (if (null heap) nil
        (let* ((result (funcall 'neovm--bhd-remove-min-tree heap))
               (min-tree (car result))
               (rest-heap (cdr result))
               (children (reverse (funcall 'neovm--bhd-children min-tree)))
               (new-heap (funcall 'neovm--bhd-merge children rest-heap)))
          (cons (funcall 'neovm--bhd-key min-tree) new-heap)))))

  (unwind-protect
      (let ((h nil))
        ;; Insert 10, 4, 7, 2, 9, 1, 6
        (dolist (k '(10 4 7 2 9 1 6))
          (setq h (funcall 'neovm--bhd-insert k h)))
        ;; Delete min repeatedly and collect extracted keys
        (let ((extracted nil))
          (let ((i 0))
            (while (and h (< i 7))
              (let ((result (funcall 'neovm--bhd-delete-min h)))
                (setq extracted (cons (car result) extracted))
                (setq h (cdr result)))
              (setq i (1+ i))))
          (list
           ;; Extracted in ascending order
           (nreverse extracted)
           ;; Heap should be empty now
           (null h)
           ;; The sorted order
           (equal (sort (copy-sequence '(10 4 7 2 9 1 6)) #'<)
                  (nreverse (copy-sequence extracted))))))
    (fmakunbound 'neovm--bhd-rank)
    (fmakunbound 'neovm--bhd-key)
    (fmakunbound 'neovm--bhd-children)
    (fmakunbound 'neovm--bhd-singleton)
    (fmakunbound 'neovm--bhd-link)
    (fmakunbound 'neovm--bhd-ins-tree)
    (fmakunbound 'neovm--bhd-merge)
    (fmakunbound 'neovm--bhd-insert)
    (fmakunbound 'neovm--bhd-find-min)
    (fmakunbound 'neovm--bhd-remove-min-tree)
    (fmakunbound 'neovm--bhd-delete-min)))"#;
    let expect = expect_test::expect![[r#""OK ((1 2 4 6 7 9 10) t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Merge property: min of merged heap = min of mins
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_binomial_heap_merge_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--bhm-rank (lambda (n) (car n)))
  (fset 'neovm--bhm-key (lambda (n) (cadr n)))
  (fset 'neovm--bhm-children (lambda (n) (caddr n)))
  (fset 'neovm--bhm-singleton (lambda (k) (list 0 k nil)))

  (fset 'neovm--bhm-link
    (lambda (t1 t2)
      (if (<= (funcall 'neovm--bhm-key t1) (funcall 'neovm--bhm-key t2))
          (list (1+ (funcall 'neovm--bhm-rank t1))
                (funcall 'neovm--bhm-key t1)
                (cons t2 (funcall 'neovm--bhm-children t1)))
        (list (1+ (funcall 'neovm--bhm-rank t2))
              (funcall 'neovm--bhm-key t2)
              (cons t1 (funcall 'neovm--bhm-children t2))))))

  (fset 'neovm--bhm-ins-tree
    (lambda (tree heap)
      (if (null heap) (list tree)
        (if (< (funcall 'neovm--bhm-rank tree) (funcall 'neovm--bhm-rank (car heap)))
            (cons tree heap)
          (funcall 'neovm--bhm-ins-tree
                   (funcall 'neovm--bhm-link tree (car heap))
                   (cdr heap))))))

  (fset 'neovm--bhm-merge
    (lambda (h1 h2)
      (cond
       ((null h1) h2)
       ((null h2) h1)
       (t (let ((t1 (car h1)) (t2 (car h2)))
            (cond
             ((< (funcall 'neovm--bhm-rank t1) (funcall 'neovm--bhm-rank t2))
              (cons t1 (funcall 'neovm--bhm-merge (cdr h1) h2)))
             ((> (funcall 'neovm--bhm-rank t1) (funcall 'neovm--bhm-rank t2))
              (cons t2 (funcall 'neovm--bhm-merge h1 (cdr h2))))
             (t (funcall 'neovm--bhm-ins-tree
                         (funcall 'neovm--bhm-link t1 t2)
                         (funcall 'neovm--bhm-merge (cdr h1) (cdr h2))))))))))

  (fset 'neovm--bhm-insert
    (lambda (key heap)
      (funcall 'neovm--bhm-ins-tree (funcall 'neovm--bhm-singleton key) heap)))

  (fset 'neovm--bhm-find-min
    (lambda (heap)
      (if (null heap) nil
        (let ((min-key (funcall 'neovm--bhm-key (car heap)))
              (rest (cdr heap)))
          (while rest
            (let ((k (funcall 'neovm--bhm-key (car rest))))
              (when (< k min-key) (setq min-key k)))
            (setq rest (cdr rest)))
          min-key))))

  (unwind-protect
      (let ((h1 nil) (h2 nil) (h3 nil))
        ;; Build three heaps
        (dolist (k '(15 3 22 8 11))
          (setq h1 (funcall 'neovm--bhm-insert k h1)))
        (dolist (k '(7 1 19 25 4))
          (setq h2 (funcall 'neovm--bhm-insert k h2)))
        (dolist (k '(30 20 10))
          (setq h3 (funcall 'neovm--bhm-insert k h3)))

        (let ((min1 (funcall 'neovm--bhm-find-min h1))
              (min2 (funcall 'neovm--bhm-find-min h2))
              (min3 (funcall 'neovm--bhm-find-min h3)))
          ;; Merge h1 and h2
          (let* ((m12 (funcall 'neovm--bhm-merge h1 h2))
                 (min12 (funcall 'neovm--bhm-find-min m12)))
            ;; Merge all three
            (let* ((m123 (funcall 'neovm--bhm-merge m12 h3))
                   (min123 (funcall 'neovm--bhm-find-min m123)))
              (list
               min1 min2 min3
               min12
               min123
               ;; min of merge = min of individual mins
               (= min12 (min min1 min2))
               (= min123 (min min1 (min min2 min3)))
               ;; Tree count in merged heaps
               (length m12)
               (length m123))))))
    (fmakunbound 'neovm--bhm-rank)
    (fmakunbound 'neovm--bhm-key)
    (fmakunbound 'neovm--bhm-children)
    (fmakunbound 'neovm--bhm-singleton)
    (fmakunbound 'neovm--bhm-link)
    (fmakunbound 'neovm--bhm-ins-tree)
    (fmakunbound 'neovm--bhm-merge)
    (fmakunbound 'neovm--bhm-insert)
    (fmakunbound 'neovm--bhm-find-min)))"#;
    let expect = expect_test::expect![[r#""OK (3 1 10 1 1 t t 2 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: priority queue — insert many, extract-min in sequence
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_binomial_heap_priority_queue() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--bhq-rank (lambda (n) (car n)))
  (fset 'neovm--bhq-key (lambda (n) (cadr n)))
  (fset 'neovm--bhq-children (lambda (n) (caddr n)))
  (fset 'neovm--bhq-singleton (lambda (k) (list 0 k nil)))

  (fset 'neovm--bhq-link
    (lambda (t1 t2)
      (if (<= (funcall 'neovm--bhq-key t1) (funcall 'neovm--bhq-key t2))
          (list (1+ (funcall 'neovm--bhq-rank t1))
                (funcall 'neovm--bhq-key t1)
                (cons t2 (funcall 'neovm--bhq-children t1)))
        (list (1+ (funcall 'neovm--bhq-rank t2))
              (funcall 'neovm--bhq-key t2)
              (cons t1 (funcall 'neovm--bhq-children t2))))))

  (fset 'neovm--bhq-ins-tree
    (lambda (tree heap)
      (if (null heap) (list tree)
        (if (< (funcall 'neovm--bhq-rank tree) (funcall 'neovm--bhq-rank (car heap)))
            (cons tree heap)
          (funcall 'neovm--bhq-ins-tree
                   (funcall 'neovm--bhq-link tree (car heap))
                   (cdr heap))))))

  (fset 'neovm--bhq-merge
    (lambda (h1 h2)
      (cond
       ((null h1) h2)
       ((null h2) h1)
       (t (let ((t1 (car h1)) (t2 (car h2)))
            (cond
             ((< (funcall 'neovm--bhq-rank t1) (funcall 'neovm--bhq-rank t2))
              (cons t1 (funcall 'neovm--bhq-merge (cdr h1) h2)))
             ((> (funcall 'neovm--bhq-rank t1) (funcall 'neovm--bhq-rank t2))
              (cons t2 (funcall 'neovm--bhq-merge h1 (cdr h2))))
             (t (funcall 'neovm--bhq-ins-tree
                         (funcall 'neovm--bhq-link t1 t2)
                         (funcall 'neovm--bhq-merge (cdr h1) (cdr h2))))))))))

  (fset 'neovm--bhq-insert
    (lambda (key heap)
      (funcall 'neovm--bhq-ins-tree (funcall 'neovm--bhq-singleton key) heap)))

  (fset 'neovm--bhq-find-min
    (lambda (heap)
      (if (null heap) nil
        (let ((min-key (funcall 'neovm--bhq-key (car heap)))
              (rest (cdr heap)))
          (while rest
            (let ((k (funcall 'neovm--bhq-key (car rest))))
              (when (< k min-key) (setq min-key k)))
            (setq rest (cdr rest)))
          min-key))))

  (fset 'neovm--bhq-remove-min-tree
    (lambda (heap)
      (if (null (cdr heap))
          (cons (car heap) nil)
        (let* ((rest-result (funcall 'neovm--bhq-remove-min-tree (cdr heap)))
               (min-rest (car rest-result))
               (heap-rest (cdr rest-result)))
          (if (<= (funcall 'neovm--bhq-key (car heap))
                  (funcall 'neovm--bhq-key min-rest))
              (cons (car heap) (cdr heap))
            (cons min-rest (cons (car heap) heap-rest)))))))

  (fset 'neovm--bhq-delete-min
    (lambda (heap)
      (if (null heap) nil
        (let* ((result (funcall 'neovm--bhq-remove-min-tree heap))
               (min-tree (car result))
               (rest-heap (cdr result))
               (children (reverse (funcall 'neovm--bhq-children min-tree)))
               (new-heap (funcall 'neovm--bhq-merge children rest-heap)))
          (cons (funcall 'neovm--bhq-key min-tree) new-heap)))))

  (unwind-protect
      (let ((pq nil))
        ;; Interleave inserts and extract-mins like a real priority queue
        ;; Insert 50, 20, 40
        (setq pq (funcall 'neovm--bhq-insert 50 pq))
        (setq pq (funcall 'neovm--bhq-insert 20 pq))
        (setq pq (funcall 'neovm--bhq-insert 40 pq))
        ;; Extract min (should be 20)
        (let* ((r1 (funcall 'neovm--bhq-delete-min pq))
               (e1 (car r1)))
          (setq pq (cdr r1))
          ;; Insert 10, 30
          (setq pq (funcall 'neovm--bhq-insert 10 pq))
          (setq pq (funcall 'neovm--bhq-insert 30 pq))
          ;; Extract min (should be 10)
          (let* ((r2 (funcall 'neovm--bhq-delete-min pq))
                 (e2 (car r2)))
            (setq pq (cdr r2))
            ;; Insert 5, 60
            (setq pq (funcall 'neovm--bhq-insert 5 pq))
            (setq pq (funcall 'neovm--bhq-insert 60 pq))
            ;; Extract all remaining
            (let ((remaining nil))
              (while pq
                (let ((r (funcall 'neovm--bhq-delete-min pq)))
                  (setq remaining (cons (car r) remaining))
                  (setq pq (cdr r))))
              (list
               e1 e2
               ;; Remaining should come out sorted
               (nreverse remaining)
               ;; Verify sorted property
               (let ((sorted t)
                     (prev nil)
                     (lst (nreverse (copy-sequence remaining))))
                 (dolist (x lst)
                   (when (and prev (< x prev))
                     (setq sorted nil))
                   (setq prev x))
                 sorted))))))
    (fmakunbound 'neovm--bhq-rank)
    (fmakunbound 'neovm--bhq-key)
    (fmakunbound 'neovm--bhq-children)
    (fmakunbound 'neovm--bhq-singleton)
    (fmakunbound 'neovm--bhq-link)
    (fmakunbound 'neovm--bhq-ins-tree)
    (fmakunbound 'neovm--bhq-merge)
    (fmakunbound 'neovm--bhq-insert)
    (fmakunbound 'neovm--bhq-find-min)
    (fmakunbound 'neovm--bhq-remove-min-tree)
    (fmakunbound 'neovm--bhq-delete-min)))"#;
    let expect = expect_test::expect![[r#""OK (20 10 (5 30 40 50 60) t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: heap sort using binomial heap
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_binomial_heap_sort() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--bhs-rank (lambda (n) (car n)))
  (fset 'neovm--bhs-key (lambda (n) (cadr n)))
  (fset 'neovm--bhs-children (lambda (n) (caddr n)))
  (fset 'neovm--bhs-singleton (lambda (k) (list 0 k nil)))

  (fset 'neovm--bhs-link
    (lambda (t1 t2)
      (if (<= (funcall 'neovm--bhs-key t1) (funcall 'neovm--bhs-key t2))
          (list (1+ (funcall 'neovm--bhs-rank t1))
                (funcall 'neovm--bhs-key t1)
                (cons t2 (funcall 'neovm--bhs-children t1)))
        (list (1+ (funcall 'neovm--bhs-rank t2))
              (funcall 'neovm--bhs-key t2)
              (cons t1 (funcall 'neovm--bhs-children t2))))))

  (fset 'neovm--bhs-ins-tree
    (lambda (tree heap)
      (if (null heap) (list tree)
        (if (< (funcall 'neovm--bhs-rank tree) (funcall 'neovm--bhs-rank (car heap)))
            (cons tree heap)
          (funcall 'neovm--bhs-ins-tree
                   (funcall 'neovm--bhs-link tree (car heap))
                   (cdr heap))))))

  (fset 'neovm--bhs-merge
    (lambda (h1 h2)
      (cond
       ((null h1) h2)
       ((null h2) h1)
       (t (let ((t1 (car h1)) (t2 (car h2)))
            (cond
             ((< (funcall 'neovm--bhs-rank t1) (funcall 'neovm--bhs-rank t2))
              (cons t1 (funcall 'neovm--bhs-merge (cdr h1) h2)))
             ((> (funcall 'neovm--bhs-rank t1) (funcall 'neovm--bhs-rank t2))
              (cons t2 (funcall 'neovm--bhs-merge h1 (cdr h2))))
             (t (funcall 'neovm--bhs-ins-tree
                         (funcall 'neovm--bhs-link t1 t2)
                         (funcall 'neovm--bhs-merge (cdr h1) (cdr h2))))))))))

  (fset 'neovm--bhs-insert
    (lambda (key heap)
      (funcall 'neovm--bhs-ins-tree (funcall 'neovm--bhs-singleton key) heap)))

  (fset 'neovm--bhs-remove-min-tree
    (lambda (heap)
      (if (null (cdr heap))
          (cons (car heap) nil)
        (let* ((rest-result (funcall 'neovm--bhs-remove-min-tree (cdr heap)))
               (min-rest (car rest-result))
               (heap-rest (cdr rest-result)))
          (if (<= (funcall 'neovm--bhs-key (car heap))
                  (funcall 'neovm--bhs-key min-rest))
              (cons (car heap) (cdr heap))
            (cons min-rest (cons (car heap) heap-rest)))))))

  (fset 'neovm--bhs-delete-min
    (lambda (heap)
      (if (null heap) nil
        (let* ((result (funcall 'neovm--bhs-remove-min-tree heap))
               (min-tree (car result))
               (rest-heap (cdr result))
               (children (reverse (funcall 'neovm--bhs-children min-tree)))
               (new-heap (funcall 'neovm--bhs-merge children rest-heap)))
          (cons (funcall 'neovm--bhs-key min-tree) new-heap)))))

  ;; Heap sort: insert all, extract all
  (fset 'neovm--bhs-sort
    (lambda (lst)
      (let ((h nil))
        ;; Insert all elements
        (dolist (k lst) (setq h (funcall 'neovm--bhs-insert k h)))
        ;; Extract all in sorted order
        (let ((result nil))
          (while h
            (let ((r (funcall 'neovm--bhs-delete-min h)))
              (setq result (cons (car r) result))
              (setq h (cdr r))))
          (nreverse result)))))

  (unwind-protect
      (list
       ;; Sort various sequences
       (funcall 'neovm--bhs-sort '(5 3 8 1 9 2 7 4 6))
       (funcall 'neovm--bhs-sort '(1 2 3 4 5))       ;; already sorted
       (funcall 'neovm--bhs-sort '(5 4 3 2 1))       ;; reverse sorted
       (funcall 'neovm--bhs-sort '(42))               ;; single element
       (funcall 'neovm--bhs-sort nil)                  ;; empty
       ;; With duplicates
       (funcall 'neovm--bhs-sort '(3 1 4 1 5 9 2 6 5 3 5))
       ;; Verify against built-in sort
       (equal (funcall 'neovm--bhs-sort '(9 7 5 3 1 8 6 4 2 0))
              (sort (copy-sequence '(9 7 5 3 1 8 6 4 2 0)) #'<)))
    (fmakunbound 'neovm--bhs-rank)
    (fmakunbound 'neovm--bhs-key)
    (fmakunbound 'neovm--bhs-children)
    (fmakunbound 'neovm--bhs-singleton)
    (fmakunbound 'neovm--bhs-link)
    (fmakunbound 'neovm--bhs-ins-tree)
    (fmakunbound 'neovm--bhs-merge)
    (fmakunbound 'neovm--bhs-insert)
    (fmakunbound 'neovm--bhs-remove-min-tree)
    (fmakunbound 'neovm--bhs-delete-min)
    (fmakunbound 'neovm--bhs-sort)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5 6 7 8 9) (1 2 3 4 5) (1 2 3 4 5) (42) nil (1 1 2 3 3 4 5 5 5 6 9) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Structural invariant: rank ordering and tree shapes
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_binomial_heap_structural_invariants() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--bhi-rank (lambda (n) (car n)))
  (fset 'neovm--bhi-key (lambda (n) (cadr n)))
  (fset 'neovm--bhi-children (lambda (n) (caddr n)))
  (fset 'neovm--bhi-singleton (lambda (k) (list 0 k nil)))

  (fset 'neovm--bhi-link
    (lambda (t1 t2)
      (if (<= (funcall 'neovm--bhi-key t1) (funcall 'neovm--bhi-key t2))
          (list (1+ (funcall 'neovm--bhi-rank t1))
                (funcall 'neovm--bhi-key t1)
                (cons t2 (funcall 'neovm--bhi-children t1)))
        (list (1+ (funcall 'neovm--bhi-rank t2))
              (funcall 'neovm--bhi-key t2)
              (cons t1 (funcall 'neovm--bhi-children t2))))))

  (fset 'neovm--bhi-ins-tree
    (lambda (tree heap)
      (if (null heap) (list tree)
        (if (< (funcall 'neovm--bhi-rank tree) (funcall 'neovm--bhi-rank (car heap)))
            (cons tree heap)
          (funcall 'neovm--bhi-ins-tree
                   (funcall 'neovm--bhi-link tree (car heap))
                   (cdr heap))))))

  (fset 'neovm--bhi-merge
    (lambda (h1 h2)
      (cond
       ((null h1) h2)
       ((null h2) h1)
       (t (let ((t1 (car h1)) (t2 (car h2)))
            (cond
             ((< (funcall 'neovm--bhi-rank t1) (funcall 'neovm--bhi-rank t2))
              (cons t1 (funcall 'neovm--bhi-merge (cdr h1) h2)))
             ((> (funcall 'neovm--bhi-rank t1) (funcall 'neovm--bhi-rank t2))
              (cons t2 (funcall 'neovm--bhi-merge h1 (cdr h2))))
             (t (funcall 'neovm--bhi-ins-tree
                         (funcall 'neovm--bhi-link t1 t2)
                         (funcall 'neovm--bhi-merge (cdr h1) (cdr h2))))))))))

  (fset 'neovm--bhi-insert
    (lambda (key heap)
      (funcall 'neovm--bhi-ins-tree (funcall 'neovm--bhi-singleton key) heap)))

  ;; Count total nodes in a tree
  (fset 'neovm--bhi-tree-size
    (lambda (tree)
      (if (null tree) 0
        (let ((count 1))
          (dolist (child (funcall 'neovm--bhi-children tree))
            (setq count (+ count (funcall 'neovm--bhi-tree-size child))))
          count))))

  ;; Check heap-order: parent key <= all children keys
  (fset 'neovm--bhi-heap-ordered-p
    (lambda (tree)
      (if (null tree) t
        (let ((k (funcall 'neovm--bhi-key tree))
              (ok t))
          (dolist (child (funcall 'neovm--bhi-children tree))
            (when (or (< (funcall 'neovm--bhi-key child) k)
                      (not (funcall 'neovm--bhi-heap-ordered-p child)))
              (setq ok nil)))
          ok))))

  (unwind-protect
      (let ((h nil))
        ;; Insert 1 through 15
        (let ((i 1))
          (while (<= i 15)
            (setq h (funcall 'neovm--bhi-insert i h))
            (setq i (1+ i))))
        ;; 15 = 1111 in binary, so should have 4 trees (ranks 0, 1, 2, 3)
        (let ((ranks (mapcar (lambda (t) (funcall 'neovm--bhi-rank t)) h))
              (sizes (mapcar (lambda (t) (funcall 'neovm--bhi-tree-size t)) h))
              (orders (mapcar (lambda (t) (funcall 'neovm--bhi-heap-ordered-p t)) h)))
          (list
           ;; Ranks should be strictly increasing
           ranks
           ;; Size of rank-k tree = 2^k
           sizes
           ;; All trees should be heap-ordered
           orders
           ;; Total nodes = 15
           (apply #'+ sizes)
           ;; Now insert one more (16 = 10000 binary) → single rank-4 tree
           (let ((h16 (funcall 'neovm--bhi-insert 0 h)))
             (list
              (length h16)
              (mapcar (lambda (t) (funcall 'neovm--bhi-rank t)) h16)
              (funcall 'neovm--bhi-tree-size (car h16)))))))
    (fmakunbound 'neovm--bhi-rank)
    (fmakunbound 'neovm--bhi-key)
    (fmakunbound 'neovm--bhi-children)
    (fmakunbound 'neovm--bhi-singleton)
    (fmakunbound 'neovm--bhi-link)
    (fmakunbound 'neovm--bhi-ins-tree)
    (fmakunbound 'neovm--bhi-merge)
    (fmakunbound 'neovm--bhi-insert)
    (fmakunbound 'neovm--bhi-tree-size)
    (fmakunbound 'neovm--bhi-heap-ordered-p)))"#;
    let expect = expect_test::expect![[r#""OK ((0 1 2 3) (1 2 4 8) (t t t t) 15 (1 (4) 16))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Empty heap edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_binomial_heap_empty_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--bhe-rank (lambda (n) (car n)))
  (fset 'neovm--bhe-key (lambda (n) (cadr n)))
  (fset 'neovm--bhe-children (lambda (n) (caddr n)))
  (fset 'neovm--bhe-singleton (lambda (k) (list 0 k nil)))

  (fset 'neovm--bhe-link
    (lambda (t1 t2)
      (if (<= (funcall 'neovm--bhe-key t1) (funcall 'neovm--bhe-key t2))
          (list (1+ (funcall 'neovm--bhe-rank t1))
                (funcall 'neovm--bhe-key t1)
                (cons t2 (funcall 'neovm--bhe-children t1)))
        (list (1+ (funcall 'neovm--bhe-rank t2))
              (funcall 'neovm--bhe-key t2)
              (cons t1 (funcall 'neovm--bhe-children t2))))))

  (fset 'neovm--bhe-ins-tree
    (lambda (tree heap)
      (if (null heap) (list tree)
        (if (< (funcall 'neovm--bhe-rank tree) (funcall 'neovm--bhe-rank (car heap)))
            (cons tree heap)
          (funcall 'neovm--bhe-ins-tree
                   (funcall 'neovm--bhe-link tree (car heap))
                   (cdr heap))))))

  (fset 'neovm--bhe-merge
    (lambda (h1 h2)
      (cond
       ((null h1) h2)
       ((null h2) h1)
       (t (let ((t1 (car h1)) (t2 (car h2)))
            (cond
             ((< (funcall 'neovm--bhe-rank t1) (funcall 'neovm--bhe-rank t2))
              (cons t1 (funcall 'neovm--bhe-merge (cdr h1) h2)))
             ((> (funcall 'neovm--bhe-rank t1) (funcall 'neovm--bhe-rank t2))
              (cons t2 (funcall 'neovm--bhe-merge h1 (cdr h2))))
             (t (funcall 'neovm--bhe-ins-tree
                         (funcall 'neovm--bhe-link t1 t2)
                         (funcall 'neovm--bhe-merge (cdr h1) (cdr h2))))))))))

  (fset 'neovm--bhe-insert
    (lambda (key heap)
      (funcall 'neovm--bhe-ins-tree (funcall 'neovm--bhe-singleton key) heap)))

  (fset 'neovm--bhe-find-min
    (lambda (heap)
      (if (null heap) nil
        (let ((min-key (funcall 'neovm--bhe-key (car heap)))
              (rest (cdr heap)))
          (while rest
            (let ((k (funcall 'neovm--bhe-key (car rest))))
              (when (< k min-key) (setq min-key k)))
            (setq rest (cdr rest)))
          min-key))))

  (unwind-protect
      (list
       ;; Empty heap operations
       (funcall 'neovm--bhe-find-min nil)
       (null nil)
       ;; Merge empty with empty
       (funcall 'neovm--bhe-merge nil nil)
       ;; Merge empty with non-empty
       (let ((h (funcall 'neovm--bhe-insert 42 nil)))
         (list
          (funcall 'neovm--bhe-find-min (funcall 'neovm--bhe-merge nil h))
          (funcall 'neovm--bhe-find-min (funcall 'neovm--bhe-merge h nil))))
       ;; Single element heap
       (let ((h (funcall 'neovm--bhe-insert 99 nil)))
         (list
          (funcall 'neovm--bhe-find-min h)
          (length h)
          (funcall 'neovm--bhe-rank (car h))))
       ;; Negative keys
       (let ((h nil))
         (setq h (funcall 'neovm--bhe-insert -5 h))
         (setq h (funcall 'neovm--bhe-insert -10 h))
         (setq h (funcall 'neovm--bhe-insert 3 h))
         (funcall 'neovm--bhe-find-min h)))
    (fmakunbound 'neovm--bhe-rank)
    (fmakunbound 'neovm--bhe-key)
    (fmakunbound 'neovm--bhe-children)
    (fmakunbound 'neovm--bhe-singleton)
    (fmakunbound 'neovm--bhe-link)
    (fmakunbound 'neovm--bhe-ins-tree)
    (fmakunbound 'neovm--bhe-merge)
    (fmakunbound 'neovm--bhe-insert)
    (fmakunbound 'neovm--bhe-find-min)))"#;
    let expect = expect_test::expect![[r#""OK (nil t nil (42 42) (99 1 0) -10)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
