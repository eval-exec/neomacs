//! Complex combination oracle parity tests: priority queue / binary heap
//! implemented in Elisp. Tests min-heap, max-heap, insert with bubble-up,
//! extract-min/max with sift-down, heapify, heap sort, and priority queue
//! with key-value pairs.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Min-heap: insert, peek, extract-min with bubble-up and sift-down
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_priority_queue_min_heap_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Min-heap: (vector . size) pair
  (fset 'neovm--pq-new (lambda (cap) (cons (make-vector cap nil) 0)))
  (fset 'neovm--pq-size (lambda (h) (cdr h)))
  (fset 'neovm--pq-peek (lambda (h) (aref (car h) 0)))

  (fset 'neovm--pq-swap
    (lambda (h i j)
      (let* ((v (car h)) (tmp (aref v i)))
        (aset v i (aref v j))
        (aset v j tmp))))

  (fset 'neovm--pq-bubble-up
    (lambda (h idx)
      (let ((v (car h)) (i idx))
        (while (and (> i 0)
                    (< (aref v i) (aref v (/ (1- i) 2))))
          (funcall 'neovm--pq-swap h i (/ (1- i) 2))
          (setq i (/ (1- i) 2))))))

  (fset 'neovm--pq-sift-down
    (lambda (h idx)
      (let* ((v (car h)) (n (cdr h)) (i idx) (done nil))
        (while (not done)
          (let ((l (+ (* 2 i) 1)) (r (+ (* 2 i) 2)) (sm i))
            (when (and (< l n) (< (aref v l) (aref v sm))) (setq sm l))
            (when (and (< r n) (< (aref v r) (aref v sm))) (setq sm r))
            (if (/= sm i)
                (progn (funcall 'neovm--pq-swap h i sm) (setq i sm))
              (setq done t)))))))

  (fset 'neovm--pq-insert
    (lambda (h val)
      (let ((sz (cdr h)))
        (aset (car h) sz val)
        (setcdr h (1+ sz))
        (funcall 'neovm--pq-bubble-up h sz))))

  (fset 'neovm--pq-extract-min
    (lambda (h)
      (let* ((v (car h)) (sz (cdr h)) (min-val (aref v 0)))
        (aset v 0 (aref v (1- sz)))
        (aset v (1- sz) nil)
        (setcdr h (1- sz))
        (when (> (cdr h) 0) (funcall 'neovm--pq-sift-down h 0))
        min-val)))

  (unwind-protect
      (let ((h (funcall 'neovm--pq-new 20)))
        ;; Insert elements in disorder
        (dolist (x '(42 17 3 25 1 99 8 13 56 2 7 33 4 88 6))
          (funcall 'neovm--pq-insert h x))
        (let ((peek-val (funcall 'neovm--pq-peek h))
              (total (funcall 'neovm--pq-size h))
              (sorted nil))
          ;; Extract all to verify sorted order
          (while (> (funcall 'neovm--pq-size h) 0)
            (setq sorted (cons (funcall 'neovm--pq-extract-min h) sorted)))
          (let ((result (nreverse sorted)))
            (list peek-val total result
                  ;; Verify sorted
                  (equal result (sort (copy-sequence result) #'<))
                  (funcall 'neovm--pq-size h)))))
    (fmakunbound 'neovm--pq-new)
    (fmakunbound 'neovm--pq-size)
    (fmakunbound 'neovm--pq-peek)
    (fmakunbound 'neovm--pq-swap)
    (fmakunbound 'neovm--pq-bubble-up)
    (fmakunbound 'neovm--pq-sift-down)
    (fmakunbound 'neovm--pq-insert)
    (fmakunbound 'neovm--pq-extract-min)))"#;
    let expect =
        expect_test::expect![[r#""OK (1 15 (1 2 3 4 6 7 8 13 17 25 33 42 56 88 99) t 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Max-heap: insert, extract-max, peek-max
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_priority_queue_max_heap() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--mxh-new (lambda (cap) (cons (make-vector cap nil) 0)))
  (fset 'neovm--mxh-swap
    (lambda (h i j)
      (let* ((v (car h)) (tmp (aref v i)))
        (aset v i (aref v j)) (aset v j tmp))))

  (fset 'neovm--mxh-bubble-up
    (lambda (h idx)
      (let ((v (car h)) (i idx))
        (while (and (> i 0)
                    (> (aref v i) (aref v (/ (1- i) 2))))
          (funcall 'neovm--mxh-swap h i (/ (1- i) 2))
          (setq i (/ (1- i) 2))))))

  (fset 'neovm--mxh-sift-down
    (lambda (h idx)
      (let* ((v (car h)) (n (cdr h)) (i idx) (done nil))
        (while (not done)
          (let ((l (+ (* 2 i) 1)) (r (+ (* 2 i) 2)) (lg i))
            (when (and (< l n) (> (aref v l) (aref v lg))) (setq lg l))
            (when (and (< r n) (> (aref v r) (aref v lg))) (setq lg r))
            (if (/= lg i)
                (progn (funcall 'neovm--mxh-swap h i lg) (setq i lg))
              (setq done t)))))))

  (fset 'neovm--mxh-insert
    (lambda (h val)
      (let ((sz (cdr h)))
        (aset (car h) sz val) (setcdr h (1+ sz))
        (funcall 'neovm--mxh-bubble-up h sz))))

  (fset 'neovm--mxh-extract-max
    (lambda (h)
      (let* ((v (car h)) (sz (cdr h)) (mx (aref v 0)))
        (aset v 0 (aref v (1- sz))) (aset v (1- sz) nil)
        (setcdr h (1- sz))
        (when (> (cdr h) 0) (funcall 'neovm--mxh-sift-down h 0))
        mx)))

  (unwind-protect
      (let ((h (funcall 'neovm--mxh-new 16)))
        (dolist (x '(10 30 20 50 40 60 5 15 25 35))
          (funcall 'neovm--mxh-insert h x))
        ;; Peek should be max
        (let ((peek (aref (car h) 0))
              (desc nil))
          ;; Extract all -> descending order
          (while (> (cdr h) 0)
            (setq desc (cons (funcall 'neovm--mxh-extract-max h) desc)))
          (let ((result (nreverse desc)))
            (list peek
                  result
                  ;; Verify descending
                  (equal result (sort (copy-sequence result) #'>))
                  ;; Should be equivalent to sorted descending
                  (equal result (sort (list 10 30 20 50 40 60 5 15 25 35) #'>))))))
    (fmakunbound 'neovm--mxh-new)
    (fmakunbound 'neovm--mxh-swap)
    (fmakunbound 'neovm--mxh-bubble-up)
    (fmakunbound 'neovm--mxh-sift-down)
    (fmakunbound 'neovm--mxh-insert)
    (fmakunbound 'neovm--mxh-extract-max)))"#;
    let expect = expect_test::expect![[r#""OK (60 (60 50 40 35 30 25 20 15 10 5) t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Heapify an existing list (Floyd's O(n) build)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_priority_queue_heapify() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--hfy-swap
    (lambda (v i j)
      (let ((tmp (aref v i))) (aset v i (aref v j)) (aset v j tmp))))

  (fset 'neovm--hfy-sift-down
    (lambda (v n idx)
      (let ((i idx) (done nil))
        (while (not done)
          (let ((l (+ (* 2 i) 1)) (r (+ (* 2 i) 2)) (sm i))
            (when (and (< l n) (< (aref v l) (aref v sm))) (setq sm l))
            (when (and (< r n) (< (aref v r) (aref v sm))) (setq sm r))
            (if (/= sm i)
                (progn (funcall 'neovm--hfy-swap v i sm) (setq i sm))
              (setq done t)))))))

  (fset 'neovm--hfy-build
    (lambda (lst)
      (let* ((v (apply 'vector lst)) (n (length v))
             (i (1- (/ n 2))))
        (while (>= i 0)
          (funcall 'neovm--hfy-sift-down v n i)
          (setq i (1- i)))
        (cons v n))))

  (fset 'neovm--hfy-drain
    (lambda (h)
      (let ((v (car h)) (sz (cdr h)) (out nil))
        (while (> sz 0)
          (setq out (cons (aref v 0) out))
          (aset v 0 (aref v (1- sz)))
          (aset v (1- sz) nil)
          (setq sz (1- sz))
          (when (> sz 0) (funcall 'neovm--hfy-sift-down v sz 0)))
        (nreverse out))))

  ;; Verify heap property
  (fset 'neovm--hfy-valid-p
    (lambda (h)
      (let* ((v (car h)) (n (cdr h)) (ok t) (i 0))
        (while (< i (/ n 2))
          (let ((l (+ (* 2 i) 1)) (r (+ (* 2 i) 2)))
            (when (and (< l n) (> (aref v i) (aref v l))) (setq ok nil))
            (when (and (< r n) (> (aref v i) (aref v r))) (setq ok nil)))
          (setq i (1+ i)))
        ok)))

  (unwind-protect
      (list
        ;; Random data
        (funcall 'neovm--hfy-drain (funcall 'neovm--hfy-build '(50 20 40 10 30 60 5)))
        ;; Already sorted
        (funcall 'neovm--hfy-drain (funcall 'neovm--hfy-build '(1 2 3 4 5 6 7)))
        ;; Reverse sorted
        (funcall 'neovm--hfy-drain (funcall 'neovm--hfy-build '(7 6 5 4 3 2 1)))
        ;; All same
        (funcall 'neovm--hfy-drain (funcall 'neovm--hfy-build '(4 4 4 4 4)))
        ;; Single element
        (funcall 'neovm--hfy-drain (funcall 'neovm--hfy-build '(99)))
        ;; With negatives and duplicates
        (funcall 'neovm--hfy-drain (funcall 'neovm--hfy-build '(-3 5 -1 7 -3 0 2 5)))
        ;; Heap property valid before drain
        (funcall 'neovm--hfy-valid-p (funcall 'neovm--hfy-build '(88 11 44 22 66 33 77 55)))
        ;; Large-ish data
        (funcall 'neovm--hfy-drain
                 (funcall 'neovm--hfy-build
                          '(15 3 9 1 22 7 4 18 6 11 20 2 14 8 17 5 13 10 19 16 12 21))))
    (fmakunbound 'neovm--hfy-swap)
    (fmakunbound 'neovm--hfy-sift-down)
    (fmakunbound 'neovm--hfy-build)
    (fmakunbound 'neovm--hfy-drain)
    (fmakunbound 'neovm--hfy-valid-p)))"#;
    let expect = expect_test::expect![[
        r#""OK ((5 10 20 30 40 50 60) (1 2 3 4 5 6 7) (1 2 3 4 5 6 7) (4 4 4 4 4) (99) (-3 -3 -1 0 2 5 5 7) t (1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Heap sort (in-place using max-heap for ascending order)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_priority_queue_heap_sort() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--hst-swap
    (lambda (v i j)
      (let ((tmp (aref v i))) (aset v i (aref v j)) (aset v j tmp))))

  (fset 'neovm--hst-max-sift
    (lambda (v n idx)
      (let ((i idx) (done nil))
        (while (not done)
          (let ((l (+ (* 2 i) 1)) (r (+ (* 2 i) 2)) (lg i))
            (when (and (< l n) (> (aref v l) (aref v lg))) (setq lg l))
            (when (and (< r n) (> (aref v r) (aref v lg))) (setq lg r))
            (if (/= lg i)
                (progn (funcall 'neovm--hst-swap v i lg) (setq i lg))
              (setq done t)))))))

  (fset 'neovm--hst-sort
    (lambda (lst)
      (let* ((v (apply 'vector lst)) (n (length v)))
        ;; Build max-heap
        (let ((i (1- (/ n 2))))
          (while (>= i 0)
            (funcall 'neovm--hst-max-sift v n i)
            (setq i (1- i))))
        ;; Extract max repeatedly
        (let ((end (1- n)))
          (while (> end 0)
            (funcall 'neovm--hst-swap v 0 end)
            (funcall 'neovm--hst-max-sift v end 0)
            (setq end (1- end))))
        (append v nil))))

  (unwind-protect
      (list
        (funcall 'neovm--hst-sort '(38 27 43 3 9 82 10))
        ;; Verify against built-in sort
        (equal (funcall 'neovm--hst-sort '(38 27 43 3 9 82 10))
               (sort (list 38 27 43 3 9 82 10) #'<))
        ;; Already sorted
        (funcall 'neovm--hst-sort '(1 2 3 4 5))
        ;; Reverse
        (funcall 'neovm--hst-sort '(5 4 3 2 1))
        ;; Duplicates
        (funcall 'neovm--hst-sort '(3 1 4 1 5 9 2 6 5 3 5))
        ;; Negatives
        (funcall 'neovm--hst-sort '(-10 -3 -7 -1 -5 -8))
        ;; Mixed
        (funcall 'neovm--hst-sort '(0 -5 3 -2 7 -9 1 4 -6 8))
        ;; Single
        (funcall 'neovm--hst-sort '(42))
        ;; Empty
        (funcall 'neovm--hst-sort nil))
    (fmakunbound 'neovm--hst-swap)
    (fmakunbound 'neovm--hst-max-sift)
    (fmakunbound 'neovm--hst-sort)))"#;
    let expect = expect_test::expect![[
        r#""OK ((3 9 10 27 38 43 82) t (1 2 3 4 5) (1 2 3 4 5) (1 1 2 3 3 4 5 5 5 6 9) (-10 -8 -7 -5 -3 -1) (-9 -6 -5 -2 0 1 3 4 7 8) (42) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Priority queue with (priority . value) key-value pairs
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_priority_queue_key_value_pairs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--kvpq-new (lambda (cap) (cons (make-vector cap nil) 0)))

  (fset 'neovm--kvpq-swap
    (lambda (h i j)
      (let* ((v (car h)) (tmp (aref v i)))
        (aset v i (aref v j)) (aset v j tmp))))

  (fset 'neovm--kvpq-up
    (lambda (h idx)
      (let ((v (car h)) (i idx))
        (while (and (> i 0)
                    (< (car (aref v i)) (car (aref v (/ (1- i) 2)))))
          (funcall 'neovm--kvpq-swap h i (/ (1- i) 2))
          (setq i (/ (1- i) 2))))))

  (fset 'neovm--kvpq-down
    (lambda (h idx)
      (let* ((v (car h)) (n (cdr h)) (i idx) (done nil))
        (while (not done)
          (let ((l (+ (* 2 i) 1)) (r (+ (* 2 i) 2)) (sm i))
            (when (and (< l n) (< (car (aref v l)) (car (aref v sm)))) (setq sm l))
            (when (and (< r n) (< (car (aref v r)) (car (aref v sm)))) (setq sm r))
            (if (/= sm i)
                (progn (funcall 'neovm--kvpq-swap h i sm) (setq i sm))
              (setq done t)))))))

  (fset 'neovm--kvpq-push
    (lambda (h priority value)
      (let ((sz (cdr h)))
        (aset (car h) sz (cons priority value))
        (setcdr h (1+ sz))
        (funcall 'neovm--kvpq-up h sz))))

  (fset 'neovm--kvpq-pop
    (lambda (h)
      (let* ((v (car h)) (sz (cdr h)) (top (aref v 0)))
        (aset v 0 (aref v (1- sz)))
        (aset v (1- sz) nil)
        (setcdr h (1- sz))
        (when (> (cdr h) 0) (funcall 'neovm--kvpq-down h 0))
        top)))

  (unwind-protect
      (let ((pq (funcall 'neovm--kvpq-new 20)))
        ;; Task scheduling scenario
        (funcall 'neovm--kvpq-push pq 5 'low-priority-task)
        (funcall 'neovm--kvpq-push pq 1 'critical-bug-fix)
        (funcall 'neovm--kvpq-push pq 3 'normal-feature)
        (funcall 'neovm--kvpq-push pq 1 'security-patch)
        (funcall 'neovm--kvpq-push pq 2 'high-priority-refactor)
        (funcall 'neovm--kvpq-push pq 4 'nice-to-have)
        (funcall 'neovm--kvpq-push pq 2 'performance-fix)
        (funcall 'neovm--kvpq-push pq 3 'documentation)
        ;; Dequeue all
        (let ((tasks nil))
          (while (> (cdr pq) 0)
            (setq tasks (cons (funcall 'neovm--kvpq-pop pq) tasks)))
          (let ((ordered (nreverse tasks)))
            (list
              ;; All dequeued tasks
              ordered
              ;; Priorities are non-decreasing
              (let ((ok t) (prev 0))
                (dolist (t ordered)
                  (when (< (car t) prev) (setq ok nil))
                  (setq prev (car t)))
                ok)
              ;; Total count
              (length ordered)
              ;; Queue empty
              (cdr pq)))))
    (fmakunbound 'neovm--kvpq-new)
    (fmakunbound 'neovm--kvpq-swap)
    (fmakunbound 'neovm--kvpq-up)
    (fmakunbound 'neovm--kvpq-down)
    (fmakunbound 'neovm--kvpq-push)
    (fmakunbound 'neovm--kvpq-pop)))"#;
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// k-way merge of sorted lists using a min-heap
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_priority_queue_k_way_merge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Heap entries: (value . remaining-list)
  (fset 'neovm--kwm-new (lambda (cap) (cons (make-vector cap nil) 0)))

  (fset 'neovm--kwm-swap
    (lambda (h i j)
      (let* ((v (car h)) (tmp (aref v i)))
        (aset v i (aref v j)) (aset v j tmp))))

  (fset 'neovm--kwm-up
    (lambda (h idx)
      (let ((v (car h)) (i idx))
        (while (and (> i 0)
                    (< (car (aref v i)) (car (aref v (/ (1- i) 2)))))
          (funcall 'neovm--kwm-swap h i (/ (1- i) 2))
          (setq i (/ (1- i) 2))))))

  (fset 'neovm--kwm-down
    (lambda (h idx)
      (let* ((v (car h)) (n (cdr h)) (i idx) (done nil))
        (while (not done)
          (let ((l (+ (* 2 i) 1)) (r (+ (* 2 i) 2)) (sm i))
            (when (and (< l n) (< (car (aref v l)) (car (aref v sm)))) (setq sm l))
            (when (and (< r n) (< (car (aref v r)) (car (aref v sm)))) (setq sm r))
            (if (/= sm i)
                (progn (funcall 'neovm--kwm-swap h i sm) (setq i sm))
              (setq done t)))))))

  (fset 'neovm--kwm-push
    (lambda (h entry)
      (let ((sz (cdr h)))
        (aset (car h) sz entry)
        (setcdr h (1+ sz))
        (funcall 'neovm--kwm-up h sz))))

  (fset 'neovm--kwm-pop
    (lambda (h)
      (let* ((v (car h)) (sz (cdr h)) (top (aref v 0)))
        (aset v 0 (aref v (1- sz)))
        (aset v (1- sz) nil)
        (setcdr h (1- sz))
        (when (> (cdr h) 0) (funcall 'neovm--kwm-down h 0))
        top)))

  (fset 'neovm--kwm-merge
    (lambda (lists)
      (let ((h (funcall 'neovm--kwm-new (length lists))))
        ;; Seed with first element of each non-empty list
        (dolist (lst lists)
          (when lst
            (funcall 'neovm--kwm-push h (cons (car lst) (cdr lst)))))
        ;; Extract min, push next from same list
        (let ((result nil))
          (while (> (cdr h) 0)
            (let ((entry (funcall 'neovm--kwm-pop h)))
              (setq result (cons (car entry) result))
              (when (cdr entry)
                (funcall 'neovm--kwm-push h
                         (cons (cadr entry) (cddr entry))))))
          (nreverse result)))))

  (unwind-protect
      (list
        ;; 3 equal-length sorted lists
        (funcall 'neovm--kwm-merge '((1 4 7 10) (2 5 8 11) (3 6 9 12)))
        ;; Varying lengths
        (funcall 'neovm--kwm-merge '((1 100) (2 3 4 5 6) (50)))
        ;; With empty lists
        (funcall 'neovm--kwm-merge '(() (1 2 3) () (4 5) ()))
        ;; Single list
        (funcall 'neovm--kwm-merge '((10 20 30 40 50)))
        ;; Overlapping ranges
        (funcall 'neovm--kwm-merge '((1 3 5) (2 3 4) (1 6 7)))
        ;; Verify correctness
        (let ((merged (funcall 'neovm--kwm-merge '((10 30 50 70) (20 40 60 80) (5 15 25 35 45 55)))))
          (list merged
                (equal merged (sort (copy-sequence merged) #'<)))))
    (fmakunbound 'neovm--kwm-new)
    (fmakunbound 'neovm--kwm-swap)
    (fmakunbound 'neovm--kwm-up)
    (fmakunbound 'neovm--kwm-down)
    (fmakunbound 'neovm--kwm-push)
    (fmakunbound 'neovm--kwm-pop)
    (fmakunbound 'neovm--kwm-merge)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5 6 7 8 9 10 11 12) (1 2 3 4 5 6 50 100) (1 2 3 4 5) (10 20 30 40 50) (1 1 2 3 3 4 5 6 7) ((5 10 15 20 25 30 35 40 45 50 55 60 70 80) t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Find k-th smallest element using a max-heap of size k
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_priority_queue_kth_smallest() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--kth-new (lambda (cap) (cons (make-vector cap nil) 0)))

  (fset 'neovm--kth-swap
    (lambda (h i j)
      (let* ((v (car h)) (tmp (aref v i)))
        (aset v i (aref v j)) (aset v j tmp))))

  (fset 'neovm--kth-up-max
    (lambda (h idx)
      (let ((v (car h)) (i idx))
        (while (and (> i 0)
                    (> (aref v i) (aref v (/ (1- i) 2))))
          (funcall 'neovm--kth-swap h i (/ (1- i) 2))
          (setq i (/ (1- i) 2))))))

  (fset 'neovm--kth-down-max
    (lambda (h idx)
      (let* ((v (car h)) (n (cdr h)) (i idx) (done nil))
        (while (not done)
          (let ((l (+ (* 2 i) 1)) (r (+ (* 2 i) 2)) (lg i))
            (when (and (< l n) (> (aref v l) (aref v lg))) (setq lg l))
            (when (and (< r n) (> (aref v r) (aref v lg))) (setq lg r))
            (if (/= lg i)
                (progn (funcall 'neovm--kth-swap h i lg) (setq i lg))
              (setq done t)))))))

  ;; Find k-th smallest (1-indexed): maintain max-heap of size k
  (fset 'neovm--kth-smallest
    (lambda (lst k)
      (let ((h (funcall 'neovm--kth-new k)))
        (dolist (x lst)
          (if (< (cdr h) k)
              (progn
                (aset (car h) (cdr h) x)
                (setcdr h (1+ (cdr h)))
                (funcall 'neovm--kth-up-max h (1- (cdr h))))
            (when (< x (aref (car h) 0))
              (aset (car h) 0 x)
              (funcall 'neovm--kth-down-max h 0))))
        ;; Root of max-heap is the k-th smallest
        (aref (car h) 0))))

  (unwind-protect
      (let ((data '(35 12 48 7 22 19 3 56 41 8 29 14 33 1 17)))
        (list
          ;; k=1: minimum
          (funcall 'neovm--kth-smallest data 1)
          ;; k=3: 3rd smallest
          (funcall 'neovm--kth-smallest data 3)
          ;; k=5: 5th smallest
          (funcall 'neovm--kth-smallest data 5)
          ;; k=n: maximum
          (funcall 'neovm--kth-smallest data (length data))
          ;; k=1 on single element
          (funcall 'neovm--kth-smallest '(42) 1)
          ;; With duplicates
          (funcall 'neovm--kth-smallest '(5 3 5 3 5 3 1 1) 4)
          ;; Verify against sorted reference
          (let ((sorted (sort (copy-sequence data) #'<)))
            (list
              (= (funcall 'neovm--kth-smallest data 1) (nth 0 sorted))
              (= (funcall 'neovm--kth-smallest data 5) (nth 4 sorted))
              (= (funcall 'neovm--kth-smallest data 10) (nth 9 sorted))))))
    (fmakunbound 'neovm--kth-new)
    (fmakunbound 'neovm--kth-swap)
    (fmakunbound 'neovm--kth-up-max)
    (fmakunbound 'neovm--kth-down-max)
    (fmakunbound 'neovm--kth-smallest)))"#;
    let expect = expect_test::expect![[r#""OK (1 7 12 56 42 3 (t t t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Median maintenance using two heaps (max-heap for lower half, min-heap for upper)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_priority_queue_running_median() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Generic heap with comparator stored in the heap structure
  ;; (vector . size) -- we use separate up/down functions for min/max
  (fset 'neovm--med-new (lambda (cap) (cons (make-vector cap nil) 0)))
  (fset 'neovm--med-swap
    (lambda (h i j)
      (let* ((v (car h)) (tmp (aref v i)))
        (aset v i (aref v j)) (aset v j tmp))))

  ;; Min-heap operations
  (fset 'neovm--med-min-up
    (lambda (h idx)
      (let ((v (car h)) (i idx))
        (while (and (> i 0) (< (aref v i) (aref v (/ (1- i) 2))))
          (funcall 'neovm--med-swap h i (/ (1- i) 2))
          (setq i (/ (1- i) 2))))))
  (fset 'neovm--med-min-down
    (lambda (h idx)
      (let* ((v (car h)) (n (cdr h)) (i idx) (done nil))
        (while (not done)
          (let ((l (+ (* 2 i) 1)) (r (+ (* 2 i) 2)) (sm i))
            (when (and (< l n) (< (aref v l) (aref v sm))) (setq sm l))
            (when (and (< r n) (< (aref v r) (aref v sm))) (setq sm r))
            (if (/= sm i)
                (progn (funcall 'neovm--med-swap h i sm) (setq i sm))
              (setq done t)))))))

  ;; Max-heap operations
  (fset 'neovm--med-max-up
    (lambda (h idx)
      (let ((v (car h)) (i idx))
        (while (and (> i 0) (> (aref v i) (aref v (/ (1- i) 2))))
          (funcall 'neovm--med-swap h i (/ (1- i) 2))
          (setq i (/ (1- i) 2))))))
  (fset 'neovm--med-max-down
    (lambda (h idx)
      (let* ((v (car h)) (n (cdr h)) (i idx) (done nil))
        (while (not done)
          (let ((l (+ (* 2 i) 1)) (r (+ (* 2 i) 2)) (lg i))
            (when (and (< l n) (> (aref v l) (aref v lg))) (setq lg l))
            (when (and (< r n) (> (aref v r) (aref v lg))) (setq lg r))
            (if (/= lg i)
                (progn (funcall 'neovm--med-swap h i lg) (setq i lg))
              (setq done t)))))))

  (fset 'neovm--med-push-min
    (lambda (h val)
      (aset (car h) (cdr h) val) (setcdr h (1+ (cdr h)))
      (funcall 'neovm--med-min-up h (1- (cdr h)))))
  (fset 'neovm--med-pop-min
    (lambda (h)
      (let* ((v (car h)) (top (aref v 0)))
        (aset v 0 (aref v (1- (cdr h)))) (aset v (1- (cdr h)) nil)
        (setcdr h (1- (cdr h)))
        (when (> (cdr h) 0) (funcall 'neovm--med-min-down h 0))
        top)))
  (fset 'neovm--med-push-max
    (lambda (h val)
      (aset (car h) (cdr h) val) (setcdr h (1+ (cdr h)))
      (funcall 'neovm--med-max-up h (1- (cdr h)))))
  (fset 'neovm--med-pop-max
    (lambda (h)
      (let* ((v (car h)) (top (aref v 0)))
        (aset v 0 (aref v (1- (cdr h)))) (aset v (1- (cdr h)) nil)
        (setcdr h (1- (cdr h)))
        (when (> (cdr h) 0) (funcall 'neovm--med-max-down h 0))
        top)))

  ;; Add number and return current median (lower median for even count)
  (fset 'neovm--med-add
    (lambda (lo hi val)
      ;; lo = max-heap (lower half), hi = min-heap (upper half)
      (if (or (= (cdr lo) 0) (<= val (aref (car lo) 0)))
          (funcall 'neovm--med-push-max lo val)
        (funcall 'neovm--med-push-min hi val))
      ;; Rebalance: lo.size can be at most 1 more than hi.size
      (when (> (cdr lo) (+ (cdr hi) 1))
        (funcall 'neovm--med-push-min hi (funcall 'neovm--med-pop-max lo)))
      (when (> (cdr hi) (cdr lo))
        (funcall 'neovm--med-push-max lo (funcall 'neovm--med-pop-min hi)))
      ;; Median is top of lo (max-heap)
      (aref (car lo) 0)))

  (unwind-protect
      (let ((lo (funcall 'neovm--med-new 16))
            (hi (funcall 'neovm--med-new 16))
            (medians nil))
        ;; Feed numbers one by one, collect running medians
        (dolist (x '(5 15 1 3 8 7 9 10 6 2))
          (setq medians (cons (funcall 'neovm--med-add lo hi x) medians)))
        (nreverse medians))
    (fmakunbound 'neovm--med-new)
    (fmakunbound 'neovm--med-swap)
    (fmakunbound 'neovm--med-min-up)
    (fmakunbound 'neovm--med-min-down)
    (fmakunbound 'neovm--med-max-up)
    (fmakunbound 'neovm--med-max-down)
    (fmakunbound 'neovm--med-push-min)
    (fmakunbound 'neovm--med-pop-min)
    (fmakunbound 'neovm--med-push-max)
    (fmakunbound 'neovm--med-pop-max)
    (fmakunbound 'neovm--med-add)))"#;
    let expect = expect_test::expect![[r#""OK (5 5 5 3 5 5 7 7 7 6)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
