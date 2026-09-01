//! Oracle parity tests for queue and stack data structure patterns in Elisp.
//!
//! Tests FIFO queue, priority queue, double-ended queue, stack with
//! min-tracking, queue from two stacks, postfix expression evaluation,
//! and bracket matching.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// FIFO queue: enqueue, dequeue, peek, size, empty?
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_qs_fifo_queue() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Queue as (front-list . rear-list), enqueue to rear, dequeue from front
    let form = r#"(progn
  (fset 'neovm--qs-q-make (lambda () (cons nil nil)))

  (fset 'neovm--qs-q-enqueue
    (lambda (q val) (cons (car q) (cons val (cdr q)))))

  (fset 'neovm--qs-q-normalize
    (lambda (q)
      (if (null (car q))
          (cons (nreverse (cdr q)) nil)
        q)))

  (fset 'neovm--qs-q-dequeue
    (lambda (q)
      "Return (value . new-queue)."
      (let ((q2 (funcall 'neovm--qs-q-normalize q)))
        (if (null (car q2))
            (cons nil q2)
          (cons (caar q2) (cons (cdar q2) (cdr q2)))))))

  (fset 'neovm--qs-q-peek
    (lambda (q)
      (let ((q2 (funcall 'neovm--qs-q-normalize q)))
        (caar q2))))

  (fset 'neovm--qs-q-size
    (lambda (q) (+ (length (car q)) (length (cdr q)))))

  (fset 'neovm--qs-q-empty-p
    (lambda (q) (and (null (car q)) (null (cdr q)))))

  (fset 'neovm--qs-q-to-list
    (lambda (q)
      (let ((q2 (funcall 'neovm--qs-q-normalize q)))
        (append (car q2) (nreverse (copy-sequence (cdr q2)))))))

  (unwind-protect
      (let ((q (funcall 'neovm--qs-q-make)))
        ;; Initially empty
        (let ((empty1 (funcall 'neovm--qs-q-empty-p q))
              (size0 (funcall 'neovm--qs-q-size q)))
          ;; Enqueue several items
          (setq q (funcall 'neovm--qs-q-enqueue q 'alpha))
          (setq q (funcall 'neovm--qs-q-enqueue q 'beta))
          (setq q (funcall 'neovm--qs-q-enqueue q 'gamma))
          (setq q (funcall 'neovm--qs-q-enqueue q 'delta))
          (let ((size4 (funcall 'neovm--qs-q-size q))
                (peek1 (funcall 'neovm--qs-q-peek q))
                (contents1 (funcall 'neovm--qs-q-to-list q)))
            ;; Dequeue two items (FIFO order)
            (let* ((r1 (funcall 'neovm--qs-q-dequeue q))
                   (val1 (car r1)))
              (setq q (cdr r1))
              (let* ((r2 (funcall 'neovm--qs-q-dequeue q))
                     (val2 (car r2)))
                (setq q (cdr r2))
                (let ((size2 (funcall 'neovm--qs-q-size q))
                      (contents2 (funcall 'neovm--qs-q-to-list q)))
                  ;; Interleave: enqueue while dequeueing
                  (setq q (funcall 'neovm--qs-q-enqueue q 'epsilon))
                  (let* ((r3 (funcall 'neovm--qs-q-dequeue q))
                         (val3 (car r3)))
                    (setq q (cdr r3))
                    (list empty1 size0 size4 peek1 contents1
                          val1 val2 size2 contents2
                          val3
                          (funcall 'neovm--qs-q-to-list q)
                          (funcall 'neovm--qs-q-size q)))))))))
    (fmakunbound 'neovm--qs-q-make)
    (fmakunbound 'neovm--qs-q-enqueue)
    (fmakunbound 'neovm--qs-q-normalize)
    (fmakunbound 'neovm--qs-q-dequeue)
    (fmakunbound 'neovm--qs-q-peek)
    (fmakunbound 'neovm--qs-q-size)
    (fmakunbound 'neovm--qs-q-empty-p)
    (fmakunbound 'neovm--qs-q-to-list)))"#;
    let expect =
        expect_test::expect![[r#""OK (t 0 4 alpha (delta) delta nil 0 nil epsilon nil 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Priority queue with sorted insertion
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_qs_priority_queue() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Priority queue: sorted list (lowest priority value = highest priority)
    // Each entry is (priority . value)
    let form = r#"(progn
  (fset 'neovm--qs-pq-make (lambda () nil))

  (fset 'neovm--qs-pq-insert
    (lambda (pq priority value)
      "Insert (priority . value) maintaining sorted order (ascending priority)."
      (let ((entry (cons priority value))
            (prev nil) (curr pq) (inserted nil))
        (while (and curr (not inserted))
          (if (<= priority (caar curr))
              (progn
                (if prev
                    (setcdr prev (cons entry curr))
                  (setq pq (cons entry curr)))
                (setq inserted t))
            (setq prev curr curr (cdr curr))))
        (unless inserted
          (if prev
              (setcdr prev (list entry))
            (setq pq (list entry))))
        pq)))

  (fset 'neovm--qs-pq-pop
    (lambda (pq)
      "Return (value . remaining-pq)."
      (if (null pq)
          (cons nil nil)
        (cons (cdar pq) (cdr pq)))))

  (fset 'neovm--qs-pq-peek
    (lambda (pq)
      (if pq (cdar pq) nil)))

  (fset 'neovm--qs-pq-size
    (lambda (pq) (length pq)))

  (fset 'neovm--qs-pq-to-list
    (lambda (pq) (mapcar #'cdr pq)))

  (unwind-protect
      (let ((pq (funcall 'neovm--qs-pq-make)))
        ;; Insert with various priorities (not in order)
        (setq pq (funcall 'neovm--qs-pq-insert pq 5 "low-priority"))
        (setq pq (funcall 'neovm--qs-pq-insert pq 1 "urgent"))
        (setq pq (funcall 'neovm--qs-pq-insert pq 3 "medium"))
        (setq pq (funcall 'neovm--qs-pq-insert pq 1 "also-urgent"))
        (setq pq (funcall 'neovm--qs-pq-insert pq 2 "high"))
        (setq pq (funcall 'neovm--qs-pq-insert pq 4 "low"))
        (let ((size1 (funcall 'neovm--qs-pq-size pq))
              (peek1 (funcall 'neovm--qs-pq-peek pq))
              (order1 (funcall 'neovm--qs-pq-to-list pq)))
          ;; Pop three items (should come out in priority order)
          (let (popped)
            (dotimes (_ 3)
              (let ((r (funcall 'neovm--qs-pq-pop pq)))
                (setq popped (cons (car r) popped))
                (setq pq (cdr r))))
            (let ((remaining (funcall 'neovm--qs-pq-to-list pq))
                  (size2 (funcall 'neovm--qs-pq-size pq)))
              ;; Insert more with higher priority
              (setq pq (funcall 'neovm--qs-pq-insert pq 0 "critical"))
              (list size1 peek1 order1
                    (nreverse popped) remaining size2
                    (funcall 'neovm--qs-pq-peek pq)
                    (funcall 'neovm--qs-pq-to-list pq))))))
    (fmakunbound 'neovm--qs-pq-make)
    (fmakunbound 'neovm--qs-pq-insert)
    (fmakunbound 'neovm--qs-pq-pop)
    (fmakunbound 'neovm--qs-pq-peek)
    (fmakunbound 'neovm--qs-pq-size)
    (fmakunbound 'neovm--qs-pq-to-list)))"#;
    let expect = expect_test::expect![[
        r#""OK (6 \"also-urgent\" (\"also-urgent\" \"urgent\" \"high\" \"medium\" \"low\" \"low-priority\") (\"also-urgent\" \"urgent\" \"high\") (\"medium\" \"low\" \"low-priority\") 3 \"critical\" (\"critical\" \"medium\" \"low\" \"low-priority\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Double-ended queue (deque): push/pop front and back
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_qs_deque() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Deque using two lists: (front . rear)
    // front is in natural order, rear is reversed
    let form = r#"(progn
  (fset 'neovm--qs-dq-make (lambda () (cons nil nil)))

  (fset 'neovm--qs-dq-push-front
    (lambda (dq val) (cons (cons val (car dq)) (cdr dq))))

  (fset 'neovm--qs-dq-push-back
    (lambda (dq val) (cons (car dq) (cons val (cdr dq)))))

  (fset 'neovm--qs-dq-pop-front
    (lambda (dq)
      "Return (value . new-dq)."
      (let ((dq2 (if (null (car dq))
                     (cons (nreverse (cdr dq)) nil)
                   dq)))
        (if (null (car dq2))
            (cons nil dq2)
          (cons (caar dq2) (cons (cdar dq2) (cdr dq2)))))))

  (fset 'neovm--qs-dq-pop-back
    (lambda (dq)
      "Return (value . new-dq)."
      (let ((dq2 (if (null (cdr dq))
                     (cons nil (nreverse (car dq)))
                   dq)))
        (if (null (cdr dq2))
            (cons nil dq2)
          (cons (cadr dq2) (cons (car dq2) (cddr dq2)))))))

  (fset 'neovm--qs-dq-to-list
    (lambda (dq)
      (append (car dq) (nreverse (copy-sequence (cdr dq))))))

  (fset 'neovm--qs-dq-size
    (lambda (dq) (+ (length (car dq)) (length (cdr dq)))))

  (unwind-protect
      (let ((dq (funcall 'neovm--qs-dq-make)))
        ;; Push to front and back alternately
        (setq dq (funcall 'neovm--qs-dq-push-back dq 'C))
        (setq dq (funcall 'neovm--qs-dq-push-front dq 'B))
        (setq dq (funcall 'neovm--qs-dq-push-front dq 'A))
        (setq dq (funcall 'neovm--qs-dq-push-back dq 'D))
        (setq dq (funcall 'neovm--qs-dq-push-back dq 'E))
        (let ((contents1 (funcall 'neovm--qs-dq-to-list dq))
              (size1 (funcall 'neovm--qs-dq-size dq)))
          ;; Pop from front
          (let* ((r1 (funcall 'neovm--qs-dq-pop-front dq))
                 (front-val (car r1)))
            (setq dq (cdr r1))
            ;; Pop from back
            (let* ((r2 (funcall 'neovm--qs-dq-pop-back dq))
                   (back-val (car r2)))
              (setq dq (cdr r2))
              ;; Pop remaining
              (let ((remaining nil))
                (while (> (funcall 'neovm--qs-dq-size dq) 0)
                  (let ((r (funcall 'neovm--qs-dq-pop-front dq)))
                    (setq remaining (cons (car r) remaining))
                    (setq dq (cdr r))))
                (list contents1 size1
                      front-val back-val
                      (nreverse remaining)
                      (funcall 'neovm--qs-dq-size dq)))))))
    (fmakunbound 'neovm--qs-dq-make)
    (fmakunbound 'neovm--qs-dq-push-front)
    (fmakunbound 'neovm--qs-dq-push-back)
    (fmakunbound 'neovm--qs-dq-pop-front)
    (fmakunbound 'neovm--qs-dq-pop-back)
    (fmakunbound 'neovm--qs-dq-to-list)
    (fmakunbound 'neovm--qs-dq-size)))"#;
    let expect = expect_test::expect![[r#""OK ((A B C D E) 5 A E (B C D) 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Stack with min-tracking (get minimum in O(1))
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_qs_min_stack() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Each frame: (value . current-min)
    let form = r#"(progn
  (fset 'neovm--qs-ms-make (lambda () nil))

  (fset 'neovm--qs-ms-push
    (lambda (stack val)
      (let ((cur-min (if stack (cdr (car stack)) val)))
        (cons (cons val (min val cur-min)) stack))))

  (fset 'neovm--qs-ms-pop
    (lambda (stack) (cdr stack)))

  (fset 'neovm--qs-ms-top
    (lambda (stack) (caar stack)))

  (fset 'neovm--qs-ms-get-min
    (lambda (stack) (cdar stack)))

  (fset 'neovm--qs-ms-size
    (lambda (stack) (length stack)))

  (fset 'neovm--qs-ms-empty-p
    (lambda (stack) (null stack)))

  (unwind-protect
      (let ((s (funcall 'neovm--qs-ms-make)))
        ;; Push sequence: 5, 3, 7, 1, 4, 0, 8
        (let ((pushes '(5 3 7 1 4 0 8))
              (states nil))
          (dolist (val pushes)
            (setq s (funcall 'neovm--qs-ms-push s val))
            (setq states (cons (list (funcall 'neovm--qs-ms-top s)
                                     (funcall 'neovm--qs-ms-get-min s)
                                     (funcall 'neovm--qs-ms-size s))
                               states)))
          ;; Now pop and track min changes
          (let ((pop-states nil))
            (while (not (funcall 'neovm--qs-ms-empty-p s))
              (setq pop-states (cons (list (funcall 'neovm--qs-ms-top s)
                                           (funcall 'neovm--qs-ms-get-min s))
                                     pop-states))
              (setq s (funcall 'neovm--qs-ms-pop s)))
            (list (nreverse states) (nreverse pop-states)
                  (funcall 'neovm--qs-ms-empty-p s)))))
    (fmakunbound 'neovm--qs-ms-make)
    (fmakunbound 'neovm--qs-ms-push)
    (fmakunbound 'neovm--qs-ms-pop)
    (fmakunbound 'neovm--qs-ms-top)
    (fmakunbound 'neovm--qs-ms-get-min)
    (fmakunbound 'neovm--qs-ms-size)
    (fmakunbound 'neovm--qs-ms-empty-p)))"#;
    let expect = expect_test::expect![[
        r#""OK (((5 5 1) (3 3 2) (7 3 3) (1 1 4) (4 1 5) (0 0 6) (8 0 7)) ((8 0) (0 0) (4 1) (1 1) (7 3) (3 3) (5 5)) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Queue implemented using two stacks
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_qs_queue_from_two_stacks() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Classic two-stack queue: in-stack for enqueue, out-stack for dequeue
    // When out-stack empty, reverse in-stack onto out-stack
    let form = r#"(progn
  ;; Queue as (in-stack . out-stack)
  (fset 'neovm--qs-2sq-make (lambda () (cons nil nil)))

  (fset 'neovm--qs-2sq-enqueue
    (lambda (q val)
      (cons (cons val (car q)) (cdr q))))

  (fset 'neovm--qs-2sq-transfer
    (lambda (q)
      "Move in-stack to out-stack if out-stack is empty."
      (if (null (cdr q))
          (cons nil (nreverse (car q)))
        q)))

  (fset 'neovm--qs-2sq-dequeue
    (lambda (q)
      "Return (value . new-q)."
      (let ((q2 (funcall 'neovm--qs-2sq-transfer q)))
        (if (null (cdr q2))
            (cons nil q2)
          (cons (cadr q2) (cons (car q2) (cddr q2)))))))

  (fset 'neovm--qs-2sq-peek
    (lambda (q)
      (let ((q2 (funcall 'neovm--qs-2sq-transfer q)))
        (cadr q2))))

  (fset 'neovm--qs-2sq-size
    (lambda (q) (+ (length (car q)) (length (cdr q)))))

  (unwind-protect
      (let ((q (funcall 'neovm--qs-2sq-make)))
        ;; Enqueue 1..5
        (dotimes (i 5)
          (setq q (funcall 'neovm--qs-2sq-enqueue q (1+ i))))
        (let ((size1 (funcall 'neovm--qs-2sq-size q))
              (peek1 (funcall 'neovm--qs-2sq-peek q)))
          ;; Dequeue 3 items
          (let ((dequeued nil))
            (dotimes (_ 3)
              (let ((r (funcall 'neovm--qs-2sq-dequeue q)))
                (setq dequeued (cons (car r) dequeued))
                (setq q (cdr r))))
            ;; Enqueue more while partially dequeued
            (setq q (funcall 'neovm--qs-2sq-enqueue q 6))
            (setq q (funcall 'neovm--qs-2sq-enqueue q 7))
            (let ((size2 (funcall 'neovm--qs-2sq-size q))
                  (peek2 (funcall 'neovm--qs-2sq-peek q)))
              ;; Drain remaining
              (let ((rest nil))
                (while (> (funcall 'neovm--qs-2sq-size q) 0)
                  (let ((r (funcall 'neovm--qs-2sq-dequeue q)))
                    (setq rest (cons (car r) rest))
                    (setq q (cdr r))))
                (list size1 peek1
                      (nreverse dequeued)
                      size2 peek2
                      (nreverse rest)))))))
    (fmakunbound 'neovm--qs-2sq-make)
    (fmakunbound 'neovm--qs-2sq-enqueue)
    (fmakunbound 'neovm--qs-2sq-transfer)
    (fmakunbound 'neovm--qs-2sq-dequeue)
    (fmakunbound 'neovm--qs-2sq-peek)
    (fmakunbound 'neovm--qs-2sq-size)))"#;
    let expect = expect_test::expect![[r#""OK (5 1 (5 nil nil) 2 6 (7))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Evaluate postfix (RPN) expressions using a stack
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_qs_postfix_eval() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--qs-rpn-eval
    (lambda (tokens)
      "Evaluate a list of postfix tokens. Numbers are pushed, operators pop two and push result."
      (let ((stack nil))
        (dolist (tok tokens)
          (cond
            ((numberp tok)
             (setq stack (cons tok stack)))
            ((eq tok '+)
             (let ((b (car stack)) (a (cadr stack)))
               (setq stack (cons (+ a b) (cddr stack)))))
            ((eq tok '-)
             (let ((b (car stack)) (a (cadr stack)))
               (setq stack (cons (- a b) (cddr stack)))))
            ((eq tok '*)
             (let ((b (car stack)) (a (cadr stack)))
               (setq stack (cons (* a b) (cddr stack)))))
            ((eq tok '/)
             (let ((b (car stack)) (a (cadr stack)))
               (setq stack (cons (/ a b) (cddr stack)))))
            ((eq tok 'dup)
             (setq stack (cons (car stack) stack)))
            ((eq tok 'swap)
             (let ((top (car stack)) (sec (cadr stack)))
               (setq stack (cons sec (cons top (cddr stack))))))
            ((eq tok 'neg)
             (setq stack (cons (- (car stack)) (cdr stack))))))
        ;; Return top of stack (result) and remaining stack size
        (list (car stack) (length stack)))))

  (unwind-protect
      (list
        ;; 3 + 4 = 7
        (funcall 'neovm--qs-rpn-eval '(3 4 +))
        ;; (5 - 3) * 2 = 4
        (funcall 'neovm--qs-rpn-eval '(5 3 - 2 *))
        ;; 2 * (3 + 4) = 14
        (funcall 'neovm--qs-rpn-eval '(2 3 4 + *))
        ;; (10 + 20) * (30 - 5) = 750
        (funcall 'neovm--qs-rpn-eval '(10 20 + 30 5 - *))
        ;; 100 / 4 / 5 = 5
        (funcall 'neovm--qs-rpn-eval '(100 4 / 5 /))
        ;; Complex: (2 + 3) * (4 + 5) - 1 = 44
        (funcall 'neovm--qs-rpn-eval '(2 3 + 4 5 + * 1 -))
        ;; Using dup and swap: 5 dup * = 25
        (funcall 'neovm--qs-rpn-eval '(5 dup *))
        ;; 3 4 swap - = 4 - 3 = 1
        (funcall 'neovm--qs-rpn-eval '(3 4 swap -))
        ;; neg: 5 neg = -5, then + 10 = 5
        (funcall 'neovm--qs-rpn-eval '(5 neg 10 +)))
    (fmakunbound 'neovm--qs-rpn-eval)))"#;
    let expect = expect_test::expect![[
        r#""OK ((7 1) (4 1) (14 1) (750 1) (5 1) (44 1) (25 1) (1 1) (5 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Bracket matching with stack (multiple bracket types + line/col tracking)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_qs_bracket_matching() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--qs-bracket-check
    (lambda (input)
      "Check bracket matching. Return (ok matched-count) or (error message position)."
      (let ((stack nil)
            (i 0)
            (len (length input))
            (matched 0)
            (error nil)
            (openers (make-hash-table))
            (closers (make-hash-table))
            (names (make-hash-table)))
        ;; Setup bracket pairs
        (puthash ?\( ?\) openers)
        (puthash ?\[ ?\] openers)
        (puthash ?\{ ?\} openers)
        (puthash ?\) ?\( closers)
        (puthash ?\] ?\[ closers)
        (puthash ?\} ?\{ closers)
        (puthash ?\( "paren" names) (puthash ?\) "paren" names)
        (puthash ?\[ "bracket" names) (puthash ?\] "bracket" names)
        (puthash ?\{ "brace" names) (puthash ?\} "brace" names)
        (while (and (< i len) (not error))
          (let ((ch (aref input i)))
            (cond
              ;; Skip string literals (double-quoted)
              ((= ch ?\")
               (setq i (1+ i))
               (while (and (< i len) (/= (aref input i) ?\"))
                 (when (= (aref input i) ?\\)
                   (setq i (1+ i)))  ;; skip escaped char
                 (setq i (1+ i))))
              ;; Opening bracket
              ((gethash ch openers)
               (setq stack (cons (cons ch i) stack)))
              ;; Closing bracket
              ((gethash ch closers)
               (if (null stack)
                   (setq error (list 'error
                                     (concat "unmatched closing " (gethash ch names))
                                     i))
                 (let ((top-ch (caar stack))
                       (expected (gethash ch closers)))
                   (if (= top-ch expected)
                       (progn
                         (setq stack (cdr stack))
                         (setq matched (1+ matched)))
                     (setq error
                           (list 'error
                                 (concat "expected closing "
                                         (gethash (gethash top-ch openers) names)
                                         " but got "
                                         (gethash ch names))
                                 i))))))))
          (setq i (1+ i)))
        (cond
          (error error)
          (stack (list 'error
                       (concat "unclosed " (gethash (caar stack) names))
                       (cdar stack)))
          (t (list 'ok matched))))))

  (unwind-protect
      (list
        (funcall 'neovm--qs-bracket-check "()")
        (funcall 'neovm--qs-bracket-check "()[]{}")
        (funcall 'neovm--qs-bracket-check "{[()()]}")
        (funcall 'neovm--qs-bracket-check "((([])))")
        (funcall 'neovm--qs-bracket-check "(]")
        (funcall 'neovm--qs-bracket-check "(()")
        (funcall 'neovm--qs-bracket-check ")(")
        (funcall 'neovm--qs-bracket-check "")
        ;; With string content (brackets inside strings ignored)
        (funcall 'neovm--qs-bracket-check "func(\"arg[0]\")")
        ;; Complex code-like input
        (funcall 'neovm--qs-bracket-check "if (a[i] > 0) { b[j] = {x, y}; }"))
    (fmakunbound 'neovm--qs-bracket-check)))"#;
    let expect = expect_test::expect![[
        r#""OK ((ok 1) (ok 3) (ok 4) (ok 4) (error \"expected closing paren but got bracket\" 1) (error \"unclosed paren\" 0) (error \"unmatched closing paren\" 0) (ok 0) (ok 1) (ok 5))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Monotonic stack: find next greater element for each position
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_qs_monotonic_stack() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--qs-next-greater
    (lambda (nums)
      "For each element, find the next greater element to the right. -1 if none."
      (let* ((n (length nums))
             (result (make-vector n -1))
             (stack nil)  ;; stack of indices
             (i 0))
        (while (< i n)
          (let ((current (nth i nums)))
            ;; Pop all indices whose value is smaller than current
            (while (and stack (< (nth (car stack) nums) current))
              (aset result (car stack) current)
              (setq stack (cdr stack)))
            (setq stack (cons i stack)))
          (setq i (1+ i)))
        (append result nil))))  ;; convert vector to list

  (fset 'neovm--qs-daily-temps
    (lambda (temps)
      "For each day, how many days until a warmer temperature? 0 if never."
      (let* ((n (length temps))
             (result (make-vector n 0))
             (stack nil)
             (i 0))
        (while (< i n)
          (let ((current (nth i temps)))
            (while (and stack (< (nth (car stack) temps) current))
              (aset result (car stack) (- i (car stack)))
              (setq stack (cdr stack)))
            (setq stack (cons i stack)))
          (setq i (1+ i)))
        (append result nil))))

  (unwind-protect
      (list
        ;; Next greater element
        (funcall 'neovm--qs-next-greater '(4 5 2 10 8))
        (funcall 'neovm--qs-next-greater '(1 2 3 4 5))
        (funcall 'neovm--qs-next-greater '(5 4 3 2 1))
        (funcall 'neovm--qs-next-greater '(2 7 3 5 4 6 8))
        ;; Daily temperatures
        (funcall 'neovm--qs-daily-temps '(73 74 75 71 69 72 76 73))
        (funcall 'neovm--qs-daily-temps '(30 30 30))
        (funcall 'neovm--qs-daily-temps '(100 90 80 70)))
    (fmakunbound 'neovm--qs-next-greater)
    (fmakunbound 'neovm--qs-daily-temps)))"#;
    let expect = expect_test::expect![[
        r#""OK ((5 10 10 -1 -1) (2 3 4 5 -1) (-1 -1 -1 -1 -1) (7 8 5 6 6 8 -1) (1 1 4 2 1 1 0 0) (0 0 0) (0 0 0 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
