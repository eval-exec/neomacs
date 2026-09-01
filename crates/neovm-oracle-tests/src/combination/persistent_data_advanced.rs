//! Advanced oracle parity tests for persistent (immutable) data structures:
//! persistent stack (cons-based), persistent map (functional balanced tree),
//! structural sharing verification, persistent vector (trie-based),
//! version history with branching, transaction-like operations.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Persistent stack with full API and structural sharing proofs
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_persistent_data_advanced_stack() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Persistent stack: push, pop, peek, size, to-list, reverse, concat
  (fset 'neovm--pda-stk-empty (lambda () nil))
  (fset 'neovm--pda-stk-push (lambda (stk val) (cons val stk)))
  (fset 'neovm--pda-stk-pop (lambda (stk) (cdr stk)))
  (fset 'neovm--pda-stk-peek (lambda (stk) (car stk)))
  (fset 'neovm--pda-stk-empty-p (lambda (stk) (null stk)))
  (fset 'neovm--pda-stk-size (lambda (stk) (length stk)))
  (fset 'neovm--pda-stk-to-list (lambda (stk) (copy-sequence stk)))
  (fset 'neovm--pda-stk-reverse
    (lambda (stk)
      (let ((result nil))
        (dolist (x stk) (setq result (cons x result)))
        result)))
  (fset 'neovm--pda-stk-concat
    (lambda (stk1 stk2)
      "Concatenate stk1 on top of stk2 (stk1 top is new top)."
      (let ((result stk2))
        (dolist (x (funcall 'neovm--pda-stk-reverse stk1))
          (setq result (cons x result)))
        result)))
  ;; Map over stack producing new stack
  (fset 'neovm--pda-stk-map
    (lambda (fn stk)
      (let ((result nil))
        (dolist (x (funcall 'neovm--pda-stk-reverse stk))
          (setq result (cons (funcall fn x) result)))
        result)))
  ;; Filter: keep only elements matching predicate
  (fset 'neovm--pda-stk-filter
    (lambda (pred stk)
      (let ((result nil))
        (dolist (x stk)
          (when (funcall pred x)
            (setq result (cons x result))))
        (nreverse result))))

  (unwind-protect
      (let* ((s0 (funcall 'neovm--pda-stk-empty))
             (s1 (funcall 'neovm--pda-stk-push s0 10))
             (s2 (funcall 'neovm--pda-stk-push s1 20))
             (s3 (funcall 'neovm--pda-stk-push s2 30))
             ;; Branch from s2: push different value
             (s2b (funcall 'neovm--pda-stk-push s2 99))
             ;; Pop from s3
             (s4 (funcall 'neovm--pda-stk-pop s3))
             ;; Concat two stacks
             (t1 (funcall 'neovm--pda-stk-push
                          (funcall 'neovm--pda-stk-push s0 'a) 'b))
             (t2 (funcall 'neovm--pda-stk-push
                          (funcall 'neovm--pda-stk-push s0 'c) 'd))
             (t3 (funcall 'neovm--pda-stk-concat t1 t2))
             ;; Map: double all values in s3
             (s5 (funcall 'neovm--pda-stk-map (lambda (x) (* x 2)) s3))
             ;; Filter: keep only values > 15
             (s6 (funcall 'neovm--pda-stk-filter
                          (lambda (x) (> x 15)) s3)))
        (list
         ;; Basic operations
         (funcall 'neovm--pda-stk-empty-p s0)
         (funcall 'neovm--pda-stk-empty-p s1)
         (funcall 'neovm--pda-stk-peek s1)
         (funcall 'neovm--pda-stk-peek s2)
         (funcall 'neovm--pda-stk-peek s3)
         (funcall 'neovm--pda-stk-size s3)
         ;; Pop returns previous version
         (equal s4 s2)
         (eq s4 s2)  ;; structural sharing: pop returns exact same tail
         ;; Branch: s3 and s2b diverge but share s2's tail
         (funcall 'neovm--pda-stk-to-list s3)
         (funcall 'neovm--pda-stk-to-list s2b)
         (eq (cdr s3) s2)
         (eq (cdr s2b) s2)
         ;; Concat
         (funcall 'neovm--pda-stk-to-list t3)
         ;; Map
         (funcall 'neovm--pda-stk-to-list s5)
         ;; Filter
         (funcall 'neovm--pda-stk-to-list s6)
         ;; Reverse
         (funcall 'neovm--pda-stk-reverse s3)
         ;; Original stacks unchanged
         (funcall 'neovm--pda-stk-to-list s3)))
    (fmakunbound 'neovm--pda-stk-empty)
    (fmakunbound 'neovm--pda-stk-push)
    (fmakunbound 'neovm--pda-stk-pop)
    (fmakunbound 'neovm--pda-stk-peek)
    (fmakunbound 'neovm--pda-stk-empty-p)
    (fmakunbound 'neovm--pda-stk-size)
    (fmakunbound 'neovm--pda-stk-to-list)
    (fmakunbound 'neovm--pda-stk-reverse)
    (fmakunbound 'neovm--pda-stk-concat)
    (fmakunbound 'neovm--pda-stk-map)
    (fmakunbound 'neovm--pda-stk-filter)))"#;
    let expect = expect_test::expect![[
        r#""OK (t nil 10 20 30 3 t t (30 20 10) (99 20 10) t t (b a d c) (60 40 20) (30 20) (10 20 30) (30 20 10))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Persistent map: functional balanced BST (AVL-like with persistent updates)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_persistent_data_advanced_functional_map() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Persistent sorted map via BST: (key value left right height)
  (fset 'neovm--pda-bst-height
    (lambda (node)
      (if (null node) 0 (nth 4 node))))

  (fset 'neovm--pda-bst-make
    (lambda (k v l r)
      (list k v l r (1+ (max (funcall 'neovm--pda-bst-height l)
                              (funcall 'neovm--pda-bst-height r))))))

  (fset 'neovm--pda-bst-balance
    (lambda (node)
      (- (funcall 'neovm--pda-bst-height (nth 2 node))
         (funcall 'neovm--pda-bst-height (nth 3 node)))))

  ;; Right rotation
  (fset 'neovm--pda-bst-rot-right
    (lambda (node)
      (let ((l (nth 2 node)))
        (funcall 'neovm--pda-bst-make
                 (car l) (cadr l) (nth 2 l)
                 (funcall 'neovm--pda-bst-make
                          (car node) (cadr node) (nth 3 l) (nth 3 node))))))

  ;; Left rotation
  (fset 'neovm--pda-bst-rot-left
    (lambda (node)
      (let ((r (nth 3 node)))
        (funcall 'neovm--pda-bst-make
                 (car r) (cadr r)
                 (funcall 'neovm--pda-bst-make
                          (car node) (cadr node) (nth 2 node) (nth 2 r))
                 (nth 3 r)))))

  ;; Rebalance after insert
  (fset 'neovm--pda-bst-rebalance
    (lambda (node)
      (let ((bal (funcall 'neovm--pda-bst-balance node)))
        (cond
         ((> bal 1)
          (if (< (funcall 'neovm--pda-bst-balance (nth 2 node)) 0)
              (funcall 'neovm--pda-bst-rot-right
                       (funcall 'neovm--pda-bst-make
                                (car node) (cadr node)
                                (funcall 'neovm--pda-bst-rot-left (nth 2 node))
                                (nth 3 node)))
            (funcall 'neovm--pda-bst-rot-right node)))
         ((< bal -1)
          (if (> (funcall 'neovm--pda-bst-balance (nth 3 node)) 0)
              (funcall 'neovm--pda-bst-rot-left
                       (funcall 'neovm--pda-bst-make
                                (car node) (cadr node)
                                (nth 2 node)
                                (funcall 'neovm--pda-bst-rot-right (nth 3 node))))
            (funcall 'neovm--pda-bst-rot-left node)))
         (t node)))))

  ;; Insert (returns new tree)
  (fset 'neovm--pda-bst-insert
    (lambda (tree k v)
      (if (null tree)
          (funcall 'neovm--pda-bst-make k v nil nil)
        (cond
         ((< k (car tree))
          (funcall 'neovm--pda-bst-rebalance
                   (funcall 'neovm--pda-bst-make
                            (car tree) (cadr tree)
                            (funcall 'neovm--pda-bst-insert (nth 2 tree) k v)
                            (nth 3 tree))))
         ((> k (car tree))
          (funcall 'neovm--pda-bst-rebalance
                   (funcall 'neovm--pda-bst-make
                            (car tree) (cadr tree)
                            (nth 2 tree)
                            (funcall 'neovm--pda-bst-insert (nth 3 tree) k v))))
         (t ;; update value
          (funcall 'neovm--pda-bst-make k v (nth 2 tree) (nth 3 tree)))))))

  ;; Lookup
  (fset 'neovm--pda-bst-lookup
    (lambda (tree k)
      (cond
       ((null tree) nil)
       ((= k (car tree)) (cadr tree))
       ((< k (car tree)) (funcall 'neovm--pda-bst-lookup (nth 2 tree) k))
       (t (funcall 'neovm--pda-bst-lookup (nth 3 tree) k)))))

  ;; In-order keys
  (fset 'neovm--pda-bst-keys
    (lambda (tree)
      (if (null tree) nil
        (append (funcall 'neovm--pda-bst-keys (nth 2 tree))
                (list (car tree))
                (funcall 'neovm--pda-bst-keys (nth 3 tree))))))

  (unwind-protect
      (let* ((t0 nil)
             (t1 (funcall 'neovm--pda-bst-insert t0 5 "five"))
             (t2 (funcall 'neovm--pda-bst-insert t1 3 "three"))
             (t3 (funcall 'neovm--pda-bst-insert t2 7 "seven"))
             (t4 (funcall 'neovm--pda-bst-insert t3 1 "one"))
             (t5 (funcall 'neovm--pda-bst-insert t4 9 "nine"))
             (t6 (funcall 'neovm--pda-bst-insert t5 4 "four"))
             (t7 (funcall 'neovm--pda-bst-insert t6 6 "six"))
             ;; Update existing key
             (t8 (funcall 'neovm--pda-bst-insert t7 5 "FIVE"))
             ;; Branch from t5
             (t5b (funcall 'neovm--pda-bst-insert t5 2 "two")))
        (list
         ;; Keys are in sorted order
         (funcall 'neovm--pda-bst-keys t7)
         ;; Lookups
         (funcall 'neovm--pda-bst-lookup t7 5)
         (funcall 'neovm--pda-bst-lookup t7 1)
         (funcall 'neovm--pda-bst-lookup t7 9)
         (funcall 'neovm--pda-bst-lookup t7 99)
         ;; After update
         (funcall 'neovm--pda-bst-lookup t8 5)
         ;; Old version unchanged
         (funcall 'neovm--pda-bst-lookup t7 5)
         ;; Branch has different keys
         (funcall 'neovm--pda-bst-keys t5b)
         (funcall 'neovm--pda-bst-lookup t5b 2)
         (funcall 'neovm--pda-bst-lookup t5 2) ;; nil in t5
         ;; Heights are balanced (should be ~3 for 7 nodes)
         (funcall 'neovm--pda-bst-height t7)
         ;; Height after sequential inserts (should stay balanced)
         (let ((tree nil))
           (dotimes (i 15)
             (setq tree (funcall 'neovm--pda-bst-insert tree i (format "v%d" i))))
           (list (funcall 'neovm--pda-bst-height tree)
                 (length (funcall 'neovm--pda-bst-keys tree))))))
    (fmakunbound 'neovm--pda-bst-height)
    (fmakunbound 'neovm--pda-bst-make)
    (fmakunbound 'neovm--pda-bst-balance)
    (fmakunbound 'neovm--pda-bst-rot-right)
    (fmakunbound 'neovm--pda-bst-rot-left)
    (fmakunbound 'neovm--pda-bst-rebalance)
    (fmakunbound 'neovm--pda-bst-insert)
    (fmakunbound 'neovm--pda-bst-lookup)
    (fmakunbound 'neovm--pda-bst-keys)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 3 4 5 6 7 9) \"five\" \"one\" \"nine\" nil \"FIVE\" \"five\" (1 2 3 5 7 9) \"two\" nil 3 (4 15))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Structural sharing verification: extensive eq checks
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_persistent_data_advanced_sharing_verification() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let* (;; Build a chain of persistent updates
           (base (list 'a 'b 'c 'd 'e))
           ;; Each prepend shares the entire base
           (v1 (cons 'x base))
           (v2 (cons 'y base))
           (v3 (cons 'z v1))
           ;; alist-based persistent map: each put prepends
           (m0 nil)
           (m1 (cons '(:k1 . 10) m0))
           (m2 (cons '(:k2 . 20) m1))
           (m3 (cons '(:k3 . 30) m2))
           ;; Branch: two updates from m2
           (m3a (cons '(:k3 . 31) m2))
           (m3b (cons '(:k3 . 32) m2))
           ;; Deep nesting for sharing tests
           (inner '(1 2 3))
           (outer1 (list 'a inner 'b))
           (outer2 (list 'c inner 'd)))
      (list
       ;; v1 and v2 share base as tail
       (eq (cdr v1) base)
       (eq (cdr v2) base)
       (eq (cdr v1) (cdr v2))
       ;; v3 shares v1 as tail, and transitively base
       (eq (cdr v3) v1)
       (eq (cddr v3) base)
       ;; m3 shares m2 as tail, m2 shares m1
       (eq (cdr m3) m2)
       (eq (cddr m3) m1)
       (eq (cdddr m3) m0)
       ;; Branches share m2
       (eq (cdr m3a) m2)
       (eq (cdr m3b) m2)
       (eq (cdr m3a) (cdr m3b))
       ;; Branches have different car
       (not (equal (car m3a) (car m3b)))
       ;; Nested: inner is shared between outer1 and outer2
       (eq (cadr outer1) (cadr outer2))
       (eq (cadr outer1) inner)
       ;; Modifying outer1 doesn't affect outer2
       (let ((outer1-mod (list 'a (cons 0 inner) 'b)))
         (list
          ;; outer1-mod has different inner list
          (not (eq (cadr outer1-mod) inner))
          ;; But original outer1 still points to inner
          (eq (cadr outer1) inner)
          ;; outer2 still points to inner
          (eq (cadr outer2) inner)))
       ;; Lengths verify no mutation
       (length base) (length v1) (length v3)
       (length m3) (length m3a) (length m3b)))"#;
    let expect =
        expect_test::expect![[r#""OK (t t t t t t t t t t t t t t (t t t) 5 6 7 3 3 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Persistent vector: trie-based with branching factor 4
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_persistent_data_advanced_trie_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Simple persistent vector using chunks of 4
  ;; Represented as a flat vector split into 4-element leaf chunks
  ;; stored in a balanced tree.
  ;; For simplicity: vector of vectors, updates copy the affected leaf.
  (defvar neovm--pda-tv-chunk 4)

  (fset 'neovm--pda-tv-from-list
    (lambda (lst)
      "Convert list to persistent vector (vector of leaf vectors)."
      (let ((chunks nil) (chunk nil) (i 0))
        (dolist (x lst)
          (setq chunk (cons x chunk))
          (setq i (1+ i))
          (when (= i neovm--pda-tv-chunk)
            (setq chunks (cons (apply #'vector (nreverse chunk)) chunks))
            (setq chunk nil) (setq i 0)))
        (when chunk
          (setq chunks (cons (apply #'vector (nreverse chunk)) chunks)))
        (apply #'vector (nreverse chunks)))))

  (fset 'neovm--pda-tv-size
    (lambda (pv)
      (let ((total 0))
        (dotimes (i (length pv))
          (setq total (+ total (length (aref pv i)))))
        total)))

  (fset 'neovm--pda-tv-get
    (lambda (pv idx)
      (let ((ci (/ idx neovm--pda-tv-chunk))
            (li (% idx neovm--pda-tv-chunk)))
        (aref (aref pv ci) li))))

  (fset 'neovm--pda-tv-set
    (lambda (pv idx val)
      "Return new persistent vector with element at IDX updated."
      (let* ((ci (/ idx neovm--pda-tv-chunk))
             (li (% idx neovm--pda-tv-chunk))
             (new-pv (copy-sequence pv))
             (new-leaf (copy-sequence (aref pv ci))))
        (aset new-leaf li val)
        (aset new-pv ci new-leaf)
        new-pv)))

  (fset 'neovm--pda-tv-to-list
    (lambda (pv)
      (let ((result nil))
        (dotimes (i (length pv))
          (dotimes (j (length (aref pv i)))
            (setq result (cons (aref (aref pv i) j) result))))
        (nreverse result))))

  (unwind-protect
      (let* ((v0 (funcall 'neovm--pda-tv-from-list '(0 1 2 3 4 5 6 7 8 9 10 11)))
             ;; Update first element
             (v1 (funcall 'neovm--pda-tv-set v0 0 99))
             ;; Update last element
             (v2 (funcall 'neovm--pda-tv-set v0 11 88))
             ;; Update middle element
             (v3 (funcall 'neovm--pda-tv-set v0 5 55))
             ;; Branch: two different updates to same index
             (v4a (funcall 'neovm--pda-tv-set v0 4 -1))
             (v4b (funcall 'neovm--pda-tv-set v0 4 -2))
             ;; Chain of updates
             (v5 (funcall 'neovm--pda-tv-set
                          (funcall 'neovm--pda-tv-set
                                   (funcall 'neovm--pda-tv-set v0 0 100) 4 200) 8 300)))
        (list
         ;; Contents
         (funcall 'neovm--pda-tv-to-list v0)
         (funcall 'neovm--pda-tv-to-list v1)
         (funcall 'neovm--pda-tv-to-list v2)
         (funcall 'neovm--pda-tv-to-list v3)
         ;; Sizes
         (funcall 'neovm--pda-tv-size v0)
         ;; Specific lookups across versions
         (funcall 'neovm--pda-tv-get v0 0)
         (funcall 'neovm--pda-tv-get v1 0)
         (funcall 'neovm--pda-tv-get v0 11)
         (funcall 'neovm--pda-tv-get v2 11)
         ;; Branches
         (funcall 'neovm--pda-tv-get v4a 4)
         (funcall 'neovm--pda-tv-get v4b 4)
         (funcall 'neovm--pda-tv-get v0 4) ;; unchanged
         ;; Chained update
         (funcall 'neovm--pda-tv-to-list v5)
         ;; Structural sharing: unmodified chunks are eq
         ;; v1 changed chunk 0, so chunk 1 and 2 are shared
         (eq (aref v1 1) (aref v0 1))
         (eq (aref v1 2) (aref v0 2))
         ;; v2 changed chunk 2, so chunk 0 and 1 are shared
         (eq (aref v2 0) (aref v0 0))
         (eq (aref v2 1) (aref v0 1))
         ;; v4a and v4b both changed chunk 1, but share chunks 0 and 2 with v0
         (eq (aref v4a 0) (aref v0 0))
         (eq (aref v4a 2) (aref v0 2))
         (eq (aref v4b 0) (aref v0 0))))
    (fmakunbound 'neovm--pda-tv-from-list)
    (fmakunbound 'neovm--pda-tv-size)
    (fmakunbound 'neovm--pda-tv-get)
    (fmakunbound 'neovm--pda-tv-set)
    (fmakunbound 'neovm--pda-tv-to-list)
    (makunbound 'neovm--pda-tv-chunk)))"#;
    let expect = expect_test::expect![[
        r#""OK ((0 1 2 3 4 5 6 7 8 9 10 11) (99 1 2 3 4 5 6 7 8 9 10 11) (0 1 2 3 4 5 6 7 8 9 10 88) (0 1 2 3 4 55 6 7 8 9 10 11) 12 0 99 11 88 -1 -2 4 (100 1 2 3 200 5 6 7 300 9 10 11) t t t t t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Version history with branching (git-like commit graph)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_persistent_data_advanced_version_history() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Version history: each commit is (id data parent-ids)
  ;; History is a vector of commits (append-only log)
  (fset 'neovm--pda-vh-init
    (lambda (data)
      "Create initial history with one root commit."
      (vector (list 0 data nil))))

  (fset 'neovm--pda-vh-commit
    (lambda (hist data parent-id)
      "Add a new commit with given data and parent."
      (let ((new-id (length hist)))
        (vconcat hist (vector (list new-id data (list parent-id)))))))

  (fset 'neovm--pda-vh-merge-commit
    (lambda (hist data parent-ids)
      "Add a merge commit with multiple parents."
      (let ((new-id (length hist)))
        (vconcat hist (vector (list new-id data parent-ids))))))

  (fset 'neovm--pda-vh-get
    (lambda (hist id) (aref hist id)))

  (fset 'neovm--pda-vh-data
    (lambda (hist id) (cadr (aref hist id))))

  (fset 'neovm--pda-vh-parents
    (lambda (hist id) (caddr (aref hist id))))

  ;; Trace ancestry: all commits reachable from a given commit
  (fset 'neovm--pda-vh-ancestors
    (lambda (hist id)
      (let ((visited nil)
            (queue (list id)))
        (while queue
          (let ((cur (car queue)))
            (setq queue (cdr queue))
            (unless (memq cur visited)
              (setq visited (cons cur visited))
              (dolist (p (funcall 'neovm--pda-vh-parents hist cur))
                (setq queue (cons p queue))))))
        (sort visited #'<))))

  ;; Find common ancestor of two commits (simple: intersection of ancestors)
  (fset 'neovm--pda-vh-common-ancestor
    (lambda (hist id1 id2)
      (let ((anc1 (funcall 'neovm--pda-vh-ancestors hist id1))
            (anc2 (funcall 'neovm--pda-vh-ancestors hist id2))
            (common nil))
        (dolist (a anc1)
          (when (memq a anc2)
            (setq common (cons a common))))
        ;; Return the latest common ancestor (highest id)
        (car (sort common #'>)))))

  (unwind-protect
      (let* (;; Root: v0
             (h0 (funcall 'neovm--pda-vh-init '(:file "initial")))
             ;; Linear: v0 -> v1 -> v2
             (h1 (funcall 'neovm--pda-vh-commit h0 '(:file "edit1") 0))
             (h2 (funcall 'neovm--pda-vh-commit h1 '(:file "edit2") 1))
             ;; Branch from v1: v1 -> v3 -> v4
             (h3 (funcall 'neovm--pda-vh-commit h2 '(:file "branch-edit1") 1))
             (h4 (funcall 'neovm--pda-vh-commit h3 '(:file "branch-edit2") 3))
             ;; Merge: v2 + v4 -> v5
             (h5 (funcall 'neovm--pda-vh-merge-commit h4 '(:file "merged") '(2 4))))
        (list
         ;; Data at each version
         (funcall 'neovm--pda-vh-data h5 0)
         (funcall 'neovm--pda-vh-data h5 1)
         (funcall 'neovm--pda-vh-data h5 2)
         (funcall 'neovm--pda-vh-data h5 3)
         (funcall 'neovm--pda-vh-data h5 4)
         (funcall 'neovm--pda-vh-data h5 5)
         ;; Parents
         (funcall 'neovm--pda-vh-parents h5 0)  ;; nil (root)
         (funcall 'neovm--pda-vh-parents h5 2)  ;; (1)
         (funcall 'neovm--pda-vh-parents h5 5)  ;; (2 4) (merge)
         ;; Ancestors
         (funcall 'neovm--pda-vh-ancestors h5 2)  ;; (0 1 2)
         (funcall 'neovm--pda-vh-ancestors h5 4)  ;; (0 1 3 4)
         (funcall 'neovm--pda-vh-ancestors h5 5)  ;; (0 1 2 3 4 5)
         ;; Common ancestor of v2 and v4 = v1
         (funcall 'neovm--pda-vh-common-ancestor h5 2 4)
         ;; Common ancestor of v2 and v5 = v2
         (funcall 'neovm--pda-vh-common-ancestor h5 2 5)
         ;; History is append-only: older versions still accessible
         (length h0) (length h2) (length h5)
         ;; h0 data still valid through h5
         (equal (funcall 'neovm--pda-vh-data h5 0)
                (funcall 'neovm--pda-vh-data h0 0))))
    (fmakunbound 'neovm--pda-vh-init)
    (fmakunbound 'neovm--pda-vh-commit)
    (fmakunbound 'neovm--pda-vh-merge-commit)
    (fmakunbound 'neovm--pda-vh-get)
    (fmakunbound 'neovm--pda-vh-data)
    (fmakunbound 'neovm--pda-vh-parents)
    (fmakunbound 'neovm--pda-vh-ancestors)
    (fmakunbound 'neovm--pda-vh-common-ancestor)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:file \"initial\") (:file \"edit1\") (:file \"edit2\") (:file \"branch-edit1\") (:file \"branch-edit2\") (:file \"merged\") nil (1) (2 4) (0 1 2) (0 1 3 4) (0 1 2 3 4 5) 1 2 1 3 6 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Transaction-like operations on persistent state
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_persistent_data_advanced_transactions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Transaction system on persistent alist-map:
  ;; begin-txn captures a snapshot, apply-ops modifies it,
  ;; commit returns new state, rollback returns original.
  ;; Validation: check preconditions before commit.

  (fset 'neovm--pda-tx-begin
    (lambda (state)
      "Begin transaction: return (original . working-copy)."
      (cons state (copy-sequence state))))

  (fset 'neovm--pda-tx-set
    (lambda (txn key val)
      "Set a key in the working copy."
      (let ((working (cdr txn)))
        ;; Shadow: prepend new binding
        (setcdr txn (cons (cons key val) working))
        txn)))

  (fset 'neovm--pda-tx-get
    (lambda (txn key)
      "Get from working copy."
      (let ((pair (assoc key (cdr txn))))
        (if pair (cdr pair) nil))))

  (fset 'neovm--pda-tx-commit
    (lambda (txn)
      "Commit: return the working copy as new state."
      (cdr txn)))

  (fset 'neovm--pda-tx-rollback
    (lambda (txn)
      "Rollback: return the original state."
      (car txn)))

  ;; Validated transaction: only commit if check passes
  (fset 'neovm--pda-tx-commit-if
    (lambda (txn check-fn)
      "Commit only if CHECK-FN returns non-nil on working copy."
      (if (funcall check-fn (cdr txn))
          (cons t (cdr txn))
        (cons nil (car txn)))))

  (unwind-protect
      (let* ((state0 '((:balance . 100) (:name . "Alice")))
             ;; Transaction 1: deposit 50
             (tx1 (funcall 'neovm--pda-tx-begin state0))
             (_ (funcall 'neovm--pda-tx-set tx1 :balance 150))
             (state1 (funcall 'neovm--pda-tx-commit tx1))
             ;; Transaction 2: withdraw 200 (should fail validation)
             (tx2 (funcall 'neovm--pda-tx-begin state1))
             (_ (funcall 'neovm--pda-tx-set tx2 :balance -50))
             (result2 (funcall 'neovm--pda-tx-commit-if
                               tx2
                               (lambda (state)
                                 (>= (cdr (assoc :balance state)) 0))))
             (state2 (cdr result2))
             ;; Transaction 3: withdraw 30 (should succeed)
             (tx3 (funcall 'neovm--pda-tx-begin state1))
             (_ (funcall 'neovm--pda-tx-set tx3 :balance 120))
             (result3 (funcall 'neovm--pda-tx-commit-if
                               tx3
                               (lambda (state)
                                 (>= (cdr (assoc :balance state)) 0))))
             (state3 (cdr result3))
             ;; Transaction 4: rollback
             (tx4 (funcall 'neovm--pda-tx-begin state3))
             (_ (funcall 'neovm--pda-tx-set tx4 :balance 0))
             (_ (funcall 'neovm--pda-tx-set tx4 :name "Bob"))
             (state4 (funcall 'neovm--pda-tx-rollback tx4)))
        (list
         ;; state0 unchanged throughout
         (cdr (assoc :balance state0))
         (cdr (assoc :name state0))
         ;; state1: balance updated
         (cdr (assoc :balance state1))
         ;; Transaction 2 failed validation
         (car result2)   ;; nil
         ;; state2 rolled back to state1
         (cdr (assoc :balance state2))
         ;; Transaction 3 succeeded
         (car result3)   ;; t
         (cdr (assoc :balance state3))
         ;; Transaction 4: rolled back, state4 = state3
         (cdr (assoc :balance state4))
         (cdr (assoc :name state4))))
    (fmakunbound 'neovm--pda-tx-begin)
    (fmakunbound 'neovm--pda-tx-set)
    (fmakunbound 'neovm--pda-tx-get)
    (fmakunbound 'neovm--pda-tx-commit)
    (fmakunbound 'neovm--pda-tx-rollback)
    (fmakunbound 'neovm--pda-tx-commit-if)))"#;
    let expect = expect_test::expect![[r#""OK (100 \"Alice\" 150 nil 150 t 120 120 \"Alice\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Persistent deque: doubly-ended queue with structural sharing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_persistent_data_advanced_deque() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Persistent deque: (front . back)
  ;; front is a list (left-to-right), back is a reversed list
  ;; push-front: cons onto front; push-back: cons onto back
  ;; pop-front: cdr front (rebalance if empty); pop-back: cdr back
  (fset 'neovm--pda-dq-empty (lambda () (cons nil nil)))

  (fset 'neovm--pda-dq-empty-p
    (lambda (dq) (and (null (car dq)) (null (cdr dq)))))

  (fset 'neovm--pda-dq-push-front
    (lambda (dq val) (cons (cons val (car dq)) (cdr dq))))

  (fset 'neovm--pda-dq-push-back
    (lambda (dq val) (cons (car dq) (cons val (cdr dq)))))

  (fset 'neovm--pda-dq-rebalance
    (lambda (dq)
      "If front is empty, move reversed back to front (or vice versa)."
      (cond
       ((and (null (car dq)) (cdr dq))
        (cons (nreverse (copy-sequence (cdr dq))) nil))
       ((and (null (cdr dq)) (car dq))
        (cons nil (nreverse (copy-sequence (car dq)))))
       (t dq))))

  (fset 'neovm--pda-dq-peek-front
    (lambda (dq)
      (let ((dq2 (funcall 'neovm--pda-dq-rebalance dq)))
        (caar dq2))))

  (fset 'neovm--pda-dq-peek-back
    (lambda (dq)
      (let ((dq2 (funcall 'neovm--pda-dq-rebalance dq)))
        (cadr dq2))))

  (fset 'neovm--pda-dq-pop-front
    (lambda (dq)
      (let ((dq2 (funcall 'neovm--pda-dq-rebalance dq)))
        (cons (cdar dq2) (cdr dq2)))))

  (fset 'neovm--pda-dq-pop-back
    (lambda (dq)
      (let ((dq2 (funcall 'neovm--pda-dq-rebalance dq)))
        (cons (car dq2) (cddr dq2)))))

  (fset 'neovm--pda-dq-to-list
    (lambda (dq)
      (append (car dq) (nreverse (copy-sequence (cdr dq))))))

  (fset 'neovm--pda-dq-size
    (lambda (dq) (+ (length (car dq)) (length (cdr dq)))))

  (unwind-protect
      (let* ((d0 (funcall 'neovm--pda-dq-empty))
             ;; Push front: 3, 2, 1 -> front=(1 2 3)
             (d1 (funcall 'neovm--pda-dq-push-front d0 3))
             (d2 (funcall 'neovm--pda-dq-push-front d1 2))
             (d3 (funcall 'neovm--pda-dq-push-front d2 1))
             ;; Push back: a, b -> back=(b a)
             (d4 (funcall 'neovm--pda-dq-push-back d3 'a))
             (d5 (funcall 'neovm--pda-dq-push-back d4 'b))
             ;; Pop front
             (d6 (funcall 'neovm--pda-dq-pop-front d5))
             ;; Pop back
             (d7 (funcall 'neovm--pda-dq-pop-back d5)))
        (list
         (funcall 'neovm--pda-dq-empty-p d0)
         (funcall 'neovm--pda-dq-empty-p d1)
         ;; Contents
         (funcall 'neovm--pda-dq-to-list d3)
         (funcall 'neovm--pda-dq-to-list d5)
         ;; Size
         (funcall 'neovm--pda-dq-size d5)
         ;; Peek
         (funcall 'neovm--pda-dq-peek-front d5)
         (funcall 'neovm--pda-dq-peek-back d5)
         ;; After pop-front: removes 1
         (funcall 'neovm--pda-dq-to-list d6)
         ;; After pop-back: removes b
         (funcall 'neovm--pda-dq-to-list d7)
         ;; Original d5 unchanged
         (funcall 'neovm--pda-dq-to-list d5)
         ;; Structural sharing: d3 front is shared in d4 and d5
         (eq (car d3) (car d4))
         (eq (car d3) (car d5))))
    (fmakunbound 'neovm--pda-dq-empty)
    (fmakunbound 'neovm--pda-dq-empty-p)
    (fmakunbound 'neovm--pda-dq-push-front)
    (fmakunbound 'neovm--pda-dq-push-back)
    (fmakunbound 'neovm--pda-dq-rebalance)
    (fmakunbound 'neovm--pda-dq-peek-front)
    (fmakunbound 'neovm--pda-dq-peek-back)
    (fmakunbound 'neovm--pda-dq-pop-front)
    (fmakunbound 'neovm--pda-dq-pop-back)
    (fmakunbound 'neovm--pda-dq-to-list)
    (fmakunbound 'neovm--pda-dq-size)))"#;
    let expect = expect_test::expect![[
        r#""OK (t nil (1 2 3) (1 2 3 a b) 5 1 b (2 3 a b) (1 2 3 a) (1 2 3 a b) t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
