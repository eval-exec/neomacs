//! Oracle parity tests for binary min-heap data structure in Elisp.
//!
//! Implements a binary min-heap using vectors with: heap-insert,
//! heap-extract-min, heapify, heap-sort, and priority queue patterns.
//! Tests with numeric and string priorities.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Core binary min-heap: insert and extract-min with numeric keys
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_heap_insert_extract_min() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Binary min-heap using a vector + size counter
  ;; Heap is (vector . size) pair
  (fset 'neovm--heap-create
    (lambda (capacity)
      (cons (make-vector capacity nil) 0)))

  (fset 'neovm--heap-parent (lambda (i) (/ (1- i) 2)))
  (fset 'neovm--heap-left   (lambda (i) (+ (* 2 i) 1)))
  (fset 'neovm--heap-right  (lambda (i) (+ (* 2 i) 2)))

  (fset 'neovm--heap-swap
    (lambda (heap i j)
      (let* ((v (car heap))
             (tmp (aref v i)))
        (aset v i (aref v j))
        (aset v j tmp))))

  (fset 'neovm--heap-sift-up
    (lambda (heap idx)
      (let ((v (car heap))
            (i idx))
        (while (and (> i 0)
                    (< (aref v i) (aref v (funcall 'neovm--heap-parent i))))
          (funcall 'neovm--heap-swap heap i (funcall 'neovm--heap-parent i))
          (setq i (funcall 'neovm--heap-parent i))))))

  (fset 'neovm--heap-sift-down
    (lambda (heap idx)
      (let* ((v (car heap))
             (size (cdr heap))
             (i idx)
             (done nil))
        (while (not done)
          (let ((left (funcall 'neovm--heap-left i))
                (right (funcall 'neovm--heap-right i))
                (smallest i))
            (when (and (< left size)
                       (< (aref v left) (aref v smallest)))
              (setq smallest left))
            (when (and (< right size)
                       (< (aref v right) (aref v smallest)))
              (setq smallest right))
            (if (/= smallest i)
                (progn
                  (funcall 'neovm--heap-swap heap i smallest)
                  (setq i smallest))
              (setq done t)))))))

  (fset 'neovm--heap-insert
    (lambda (heap val)
      (let* ((v (car heap))
             (size (cdr heap)))
        (aset v size val)
        (setcdr heap (1+ size))
        (funcall 'neovm--heap-sift-up heap size))))

  (fset 'neovm--heap-extract-min
    (lambda (heap)
      (let* ((v (car heap))
             (size (cdr heap))
             (min-val (aref v 0)))
        (aset v 0 (aref v (1- size)))
        (aset v (1- size) nil)
        (setcdr heap (1- size))
        (when (> (cdr heap) 0)
          (funcall 'neovm--heap-sift-down heap 0))
        min-val)))

  (fset 'neovm--heap-peek
    (lambda (heap) (aref (car heap) 0)))

  (fset 'neovm--heap-size
    (lambda (heap) (cdr heap)))

  (unwind-protect
      (let ((h (funcall 'neovm--heap-create 16)))
        ;; Insert values in random order
        (funcall 'neovm--heap-insert h 5)
        (funcall 'neovm--heap-insert h 3)
        (funcall 'neovm--heap-insert h 8)
        (funcall 'neovm--heap-insert h 1)
        (funcall 'neovm--heap-insert h 9)
        (funcall 'neovm--heap-insert h 2)
        (funcall 'neovm--heap-insert h 7)
        (let ((peek-val (funcall 'neovm--heap-peek h))
              (size-before (funcall 'neovm--heap-size h)))
          ;; Extract all in order
          (let ((results nil))
            (while (> (funcall 'neovm--heap-size h) 0)
              (setq results (cons (funcall 'neovm--heap-extract-min h)
                                  results)))
            (list
             peek-val
             size-before
             (nreverse results)
             (funcall 'neovm--heap-size h)))))
    (fmakunbound 'neovm--heap-create)
    (fmakunbound 'neovm--heap-parent)
    (fmakunbound 'neovm--heap-left)
    (fmakunbound 'neovm--heap-right)
    (fmakunbound 'neovm--heap-swap)
    (fmakunbound 'neovm--heap-sift-up)
    (fmakunbound 'neovm--heap-sift-down)
    (fmakunbound 'neovm--heap-insert)
    (fmakunbound 'neovm--heap-extract-min)
    (fmakunbound 'neovm--heap-peek)
    (fmakunbound 'neovm--heap-size)))"#;
    let expect = expect_test::expect![[r#""OK (1 7 (1 2 3 5 7 8 9) 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Heapify: build a heap from an existing list in O(n)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_heap_heapify() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--hp-swap
    (lambda (v i j)
      (let ((tmp (aref v i)))
        (aset v i (aref v j))
        (aset v j tmp))))

  (fset 'neovm--hp-sift-down
    (lambda (v size idx)
      (let ((i idx) (done nil))
        (while (not done)
          (let ((left (+ (* 2 i) 1))
                (right (+ (* 2 i) 2))
                (smallest i))
            (when (and (< left size)
                       (< (aref v left) (aref v smallest)))
              (setq smallest left))
            (when (and (< right size)
                       (< (aref v right) (aref v smallest)))
              (setq smallest right))
            (if (/= smallest i)
                (progn
                  (funcall 'neovm--hp-swap v i smallest)
                  (setq i smallest))
              (setq done t)))))))

  ;; Build min-heap in-place (Floyd's algorithm)
  (fset 'neovm--hp-heapify
    (lambda (lst)
      (let* ((v (apply 'vector lst))
             (n (length v))
             (i (1- (/ n 2))))
        (while (>= i 0)
          (funcall 'neovm--hp-sift-down v n i)
          (setq i (1- i)))
        (cons v n))))

  ;; Extract all in sorted order
  (fset 'neovm--hp-drain
    (lambda (heap)
      (let ((v (car heap))
            (size (cdr heap))
            (result nil))
        (while (> size 0)
          (setq result (cons (aref v 0) result))
          (aset v 0 (aref v (1- size)))
          (aset v (1- size) nil)
          (setq size (1- size))
          (when (> size 0)
            (funcall 'neovm--hp-sift-down v size 0)))
        (nreverse result))))

  (unwind-protect
      (list
       ;; Heapify random data and drain
       (funcall 'neovm--hp-drain
                (funcall 'neovm--hp-heapify '(9 4 7 1 3 8 2 6 5)))
       ;; Already sorted
       (funcall 'neovm--hp-drain
                (funcall 'neovm--hp-heapify '(1 2 3 4 5)))
       ;; Reverse sorted
       (funcall 'neovm--hp-drain
                (funcall 'neovm--hp-heapify '(5 4 3 2 1)))
       ;; Duplicates
       (funcall 'neovm--hp-drain
                (funcall 'neovm--hp-heapify '(3 1 4 1 5 9 2 6 5 3)))
       ;; Single element
       (funcall 'neovm--hp-drain
                (funcall 'neovm--hp-heapify '(42)))
       ;; Two elements
       (funcall 'neovm--hp-drain
                (funcall 'neovm--hp-heapify '(7 2)))
       ;; Verify heap property after heapify: root <= children
       (let* ((h (funcall 'neovm--hp-heapify '(15 8 23 4 42 16 7 1 11)))
              (v (car h))
              (n (cdr h))
              (valid t))
         (let ((i 0))
           (while (< i (/ n 2))
             (let ((left (+ (* 2 i) 1))
                   (right (+ (* 2 i) 2)))
               (when (and (< left n) (> (aref v i) (aref v left)))
                 (setq valid nil))
               (when (and (< right n) (> (aref v i) (aref v right)))
                 (setq valid nil)))
             (setq i (1+ i))))
         valid))
    (fmakunbound 'neovm--hp-swap)
    (fmakunbound 'neovm--hp-sift-down)
    (fmakunbound 'neovm--hp-heapify)
    (fmakunbound 'neovm--hp-drain)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5 6 7 8 9) (1 2 3 4 5) (1 2 3 4 5) (1 1 2 3 3 4 5 5 6 9) (42) (2 7) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Heap sort: sort a list using a heap
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_heap_sort() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--hs-swap
    (lambda (v i j)
      (let ((tmp (aref v i)))
        (aset v i (aref v j))
        (aset v j tmp))))

  ;; Max-heap sift-down for ascending sort
  (fset 'neovm--hs-sift-down-max
    (lambda (v size idx)
      (let ((i idx) (done nil))
        (while (not done)
          (let ((left (+ (* 2 i) 1))
                (right (+ (* 2 i) 2))
                (largest i))
            (when (and (< left size)
                       (> (aref v left) (aref v largest)))
              (setq largest left))
            (when (and (< right size)
                       (> (aref v right) (aref v largest)))
              (setq largest right))
            (if (/= largest i)
                (progn
                  (funcall 'neovm--hs-swap v i largest)
                  (setq i largest))
              (setq done t)))))))

  (fset 'neovm--hs-heapsort
    (lambda (lst)
      (let* ((v (apply 'vector lst))
             (n (length v)))
        ;; Build max-heap
        (let ((i (1- (/ n 2))))
          (while (>= i 0)
            (funcall 'neovm--hs-sift-down-max v n i)
            (setq i (1- i))))
        ;; Extract max elements
        (let ((end (1- n)))
          (while (> end 0)
            (funcall 'neovm--hs-swap v 0 end)
            (funcall 'neovm--hs-sift-down-max v end 0)
            (setq end (1- end))))
        (append v nil))))

  (unwind-protect
      (list
       ;; Random data
       (funcall 'neovm--hs-heapsort '(38 27 43 3 9 82 10))
       ;; Verify matches built-in sort
       (equal (funcall 'neovm--hs-heapsort '(38 27 43 3 9 82 10))
              (sort (list 38 27 43 3 9 82 10) #'<))
       ;; Already sorted
       (funcall 'neovm--hs-heapsort '(1 2 3 4 5 6 7 8 9 10))
       ;; Reverse sorted
       (funcall 'neovm--hs-heapsort '(10 9 8 7 6 5 4 3 2 1))
       ;; All duplicates
       (funcall 'neovm--hs-heapsort '(5 5 5 5 5))
       ;; Negative numbers
       (funcall 'neovm--hs-heapsort '(-3 -1 -4 -1 -5 -9 -2 -6))
       ;; Mixed positive and negative
       (funcall 'neovm--hs-heapsort '(3 -1 4 -1 5 -9 2 -6))
       ;; Single element
       (funcall 'neovm--hs-heapsort '(42))
       ;; Two elements reversed
       (funcall 'neovm--hs-heapsort '(7 2)))
    (fmakunbound 'neovm--hs-swap)
    (fmakunbound 'neovm--hs-sift-down-max)
    (fmakunbound 'neovm--hs-heapsort)))"#;
    let expect = expect_test::expect![[
        r#""OK ((3 9 10 27 38 43 82) t (1 2 3 4 5 6 7 8 9 10) (1 2 3 4 5 6 7 8 9 10) (5 5 5 5 5) (-9 -6 -5 -4 -3 -2 -1 -1) (-9 -6 -1 -1 2 3 4 5) (42) (2 7))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Priority queue with (priority . value) pairs
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_heap_priority_queue_pairs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Priority queue storing (priority . value) pairs in a min-heap
  (fset 'neovm--pq-create
    (lambda (capacity)
      (cons (make-vector capacity nil) 0)))

  (fset 'neovm--pq-swap
    (lambda (v i j)
      (let ((tmp (aref v i)))
        (aset v i (aref v j))
        (aset v j tmp))))

  (fset 'neovm--pq-sift-up
    (lambda (heap idx)
      (let ((v (car heap)) (i idx))
        (while (and (> i 0)
                    (< (car (aref v i))
                       (car (aref v (/ (1- i) 2)))))
          (funcall 'neovm--pq-swap v i (/ (1- i) 2))
          (setq i (/ (1- i) 2))))))

  (fset 'neovm--pq-sift-down
    (lambda (heap idx)
      (let* ((v (car heap))
             (size (cdr heap))
             (i idx)
             (done nil))
        (while (not done)
          (let ((left (+ (* 2 i) 1))
                (right (+ (* 2 i) 2))
                (smallest i))
            (when (and (< left size)
                       (< (car (aref v left)) (car (aref v smallest))))
              (setq smallest left))
            (when (and (< right size)
                       (< (car (aref v right)) (car (aref v smallest))))
              (setq smallest right))
            (if (/= smallest i)
                (progn
                  (funcall 'neovm--pq-swap v i smallest)
                  (setq i smallest))
              (setq done t)))))))

  (fset 'neovm--pq-enqueue
    (lambda (pq priority value)
      (let ((v (car pq))
            (size (cdr pq)))
        (aset v size (cons priority value))
        (setcdr pq (1+ size))
        (funcall 'neovm--pq-sift-up pq size))))

  (fset 'neovm--pq-dequeue
    (lambda (pq)
      (let* ((v (car pq))
             (size (cdr pq))
             (top (aref v 0)))
        (aset v 0 (aref v (1- size)))
        (aset v (1- size) nil)
        (setcdr pq (1- size))
        (when (> (cdr pq) 0)
          (funcall 'neovm--pq-sift-down pq 0))
        top)))

  (unwind-protect
      (let ((pq (funcall 'neovm--pq-create 16)))
        ;; Enqueue tasks with priorities
        (funcall 'neovm--pq-enqueue pq 3 'medium-task)
        (funcall 'neovm--pq-enqueue pq 1 'critical-task)
        (funcall 'neovm--pq-enqueue pq 5 'low-task)
        (funcall 'neovm--pq-enqueue pq 2 'high-task)
        (funcall 'neovm--pq-enqueue pq 1 'also-critical)
        (funcall 'neovm--pq-enqueue pq 4 'normal-task)
        ;; Dequeue all -- should come out in priority order
        (let ((results nil))
          (dotimes (_ 6)
            (setq results (cons (funcall 'neovm--pq-dequeue pq) results)))
          (let ((ordered (nreverse results)))
            (list
             ;; The dequeued items
             ordered
             ;; Verify priorities are non-decreasing
             (let ((valid t)
                   (prev 0))
               (dolist (item ordered)
                 (when (< (car item) prev)
                   (setq valid nil))
                 (setq prev (car item)))
               valid)
             ;; Queue should be empty
             (cdr pq)))))
    (fmakunbound 'neovm--pq-create)
    (fmakunbound 'neovm--pq-swap)
    (fmakunbound 'neovm--pq-sift-up)
    (fmakunbound 'neovm--pq-sift-down)
    (fmakunbound 'neovm--pq-enqueue)
    (fmakunbound 'neovm--pq-dequeue)))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 . critical-task) (1 . also-critical) (2 . high-task) (3 . medium-task) (4 . normal-task) (5 . low-task)) t 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Priority queue with string priorities (lexicographic comparison)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_heap_string_priority() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Min-heap using string< for comparison
  (fset 'neovm--shp-create
    (lambda (cap) (cons (make-vector cap nil) 0)))

  (fset 'neovm--shp-swap
    (lambda (v i j)
      (let ((tmp (aref v i)))
        (aset v i (aref v j))
        (aset v j tmp))))

  (fset 'neovm--shp-sift-up
    (lambda (heap idx)
      (let ((v (car heap)) (i idx))
        (while (and (> i 0)
                    (string< (aref v i) (aref v (/ (1- i) 2))))
          (funcall 'neovm--shp-swap v i (/ (1- i) 2))
          (setq i (/ (1- i) 2))))))

  (fset 'neovm--shp-sift-down
    (lambda (heap idx)
      (let* ((v (car heap))
             (size (cdr heap))
             (i idx)
             (done nil))
        (while (not done)
          (let ((left (+ (* 2 i) 1))
                (right (+ (* 2 i) 2))
                (smallest i))
            (when (and (< left size)
                       (string< (aref v left) (aref v smallest)))
              (setq smallest left))
            (when (and (< right size)
                       (string< (aref v right) (aref v smallest)))
              (setq smallest right))
            (if (/= smallest i)
                (progn
                  (funcall 'neovm--shp-swap v i smallest)
                  (setq i smallest))
              (setq done t)))))))

  (fset 'neovm--shp-insert
    (lambda (heap val)
      (let ((size (cdr heap)))
        (aset (car heap) size val)
        (setcdr heap (1+ size))
        (funcall 'neovm--shp-sift-up heap size))))

  (fset 'neovm--shp-extract
    (lambda (heap)
      (let* ((v (car heap))
             (size (cdr heap))
             (min-val (aref v 0)))
        (aset v 0 (aref v (1- size)))
        (aset v (1- size) nil)
        (setcdr heap (1- size))
        (when (> (cdr heap) 0)
          (funcall 'neovm--shp-sift-down heap 0))
        min-val)))

  (unwind-protect
      (let ((h (funcall 'neovm--shp-create 16)))
        ;; Insert strings
        (funcall 'neovm--shp-insert h "banana")
        (funcall 'neovm--shp-insert h "apple")
        (funcall 'neovm--shp-insert h "cherry")
        (funcall 'neovm--shp-insert h "date")
        (funcall 'neovm--shp-insert h "elderberry")
        (funcall 'neovm--shp-insert h "fig")
        (funcall 'neovm--shp-insert h "avocado")
        ;; Extract all in lexicographic order
        (let ((results nil))
          (while (> (cdr h) 0)
            (setq results (cons (funcall 'neovm--shp-extract h) results)))
          (let ((sorted (nreverse results)))
            (list
             sorted
             ;; Verify sorted
             (equal sorted (sort (copy-sequence sorted) 'string<))
             (length sorted)))))
    (fmakunbound 'neovm--shp-create)
    (fmakunbound 'neovm--shp-swap)
    (fmakunbound 'neovm--shp-sift-up)
    (fmakunbound 'neovm--shp-sift-down)
    (fmakunbound 'neovm--shp-insert)
    (fmakunbound 'neovm--shp-extract)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"apple\" \"avocado\" \"banana\" \"cherry\" \"date\" \"elderberry\" \"fig\") t 7)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Max-heap variant and k-smallest elements
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_heap_max_and_k_smallest() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Max-heap + k-smallest using a bounded max-heap of size k
  (fset 'neovm--mh-create
    (lambda (cap) (cons (make-vector cap nil) 0)))

  (fset 'neovm--mh-swap
    (lambda (v i j)
      (let ((tmp (aref v i)))
        (aset v i (aref v j))
        (aset v j tmp))))

  (fset 'neovm--mh-sift-up-max
    (lambda (heap idx)
      (let ((v (car heap)) (i idx))
        (while (and (> i 0)
                    (> (aref v i) (aref v (/ (1- i) 2))))
          (funcall 'neovm--mh-swap v i (/ (1- i) 2))
          (setq i (/ (1- i) 2))))))

  (fset 'neovm--mh-sift-down-max
    (lambda (heap idx)
      (let* ((v (car heap))
             (size (cdr heap))
             (i idx) (done nil))
        (while (not done)
          (let ((left (+ (* 2 i) 1))
                (right (+ (* 2 i) 2))
                (largest i))
            (when (and (< left size)
                       (> (aref v left) (aref v largest)))
              (setq largest left))
            (when (and (< right size)
                       (> (aref v right) (aref v largest)))
              (setq largest right))
            (if (/= largest i)
                (progn
                  (funcall 'neovm--mh-swap v i largest)
                  (setq i largest))
              (setq done t)))))))

  (fset 'neovm--mh-insert
    (lambda (heap val)
      (let ((size (cdr heap)))
        (aset (car heap) size val)
        (setcdr heap (1+ size))
        (funcall 'neovm--mh-sift-up-max heap size))))

  (fset 'neovm--mh-extract-max
    (lambda (heap)
      (let* ((v (car heap))
             (size (cdr heap))
             (max-val (aref v 0)))
        (aset v 0 (aref v (1- size)))
        (aset v (1- size) nil)
        (setcdr heap (1- size))
        (when (> (cdr heap) 0)
          (funcall 'neovm--mh-sift-down-max heap 0))
        max-val)))

  ;; Find k smallest using a bounded max-heap
  (fset 'neovm--mh-k-smallest
    (lambda (lst k)
      (let ((h (funcall 'neovm--mh-create k)))
        (dolist (x lst)
          (if (< (cdr h) k)
              ;; Heap not full yet, just insert
              (funcall 'neovm--mh-insert h x)
            ;; Heap full, replace max if current < max
            (when (< x (aref (car h) 0))
              (aset (car h) 0 x)
              (funcall 'neovm--mh-sift-down-max h 0))))
        ;; Extract all from max-heap, reverse for ascending order
        (let ((result nil))
          (while (> (cdr h) 0)
            (setq result (cons (funcall 'neovm--mh-extract-max h) result)))
          result))))

  (unwind-protect
      (list
       ;; Max-heap basic: extract in descending order
       (let ((h (funcall 'neovm--mh-create 8)))
         (dolist (x '(4 1 7 3 9 2))
           (funcall 'neovm--mh-insert h x))
         (let ((results nil))
           (while (> (cdr h) 0)
             (setq results (cons (funcall 'neovm--mh-extract-max h) results)))
           (nreverse results)))
       ;; k-smallest: find 3 smallest from 10 numbers
       (funcall 'neovm--mh-k-smallest '(15 3 9 1 22 7 4 18 6 11) 3)
       ;; k-smallest: k=1 (find minimum)
       (funcall 'neovm--mh-k-smallest '(5 3 8 1 9) 1)
       ;; k-smallest: k=n (sort entire list)
       (funcall 'neovm--mh-k-smallest '(5 3 8 1 9) 5)
       ;; k-smallest with duplicates
       (funcall 'neovm--mh-k-smallest '(3 1 4 1 5 9 2 6 5 3) 4))
    (fmakunbound 'neovm--mh-create)
    (fmakunbound 'neovm--mh-swap)
    (fmakunbound 'neovm--mh-sift-up-max)
    (fmakunbound 'neovm--mh-sift-down-max)
    (fmakunbound 'neovm--mh-insert)
    (fmakunbound 'neovm--mh-extract-max)
    (fmakunbound 'neovm--mh-k-smallest)))"#;
    let expect =
        expect_test::expect![[r#""OK ((9 7 4 3 2 1) (1 3 4) (1) (1 3 5 8 9) (1 1 2 3))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Merge k sorted lists using a heap
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_heap_merge_k_sorted_lists() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Merge k sorted lists into one sorted list using a min-heap
  ;; Heap entries are (value list-index remaining-list)
  (fset 'neovm--mk-create
    (lambda (cap) (cons (make-vector cap nil) 0)))

  (fset 'neovm--mk-swap
    (lambda (v i j)
      (let ((tmp (aref v i)))
        (aset v i (aref v j))
        (aset v j tmp))))

  (fset 'neovm--mk-sift-up
    (lambda (heap idx)
      (let ((v (car heap)) (i idx))
        (while (and (> i 0)
                    (< (car (aref v i))
                       (car (aref v (/ (1- i) 2)))))
          (funcall 'neovm--mk-swap v i (/ (1- i) 2))
          (setq i (/ (1- i) 2))))))

  (fset 'neovm--mk-sift-down
    (lambda (heap idx)
      (let* ((v (car heap))
             (size (cdr heap))
             (i idx) (done nil))
        (while (not done)
          (let ((left (+ (* 2 i) 1))
                (right (+ (* 2 i) 2))
                (smallest i))
            (when (and (< left size)
                       (< (car (aref v left)) (car (aref v smallest))))
              (setq smallest left))
            (when (and (< right size)
                       (< (car (aref v right)) (car (aref v smallest))))
              (setq smallest right))
            (if (/= smallest i)
                (progn
                  (funcall 'neovm--mk-swap v i smallest)
                  (setq i smallest))
              (setq done t)))))))

  (fset 'neovm--mk-insert
    (lambda (heap entry)
      (let ((size (cdr heap)))
        (aset (car heap) size entry)
        (setcdr heap (1+ size))
        (funcall 'neovm--mk-sift-up heap size))))

  (fset 'neovm--mk-extract
    (lambda (heap)
      (let* ((v (car heap))
             (size (cdr heap))
             (top (aref v 0)))
        (aset v 0 (aref v (1- size)))
        (aset v (1- size) nil)
        (setcdr heap (1- size))
        (when (> (cdr heap) 0)
          (funcall 'neovm--mk-sift-down heap 0))
        top)))

  (fset 'neovm--mk-merge
    (lambda (lists)
      (let ((h (funcall 'neovm--mk-create (length lists))))
        ;; Initialize heap with first element of each non-empty list
        (let ((idx 0))
          (dolist (lst lists)
            (when lst
              (funcall 'neovm--mk-insert h (list (car lst) idx (cdr lst))))
            (setq idx (1+ idx))))
        ;; Extract min, push next from same list
        (let ((result nil))
          (while (> (cdr h) 0)
            (let ((entry (funcall 'neovm--mk-extract h)))
              (setq result (cons (car entry) result))
              (let ((rest (nth 2 entry))
                    (list-idx (nth 1 entry)))
                (when rest
                  (funcall 'neovm--mk-insert h
                           (list (car rest) list-idx (cdr rest)))))))
          (nreverse result)))))

  (unwind-protect
      (list
       ;; Merge 3 sorted lists
       (funcall 'neovm--mk-merge '((1 4 7) (2 5 8) (3 6 9)))
       ;; Merge lists of different sizes
       (funcall 'neovm--mk-merge '((1 3 5 7 9) (2 4) (6 8 10 12)))
       ;; Merge with an empty list
       (funcall 'neovm--mk-merge '((1 2 3) () (4 5 6)))
       ;; Merge single list
       (funcall 'neovm--mk-merge '((5 10 15 20)))
       ;; Merge overlapping ranges
       (funcall 'neovm--mk-merge '((1 2 3) (2 3 4) (3 4 5)))
       ;; Verify correctness against simple sort
       (let ((merged (funcall 'neovm--mk-merge '((10 30 50) (20 40) (5 15 25 35 45)))))
         (equal merged (sort (copy-sequence merged) #'<))))
    (fmakunbound 'neovm--mk-create)
    (fmakunbound 'neovm--mk-swap)
    (fmakunbound 'neovm--mk-sift-up)
    (fmakunbound 'neovm--mk-sift-down)
    (fmakunbound 'neovm--mk-insert)
    (fmakunbound 'neovm--mk-extract)
    (fmakunbound 'neovm--mk-merge)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5 6 7 8 9) (1 2 3 4 5 6 7 8 9 10 12) (1 2 3 4 5 6) (5 10 15 20) (1 2 2 3 3 3 4 4 5) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
