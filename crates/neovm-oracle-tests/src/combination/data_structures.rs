//! Complex oracle tests for data structure patterns in Elisp.
//!
//! Tests implementation of common data structures: priority queues,
//! graphs, sets, ring buffers, trie, and LRU cache using only
//! Elisp primitives.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Set operations via hash tables
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ds_set_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Set union, intersection, difference using hash tables
    let form = "(let ((make-set
                       (lambda (lst)
                         (let ((s (make-hash-table :test 'equal)))
                           (dolist (x lst) (puthash x t s))
                           s)))
                      (set-to-sorted-list
                       (lambda (s)
                         (let ((r nil))
                           (maphash (lambda (k _v)
                                      (setq r (cons k r)))
                                    s)
                           (sort r #'<))))
                      (set-union
                       (lambda (a b)
                         (let ((result (make-hash-table :test 'equal)))
                           (maphash (lambda (k _v)
                                      (puthash k t result)) a)
                           (maphash (lambda (k _v)
                                      (puthash k t result)) b)
                           result)))
                      (set-intersection
                       (lambda (a b)
                         (let ((result (make-hash-table :test 'equal)))
                           (maphash (lambda (k _v)
                                      (when (gethash k b)
                                        (puthash k t result)))
                                    a)
                           result)))
                      (set-difference
                       (lambda (a b)
                         (let ((result (make-hash-table :test 'equal)))
                           (maphash (lambda (k _v)
                                      (unless (gethash k b)
                                        (puthash k t result)))
                                    a)
                           result))))
                  (let ((a (funcall make-set '(1 2 3 4 5)))
                        (b (funcall make-set '(3 4 5 6 7))))
                    (list
                      (funcall set-to-sorted-list
                               (funcall set-union a b))
                      (funcall set-to-sorted-list
                               (funcall set-intersection a b))
                      (funcall set-to-sorted-list
                               (funcall set-difference a b)))))";
    let expect = expect_test::expect![[r#""OK ((1 2 3 4 5 6 7) (3 4 5) (1 2))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Ring buffer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ds_ring_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Fixed-size ring buffer using a vector
    let form = "(let ((make-ring
                       (lambda (size)
                         (list (make-vector size nil) 0 0 size)))
                      (ring-push
                       (lambda (ring val)
                         (let ((buf (car ring))
                               (write-pos (cadr ring))
                               (size (cadddr ring)))
                           (aset buf write-pos val)
                           (setcar (cdr ring)
                                   (% (1+ write-pos) size)))))
                      (ring-contents
                       (lambda (ring)
                         (let ((buf (car ring))
                               (result nil))
                           (dotimes (i (length buf))
                             (let ((v (aref buf i)))
                               (when v
                                 (setq result (cons v result)))))
                           (nreverse result)))))
                  (let ((r (funcall make-ring 3)))
                    (funcall ring-push r 'a)
                    (funcall ring-push r 'b)
                    (funcall ring-push r 'c)
                    (let ((after-3 (funcall ring-contents r)))
                      ;; Overflow: d overwrites a
                      (funcall ring-push r 'd)
                      (let ((after-4 (funcall ring-contents r)))
                        (list after-3 after-4)))))";
    let expect = expect_test::expect![[r#""OK ((a b c) (d b c))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Priority queue (min-heap via sorted list)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ds_priority_queue() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simple priority queue using sorted insertion
    let form = "(let ((pq-insert
                       (lambda (pq priority value)
                         (let ((entry (cons priority value))
                               (prev nil)
                               (curr pq))
                           (while (and curr
                                       (<= (caar curr) priority))
                             (setq prev curr curr (cdr curr)))
                           (if prev
                               (progn
                                 (setcdr prev (cons entry curr))
                                 pq)
                             (cons entry pq)))))
                      (pq-pop
                       (lambda (pq)
                         (if (null pq)
                             (list nil nil)
                           (list (cdar pq) (cdr pq))))))
                  (let ((q nil))
                    (setq q (funcall pq-insert q 3 'medium))
                    (setq q (funcall pq-insert q 1 'urgent))
                    (setq q (funcall pq-insert q 5 'low))
                    (setq q (funcall pq-insert q 2 'high))
                    (setq q (funcall pq-insert q 1 'critical))
                    ;; Pop items in priority order
                    (let ((results nil))
                      (dotimes (_ 5)
                        (let ((r (funcall pq-pop q)))
                          (setq results (cons (car r) results)
                                q (cadr r))))
                      (nreverse results))))";
    let expect = expect_test::expect![[r#""OK (urgent critical high medium low)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Graph: adjacency list + BFS
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ds_graph_bfs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // BFS on an adjacency list graph
    let form = "(let ((graph (make-hash-table)))
                  ;; Build graph
                  (puthash 'a '(b c) graph)
                  (puthash 'b '(a d) graph)
                  (puthash 'c '(a d e) graph)
                  (puthash 'd '(b c) graph)
                  (puthash 'e '(c) graph)
                  ;; BFS from 'a
                  (let ((visited (make-hash-table))
                        (queue (list 'a))
                        (order nil))
                    (puthash 'a t visited)
                    (while queue
                      (let ((node (car queue)))
                        (setq queue (cdr queue))
                        (setq order (cons node order))
                        (dolist (neighbor (gethash node graph))
                          (unless (gethash neighbor visited)
                            (puthash neighbor t visited)
                            (setq queue
                                  (append queue
                                          (list neighbor)))))))
                    (nreverse order)))";
    let expect = expect_test::expect![[r#""OK (a b c d e)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Stack-based expression evaluator
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ds_stack_calculator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // RPN (Reverse Polish Notation) calculator
    let form = "(let ((rpn-eval
                       (lambda (tokens)
                         (let ((stack nil))
                           (dolist (tok tokens)
                             (cond
                               ((numberp tok)
                                (setq stack (cons tok stack)))
                               ((eq tok '+)
                                (let ((b (car stack))
                                      (a (cadr stack)))
                                  (setq stack
                                        (cons (+ a b) (cddr stack)))))
                               ((eq tok '-)
                                (let ((b (car stack))
                                      (a (cadr stack)))
                                  (setq stack
                                        (cons (- a b) (cddr stack)))))
                               ((eq tok '*)
                                (let ((b (car stack))
                                      (a (cadr stack)))
                                  (setq stack
                                        (cons (* a b) (cddr stack)))))
                               ((eq tok '/)
                                (let ((b (car stack))
                                      (a (cadr stack)))
                                  (setq stack
                                        (cons (/ a b)
                                              (cddr stack)))))))
                           (car stack)))))
                  (list
                    ;; 3 + 4
                    (funcall rpn-eval '(3 4 +))
                    ;; (3 + 4) * 2
                    (funcall rpn-eval '(3 4 + 2 *))
                    ;; 5 + ((1 + 2) * 4) - 3
                    (funcall rpn-eval '(5 1 2 + 4 * + 3 -))))";
    let expect = expect_test::expect![[r#""OK (7 14 14)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(7 14 14)", &o, &n);
}

// ---------------------------------------------------------------------------
// Trie (prefix tree)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ds_trie() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a trie and search for prefixes
    let form = r#"(let ((make-trie
                     (lambda () (make-hash-table)))
                    (trie-insert
                     (lambda (trie word)
                       (let ((node trie))
                         (dotimes (i (length word))
                           (let ((ch (aref word i)))
                             (unless (gethash ch node)
                               (puthash ch (make-hash-table) node))
                             (setq node (gethash ch node))))
                         (puthash 'end t node))))
                    (trie-search
                     (lambda (trie word)
                       (let ((node trie)
                             (found t))
                         (dotimes (i (length word))
                           (let ((ch (aref word i)))
                             (if (gethash ch node)
                                 (setq node (gethash ch node))
                               (setq found nil))))
                         (and found (gethash 'end node nil)))))
                    (trie-prefix-p
                     (lambda (trie prefix)
                       (let ((node trie)
                             (found t))
                         (dotimes (i (length prefix))
                           (let ((ch (aref prefix i)))
                             (if (gethash ch node)
                                 (setq node (gethash ch node))
                               (setq found nil))))
                         found))))
                  (let ((t1 (funcall make-trie)))
                    (funcall trie-insert t1 "hello")
                    (funcall trie-insert t1 "help")
                    (funcall trie-insert t1 "world")
                    (list
                      (funcall trie-search t1 "hello")
                      (funcall trie-search t1 "help")
                      (funcall trie-search t1 "hel")
                      (funcall trie-search t1 "world")
                      (funcall trie-prefix-p t1 "hel")
                      (funcall trie-prefix-p t1 "wor")
                      (funcall trie-prefix-p t1 "xyz"))))"#;
    let expect = expect_test::expect![[r#""OK (t t nil t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// LRU Cache
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ds_lru_cache() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // LRU cache using alist + hash table
    let form = "(let ((max-size 3)
                      (cache-order nil)
                      (cache-table (make-hash-table :test 'equal)))
                  (let ((cache-get
                         (lambda (key)
                           (let ((val (gethash key cache-table 'miss)))
                             (when (not (eq val 'miss))
                               ;; Move to front
                               (setq cache-order
                                     (cons key
                                           (delete key cache-order))))
                             (if (eq val 'miss) nil val))))
                        (cache-put
                         (lambda (key val)
                           ;; Remove if existing
                           (setq cache-order
                                 (delete key cache-order))
                           ;; Add to front
                           (setq cache-order
                                 (cons key cache-order))
                           (puthash key val cache-table)
                           ;; Evict if over size
                           (when (> (length cache-order) max-size)
                             (let ((evicted (car (last cache-order))))
                               (remhash evicted cache-table)
                               (setq cache-order
                                     (butlast cache-order)))))))
                    ;; Operations
                    (funcall cache-put 'a 1)
                    (funcall cache-put 'b 2)
                    (funcall cache-put 'c 3)
                    (let ((after-3 (copy-sequence cache-order)))
                      ;; Access 'a' moves it to front
                      (funcall cache-get 'a)
                      (let ((after-access (copy-sequence cache-order)))
                        ;; Adding 'd' should evict 'b' (least recently used)
                        (funcall cache-put 'd 4)
                        (list after-3
                              after-access
                              cache-order
                              (gethash 'b cache-table)
                              (gethash 'a cache-table))))))";
    let expect = expect_test::expect![[r#""OK ((c b a) (a c b) (d a c) nil 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
