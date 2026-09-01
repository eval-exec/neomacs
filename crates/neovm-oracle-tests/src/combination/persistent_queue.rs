//! Oracle parity tests for a persistent (functional) queue implemented in Elisp.
//!
//! Tests a two-list queue (front for dequeue, rear for enqueue), enqueue,
//! dequeue, peek, batch operations, priority queue via sorted insertion,
//! and circular buffer simulation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Basic persistent queue: enqueue, dequeue, peek, empty
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_persistent_queue_basic_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Queue representation: (front . rear)
  ;; front is a list for dequeue (head), rear is a reversed list for enqueue (tail)
  (fset 'neovm--pq-make (lambda () (cons nil nil)))

  (fset 'neovm--pq-empty-p
    (lambda (q) (and (null (car q)) (null (cdr q)))))

  (fset 'neovm--pq-size
    (lambda (q) (+ (length (car q)) (length (cdr q)))))

  (fset 'neovm--pq-enqueue
    (lambda (q val)
      "Add VAL to the back of the queue. Returns new queue."
      (cons (car q) (cons val (cdr q)))))

  (fset 'neovm--pq-normalize
    (lambda (q)
      "If front is empty and rear is not, reverse rear to front."
      (if (and (null (car q)) (cdr q))
          (cons (nreverse (copy-sequence (cdr q))) nil)
        q)))

  (fset 'neovm--pq-dequeue
    (lambda (q)
      "Remove and return front element. Returns (value . new-queue)."
      (let ((nq (funcall 'neovm--pq-normalize q)))
        (if (null (car nq))
            (cons nil q)  ;; empty queue
          (cons (caar nq)
                (cons (cdar nq) (cdr nq)))))))

  (fset 'neovm--pq-peek
    (lambda (q)
      "Return front element without removing it."
      (let ((nq (funcall 'neovm--pq-normalize q)))
        (if (null (car nq))
            nil
          (caar nq)))))

  (fset 'neovm--pq-to-list
    (lambda (q)
      "Convert queue to list in FIFO order."
      (let ((nq (funcall 'neovm--pq-normalize q)))
        (append (car nq) (nreverse (copy-sequence (cdr nq)))))))

  (unwind-protect
      (let* ((q0 (funcall 'neovm--pq-make))
             ;; Enqueue 1, 2, 3
             (q1 (funcall 'neovm--pq-enqueue q0 1))
             (q2 (funcall 'neovm--pq-enqueue q1 2))
             (q3 (funcall 'neovm--pq-enqueue q2 3))
             ;; Dequeue from q3
             (r1 (funcall 'neovm--pq-dequeue q3))
             (val1 (car r1))
             (q4 (cdr r1))
             ;; Dequeue again
             (r2 (funcall 'neovm--pq-dequeue q4))
             (val2 (car r2))
             (q5 (cdr r2))
             ;; Enqueue more after dequeue
             (q6 (funcall 'neovm--pq-enqueue q5 4))
             (q7 (funcall 'neovm--pq-enqueue q6 5))
             ;; Dequeue rest
             (r3 (funcall 'neovm--pq-dequeue q7))
             (val3 (car r3))
             (q8 (cdr r3)))
        (list
          ;; Empty check
          (funcall 'neovm--pq-empty-p q0)
          (funcall 'neovm--pq-empty-p q1)
          ;; Sizes
          (funcall 'neovm--pq-size q0)
          (funcall 'neovm--pq-size q1)
          (funcall 'neovm--pq-size q3)
          ;; Peek
          (funcall 'neovm--pq-peek q3)
          ;; Dequeued values (FIFO order)
          val1 val2 val3
          ;; Queue contents at various stages
          (funcall 'neovm--pq-to-list q3)
          (funcall 'neovm--pq-to-list q4)
          (funcall 'neovm--pq-to-list q7)
          (funcall 'neovm--pq-to-list q8)
          ;; Persistence: q3 is unchanged after dequeue
          (funcall 'neovm--pq-to-list q3)
          (funcall 'neovm--pq-size q3)
          ;; Dequeue from empty
          (car (funcall 'neovm--pq-dequeue q0))))
    (fmakunbound 'neovm--pq-make)
    (fmakunbound 'neovm--pq-empty-p)
    (fmakunbound 'neovm--pq-size)
    (fmakunbound 'neovm--pq-enqueue)
    (fmakunbound 'neovm--pq-normalize)
    (fmakunbound 'neovm--pq-dequeue)
    (fmakunbound 'neovm--pq-peek)
    (fmakunbound 'neovm--pq-to-list)))"#;
    let expect = expect_test::expect![[
        r#""OK (t nil 0 1 3 1 1 2 3 (1 2 3) (2 3) (3 4 5) (4 5) (1 2 3) 3 nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Batch operations: enqueue-all, dequeue-n
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_persistent_queue_batch_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--pq-make (lambda () (cons nil nil)))
  (fset 'neovm--pq-empty-p
    (lambda (q) (and (null (car q)) (null (cdr q)))))
  (fset 'neovm--pq-size
    (lambda (q) (+ (length (car q)) (length (cdr q)))))
  (fset 'neovm--pq-enqueue
    (lambda (q val) (cons (car q) (cons val (cdr q)))))
  (fset 'neovm--pq-normalize
    (lambda (q)
      (if (and (null (car q)) (cdr q))
          (cons (nreverse (copy-sequence (cdr q))) nil)
        q)))
  (fset 'neovm--pq-dequeue
    (lambda (q)
      (let ((nq (funcall 'neovm--pq-normalize q)))
        (if (null (car nq))
            (cons nil q)
          (cons (caar nq) (cons (cdar nq) (cdr nq)))))))
  (fset 'neovm--pq-to-list
    (lambda (q)
      (let ((nq (funcall 'neovm--pq-normalize q)))
        (append (car nq) (nreverse (copy-sequence (cdr nq)))))))

  ;; Batch enqueue: add all elements from a list
  (fset 'neovm--pq-enqueue-all
    (lambda (q items)
      (let ((result q))
        (dolist (item items)
          (setq result (funcall 'neovm--pq-enqueue result item)))
        result)))

  ;; Dequeue n elements: returns (dequeued-list . remaining-queue)
  (fset 'neovm--pq-dequeue-n
    (lambda (q n)
      (let ((result nil)
            (current q)
            (i 0))
        (while (and (< i n) (not (funcall 'neovm--pq-empty-p current)))
          (let ((r (funcall 'neovm--pq-dequeue current)))
            (setq result (cons (car r) result))
            (setq current (cdr r))
            (setq i (1+ i))))
        (cons (nreverse result) current))))

  ;; Drain: dequeue all elements
  (fset 'neovm--pq-drain
    (lambda (q)
      (let ((result nil)
            (current q))
        (while (not (funcall 'neovm--pq-empty-p current))
          (let ((r (funcall 'neovm--pq-dequeue current)))
            (setq result (cons (car r) result))
            (setq current (cdr r))))
        (nreverse result))))

  ;; From list: convenience constructor
  (fset 'neovm--pq-from-list
    (lambda (items)
      (funcall 'neovm--pq-enqueue-all (funcall 'neovm--pq-make) items)))

  (unwind-protect
      (let* ((q0 (funcall 'neovm--pq-make))
             ;; Batch enqueue
             (q1 (funcall 'neovm--pq-enqueue-all q0 '(10 20 30 40 50)))
             ;; Dequeue first 3
             (r1 (funcall 'neovm--pq-dequeue-n q1 3))
             (dequeued1 (car r1))
             (q2 (cdr r1))
             ;; Enqueue more after partial dequeue
             (q3 (funcall 'neovm--pq-enqueue-all q2 '(60 70)))
             ;; Dequeue-n more than available
             (r2 (funcall 'neovm--pq-dequeue-n q3 100))
             ;; Drain a fresh queue
             (q4 (funcall 'neovm--pq-from-list '(a b c d e)))
             (drained (funcall 'neovm--pq-drain q4))
             ;; From-list and back
             (q5 (funcall 'neovm--pq-from-list '(1 2 3)))
             ;; Dequeue-n with 0
             (r3 (funcall 'neovm--pq-dequeue-n q5 0)))
        (list
          ;; Batch enqueue result
          (funcall 'neovm--pq-to-list q1)
          (funcall 'neovm--pq-size q1)
          ;; Partial dequeue
          dequeued1
          (funcall 'neovm--pq-to-list q2)
          ;; After more enqueue
          (funcall 'neovm--pq-to-list q3)
          ;; Dequeue-n beyond size
          (car r2)
          (funcall 'neovm--pq-empty-p (cdr r2))
          ;; Drain
          drained
          ;; from-list round-trip
          (funcall 'neovm--pq-to-list q5)
          ;; Dequeue-n 0
          (car r3)
          (equal (funcall 'neovm--pq-to-list (cdr r3))
                 (funcall 'neovm--pq-to-list q5))
          ;; Persistence: q1 still intact
          (funcall 'neovm--pq-to-list q1)))
    (fmakunbound 'neovm--pq-make)
    (fmakunbound 'neovm--pq-empty-p)
    (fmakunbound 'neovm--pq-size)
    (fmakunbound 'neovm--pq-enqueue)
    (fmakunbound 'neovm--pq-normalize)
    (fmakunbound 'neovm--pq-dequeue)
    (fmakunbound 'neovm--pq-to-list)
    (fmakunbound 'neovm--pq-enqueue-all)
    (fmakunbound 'neovm--pq-dequeue-n)
    (fmakunbound 'neovm--pq-drain)
    (fmakunbound 'neovm--pq-from-list)))"#;
    let expect = expect_test::expect![[
        r#""OK ((10 20 30 40 50) 5 (10 20 30) (40 50) (40 50 60 70) (40 50 60 70) t (a b c d e) (1 2 3) nil t (10 20 30 40 50))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Priority queue via sorted insertion
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_persistent_queue_priority() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Priority queue: sorted list where lowest priority number = highest priority
  ;; Elements are (priority . value)
  (fset 'neovm--ppq-make (lambda () nil))

  (fset 'neovm--ppq-empty-p (lambda (pq) (null pq)))

  (fset 'neovm--ppq-size (lambda (pq) (length pq)))

  (fset 'neovm--ppq-insert
    (lambda (pq priority value)
      "Insert (priority . value) in sorted position. Returns new pqueue."
      (let ((entry (cons priority value)))
        (cond
          ((null pq) (list entry))
          ((<= priority (caar pq))
           (cons entry pq))
          (t (cons (car pq)
                   (funcall 'neovm--ppq-insert (cdr pq) priority value)))))))

  (fset 'neovm--ppq-peek
    (lambda (pq)
      "Return highest-priority (lowest number) element."
      (if pq (car pq) nil)))

  (fset 'neovm--ppq-pop
    (lambda (pq)
      "Remove highest-priority element. Returns (element . new-pqueue)."
      (if pq
          (cons (car pq) (cdr pq))
        (cons nil nil))))

  (fset 'neovm--ppq-to-list
    (lambda (pq) (mapcar #'cdr pq)))

  (fset 'neovm--ppq-priorities
    (lambda (pq) (mapcar #'car pq)))

  ;; Merge two priority queues
  (fset 'neovm--ppq-merge
    (lambda (pq1 pq2)
      (let ((result pq1))
        (dolist (entry pq2)
          (setq result (funcall 'neovm--ppq-insert result (car entry) (cdr entry))))
        result)))

  ;; Drain in priority order
  (fset 'neovm--ppq-drain
    (lambda (pq)
      (let ((result nil) (current pq))
        (while (not (funcall 'neovm--ppq-empty-p current))
          (let ((r (funcall 'neovm--ppq-pop current)))
            (setq result (cons (car r) result))
            (setq current (cdr r))))
        (nreverse result))))

  (unwind-protect
      (let* ((pq0 (funcall 'neovm--ppq-make))
             ;; Insert with various priorities (not in order)
             (pq1 (funcall 'neovm--ppq-insert pq0 3 "low"))
             (pq2 (funcall 'neovm--ppq-insert pq1 1 "high"))
             (pq3 (funcall 'neovm--ppq-insert pq2 2 "medium"))
             (pq4 (funcall 'neovm--ppq-insert pq3 1 "also-high"))
             (pq5 (funcall 'neovm--ppq-insert pq4 5 "very-low"))
             ;; Pop highest priority
             (r1 (funcall 'neovm--ppq-pop pq5))
             (top (car r1))
             (pq6 (cdr r1))
             ;; Pop next
             (r2 (funcall 'neovm--ppq-pop pq6))
             ;; Second priority queue for merge
             (pq-b (funcall 'neovm--ppq-insert
                            (funcall 'neovm--ppq-insert
                                     (funcall 'neovm--ppq-make) 0 "urgent")
                            4 "normal"))
             ;; Merge
             (merged (funcall 'neovm--ppq-merge pq5 pq-b)))
        (list
          ;; Empty check
          (funcall 'neovm--ppq-empty-p pq0)
          (funcall 'neovm--ppq-empty-p pq1)
          ;; Size
          (funcall 'neovm--ppq-size pq5)
          ;; Peek (should be highest priority = lowest number)
          (funcall 'neovm--ppq-peek pq5)
          ;; Values in priority order
          (funcall 'neovm--ppq-to-list pq5)
          ;; Priorities are sorted
          (funcall 'neovm--ppq-priorities pq5)
          ;; Pop results
          top
          (car r2)
          ;; After two pops
          (funcall 'neovm--ppq-to-list (cdr r2))
          ;; Merged queue in order
          (funcall 'neovm--ppq-to-list merged)
          (funcall 'neovm--ppq-priorities merged)
          ;; Drain: all elements in priority order
          (funcall 'neovm--ppq-drain pq5)
          ;; Persistence: pq5 unchanged
          (funcall 'neovm--ppq-size pq5)
          (funcall 'neovm--ppq-to-list pq5)
          ;; Pop from empty
          (funcall 'neovm--ppq-pop pq0)))
    (fmakunbound 'neovm--ppq-make)
    (fmakunbound 'neovm--ppq-empty-p)
    (fmakunbound 'neovm--ppq-size)
    (fmakunbound 'neovm--ppq-insert)
    (fmakunbound 'neovm--ppq-peek)
    (fmakunbound 'neovm--ppq-pop)
    (fmakunbound 'neovm--ppq-to-list)
    (fmakunbound 'neovm--ppq-priorities)
    (fmakunbound 'neovm--ppq-merge)
    (fmakunbound 'neovm--ppq-drain)))"#;
    let expect = expect_test::expect![[
        r#""OK (t nil 5 (1 . \"also-high\") (\"also-high\" \"high\" \"medium\" \"low\" \"very-low\") (1 1 2 3 5) (1 . \"also-high\") (1 . \"high\") (\"medium\" \"low\" \"very-low\") (\"urgent\" \"also-high\" \"high\" \"medium\" \"low\" \"normal\" \"very-low\") (0 1 1 2 3 4 5) ((1 . \"also-high\") (1 . \"high\") (2 . \"medium\") (3 . \"low\") (5 . \"very-low\")) 5 (\"also-high\" \"high\" \"medium\" \"low\" \"very-low\") (nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Circular buffer simulation using persistent queue
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_persistent_queue_circular_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Circular buffer: queue with a max capacity.
  ;; When full, adding drops the oldest element.
  ;; Representation: (capacity front . rear)
  (fset 'neovm--cb-make
    (lambda (capacity) (list capacity nil)))

  (fset 'neovm--cb-capacity
    (lambda (cb) (car cb)))

  (fset 'neovm--cb-size
    (lambda (cb)
      (+ (length (cadr cb)) (length (cddr cb)))))

  (fset 'neovm--cb-full-p
    (lambda (cb) (>= (funcall 'neovm--cb-size cb) (car cb))))

  (fset 'neovm--cb-empty-p
    (lambda (cb) (and (null (cadr cb)) (null (cddr cb)))))

  (fset 'neovm--cb-normalize
    (lambda (cb)
      (if (and (null (cadr cb)) (cddr cb))
          (list (car cb) (nreverse (copy-sequence (cddr cb))))
        cb)))

  (fset 'neovm--cb-add
    (lambda (cb val)
      "Add VAL. If full, drop oldest first."
      (if (funcall 'neovm--cb-full-p cb)
          ;; Drop oldest (front head), then add
          (let ((normalized (funcall 'neovm--cb-normalize cb)))
            (list (car normalized)
                  (cdr (cadr normalized))
                  (cons val (cddr normalized))))
        ;; Not full, just enqueue
        (list (car cb)
              (cadr cb)
              (cons val (cddr cb))))))

  (fset 'neovm--cb-peek-oldest
    (lambda (cb)
      (let ((n (funcall 'neovm--cb-normalize cb)))
        (if (cadr n) (car (cadr n)) nil))))

  (fset 'neovm--cb-peek-newest
    (lambda (cb)
      (if (cddr cb)
          (car (cddr cb))
        ;; If rear is empty, newest is last of front
        (if (cadr cb)
            (car (last (cadr cb)))
          nil))))

  (fset 'neovm--cb-to-list
    (lambda (cb)
      "Elements in oldest-first order."
      (let ((n (funcall 'neovm--cb-normalize cb)))
        (append (cadr n) (nreverse (copy-sequence (cddr n)))))))

  ;; Add multiple elements
  (fset 'neovm--cb-add-all
    (lambda (cb items)
      (let ((result cb))
        (dolist (item items)
          (setq result (funcall 'neovm--cb-add result item)))
        result)))

  (unwind-protect
      (let* (;; Buffer of capacity 5
             (cb0 (funcall 'neovm--cb-make 5))
             ;; Add 1..5 (fills up)
             (cb1 (funcall 'neovm--cb-add-all cb0 '(1 2 3 4 5)))
             ;; Add 6 (should drop 1)
             (cb2 (funcall 'neovm--cb-add cb1 6))
             ;; Add 7 (should drop 2)
             (cb3 (funcall 'neovm--cb-add cb2 7))
             ;; Add several more
             (cb4 (funcall 'neovm--cb-add-all cb3 '(8 9 10)))
             ;; Capacity 3 buffer with overflow
             (small (funcall 'neovm--cb-make 3))
             (s1 (funcall 'neovm--cb-add-all small '(10 20 30 40 50))))
        (list
          ;; Initial state
          (funcall 'neovm--cb-empty-p cb0)
          (funcall 'neovm--cb-size cb0)
          ;; After filling
          (funcall 'neovm--cb-to-list cb1)
          (funcall 'neovm--cb-size cb1)
          (funcall 'neovm--cb-full-p cb1)
          ;; After overflow
          (funcall 'neovm--cb-to-list cb2)
          (funcall 'neovm--cb-peek-oldest cb2)
          (funcall 'neovm--cb-peek-newest cb2)
          ;; More overflow
          (funcall 'neovm--cb-to-list cb3)
          ;; Lots of overflow
          (funcall 'neovm--cb-to-list cb4)
          (funcall 'neovm--cb-size cb4)
          ;; Small buffer overflow
          (funcall 'neovm--cb-to-list s1)
          (funcall 'neovm--cb-peek-oldest s1)
          (funcall 'neovm--cb-peek-newest s1)
          ;; Capacity is preserved
          (funcall 'neovm--cb-capacity cb4)
          (funcall 'neovm--cb-capacity s1)
          ;; Persistence: cb1 unchanged
          (funcall 'neovm--cb-to-list cb1)))
    (fmakunbound 'neovm--cb-make)
    (fmakunbound 'neovm--cb-capacity)
    (fmakunbound 'neovm--cb-size)
    (fmakunbound 'neovm--cb-full-p)
    (fmakunbound 'neovm--cb-empty-p)
    (fmakunbound 'neovm--cb-normalize)
    (fmakunbound 'neovm--cb-add)
    (fmakunbound 'neovm--cb-peek-oldest)
    (fmakunbound 'neovm--cb-peek-newest)
    (fmakunbound 'neovm--cb-to-list)
    (fmakunbound 'neovm--cb-add-all)))"#;
    let expect = expect_test::expect![[
        r#""OK (t 0 ((5 (4 (3 (2 (1)))))) 1 nil ((6 (5 (4 (3 (2 (1))))))) (6 (5 (4 (3 (2 (1)))))) (6 (5 (4 (3 (2 (1)))))) ((7 (6 (5 (4 (3 (2 (1)))))))) ((10 (9 (8 (7 (6 (5 (4 (3 (2 (1))))))))))) 1 ((50 (40 (30 (20 (10)))))) (50 (40 (30 (20 (10))))) (50 (40 (30 (20 (10))))) 5 3 ((5 (4 (3 (2 (1)))))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Queue-based BFS (breadth-first search) using persistent queue
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_persistent_queue_bfs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Queue primitives (inline for this test)
  (fset 'neovm--bq-make (lambda () (cons nil nil)))
  (fset 'neovm--bq-empty-p
    (lambda (q) (and (null (car q)) (null (cdr q)))))
  (fset 'neovm--bq-enqueue
    (lambda (q val) (cons (car q) (cons val (cdr q)))))
  (fset 'neovm--bq-normalize
    (lambda (q)
      (if (and (null (car q)) (cdr q))
          (cons (nreverse (copy-sequence (cdr q))) nil)
        q)))
  (fset 'neovm--bq-dequeue
    (lambda (q)
      (let ((nq (funcall 'neovm--bq-normalize q)))
        (if (null (car nq))
            (cons nil q)
          (cons (caar nq) (cons (cdar nq) (cdr nq)))))))

  ;; BFS on a graph represented as alist: ((node . (neighbor ...)) ...)
  (fset 'neovm--bq-bfs
    (lambda (graph start)
      "BFS traversal from START. Returns list of nodes in visit order."
      (let ((visited (make-hash-table :test 'eq))
            (result nil)
            (queue (funcall 'neovm--bq-enqueue (funcall 'neovm--bq-make) start)))
        (puthash start t visited)
        (while (not (funcall 'neovm--bq-empty-p queue))
          (let* ((r (funcall 'neovm--bq-dequeue queue))
                 (node (car r)))
            (setq queue (cdr r))
            (setq result (cons node result))
            ;; Enqueue unvisited neighbors
            (let ((neighbors (cdr (assq node graph))))
              (dolist (n neighbors)
                (unless (gethash n visited)
                  (puthash n t visited)
                  (setq queue (funcall 'neovm--bq-enqueue queue n)))))))
        (nreverse result))))

  ;; Level-order BFS: returns list of lists, one per level
  (fset 'neovm--bq-bfs-levels
    (lambda (graph start)
      "BFS returning nodes grouped by level."
      (let ((visited (make-hash-table :test 'eq))
            (levels nil)
            (queue (funcall 'neovm--bq-enqueue (funcall 'neovm--bq-make)
                            (cons start 0))))
        (puthash start t visited)
        (while (not (funcall 'neovm--bq-empty-p queue))
          (let* ((r (funcall 'neovm--bq-dequeue queue))
                 (entry (car r))
                 (node (car entry))
                 (level (cdr entry)))
            (setq queue (cdr r))
            ;; Ensure levels list is long enough
            (while (<= (length levels) level)
              (setq levels (append levels (list nil))))
            ;; Add node to its level
            (let ((cur (nth level levels)))
              (setcar (nthcdr level levels) (append cur (list node))))
            ;; Enqueue neighbors
            (dolist (n (cdr (assq node graph)))
              (unless (gethash n visited)
                (puthash n t visited)
                (setq queue (funcall 'neovm--bq-enqueue queue
                                     (cons n (1+ level))))))))
        levels)))

  (unwind-protect
      (let ((graph '((a . (b c))
                     (b . (a d e))
                     (c . (a f))
                     (d . (b))
                     (e . (b f))
                     (f . (c e)))))
        (list
          ;; BFS from a
          (funcall 'neovm--bq-bfs graph 'a)
          ;; BFS from d
          (funcall 'neovm--bq-bfs graph 'd)
          ;; BFS levels from a
          (funcall 'neovm--bq-bfs-levels graph 'a)
          ;; BFS levels from d
          (funcall 'neovm--bq-bfs-levels graph 'd)
          ;; All nodes visited
          (= (length (funcall 'neovm--bq-bfs graph 'a)) 6)
          ;; Linear graph
          (let ((linear '((x . (y)) (y . (z)) (z . nil))))
            (list (funcall 'neovm--bq-bfs linear 'x)
                  (funcall 'neovm--bq-bfs-levels linear 'x)))
          ;; Single node
          (funcall 'neovm--bq-bfs '((solo . nil)) 'solo)))
    (fmakunbound 'neovm--bq-make)
    (fmakunbound 'neovm--bq-empty-p)
    (fmakunbound 'neovm--bq-enqueue)
    (fmakunbound 'neovm--bq-normalize)
    (fmakunbound 'neovm--bq-dequeue)
    (fmakunbound 'neovm--bq-bfs)
    (fmakunbound 'neovm--bq-bfs-levels)))"#;
    let expect = expect_test::expect![[
        r#""OK ((a b c d e f) (d b a e c f) ((a) (b c) (d e f)) ((d) (b) (a e) (c f)) t ((x y z) ((x) (y) (z))) (solo))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Double-ended queue (deque) with persistent semantics
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_persistent_queue_deque() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Deque: (front . rear) where both front and rear can be pushed/popped
  (fset 'neovm--dq-make (lambda () (cons nil nil)))

  (fset 'neovm--dq-empty-p
    (lambda (dq) (and (null (car dq)) (null (cdr dq)))))

  (fset 'neovm--dq-size
    (lambda (dq) (+ (length (car dq)) (length (cdr dq)))))

  (fset 'neovm--dq-push-front
    (lambda (dq val) (cons (cons val (car dq)) (cdr dq))))

  (fset 'neovm--dq-push-back
    (lambda (dq val) (cons (car dq) (cons val (cdr dq)))))

  ;; Balance: when one side empty, split the other in half
  (fset 'neovm--dq-balance
    (lambda (dq)
      (cond
        ;; Front empty, rear has elements
        ((and (null (car dq)) (cdr dq))
         (let* ((rev (nreverse (copy-sequence (cdr dq))))
                (mid (/ (length rev) 2))
                (front (let ((r nil) (i 0) (l rev))
                         (while (< i mid)
                           (setq r (cons (car l) r))
                           (setq l (cdr l))
                           (setq i (1+ i)))
                         (nreverse r)))
                (rear (nreverse (nthcdr mid rev))))
           (cons front rear)))
        ;; Rear empty, front has elements
        ((and (car dq) (null (cdr dq)))
         (let* ((mid (/ (length (car dq)) 2))
                (front (let ((r nil) (i 0) (l (car dq)))
                          (while (< i mid)
                            (setq r (cons (car l) r))
                            (setq l (cdr l))
                            (setq i (1+ i)))
                          (nreverse r)))
                (rear (nreverse (nthcdr mid (car dq)))))
           (cons front rear)))
        (t dq))))

  (fset 'neovm--dq-pop-front
    (lambda (dq)
      "Returns (value . new-deque)."
      (let ((balanced (funcall 'neovm--dq-balance dq)))
        (if (null (car balanced))
            (cons nil dq)  ;; empty
          (cons (caar balanced)
                (cons (cdar balanced) (cdr balanced)))))))

  (fset 'neovm--dq-pop-back
    (lambda (dq)
      "Returns (value . new-deque)."
      (let ((balanced (funcall 'neovm--dq-balance dq)))
        (if (null (cdr balanced))
            (cons nil dq)  ;; empty
          (cons (cadr balanced)
                (cons (car balanced) (cddr balanced)))))))

  (fset 'neovm--dq-to-list
    (lambda (dq)
      (append (car dq) (nreverse (copy-sequence (cdr dq))))))

  (unwind-protect
      (let* ((dq0 (funcall 'neovm--dq-make))
             ;; Push front: 3, 2, 1
             (dq1 (funcall 'neovm--dq-push-front dq0 3))
             (dq2 (funcall 'neovm--dq-push-front dq1 2))
             (dq3 (funcall 'neovm--dq-push-front dq2 1))
             ;; Push back: 4, 5
             (dq4 (funcall 'neovm--dq-push-back dq3 4))
             (dq5 (funcall 'neovm--dq-push-back dq4 5))
             ;; Pop front
             (r1 (funcall 'neovm--dq-pop-front dq5))
             ;; Pop back
             (r2 (funcall 'neovm--dq-pop-back dq5))
             ;; Build deque from only push-back
             (dq-back (funcall 'neovm--dq-push-back
                               (funcall 'neovm--dq-push-back
                                        (funcall 'neovm--dq-push-back dq0 10) 20) 30))
             ;; Pop front from rear-only deque (triggers balancing)
             (r3 (funcall 'neovm--dq-pop-front dq-back)))
        (list
          ;; Contents
          (funcall 'neovm--dq-to-list dq5)
          (funcall 'neovm--dq-size dq5)
          ;; Pop front returns 1
          (car r1)
          (funcall 'neovm--dq-to-list (cdr r1))
          ;; Pop back returns 5
          (car r2)
          (funcall 'neovm--dq-to-list (cdr r2))
          ;; Rear-only deque
          (funcall 'neovm--dq-to-list dq-back)
          ;; Pop front from rear-only (balanced)
          (car r3)
          (funcall 'neovm--dq-to-list (cdr r3))
          ;; Persistence
          (funcall 'neovm--dq-to-list dq5)
          (funcall 'neovm--dq-empty-p dq0)
          (funcall 'neovm--dq-empty-p dq5)))
    (fmakunbound 'neovm--dq-make)
    (fmakunbound 'neovm--dq-empty-p)
    (fmakunbound 'neovm--dq-size)
    (fmakunbound 'neovm--dq-push-front)
    (fmakunbound 'neovm--dq-push-back)
    (fmakunbound 'neovm--dq-balance)
    (fmakunbound 'neovm--dq-pop-front)
    (fmakunbound 'neovm--dq-pop-back)
    (fmakunbound 'neovm--dq-to-list)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5) 5 1 (2 3 4 5) 5 (1 2 3 4) (10 20 30) 10 (20 30) (1 2 3 4 5) t nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
