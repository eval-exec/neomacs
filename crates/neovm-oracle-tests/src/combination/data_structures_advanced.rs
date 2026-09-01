//! Advanced oracle parity tests for complex data structure implementations
//! in pure Elisp.
//!
//! Covers doubly-linked list, AVL-like balanced BST, skip list-inspired
//! structure, LRU cache, trie with prefix search, and priority queue
//! (min-heap with sift operations).

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Doubly-linked list using vectors of (prev, value, next)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ds_adv_doubly_linked_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Each node is a 3-element vector: [prev-idx, value, next-idx].
    // Sentinel head/tail nodes at indices 0 and 1.
    // Free list managed via a counter.
    let form = "(let ((nodes (make-vector 20 nil))
                      (next-free 2)
                      (head-idx 0)
                      (tail-idx 1))
                  ;; Initialize head and tail sentinels
                  (aset nodes head-idx (vector -1 'HEAD tail-idx))
                  (aset nodes tail-idx (vector head-idx 'TAIL -1))
                  (let ((alloc-node
                         (lambda (val)
                           (let ((idx next-free))
                             (aset nodes idx (vector -1 val -1))
                             (setq next-free (1+ next-free))
                             idx)))
                        (node-prev  (lambda (idx) (aref (aref nodes idx) 0)))
                        (node-val   (lambda (idx) (aref (aref nodes idx) 1)))
                        (node-next  (lambda (idx) (aref (aref nodes idx) 2)))
                        (set-prev   (lambda (idx v) (aset (aref nodes idx) 0 v)))
                        (set-next   (lambda (idx v) (aset (aref nodes idx) 2 v))))
                    ;; Insert before tail
                    (let ((insert-back
                           (lambda (val)
                             (let* ((new-idx (funcall alloc-node val))
                                    (prev-of-tail (aref (aref nodes tail-idx) 0)))
                               ;; Link new node
                               (funcall set-prev new-idx prev-of-tail)
                               (funcall set-next new-idx tail-idx)
                               ;; Update old prev's next
                               (funcall set-next prev-of-tail new-idx)
                               ;; Update tail's prev
                               (funcall set-prev tail-idx new-idx)
                               new-idx)))
                          ;; Insert after head
                          (insert-front
                           (lambda (val)
                             (let* ((new-idx (funcall alloc-node val))
                                    (next-of-head (aref (aref nodes head-idx) 2)))
                               (funcall set-prev new-idx head-idx)
                               (funcall set-next new-idx next-of-head)
                               (funcall set-next head-idx new-idx)
                               (funcall set-prev next-of-head new-idx)
                               new-idx)))
                          ;; Remove a node by index
                          (remove-node
                           (lambda (idx)
                             (let ((p (funcall node-prev idx))
                                   (n (funcall node-next idx)))
                               (funcall set-next p n)
                               (funcall set-prev n p)
                               (funcall node-val idx))))
                          ;; Traverse forward: head -> tail
                          (to-list-fwd
                           (lambda ()
                             (let ((result nil)
                                   (cur (aref (aref nodes head-idx) 2)))
                               (while (/= cur tail-idx)
                                 (setq result (cons (funcall node-val cur) result))
                                 (setq cur (funcall node-next cur)))
                               (nreverse result))))
                          ;; Traverse backward: tail -> head
                          (to-list-bwd
                           (lambda ()
                             (let ((result nil)
                                   (cur (aref (aref nodes tail-idx) 0)))
                               (while (/= cur head-idx)
                                 (setq result (cons (funcall node-val cur) result))
                                 (setq cur (funcall node-prev cur)))
                               result))))
                      ;; Build list: front(10) back(20) back(30) front(5)
                      (funcall insert-front 10)
                      (let ((n20 (funcall insert-back 20)))
                        (funcall insert-back 30)
                        (funcall insert-front 5)
                        (let ((fwd1 (funcall to-list-fwd))
                              (bwd1 (funcall to-list-bwd)))
                          ;; Remove node 20 (middle)
                          (funcall remove-node n20)
                          (let ((fwd2 (funcall to-list-fwd))
                                (bwd2 (funcall to-list-bwd)))
                            (list fwd1 bwd1 fwd2 bwd2)))))))";
    let expect = expect_test::expect![[r#""OK ((5 10 20 30) (5 10 20 30) (5 10 30) (5 10 30))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// AVL-like balanced BST (insert with rotation)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ds_adv_avl_bst() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // AVL tree stored in vectors: nodes[i] = [key, left, right, height].
    // -1 means null. Supports insert with rebalancing, in-order traversal.
    let form = "(let ((nodes (make-vector 30 nil))
                      (next-id 0)
                      (root -1))
                  (let ((new-node
                         (lambda (key)
                           (let ((id next-id))
                             (aset nodes id (vector key -1 -1 1))
                             (setq next-id (1+ next-id))
                             id)))
                        (height
                         (lambda (id)
                           (if (= id -1) 0 (aref (aref nodes id) 3))))
                        (update-height
                         (lambda (id)
                           (let ((lh (if (= (aref (aref nodes id) 1) -1) 0
                                       (aref (aref nodes (aref (aref nodes id) 1)) 3)))
                                 (rh (if (= (aref (aref nodes id) 2) -1) 0
                                       (aref (aref nodes (aref (aref nodes id) 2)) 3))))
                             (aset (aref nodes id) 3 (1+ (max lh rh))))))
                        (balance-factor
                         (lambda (id)
                           (let ((lh (if (= (aref (aref nodes id) 1) -1) 0
                                       (aref (aref nodes (aref (aref nodes id) 1)) 3)))
                                 (rh (if (= (aref (aref nodes id) 2) -1) 0
                                       (aref (aref nodes (aref (aref nodes id) 2)) 3))))
                             (- lh rh))))
                        ;; Right rotate around y: y.left=x, x.right becomes y.left
                        (rotate-right
                         (lambda (y)
                           (let ((x (aref (aref nodes y) 1)))
                             (let ((t2 (aref (aref nodes x) 2)))
                               (aset (aref nodes x) 2 y)
                               (aset (aref nodes y) 1 t2)
                               (funcall update-height y)
                               (funcall update-height x)
                               x))))
                        ;; Left rotate around x: x.right=y, y.left becomes x.right
                        (rotate-left
                         (lambda (x)
                           (let ((y (aref (aref nodes x) 2)))
                             (let ((t2 (aref (aref nodes y) 1)))
                               (aset (aref nodes y) 1 x)
                               (aset (aref nodes x) 2 t2)
                               (funcall update-height x)
                               (funcall update-height y)
                               y)))))
                    (fset 'neovm--avl-insert
                      (lambda (node key)
                        (if (= node -1)
                            (funcall new-node key)
                          (let ((nk (aref (aref nodes node) 0)))
                            (cond
                              ((< key nk)
                               (aset (aref nodes node) 1
                                     (funcall 'neovm--avl-insert
                                              (aref (aref nodes node) 1) key)))
                              ((> key nk)
                               (aset (aref nodes node) 2
                                     (funcall 'neovm--avl-insert
                                              (aref (aref nodes node) 2) key)))
                              (t node)))  ;; duplicate, ignore
                          (funcall update-height node)
                          (let ((bf (funcall balance-factor node)))
                            (cond
                              ;; Left-heavy
                              ((> bf 1)
                               (if (< key (aref (aref nodes (aref (aref nodes node) 1)) 0))
                                   (funcall rotate-right node)
                                 (aset (aref nodes node) 1
                                       (funcall rotate-left (aref (aref nodes node) 1)))
                                 (funcall rotate-right node)))
                              ;; Right-heavy
                              ((< bf -1)
                               (if (> key (aref (aref nodes (aref (aref nodes node) 2)) 0))
                                   (funcall rotate-left node)
                                 (aset (aref nodes node) 2
                                       (funcall rotate-right (aref (aref nodes node) 2)))
                                 (funcall rotate-left node)))
                              (t node))))))
                    (fset 'neovm--avl-inorder
                      (lambda (node)
                        (if (= node -1)
                            nil
                          (append (funcall 'neovm--avl-inorder (aref (aref nodes node) 1))
                                  (list (aref (aref nodes node) 0))
                                  (funcall 'neovm--avl-inorder (aref (aref nodes node) 2))))))
                    (unwind-protect
                        (progn
                          ;; Insert values that would cause imbalance in naive BST
                          (dolist (key '(10 20 30 15 25 5 3 7 12 17))
                            (setq root (funcall 'neovm--avl-insert root key)))
                          (let ((sorted (funcall 'neovm--avl-inorder root))
                                (root-height (aref (aref nodes root) 3)))
                            (list
                              ;; In-order traversal is sorted
                              sorted
                              ;; Height is bounded (AVL: <= 1.44 * log2(n))
                              ;; For 10 elements, max height = 4
                              (<= root-height 5)
                              ;; Root key (should not be 10 or 30 due to rotations)
                              (aref (aref nodes root) 0))))
                      (fmakunbound 'neovm--avl-insert)
                      (fmakunbound 'neovm--avl-inorder))))";
    let expect = expect_test::expect![[r#""ERR (void-variable update-height)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Skip list-inspired probabilistic structure
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ds_adv_skip_list_like() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Deterministic "skip list": multiple sorted layers.
    // Layer 0 has all elements, layer 1 has every 2nd, layer 2 every 4th.
    // Search starts at highest layer, drops down.
    let form = r#"(let ((max-layers 3))
                    (let ((layers (make-vector max-layers nil)))
                      ;; Build layers from sorted input
                      (let ((build
                             (lambda (sorted-list)
                               ;; Layer 0: all elements
                               (aset layers 0 (copy-sequence sorted-list))
                               ;; Layer 1: every 2nd element
                               (let ((result nil) (i 0))
                                 (dolist (x sorted-list)
                                   (when (= 0 (% i 2))
                                     (setq result (cons x result)))
                                   (setq i (1+ i)))
                                 (aset layers 1 (nreverse result)))
                               ;; Layer 2: every 4th element
                               (let ((result nil) (i 0))
                                 (dolist (x sorted-list)
                                   (when (= 0 (% i 4))
                                     (setq result (cons x result)))
                                   (setq i (1+ i)))
                                 (aset layers 2 (nreverse result)))))
                            ;; Search: start from top layer, find position, drill down
                            (search
                             (lambda (target)
                               (let ((steps 0)
                                     (found nil)
                                     (layer (1- max-layers)))
                                 (while (and (>= layer 0) (not found))
                                   (let ((lst (aref layers layer)))
                                     (while (and lst (<= (car lst) target))
                                       (setq steps (1+ steps))
                                       (when (= (car lst) target)
                                         (setq found t))
                                       (setq lst (cdr lst))))
                                   (setq layer (1- layer)))
                                 (cons found steps)))))
                        ;; Build from sorted data
                        (funcall build '(2 5 8 11 14 17 20 23 26 29 32 35 38 41 44 47))
                        (list
                          ;; Layer contents
                          (aref layers 0)
                          (aref layers 1)
                          (aref layers 2)
                          ;; Search for existing elements
                          (funcall search 2)
                          (funcall search 20)
                          (funcall search 47)
                          ;; Search for non-existing
                          (funcall search 10)
                          (funcall search 50)
                          ;; Verify layer sizes
                          (length (aref layers 0))
                          (length (aref layers 1))
                          (length (aref layers 2))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((2 5 8 11 14 17 20 23 26 29 32 35 38 41 44 47) (2 8 14 20 26 32 38 44) (2 14 26 38) (t . 1) (t . 6) (t . 28) (nil . 6) (nil . 28) 16 8 4)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// LRU cache with hash table + doubly-linked list
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ds_adv_lru_cache_full() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Full LRU cache: O(1) get/put using a hash table for lookup
    // and a doubly-linked list (vector-based) for ordering.
    // Nodes: [prev, next, key, value]. Sentinel head=0, tail=1.
    let form = "(let ((capacity 3)
                      (table (make-hash-table :test 'equal))
                      (nodes (make-vector 20 nil))
                      (next-slot 2)
                      (head 0)
                      (tail 1))
                  ;; Sentinel initialization
                  (aset nodes head (vector -1 tail 'HEAD nil))
                  (aset nodes tail (vector head -1 'TAIL nil))
                  (let ((detach
                         (lambda (idx)
                           (let ((p (aref (aref nodes idx) 0))
                                 (n (aref (aref nodes idx) 1)))
                             (aset (aref nodes p) 1 n)
                             (aset (aref nodes n) 0 p))))
                        (attach-after-head
                         (lambda (idx)
                           (let ((first (aref (aref nodes head) 1)))
                             (aset (aref nodes idx) 0 head)
                             (aset (aref nodes idx) 1 first)
                             (aset (aref nodes head) 1 idx)
                             (aset (aref nodes first) 0 idx))))
                        (last-before-tail
                         (lambda ()
                           (aref (aref nodes tail) 0))))
                    (let ((lru-get
                           (lambda (key)
                             (let ((idx (gethash key table)))
                               (if idx
                                   (progn
                                     ;; Move to front
                                     (funcall detach idx)
                                     (funcall attach-after-head idx)
                                     (aref (aref nodes idx) 3))
                                 nil))))
                          (lru-put
                           (lambda (key val)
                             (let ((existing (gethash key table)))
                               (if existing
                                   ;; Update and move to front
                                   (progn
                                     (aset (aref nodes existing) 3 val)
                                     (funcall detach existing)
                                     (funcall attach-after-head existing))
                                 ;; New entry
                                 (let ((idx next-slot))
                                   (aset nodes idx (vector -1 -1 key val))
                                   (setq next-slot (1+ next-slot))
                                   (puthash key idx table)
                                   (funcall attach-after-head idx)
                                   ;; Evict if over capacity
                                   (when (> (hash-table-count table) capacity)
                                     (let ((victim (funcall last-before-tail)))
                                       (funcall detach victim)
                                       (remhash (aref (aref nodes victim) 2) table))))))))
                          (lru-keys
                           (lambda ()
                             (let ((result nil)
                                   (cur (aref (aref nodes head) 1)))
                               (while (/= cur tail)
                                 (setq result (cons (aref (aref nodes cur) 2) result))
                                 (setq cur (aref (aref nodes cur) 1)))
                               (nreverse result)))))
                      ;; Operations
                      (funcall lru-put 'a 1)
                      (funcall lru-put 'b 2)
                      (funcall lru-put 'c 3)
                      (let ((keys1 (funcall lru-keys)))
                        ;; Access 'a' -> moves to front
                        (let ((val-a (funcall lru-get 'a)))
                          (let ((keys2 (funcall lru-keys)))
                            ;; Add 'd' -> evicts 'b' (LRU)
                            (funcall lru-put 'd 4)
                            (let ((keys3 (funcall lru-keys)))
                              ;; 'b' should be gone
                              (let ((val-b (funcall lru-get 'b)))
                                ;; Update 'c' value
                                (funcall lru-put 'c 30)
                                (let ((keys4 (funcall lru-keys))
                                      (val-c (funcall lru-get 'c)))
                                  (list keys1 val-a keys2 keys3 val-b
                                        keys4 val-c
                                        (hash-table-count table)))))))))))))";
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 83 75)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Trie with prefix search and auto-complete
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ds_adv_trie_autocomplete() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Full trie: insert words, exact search, prefix check,
    // and auto-complete (collect all words with given prefix).
    let form = r#"(let ((make-trie-node
                         (lambda () (list (make-hash-table) nil))))
                    (let ((trie (funcall make-trie-node)))
                      (let ((trie-insert
                             (lambda (word)
                               (let ((node trie))
                                 (dotimes (i (length word))
                                   (let ((ch (aref word i))
                                         (children (car node)))
                                     (unless (gethash ch children)
                                       (puthash ch (funcall make-trie-node) children))
                                     (setq node (gethash ch children))))
                                 ;; Mark end of word
                                 (setcar (cdr node) t))))
                            (trie-search
                             (lambda (word)
                               (let ((node trie)
                                     (ok t))
                                 (dotimes (i (length word))
                                   (let ((ch (aref word i))
                                         (children (car node)))
                                     (if (gethash ch children)
                                         (setq node (gethash ch children))
                                       (setq ok nil))))
                                 (and ok (cadr node)
                                      t))))
                            (trie-prefix-node
                             (lambda (prefix)
                               (let ((node trie)
                                     (ok t))
                                 (dotimes (i (length prefix))
                                   (let ((ch (aref prefix i))
                                         (children (car node)))
                                     (if (gethash ch children)
                                         (setq node (gethash ch children))
                                       (setq ok nil))))
                                 (if ok node nil)))))
                        ;; Collect all words from a node with given prefix
                        (fset 'neovm--trie-collect
                          (lambda (node prefix)
                            (let ((result nil))
                              (when (cadr node)
                                (setq result (list prefix)))
                              (maphash (lambda (ch child)
                                         (let ((sub (funcall 'neovm--trie-collect
                                                             child
                                                             (concat prefix (char-to-string ch)))))
                                           (setq result (append result sub))))
                                       (car node))
                              result)))
                        (let ((autocomplete
                               (lambda (prefix)
                                 (let ((node (funcall trie-prefix-node prefix)))
                                   (if node
                                       (sort (funcall 'neovm--trie-collect node prefix)
                                             #'string<)
                                     nil)))))
                          (unwind-protect
                              (progn
                                ;; Insert words
                                (dolist (w '("apple" "app" "application" "apply"
                                             "banana" "band" "ban"
                                             "cat" "car" "card" "care"))
                                  (funcall trie-insert w))
                                (list
                                  ;; Exact search
                                  (funcall trie-search "apple")
                                  (funcall trie-search "app")
                                  (funcall trie-search "ap")
                                  (funcall trie-search "banana")
                                  (funcall trie-search "xyz")
                                  ;; Autocomplete
                                  (funcall autocomplete "app")
                                  (funcall autocomplete "ban")
                                  (funcall autocomplete "car")
                                  (funcall autocomplete "z")
                                  ;; Count total unique completions from ""
                                  (length (funcall autocomplete ""))))
                            (fmakunbound 'neovm--trie-collect))))))"#;
    let expect = expect_test::expect![[
        r#""OK (t t nil t nil (\"app\" \"apple\" \"application\" \"apply\") (\"ban\" \"banana\" \"band\") (\"car\" \"card\" \"care\") nil 11)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Priority queue (min-heap) with sift operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ds_adv_priority_queue_heap() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Min-heap with (priority . value) pairs. Supports push, pop,
    // peek, decrease-key (by index).
    let form = "(let ((heap (make-vector 20 nil))
                      (size 0))
                  (let ((heap-swap
                         (lambda (i j)
                           (let ((tmp (aref heap i)))
                             (aset heap i (aref heap j))
                             (aset heap j tmp))))
                        (heap-push
                         (lambda (priority value)
                           (aset heap size (cons priority value))
                           (setq size (1+ size))
                           ;; Sift up
                           (let ((i (1- size)))
                             (while (> i 0)
                               (let ((parent (/ (1- i) 2)))
                                 (if (< (car (aref heap i))
                                        (car (aref heap parent)))
                                     (progn
                                       (let ((tmp (aref heap i)))
                                         (aset heap i (aref heap parent))
                                         (aset heap parent tmp))
                                       (setq i parent))
                                   (setq i 0)))))))
                        (heap-pop
                         (lambda ()
                           (if (= size 0) nil
                             (let ((min-entry (aref heap 0)))
                               (setq size (1- size))
                               (aset heap 0 (aref heap size))
                               ;; Sift down
                               (let ((i 0) (done nil))
                                 (while (not done)
                                   (let ((smallest i)
                                         (left (1+ (* 2 i)))
                                         (right (+ 2 (* 2 i))))
                                     (when (and (< left size)
                                                (< (car (aref heap left))
                                                   (car (aref heap smallest))))
                                       (setq smallest left))
                                     (when (and (< right size)
                                                (< (car (aref heap right))
                                                   (car (aref heap smallest))))
                                       (setq smallest right))
                                     (if (= smallest i)
                                         (setq done t)
                                       (let ((tmp (aref heap i)))
                                         (aset heap i (aref heap smallest))
                                         (aset heap smallest tmp))
                                       (setq i smallest)))))
                               min-entry))))
                        (heap-peek
                         (lambda ()
                           (if (= size 0) nil (aref heap 0)))))
                    ;; Simulate a task scheduler
                    (funcall heap-push 5 'email)
                    (funcall heap-push 1 'critical-bug)
                    (funcall heap-push 3 'code-review)
                    (funcall heap-push 2 'deploy)
                    (funcall heap-push 4 'meeting)
                    (funcall heap-push 1 'security-patch)
                    (funcall heap-push 3 'testing)
                    ;; Peek at highest priority
                    (let ((top (funcall heap-peek)))
                      ;; Pop all in priority order
                      (let ((order nil))
                        (dotimes (_ 7)
                          (let ((entry (funcall heap-pop)))
                            (setq order (cons (list (car entry) (cdr entry)) order))))
                        (list
                          ;; Top was priority 1
                          top
                          ;; Full extraction order
                          (nreverse order)
                          ;; Heap is now empty
                          size
                          (funcall heap-peek))))))";
    let expect = expect_test::expect![[
        r#""OK ((1 . critical-bug) ((1 critical-bug) (1 security-patch) (2 deploy) (3 code-review) (3 testing) (4 meeting) (5 email)) 0 nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
