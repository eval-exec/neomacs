//! Advanced oracle parity tests for deque operations in Elisp.
//!
//! Extends the basic deque with: circular buffer implementation,
//! sliding window min, work-stealing deque simulation, deque-based BFS,
//! priority deque with sorted insertion, deque reversal, deque from/to
//! various sequences, and stress tests with many elements.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Shared deque preamble (same core as combination_deque_operations)
// ---------------------------------------------------------------------------

const DEQUE_PREAMBLE: &str = r#"
  (fset 'neovm--dqa-make (lambda () (cons nil (cons nil 0))))
  (fset 'neovm--dqa-front (lambda (dq) (car dq)))
  (fset 'neovm--dqa-back  (lambda (dq) (cadr dq)))
  (fset 'neovm--dqa-size  (lambda (dq) (cddr dq)))
  (fset 'neovm--dqa-set-front (lambda (dq v) (setcar dq v)))
  (fset 'neovm--dqa-set-back  (lambda (dq v) (setcar (cdr dq) v)))
  (fset 'neovm--dqa-set-size  (lambda (dq v) (setcdr (cdr dq) v)))

  (fset 'neovm--dqa-rebalance-front
    (lambda (dq)
      (when (and (null (funcall 'neovm--dqa-front dq))
                 (not (null (funcall 'neovm--dqa-back dq))))
        (funcall 'neovm--dqa-set-front dq (nreverse (funcall 'neovm--dqa-back dq)))
        (funcall 'neovm--dqa-set-back dq nil))))

  (fset 'neovm--dqa-rebalance-back
    (lambda (dq)
      (when (and (null (funcall 'neovm--dqa-back dq))
                 (not (null (funcall 'neovm--dqa-front dq))))
        (funcall 'neovm--dqa-set-back dq (nreverse (funcall 'neovm--dqa-front dq)))
        (funcall 'neovm--dqa-set-front dq nil))))

  (fset 'neovm--dqa-push-front
    (lambda (dq val)
      (funcall 'neovm--dqa-set-front dq (cons val (funcall 'neovm--dqa-front dq)))
      (funcall 'neovm--dqa-set-size dq (1+ (funcall 'neovm--dqa-size dq)))))

  (fset 'neovm--dqa-push-back
    (lambda (dq val)
      (funcall 'neovm--dqa-set-back dq (cons val (funcall 'neovm--dqa-back dq)))
      (funcall 'neovm--dqa-set-size dq (1+ (funcall 'neovm--dqa-size dq)))))

  (fset 'neovm--dqa-pop-front
    (lambda (dq)
      (funcall 'neovm--dqa-rebalance-front dq)
      (if (null (funcall 'neovm--dqa-front dq))
          (error "deque underflow")
        (let ((val (car (funcall 'neovm--dqa-front dq))))
          (funcall 'neovm--dqa-set-front dq (cdr (funcall 'neovm--dqa-front dq)))
          (funcall 'neovm--dqa-set-size dq (1- (funcall 'neovm--dqa-size dq)))
          val))))

  (fset 'neovm--dqa-pop-back
    (lambda (dq)
      (funcall 'neovm--dqa-rebalance-back dq)
      (if (null (funcall 'neovm--dqa-back dq))
          (error "deque underflow")
        (let ((val (car (funcall 'neovm--dqa-back dq))))
          (funcall 'neovm--dqa-set-back dq (cdr (funcall 'neovm--dqa-back dq)))
          (funcall 'neovm--dqa-set-size dq (1- (funcall 'neovm--dqa-size dq)))
          val))))

  (fset 'neovm--dqa-peek-front
    (lambda (dq)
      (funcall 'neovm--dqa-rebalance-front dq)
      (car (funcall 'neovm--dqa-front dq))))

  (fset 'neovm--dqa-peek-back
    (lambda (dq)
      (funcall 'neovm--dqa-rebalance-back dq)
      (car (funcall 'neovm--dqa-back dq))))

  (fset 'neovm--dqa-empty-p
    (lambda (dq) (= 0 (funcall 'neovm--dqa-size dq))))

  (fset 'neovm--dqa-to-list
    (lambda (dq)
      (append (funcall 'neovm--dqa-front dq)
              (nreverse (copy-sequence (funcall 'neovm--dqa-back dq))))))

  (fset 'neovm--dqa-clear
    (lambda (dq)
      (funcall 'neovm--dqa-set-front dq nil)
      (funcall 'neovm--dqa-set-back dq nil)
      (funcall 'neovm--dqa-set-size dq 0)))

  (fset 'neovm--dqa-from-list
    (lambda (lst)
      (let ((dq (funcall 'neovm--dqa-make)))
        (dolist (x lst) (funcall 'neovm--dqa-push-back dq x))
        dq)))
"#;

const DEQUE_CLEANUP: &str = r#"
    (fmakunbound 'neovm--dqa-make)
    (fmakunbound 'neovm--dqa-front)
    (fmakunbound 'neovm--dqa-back)
    (fmakunbound 'neovm--dqa-size)
    (fmakunbound 'neovm--dqa-set-front)
    (fmakunbound 'neovm--dqa-set-back)
    (fmakunbound 'neovm--dqa-set-size)
    (fmakunbound 'neovm--dqa-rebalance-front)
    (fmakunbound 'neovm--dqa-rebalance-back)
    (fmakunbound 'neovm--dqa-push-front)
    (fmakunbound 'neovm--dqa-push-back)
    (fmakunbound 'neovm--dqa-pop-front)
    (fmakunbound 'neovm--dqa-pop-back)
    (fmakunbound 'neovm--dqa-peek-front)
    (fmakunbound 'neovm--dqa-peek-back)
    (fmakunbound 'neovm--dqa-empty-p)
    (fmakunbound 'neovm--dqa-to-list)
    (fmakunbound 'neovm--dqa-clear)
    (fmakunbound 'neovm--dqa-from-list)
"#;

// ---------------------------------------------------------------------------
// Circular buffer implemented on top of deque with capacity limit
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_deque_circular_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {DEQUE_PREAMBLE}

  ;; Circular buffer: push-back evicts front when capacity exceeded
  (fset 'neovm--dqa-cbuf-make
    (lambda (capacity)
      (cons capacity (funcall 'neovm--dqa-make))))

  (fset 'neovm--dqa-cbuf-capacity (lambda (cb) (car cb)))
  (fset 'neovm--dqa-cbuf-deque (lambda (cb) (cdr cb)))

  (fset 'neovm--dqa-cbuf-push
    (lambda (cb val)
      (let ((dq (funcall 'neovm--dqa-cbuf-deque cb))
            (cap (funcall 'neovm--dqa-cbuf-capacity cb)))
        (when (>= (funcall 'neovm--dqa-size dq) cap)
          (funcall 'neovm--dqa-pop-front dq))
        (funcall 'neovm--dqa-push-back dq val))))

  (fset 'neovm--dqa-cbuf-contents
    (lambda (cb) (funcall 'neovm--dqa-to-list (funcall 'neovm--dqa-cbuf-deque cb))))

  (fset 'neovm--dqa-cbuf-size
    (lambda (cb) (funcall 'neovm--dqa-size (funcall 'neovm--dqa-cbuf-deque cb))))

  (unwind-protect
      (let ((cb (funcall 'neovm--dqa-cbuf-make 5)))
        ;; Push 1..8 into capacity-5 buffer
        (dotimes (i 8)
          (funcall 'neovm--dqa-cbuf-push cb (1+ i)))
        (let ((after-8 (funcall 'neovm--dqa-cbuf-contents cb))
              (size-after-8 (funcall 'neovm--dqa-cbuf-size cb)))
          ;; Push 3 more
          (funcall 'neovm--dqa-cbuf-push cb 9)
          (funcall 'neovm--dqa-cbuf-push cb 10)
          (funcall 'neovm--dqa-cbuf-push cb 11)
          (let ((after-11 (funcall 'neovm--dqa-cbuf-contents cb)))
            ;; Capacity-1 buffer: always last element
            (let ((cb1 (funcall 'neovm--dqa-cbuf-make 1)))
              (funcall 'neovm--dqa-cbuf-push cb1 'a)
              (funcall 'neovm--dqa-cbuf-push cb1 'b)
              (funcall 'neovm--dqa-cbuf-push cb1 'c)
              (list after-8 size-after-8
                    after-11
                    (funcall 'neovm--dqa-cbuf-contents cb1)
                    (funcall 'neovm--dqa-cbuf-size cb1))))))
    (fmakunbound 'neovm--dqa-cbuf-make)
    (fmakunbound 'neovm--dqa-cbuf-capacity)
    (fmakunbound 'neovm--dqa-cbuf-deque)
    (fmakunbound 'neovm--dqa-cbuf-push)
    (fmakunbound 'neovm--dqa-cbuf-contents)
    (fmakunbound 'neovm--dqa-cbuf-size)
    {DEQUE_CLEANUP}))"#
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Sliding window minimum using deque (complement to max in basic tests)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_deque_sliding_window_min() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {DEQUE_PREAMBLE}

  (fset 'neovm--dqa-sliding-min
    (lambda (arr k)
      "Return list of min values for each window of size K over ARR."
      (let ((dq (funcall 'neovm--dqa-make))
            (vec (vconcat arr))
            (n (length arr))
            (result nil)
            (i 0))
        (while (< i n)
          ;; Remove from back if >= current (maintain increasing order)
          (while (and (not (funcall 'neovm--dqa-empty-p dq))
                      (>= (aref vec (funcall 'neovm--dqa-peek-back dq))
                          (aref vec i)))
            (funcall 'neovm--dqa-pop-back dq))
          (funcall 'neovm--dqa-push-back dq i)
          ;; Remove front if out of window
          (when (< (funcall 'neovm--dqa-peek-front dq) (- i (1- k)))
            (funcall 'neovm--dqa-pop-front dq))
          ;; Record min for complete windows
          (when (>= i (1- k))
            (setq result (cons (aref vec (funcall 'neovm--dqa-peek-front dq)) result)))
          (setq i (1+ i)))
        (nreverse result))))

  (unwind-protect
      (list
       ;; Window of 3 over [1 3 -1 -3 5 3 6 7]
       (funcall 'neovm--dqa-sliding-min '(1 3 -1 -3 5 3 6 7) 3)
       ;; Window of 1 (each element is its own min)
       (funcall 'neovm--dqa-sliding-min '(4 2 7 1 9) 1)
       ;; Window of full length
       (funcall 'neovm--dqa-sliding-min '(3 1 4 1 5 9 2 6) 8)
       ;; Window of 2
       (funcall 'neovm--dqa-sliding-min '(10 5 8 3 12 7) 2)
       ;; Ascending input
       (funcall 'neovm--dqa-sliding-min '(1 2 3 4 5 6 7 8 9) 3)
       ;; Descending input
       (funcall 'neovm--dqa-sliding-min '(9 8 7 6 5 4 3 2 1) 3)
       ;; All same values
       (funcall 'neovm--dqa-sliding-min '(5 5 5 5 5) 3)
       ;; Window of 4 with negative values
       (funcall 'neovm--dqa-sliding-min '(-2 3 -5 1 4 -3 2 0) 4))
    (fmakunbound 'neovm--dqa-sliding-min)
    {DEQUE_CLEANUP}))"#
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Deque reversal and rotation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_deque_reverse_rotate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {DEQUE_PREAMBLE}

  ;; Reverse deque in place: pop all from front, push to front of new deque
  (fset 'neovm--dqa-reverse
    (lambda (dq)
      (let ((items nil))
        (while (not (funcall 'neovm--dqa-empty-p dq))
          (setq items (cons (funcall 'neovm--dqa-pop-front dq) items)))
        (dolist (x items) (funcall 'neovm--dqa-push-front dq x)))))

  ;; Rotate n elements from front to back
  (fset 'neovm--dqa-rotate-left
    (lambda (dq n)
      (dotimes (_ n)
        (unless (funcall 'neovm--dqa-empty-p dq)
          (funcall 'neovm--dqa-push-back dq (funcall 'neovm--dqa-pop-front dq))))))

  ;; Rotate n elements from back to front
  (fset 'neovm--dqa-rotate-right
    (lambda (dq n)
      (dotimes (_ n)
        (unless (funcall 'neovm--dqa-empty-p dq)
          (funcall 'neovm--dqa-push-front dq (funcall 'neovm--dqa-pop-back dq))))))

  (unwind-protect
      (let ((dq (funcall 'neovm--dqa-from-list '(1 2 3 4 5))))
        (let ((original (funcall 'neovm--dqa-to-list dq)))
          ;; Reverse
          (funcall 'neovm--dqa-reverse dq)
          (let ((reversed (funcall 'neovm--dqa-to-list dq)))
            ;; Reverse again (back to original)
            (funcall 'neovm--dqa-reverse dq)
            (let ((double-rev (funcall 'neovm--dqa-to-list dq)))
              ;; Rotate left by 2: [1 2 3 4 5] -> [3 4 5 1 2]
              (funcall 'neovm--dqa-rotate-left dq 2)
              (let ((rotated-l (funcall 'neovm--dqa-to-list dq)))
                ;; Rotate right by 2: back to original
                (funcall 'neovm--dqa-rotate-right dq 2)
                (let ((back-to-orig (funcall 'neovm--dqa-to-list dq)))
                  ;; Rotate left by full size = identity
                  (funcall 'neovm--dqa-rotate-left dq 5)
                  (let ((full-rotate (funcall 'neovm--dqa-to-list dq)))
                    ;; Rotate on empty deque (no-op)
                    (let ((empty-dq (funcall 'neovm--dqa-make)))
                      (funcall 'neovm--dqa-rotate-left empty-dq 3)
                      (list original reversed double-rev
                            rotated-l back-to-orig full-rotate
                            (funcall 'neovm--dqa-size dq)
                            (funcall 'neovm--dqa-empty-p empty-dq))))))))))
    (fmakunbound 'neovm--dqa-reverse)
    (fmakunbound 'neovm--dqa-rotate-left)
    (fmakunbound 'neovm--dqa-rotate-right)
    {DEQUE_CLEANUP}))"#
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Work-stealing deque simulation: owner pushes/pops back, thieves steal front
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_deque_work_stealing_sim() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {DEQUE_PREAMBLE}

  (unwind-protect
      (let ((owner-deque (funcall 'neovm--dqa-make))
            (thief1-work nil)
            (thief2-work nil)
            (owner-work nil))
        ;; Owner pushes tasks 1..12 (via push-back, owner end)
        (dotimes (i 12)
          (funcall 'neovm--dqa-push-back owner-deque (1+ i)))
        ;; Thief 1 steals 3 tasks from front
        (dotimes (_ 3)
          (unless (funcall 'neovm--dqa-empty-p owner-deque)
            (setq thief1-work (cons (funcall 'neovm--dqa-pop-front owner-deque) thief1-work))))
        ;; Owner processes 2 tasks from back
        (dotimes (_ 2)
          (unless (funcall 'neovm--dqa-empty-p owner-deque)
            (setq owner-work (cons (funcall 'neovm--dqa-pop-back owner-deque) owner-work))))
        ;; Owner adds 3 more tasks
        (dolist (t '(13 14 15))
          (funcall 'neovm--dqa-push-back owner-deque t))
        ;; Thief 2 steals 4 tasks from front
        (dotimes (_ 4)
          (unless (funcall 'neovm--dqa-empty-p owner-deque)
            (setq thief2-work (cons (funcall 'neovm--dqa-pop-front owner-deque) thief2-work))))
        ;; Owner drains remainder
        (while (not (funcall 'neovm--dqa-empty-p owner-deque))
          (setq owner-work (cons (funcall 'neovm--dqa-pop-back owner-deque) owner-work)))
        ;; Verify all tasks processed, total = 15
        (let ((all-work (sort (append (nreverse thief1-work)
                                      (nreverse thief2-work)
                                      (nreverse owner-work))
                              #'<)))
          (list (nreverse thief1-work)
                (nreverse thief2-work)
                (nreverse owner-work)
                all-work
                (length all-work)
                (funcall 'neovm--dqa-empty-p owner-deque))))
    {DEQUE_CLEANUP}))"#
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Deque-based BFS on an adjacency list graph
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_deque_bfs_graph() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {DEQUE_PREAMBLE}

  (fset 'neovm--dqa-bfs
    (lambda (graph start)
      "BFS from START on GRAPH (alist of node -> neighbor-list). Returns visit order."
      (let ((queue (funcall 'neovm--dqa-make))
            (visited nil)
            (order nil))
        (funcall 'neovm--dqa-push-back queue start)
        (setq visited (list start))
        (while (not (funcall 'neovm--dqa-empty-p queue))
          (let ((node (funcall 'neovm--dqa-pop-front queue)))
            (setq order (cons node order))
            (dolist (neighbor (cdr (assq node graph)))
              (unless (memq neighbor visited)
                (setq visited (cons neighbor visited))
                (funcall 'neovm--dqa-push-back queue neighbor)))))
        (nreverse order))))

  (unwind-protect
      (let ((graph '((a b c)
                     (b a d e)
                     (c a f)
                     (d b)
                     (e b f)
                     (f c e))))
        (list
         ;; BFS from a
         (funcall 'neovm--dqa-bfs graph 'a)
         ;; BFS from d
         (funcall 'neovm--dqa-bfs graph 'd)
         ;; BFS from f
         (funcall 'neovm--dqa-bfs graph 'f)
         ;; Linear graph: 1 -> 2 -> 3 -> 4 -> 5
         (funcall 'neovm--dqa-bfs '((1 2) (2 3) (3 4) (4 5) (5)) 1)
         ;; Star graph: center -> all
         (funcall 'neovm--dqa-bfs '((center a b c d) (a center) (b center) (c center) (d center)) 'center)
         ;; Single node
         (funcall 'neovm--dqa-bfs '((x)) 'x)))
    (fmakunbound 'neovm--dqa-bfs)
    {DEQUE_CLEANUP}))"#
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Priority deque with sorted insertion, extract-min/max, peek
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_deque_priority_advanced() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {DEQUE_PREAMBLE}

  (fset 'neovm--dqa-pdq-insert
    (lambda (dq val)
      "Insert VAL into sorted deque (ascending)."
      (let ((all (funcall 'neovm--dqa-to-list dq)))
        (funcall 'neovm--dqa-clear dq)
        (let ((inserted nil)
              (result nil))
          (dolist (x all)
            (when (and (not inserted) (< val x))
              (setq result (cons val result))
              (setq inserted t))
            (setq result (cons x result)))
          (unless inserted (setq result (cons val result)))
          (dolist (x result) (funcall 'neovm--dqa-push-front dq x))))))

  (unwind-protect
      (let ((pdq (funcall 'neovm--dqa-make)))
        ;; Insert shuffled values
        (dolist (v '(50 20 80 10 60 30 90 40 70))
          (funcall 'neovm--dqa-pdq-insert pdq v))
        (let ((sorted (funcall 'neovm--dqa-to-list pdq))
              (min-val (funcall 'neovm--dqa-peek-front pdq))
              (max-val (funcall 'neovm--dqa-peek-back pdq))
              (sz (funcall 'neovm--dqa-size pdq)))
          ;; Extract alternating min/max
          (let ((emin1 (funcall 'neovm--dqa-pop-front pdq))
                (emax1 (funcall 'neovm--dqa-pop-back pdq))
                (emin2 (funcall 'neovm--dqa-pop-front pdq))
                (emax2 (funcall 'neovm--dqa-pop-back pdq))
                (emin3 (funcall 'neovm--dqa-pop-front pdq)))
            ;; Insert into partially extracted deque
            (funcall 'neovm--dqa-pdq-insert pdq 5)
            (funcall 'neovm--dqa-pdq-insert pdq 95)
            (let ((after-insert (funcall 'neovm--dqa-to-list pdq)))
              (list sorted min-val max-val sz
                    emin1 emax1 emin2 emax2 emin3
                    after-insert
                    (funcall 'neovm--dqa-size pdq))))))
    (fmakunbound 'neovm--dqa-pdq-insert)
    {DEQUE_CLEANUP}))"#
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Deque stress test: many push/pop operations with verification
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_deque_stress_many_elements() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {DEQUE_PREAMBLE}
  (unwind-protect
      (let ((dq (funcall 'neovm--dqa-make)))
        ;; Push 50 elements to back
        (dotimes (i 50) (funcall 'neovm--dqa-push-back dq i))
        (let ((size-50 (funcall 'neovm--dqa-size dq))
              (front-val (funcall 'neovm--dqa-peek-front dq))
              (back-val (funcall 'neovm--dqa-peek-back dq)))
          ;; Pop 25 from front
          (let ((front-25 nil))
            (dotimes (_ 25)
              (setq front-25 (cons (funcall 'neovm--dqa-pop-front dq) front-25)))
            ;; Push 25 to front
            (dotimes (i 25) (funcall 'neovm--dqa-push-front dq (+ 100 i)))
            ;; Pop 25 from back
            (let ((back-25 nil))
              (dotimes (_ 25)
                (setq back-25 (cons (funcall 'neovm--dqa-pop-back dq) back-25)))
              ;; Remaining should have 25 elements
              (let ((remaining (funcall 'neovm--dqa-to-list dq)))
                (list size-50 front-val back-val
                      (nreverse front-25)
                      (nreverse back-25)
                      (funcall 'neovm--dqa-size dq)
                      (length remaining)))))))
    {DEQUE_CLEANUP}))"#
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Palindrome checker comparing front/back characters
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_deque_palindrome_advanced() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {DEQUE_PREAMBLE}

  (fset 'neovm--dqa-palindrome-p
    (lambda (str)
      "Check palindrome, ignoring case and non-alphanumeric chars."
      (let ((dq (funcall 'neovm--dqa-make))
            (i 0)
            (len (length str)))
        ;; Push only alphanumeric chars, lowercased
        (while (< i len)
          (let ((ch (aref str i)))
            (when (or (and (>= ch ?a) (<= ch ?z))
                      (and (>= ch ?A) (<= ch ?Z))
                      (and (>= ch ?0) (<= ch ?9)))
              (funcall 'neovm--dqa-push-back dq
                       (if (and (>= ch ?A) (<= ch ?Z))
                           (+ ch 32)
                         ch))))
          (setq i (1+ i)))
        ;; Compare from both ends
        (let ((is-pal t))
          (while (and is-pal (> (funcall 'neovm--dqa-size dq) 1))
            (unless (= (funcall 'neovm--dqa-pop-front dq)
                       (funcall 'neovm--dqa-pop-back dq))
              (setq is-pal nil)))
          is-pal))))

  (unwind-protect
      (list
       (funcall 'neovm--dqa-palindrome-p "racecar")
       (funcall 'neovm--dqa-palindrome-p "RaceCar")
       (funcall 'neovm--dqa-palindrome-p "A man, a plan, a canal: Panama")
       (funcall 'neovm--dqa-palindrome-p "hello")
       (funcall 'neovm--dqa-palindrome-p "")
       (funcall 'neovm--dqa-palindrome-p "a")
       (funcall 'neovm--dqa-palindrome-p "ab")
       (funcall 'neovm--dqa-palindrome-p "aba")
       (funcall 'neovm--dqa-palindrome-p "Was it a car or a cat I saw?")
       (funcall 'neovm--dqa-palindrome-p "No lemon, no melon")
       (funcall 'neovm--dqa-palindrome-p "not a palindrome"))
    (fmakunbound 'neovm--dqa-palindrome-p)
    {DEQUE_CLEANUP}))"#
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Deque as sliding window with both max and min simultaneously
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_deque_sliding_window_max_min() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {DEQUE_PREAMBLE}

  (fset 'neovm--dqa-sliding-maxmin
    (lambda (arr k)
      "Return list of (max . min) for each window of size K."
      (let ((max-dq (funcall 'neovm--dqa-make))
            (min-dq (funcall 'neovm--dqa-make))
            (vec (vconcat arr))
            (n (length arr))
            (result nil)
            (i 0))
        (while (< i n)
          ;; Max deque: remove from back if <= current
          (while (and (not (funcall 'neovm--dqa-empty-p max-dq))
                      (<= (aref vec (funcall 'neovm--dqa-peek-back max-dq)) (aref vec i)))
            (funcall 'neovm--dqa-pop-back max-dq))
          ;; Min deque: remove from back if >= current
          (while (and (not (funcall 'neovm--dqa-empty-p min-dq))
                      (>= (aref vec (funcall 'neovm--dqa-peek-back min-dq)) (aref vec i)))
            (funcall 'neovm--dqa-pop-back min-dq))
          (funcall 'neovm--dqa-push-back max-dq i)
          (funcall 'neovm--dqa-push-back min-dq i)
          ;; Remove out-of-window from front
          (when (< (funcall 'neovm--dqa-peek-front max-dq) (- i (1- k)))
            (funcall 'neovm--dqa-pop-front max-dq))
          (when (< (funcall 'neovm--dqa-peek-front min-dq) (- i (1- k)))
            (funcall 'neovm--dqa-pop-front min-dq))
          (when (>= i (1- k))
            (setq result (cons (cons (aref vec (funcall 'neovm--dqa-peek-front max-dq))
                                     (aref vec (funcall 'neovm--dqa-peek-front min-dq)))
                               result)))
          (setq i (1+ i)))
        (nreverse result))))

  (unwind-protect
      (list
       (funcall 'neovm--dqa-sliding-maxmin '(1 3 -1 -3 5 3 6 7) 3)
       (funcall 'neovm--dqa-sliding-maxmin '(5 5 5 5 5) 2)
       (funcall 'neovm--dqa-sliding-maxmin '(1 2 3 4 5) 5)
       (funcall 'neovm--dqa-sliding-maxmin '(9 1 8 2 7 3 6 4 5) 3))
    (fmakunbound 'neovm--dqa-sliding-maxmin)
    {DEQUE_CLEANUP}))"#
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Deque interleave: merge two deques alternating elements
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_deque_interleave_and_split() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {DEQUE_PREAMBLE}

  ;; Interleave two deques: take alternately from each
  (fset 'neovm--dqa-interleave
    (lambda (dq1 dq2)
      (let ((result (funcall 'neovm--dqa-make))
            (turn 0))
        (while (or (not (funcall 'neovm--dqa-empty-p dq1))
                   (not (funcall 'neovm--dqa-empty-p dq2)))
          (cond
           ((and (= (% turn 2) 0) (not (funcall 'neovm--dqa-empty-p dq1)))
            (funcall 'neovm--dqa-push-back result (funcall 'neovm--dqa-pop-front dq1)))
           ((not (funcall 'neovm--dqa-empty-p dq2))
            (funcall 'neovm--dqa-push-back result (funcall 'neovm--dqa-pop-front dq2)))
           ((not (funcall 'neovm--dqa-empty-p dq1))
            (funcall 'neovm--dqa-push-back result (funcall 'neovm--dqa-pop-front dq1))))
          (setq turn (1+ turn)))
        result)))

  ;; Split a deque into two: evens and odds by position
  (fset 'neovm--dqa-split
    (lambda (dq)
      (let ((evens (funcall 'neovm--dqa-make))
            (odds (funcall 'neovm--dqa-make))
            (idx 0))
        (while (not (funcall 'neovm--dqa-empty-p dq))
          (let ((val (funcall 'neovm--dqa-pop-front dq)))
            (if (= (% idx 2) 0)
                (funcall 'neovm--dqa-push-back evens val)
              (funcall 'neovm--dqa-push-back odds val)))
          (setq idx (1+ idx)))
        (cons evens odds))))

  (unwind-protect
      (let ((dq1 (funcall 'neovm--dqa-from-list '(a b c d e)))
            (dq2 (funcall 'neovm--dqa-from-list '(1 2 3 4 5))))
        (let* ((merged (funcall 'neovm--dqa-interleave dq1 dq2))
               (merged-list (funcall 'neovm--dqa-to-list merged))
               ;; Split the merged result
               (split (funcall 'neovm--dqa-split merged))
               (evens-list (funcall 'neovm--dqa-to-list (car split)))
               (odds-list (funcall 'neovm--dqa-to-list (cdr split))))
          ;; Unequal lengths
          (let ((short (funcall 'neovm--dqa-from-list '(x y)))
                (long (funcall 'neovm--dqa-from-list '(1 2 3 4 5 6))))
            (let ((merged2 (funcall 'neovm--dqa-interleave short long)))
              (list merged-list
                    evens-list
                    odds-list
                    (funcall 'neovm--dqa-to-list merged2))))))
    (fmakunbound 'neovm--dqa-interleave)
    (fmakunbound 'neovm--dqa-split)
    {DEQUE_CLEANUP}))"#
    );
    crate::common::assert_oracle_parity(&form);
}
