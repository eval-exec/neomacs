//! Oracle parity tests for a double-ended queue (deque) implemented in Elisp.
//!
//! The deque is built on cons-cell lists with front/back sentinels and size
//! tracking.  Tests cover push/pop front/back, peek, iteration, sliding
//! window algorithms, and priority deque (sorted insertion).

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Shared deque preamble — provides: make-deque, push-front, push-back,
// pop-front, pop-back, peek-front, peek-back, deque-size, deque-empty-p,
// deque-to-list, deque-to-list-rev, deque-clear.
//
// Representation: (front-list . (back-list . size))
// Invariant: when front is empty and back is non-empty, rebalance by
// reversing back into front (amortized O(1)).
// ---------------------------------------------------------------------------

const DEQUE_PREAMBLE: &str = r#"
  ;; ---- Constructor ----
  (fset 'neovm--dq-make (lambda () (cons nil (cons nil 0))))

  ;; ---- Accessors ----
  (fset 'neovm--dq-front (lambda (dq) (car dq)))
  (fset 'neovm--dq-back  (lambda (dq) (cadr dq)))
  (fset 'neovm--dq-size  (lambda (dq) (cddr dq)))

  ;; ---- Setters (mutating) ----
  (fset 'neovm--dq-set-front (lambda (dq v) (setcar dq v)))
  (fset 'neovm--dq-set-back  (lambda (dq v) (setcar (cdr dq) v)))
  (fset 'neovm--dq-set-size  (lambda (dq v) (setcdr (cdr dq) v)))

  ;; ---- Rebalance: if front empty, reverse back into front ----
  (fset 'neovm--dq-rebalance-front
    (lambda (dq)
      (when (and (null (funcall 'neovm--dq-front dq))
                 (not (null (funcall 'neovm--dq-back dq))))
        (funcall 'neovm--dq-set-front dq (nreverse (funcall 'neovm--dq-back dq)))
        (funcall 'neovm--dq-set-back dq nil))))

  ;; ---- Rebalance: if back empty, reverse front into back ----
  (fset 'neovm--dq-rebalance-back
    (lambda (dq)
      (when (and (null (funcall 'neovm--dq-back dq))
                 (not (null (funcall 'neovm--dq-front dq))))
        (funcall 'neovm--dq-set-back dq (nreverse (funcall 'neovm--dq-front dq)))
        (funcall 'neovm--dq-set-front dq nil))))

  ;; ---- Push front ----
  (fset 'neovm--dq-push-front
    (lambda (dq val)
      (funcall 'neovm--dq-set-front dq (cons val (funcall 'neovm--dq-front dq)))
      (funcall 'neovm--dq-set-size dq (1+ (funcall 'neovm--dq-size dq)))))

  ;; ---- Push back ----
  (fset 'neovm--dq-push-back
    (lambda (dq val)
      (funcall 'neovm--dq-set-back dq (cons val (funcall 'neovm--dq-back dq)))
      (funcall 'neovm--dq-set-size dq (1+ (funcall 'neovm--dq-size dq)))))

  ;; ---- Pop front ----
  (fset 'neovm--dq-pop-front
    (lambda (dq)
      (funcall 'neovm--dq-rebalance-front dq)
      (if (null (funcall 'neovm--dq-front dq))
          (error "deque underflow")
        (let ((val (car (funcall 'neovm--dq-front dq))))
          (funcall 'neovm--dq-set-front dq (cdr (funcall 'neovm--dq-front dq)))
          (funcall 'neovm--dq-set-size dq (1- (funcall 'neovm--dq-size dq)))
          val))))

  ;; ---- Pop back ----
  (fset 'neovm--dq-pop-back
    (lambda (dq)
      (funcall 'neovm--dq-rebalance-back dq)
      (if (null (funcall 'neovm--dq-back dq))
          (error "deque underflow")
        (let ((val (car (funcall 'neovm--dq-back dq))))
          (funcall 'neovm--dq-set-back dq (cdr (funcall 'neovm--dq-back dq)))
          (funcall 'neovm--dq-set-size dq (1- (funcall 'neovm--dq-size dq)))
          val))))

  ;; ---- Peek front ----
  (fset 'neovm--dq-peek-front
    (lambda (dq)
      (funcall 'neovm--dq-rebalance-front dq)
      (car (funcall 'neovm--dq-front dq))))

  ;; ---- Peek back ----
  (fset 'neovm--dq-peek-back
    (lambda (dq)
      (funcall 'neovm--dq-rebalance-back dq)
      (car (funcall 'neovm--dq-back dq))))

  ;; ---- Empty predicate ----
  (fset 'neovm--dq-empty-p
    (lambda (dq) (= 0 (funcall 'neovm--dq-size dq))))

  ;; ---- Convert to list (front to back order) ----
  (fset 'neovm--dq-to-list
    (lambda (dq)
      (append (funcall 'neovm--dq-front dq)
              (nreverse (copy-sequence (funcall 'neovm--dq-back dq))))))

  ;; ---- Convert to list (back to front order) ----
  (fset 'neovm--dq-to-list-rev
    (lambda (dq)
      (append (funcall 'neovm--dq-back dq)
              (nreverse (copy-sequence (funcall 'neovm--dq-front dq))))))

  ;; ---- Clear ----
  (fset 'neovm--dq-clear
    (lambda (dq)
      (funcall 'neovm--dq-set-front dq nil)
      (funcall 'neovm--dq-set-back dq nil)
      (funcall 'neovm--dq-set-size dq 0)))
"#;

const DEQUE_CLEANUP: &str = r#"
    (fmakunbound 'neovm--dq-make)
    (fmakunbound 'neovm--dq-front)
    (fmakunbound 'neovm--dq-back)
    (fmakunbound 'neovm--dq-size)
    (fmakunbound 'neovm--dq-set-front)
    (fmakunbound 'neovm--dq-set-back)
    (fmakunbound 'neovm--dq-set-size)
    (fmakunbound 'neovm--dq-rebalance-front)
    (fmakunbound 'neovm--dq-rebalance-back)
    (fmakunbound 'neovm--dq-push-front)
    (fmakunbound 'neovm--dq-push-back)
    (fmakunbound 'neovm--dq-pop-front)
    (fmakunbound 'neovm--dq-pop-back)
    (fmakunbound 'neovm--dq-peek-front)
    (fmakunbound 'neovm--dq-peek-back)
    (fmakunbound 'neovm--dq-empty-p)
    (fmakunbound 'neovm--dq-to-list)
    (fmakunbound 'neovm--dq-to-list-rev)
    (fmakunbound 'neovm--dq-clear)
"#;

// ---------------------------------------------------------------------------
// Push front/back and pop front/back — basic FIFO/LIFO behavior
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_deque_push_pop_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {DEQUE_PREAMBLE}
  (unwind-protect
      (let ((dq (funcall 'neovm--dq-make)))
        ;; Push front: 3, 2, 1 => front-to-back: 1, 2, 3
        (funcall 'neovm--dq-push-front dq 3)
        (funcall 'neovm--dq-push-front dq 2)
        (funcall 'neovm--dq-push-front dq 1)
        (let ((after-push-front (funcall 'neovm--dq-to-list dq))
              (size-3 (funcall 'neovm--dq-size dq)))
          ;; Push back: 4, 5
          (funcall 'neovm--dq-push-back dq 4)
          (funcall 'neovm--dq-push-back dq 5)
          (let ((after-push-back (funcall 'neovm--dq-to-list dq))
                (size-5 (funcall 'neovm--dq-size dq)))
            ;; Pop front: should get 1
            (let ((pf1 (funcall 'neovm--dq-pop-front dq))
                  (pf2 (funcall 'neovm--dq-pop-front dq)))
              ;; Pop back: should get 5
              (let ((pb1 (funcall 'neovm--dq-pop-back dq))
                    (pb2 (funcall 'neovm--dq-pop-back dq)))
                ;; One element remains: 3
                (let ((remaining (funcall 'neovm--dq-to-list dq)))
                  (list after-push-front size-3
                        after-push-back size-5
                        pf1 pf2 pb1 pb2
                        remaining
                        (funcall 'neovm--dq-size dq))))))))
    {DEQUE_CLEANUP}))"#
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Peek operations — non-destructive inspection of front/back
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_deque_peek_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {DEQUE_PREAMBLE}
  (unwind-protect
      (let ((dq (funcall 'neovm--dq-make)))
        ;; Push back only: peek-front should still work via rebalance
        (funcall 'neovm--dq-push-back dq 'a)
        (funcall 'neovm--dq-push-back dq 'b)
        (funcall 'neovm--dq-push-back dq 'c)
        (let ((pf (funcall 'neovm--dq-peek-front dq))
              (pb (funcall 'neovm--dq-peek-back dq))
              (sz-before (funcall 'neovm--dq-size dq)))
          ;; Peek should not change size
          (let ((sz-after (funcall 'neovm--dq-size dq)))
            ;; Push front only into new deque: peek-back should work
            (let ((dq2 (funcall 'neovm--dq-make)))
              (funcall 'neovm--dq-push-front dq2 10)
              (funcall 'neovm--dq-push-front dq2 20)
              (funcall 'neovm--dq-push-front dq2 30)
              (let ((pf2 (funcall 'neovm--dq-peek-front dq2))
                    (pb2 (funcall 'neovm--dq-peek-back dq2)))
                ;; Multiple peeks are idempotent
                (let ((pf3 (funcall 'neovm--dq-peek-front dq2))
                      (pb3 (funcall 'neovm--dq-peek-back dq2)))
                  (list pf pb sz-before sz-after
                        pf2 pb2
                        (eq pf2 pf3) (eq pb2 pb3)
                        (funcall 'neovm--dq-size dq2))))))))
    {DEQUE_CLEANUP}))"#
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Size tracking and empty predicate
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_deque_size_tracking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {DEQUE_PREAMBLE}
  (unwind-protect
      (let ((dq (funcall 'neovm--dq-make)))
        (let ((sizes nil))
          ;; Track size after each operation
          (setq sizes (cons (funcall 'neovm--dq-size dq) sizes))  ;; 0
          (setq sizes (cons (funcall 'neovm--dq-empty-p dq) sizes))  ;; t
          (funcall 'neovm--dq-push-front dq 'x)
          (setq sizes (cons (funcall 'neovm--dq-size dq) sizes))  ;; 1
          (setq sizes (cons (funcall 'neovm--dq-empty-p dq) sizes))  ;; nil
          (funcall 'neovm--dq-push-back dq 'y)
          (setq sizes (cons (funcall 'neovm--dq-size dq) sizes))  ;; 2
          (funcall 'neovm--dq-push-front dq 'z)
          (setq sizes (cons (funcall 'neovm--dq-size dq) sizes))  ;; 3
          (funcall 'neovm--dq-pop-front dq)
          (setq sizes (cons (funcall 'neovm--dq-size dq) sizes))  ;; 2
          (funcall 'neovm--dq-pop-back dq)
          (setq sizes (cons (funcall 'neovm--dq-size dq) sizes))  ;; 1
          (funcall 'neovm--dq-pop-front dq)
          (setq sizes (cons (funcall 'neovm--dq-size dq) sizes))  ;; 0
          (setq sizes (cons (funcall 'neovm--dq-empty-p dq) sizes))  ;; t
          ;; Clear test
          (funcall 'neovm--dq-push-back dq 1)
          (funcall 'neovm--dq-push-back dq 2)
          (funcall 'neovm--dq-push-back dq 3)
          (funcall 'neovm--dq-clear dq)
          (setq sizes (cons (funcall 'neovm--dq-size dq) sizes))  ;; 0
          (setq sizes (cons (funcall 'neovm--dq-empty-p dq) sizes))  ;; t
          (nreverse sizes)))
    {DEQUE_CLEANUP}))"#
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Iterate forward (front-to-back) and backward (back-to-front)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_deque_iteration() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {DEQUE_PREAMBLE}
  (unwind-protect
      (let ((dq (funcall 'neovm--dq-make)))
        ;; Build deque with mixed push-front and push-back
        (funcall 'neovm--dq-push-back dq 'c)
        (funcall 'neovm--dq-push-back dq 'd)
        (funcall 'neovm--dq-push-back dq 'e)
        (funcall 'neovm--dq-push-front dq 'b)
        (funcall 'neovm--dq-push-front dq 'a)
        ;; Forward iteration: a b c d e
        (let ((fwd (funcall 'neovm--dq-to-list dq))
              ;; Reverse iteration: e d c b a
              (bwd (funcall 'neovm--dq-to-list-rev dq)))
          ;; Drain via pop-front to verify order
          (let ((drain-fwd nil))
            (while (not (funcall 'neovm--dq-empty-p dq))
              (setq drain-fwd (cons (funcall 'neovm--dq-pop-front dq) drain-fwd)))
            ;; Rebuild and drain via pop-back
            (dolist (x '(a b c d e))
              (funcall 'neovm--dq-push-back dq x))
            (let ((drain-bwd nil))
              (while (not (funcall 'neovm--dq-empty-p dq))
                (setq drain-bwd (cons (funcall 'neovm--dq-pop-back dq) drain-bwd)))
              (list fwd bwd
                    (nreverse drain-fwd)
                    (nreverse drain-bwd))))))
    {DEQUE_CLEANUP}))"#
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Sliding window maximum using deque
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_deque_sliding_window_max() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Classic algorithm: maintain a deque of indices where values are
    // monotonically decreasing.  For each new element, pop from back
    // while back value <= new value.  Pop from front if out of window.
    let form = format!(
        r#"(progn
  {DEQUE_PREAMBLE}

  (fset 'neovm--dq-sliding-window-max
    (lambda (arr k)
      "Return list of max values for each window of size K over ARR (a list)."
      (let ((dq (funcall 'neovm--dq-make))  ;; deque of indices
            (vec (vconcat arr))
            (n (length arr))
            (result nil))
        (let ((i 0))
          (while (< i n)
            ;; Remove elements from back that are <= current
            (while (and (not (funcall 'neovm--dq-empty-p dq))
                        (<= (aref vec (funcall 'neovm--dq-peek-back dq))
                            (aref vec i)))
              (funcall 'neovm--dq-pop-back dq))
            ;; Push current index to back
            (funcall 'neovm--dq-push-back dq i)
            ;; Remove front if out of window
            (when (< (funcall 'neovm--dq-peek-front dq) (- i (1- k)))
              (funcall 'neovm--dq-pop-front dq))
            ;; Record max for complete windows
            (when (>= i (1- k))
              (setq result (cons (aref vec (funcall 'neovm--dq-peek-front dq)) result)))
            (setq i (1+ i))))
        (nreverse result))))

  (unwind-protect
      (list
       ;; Window of 3 over [1 3 -1 -3 5 3 6 7]
       (funcall 'neovm--dq-sliding-window-max '(1 3 -1 -3 5 3 6 7) 3)
       ;; Window of 1 (each element is its own max)
       (funcall 'neovm--dq-sliding-window-max '(4 2 7 1 9) 1)
       ;; Window of full length (global max repeated once)
       (funcall 'neovm--dq-sliding-window-max '(3 1 4 1 5 9 2 6) 8)
       ;; Window of 2
       (funcall 'neovm--dq-sliding-window-max '(10 5 8 3 12 7) 2)
       ;; Descending input
       (funcall 'neovm--dq-sliding-window-max '(9 8 7 6 5 4 3 2 1) 3)
       ;; Ascending input
       (funcall 'neovm--dq-sliding-window-max '(1 2 3 4 5 6 7 8 9) 3)
       ;; All same values
       (funcall 'neovm--dq-sliding-window-max '(5 5 5 5 5) 3))
    (fmakunbound 'neovm--dq-sliding-window-max)
    {DEQUE_CLEANUP}))"#
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Priority deque — sorted insertion, extract min/max
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_deque_priority_sorted_insertion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A priority deque maintains elements in sorted order.
    // We implement it as a sorted list wrapped in our deque interface.
    let form = format!(
        r#"(progn
  {DEQUE_PREAMBLE}

  ;; Priority deque: maintains sorted order in front list.
  ;; Insert scans to find correct position.
  ;; Min = peek-front, Max = peek-back (after rebalance).
  (fset 'neovm--pdq-make (lambda () (funcall 'neovm--dq-make)))

  (fset 'neovm--pdq-insert
    (lambda (dq val)
      "Insert VAL into sorted deque."
      ;; First, materialize all elements into a single sorted list
      (let ((all (funcall 'neovm--dq-to-list dq)))
        ;; Clear the deque
        (funcall 'neovm--dq-clear dq)
        ;; Insert val into sorted position
        (let ((inserted nil)
              (result nil))
          (dolist (x all)
            (when (and (not inserted) (< val x))
              (setq result (cons val result))
              (setq inserted t))
            (setq result (cons x result)))
          (unless inserted (setq result (cons val result)))
          ;; Rebuild deque from sorted list (reversed, so push-front)
          (dolist (x result)
            (funcall 'neovm--dq-push-front dq x))))))

  (fset 'neovm--pdq-extract-min
    (lambda (dq) (funcall 'neovm--dq-pop-front dq)))

  (fset 'neovm--pdq-extract-max
    (lambda (dq) (funcall 'neovm--dq-pop-back dq)))

  (fset 'neovm--pdq-peek-min
    (lambda (dq) (funcall 'neovm--dq-peek-front dq)))

  (fset 'neovm--pdq-peek-max
    (lambda (dq) (funcall 'neovm--dq-peek-back dq)))

  (unwind-protect
      (let ((pdq (funcall 'neovm--pdq-make)))
        ;; Insert in random order
        (funcall 'neovm--pdq-insert pdq 50)
        (funcall 'neovm--pdq-insert pdq 30)
        (funcall 'neovm--pdq-insert pdq 70)
        (funcall 'neovm--pdq-insert pdq 10)
        (funcall 'neovm--pdq-insert pdq 90)
        (funcall 'neovm--pdq-insert pdq 20)
        (funcall 'neovm--pdq-insert pdq 80)
        (let ((sorted-list (funcall 'neovm--dq-to-list pdq))
              (min-val (funcall 'neovm--pdq-peek-min pdq))
              (max-val (funcall 'neovm--pdq-peek-max pdq))
              (sz (funcall 'neovm--dq-size pdq)))
          ;; Extract min and max alternately
          (let ((emin1 (funcall 'neovm--pdq-extract-min pdq))
                (emax1 (funcall 'neovm--pdq-extract-max pdq))
                (emin2 (funcall 'neovm--pdq-extract-min pdq))
                (emax2 (funcall 'neovm--pdq-extract-max pdq)))
            (let ((remaining (funcall 'neovm--dq-to-list pdq)))
              (list sorted-list min-val max-val sz
                    emin1 emax1 emin2 emax2
                    remaining
                    (funcall 'neovm--dq-size pdq))))))
    (fmakunbound 'neovm--pdq-make)
    (fmakunbound 'neovm--pdq-insert)
    (fmakunbound 'neovm--pdq-extract-min)
    (fmakunbound 'neovm--pdq-extract-max)
    (fmakunbound 'neovm--pdq-peek-min)
    (fmakunbound 'neovm--pdq-peek-max)
    {DEQUE_CLEANUP}))"#
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Deque used as both stack and queue — interleaved operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_deque_stack_queue_interleaved() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {DEQUE_PREAMBLE}
  (unwind-protect
      (let ((dq (funcall 'neovm--dq-make)))
        ;; Use as stack (LIFO) via push-front/pop-front
        (funcall 'neovm--dq-push-front dq 'a)
        (funcall 'neovm--dq-push-front dq 'b)
        (funcall 'neovm--dq-push-front dq 'c)
        (let ((stack-pop1 (funcall 'neovm--dq-pop-front dq))   ;; c
              (stack-pop2 (funcall 'neovm--dq-pop-front dq)))  ;; b
          ;; Now use as queue (FIFO) via push-back/pop-front
          ;; Currently has: [a]
          (funcall 'neovm--dq-push-back dq 'd)
          (funcall 'neovm--dq-push-back dq 'e)
          (funcall 'neovm--dq-push-back dq 'f)
          ;; Queue order front-to-back: a d e f
          (let ((q-pop1 (funcall 'neovm--dq-pop-front dq))   ;; a
                (q-pop2 (funcall 'neovm--dq-pop-front dq)))  ;; d
            ;; Interleave: push front and back, then drain
            (funcall 'neovm--dq-push-front dq 'X)
            (funcall 'neovm--dq-push-back dq 'Y)
            ;; Current: X e f Y
            (let ((contents (funcall 'neovm--dq-to-list dq)))
              ;; Drain all via pop-back
              (let ((drain nil))
                (while (not (funcall 'neovm--dq-empty-p dq))
                  (setq drain (cons (funcall 'neovm--dq-pop-back dq) drain)))
                (list stack-pop1 stack-pop2
                      q-pop1 q-pop2
                      contents
                      drain
                      (funcall 'neovm--dq-empty-p dq)))))))
    {DEQUE_CLEANUP}))"#
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Deque palindrome checker — push chars from both ends and compare
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_deque_palindrome_checker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {DEQUE_PREAMBLE}

  (fset 'neovm--dq-palindrome-p
    (lambda (str)
      "Check if STR is a palindrome using a deque."
      (let ((dq (funcall 'neovm--dq-make))
            (i 0)
            (len (length str)))
        ;; Push all chars
        (while (< i len)
          (funcall 'neovm--dq-push-back dq (aref str i))
          (setq i (1+ i)))
        ;; Compare from both ends
        (let ((is-palindrome t))
          (while (and is-palindrome (> (funcall 'neovm--dq-size dq) 1))
            (let ((front (funcall 'neovm--dq-pop-front dq))
                  (back (funcall 'neovm--dq-pop-back dq)))
              (unless (= front back)
                (setq is-palindrome nil))))
          is-palindrome))))

  (unwind-protect
      (list
       (funcall 'neovm--dq-palindrome-p "racecar")
       (funcall 'neovm--dq-palindrome-p "madam")
       (funcall 'neovm--dq-palindrome-p "hello")
       (funcall 'neovm--dq-palindrome-p "a")
       (funcall 'neovm--dq-palindrome-p "ab")
       (funcall 'neovm--dq-palindrome-p "aba")
       (funcall 'neovm--dq-palindrome-p "abba")
       (funcall 'neovm--dq-palindrome-p "abcd")
       (funcall 'neovm--dq-palindrome-p ""))
    (fmakunbound 'neovm--dq-palindrome-p)
    {DEQUE_CLEANUP}))"#
    );
    crate::common::assert_oracle_parity(&form);
}
