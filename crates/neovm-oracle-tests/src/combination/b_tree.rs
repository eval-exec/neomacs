//! Oracle parity tests for a B-tree (order 3) implemented in Elisp using lists.
//!
//! Node structure: (keys children leaf-p)
//! - keys: sorted list of key values
//! - children: list of child nodes (nil for leaf nodes)
//! - leaf-p: t if leaf, nil if internal
//!
//! Implements search, insert with node splitting, in-order traversal,
//! bulk insert, and range queries.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// B-tree core: node construction, search, and simple insert
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_btree_search_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; B-tree order 3: each node holds at most 2 keys, at least 1 (except root)
  ;; Node: (keys children leaf-p)

  (fset 'neovm--bt-make-leaf
    (lambda (keys)
      (list keys nil t)))

  (fset 'neovm--bt-make-internal
    (lambda (keys children)
      (list keys children nil)))

  (fset 'neovm--bt-keys (lambda (node) (car node)))
  (fset 'neovm--bt-children (lambda (node) (cadr node)))
  (fset 'neovm--bt-leaf-p (lambda (node) (caddr node)))

  (fset 'neovm--bt-search
    (lambda (node key)
      "Search for KEY in B-tree rooted at NODE. Returns t if found, nil otherwise."
      (if (null node)
          nil
        (let ((keys (funcall 'neovm--bt-keys node))
              (found nil)
              (idx 0))
          ;; Check if key is in this node's keys
          (dolist (k keys)
            (when (= k key) (setq found t)))
          (if found
              t
            (if (funcall 'neovm--bt-leaf-p node)
                nil
              ;; Find correct child
              (let ((children (funcall 'neovm--bt-children node))
                    (pos 0))
                (dolist (k keys)
                  (when (< key k)
                    ;; break-like: only increment if key >= k
                    )
                  (when (>= key k)
                    (setq pos (1+ pos))))
                (funcall 'neovm--bt-search (nth pos children) key))))))))

  (unwind-protect
      (let* (;; Manually construct a B-tree:
             ;;        [10 20]
             ;;       /   |   \
             ;;    [5]  [15]  [25 30]
             (left (funcall 'neovm--bt-make-leaf '(5)))
             (mid (funcall 'neovm--bt-make-leaf '(15)))
             (right (funcall 'neovm--bt-make-leaf '(25 30)))
             (root (funcall 'neovm--bt-make-internal '(10 20) (list left mid right))))
        (list
          ;; Search for keys in root
          (funcall 'neovm--bt-search root 10)
          (funcall 'neovm--bt-search root 20)
          ;; Search for keys in leaves
          (funcall 'neovm--bt-search root 5)
          (funcall 'neovm--bt-search root 15)
          (funcall 'neovm--bt-search root 25)
          (funcall 'neovm--bt-search root 30)
          ;; Search for missing keys
          (funcall 'neovm--bt-search root 1)
          (funcall 'neovm--bt-search root 12)
          (funcall 'neovm--bt-search root 22)
          (funcall 'neovm--bt-search root 100)
          ;; Search nil tree
          (funcall 'neovm--bt-search nil 5)))
    (fmakunbound 'neovm--bt-make-leaf)
    (fmakunbound 'neovm--bt-make-internal)
    (fmakunbound 'neovm--bt-keys)
    (fmakunbound 'neovm--bt-children)
    (fmakunbound 'neovm--bt-leaf-p)
    (fmakunbound 'neovm--bt-search)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t nil nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// B-tree insert with node splitting (order 3)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_btree_insert_with_splitting() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Order-3 B-tree: max 2 keys per node, split when 3.

  (fset 'neovm--bt2-make-leaf (lambda (keys) (list keys nil t)))
  (fset 'neovm--bt2-make-node (lambda (keys children leaf-p) (list keys children leaf-p)))
  (fset 'neovm--bt2-keys (lambda (n) (car n)))
  (fset 'neovm--bt2-children (lambda (n) (cadr n)))
  (fset 'neovm--bt2-leaf-p (lambda (n) (caddr n)))

  ;; Insert into sorted list
  (fset 'neovm--bt2-sorted-insert
    (lambda (lst val)
      (cond
        ((null lst) (list val))
        ((<= val (car lst)) (cons val lst))
        (t (cons (car lst) (funcall 'neovm--bt2-sorted-insert (cdr lst) val))))))

  ;; Find position of val in sorted key list
  (fset 'neovm--bt2-find-child-idx
    (lambda (keys val)
      (let ((idx 0))
        (dolist (k keys)
          (when (>= val k) (setq idx (1+ idx))))
        idx)))

  ;; Replace element at position in list
  (fset 'neovm--bt2-list-set
    (lambda (lst idx val)
      (if (= idx 0)
          (cons val (cdr lst))
        (cons (car lst) (funcall 'neovm--bt2-list-set (cdr lst) (1- idx) val)))))

  ;; Insert element at position in list
  (fset 'neovm--bt2-list-insert
    (lambda (lst idx val)
      (if (= idx 0)
          (cons val lst)
        (cons (car lst) (funcall 'neovm--bt2-list-insert (cdr lst) (1- idx) val)))))

  ;; Split result: (median left-node right-node)
  ;; Insert returns either a single node or a split triple

  (fset 'neovm--bt2-insert-impl
    (lambda (node key)
      "Insert KEY into NODE. Returns (node) for no split, or (median left right) for split."
      (if (funcall 'neovm--bt2-leaf-p node)
          ;; Leaf insert
          (let ((new-keys (funcall 'neovm--bt2-sorted-insert
                                   (funcall 'neovm--bt2-keys node) key)))
            (if (<= (length new-keys) 2)
                ;; No split needed
                (list (funcall 'neovm--bt2-make-leaf new-keys))
              ;; Split: 3 keys -> median is middle
              (let ((left (funcall 'neovm--bt2-make-leaf (list (car new-keys))))
                    (right (funcall 'neovm--bt2-make-leaf (list (caddr new-keys))))
                    (median (cadr new-keys)))
                (list median left right))))
        ;; Internal node insert
        (let* ((keys (funcall 'neovm--bt2-keys node))
               (children (funcall 'neovm--bt2-children node))
               (idx (funcall 'neovm--bt2-find-child-idx keys key))
               (child (nth idx children))
               (result (funcall 'neovm--bt2-insert-impl child key)))
          (if (= (length result) 1)
              ;; Child didn't split
              (list (funcall 'neovm--bt2-make-node
                             keys
                             (funcall 'neovm--bt2-list-set children idx (car result))
                             nil))
            ;; Child split: result = (median left right)
            (let ((median (car result))
                  (left-child (cadr result))
                  (right-child (caddr result)))
              (let ((new-keys (funcall 'neovm--bt2-sorted-insert keys median))
                    (new-children
                     (funcall 'neovm--bt2-list-insert
                              (funcall 'neovm--bt2-list-set children idx left-child)
                              (1+ idx)
                              right-child)))
                (if (<= (length new-keys) 2)
                    (list (funcall 'neovm--bt2-make-node new-keys new-children nil))
                  ;; Internal split
                  (let ((med (cadr new-keys))
                        (left-keys (list (car new-keys)))
                        (right-keys (list (caddr new-keys)))
                        (left-ch (list (car new-children) (cadr new-children)))
                        (right-ch (list (caddr new-children) (cadddr new-children))))
                    (list med
                          (funcall 'neovm--bt2-make-node left-keys left-ch nil)
                          (funcall 'neovm--bt2-make-node right-keys right-ch nil)))))))))))

  (fset 'neovm--bt2-insert
    (lambda (root key)
      "Insert KEY into tree ROOT. Returns new root."
      (if (null root)
          (funcall 'neovm--bt2-make-leaf (list key))
        (let ((result (funcall 'neovm--bt2-insert-impl root key)))
          (if (= (length result) 1)
              (car result)
            ;; Root split: create new root
            (funcall 'neovm--bt2-make-node
                     (list (car result))
                     (list (cadr result) (caddr result))
                     nil))))))

  ;; In-order traversal
  (fset 'neovm--bt2-inorder
    (lambda (node)
      (if (null node)
          nil
        (if (funcall 'neovm--bt2-leaf-p node)
            (copy-sequence (funcall 'neovm--bt2-keys node))
          (let ((keys (funcall 'neovm--bt2-keys node))
                (children (funcall 'neovm--bt2-children node))
                (result nil))
            ;; Interleave children traversals with keys
            (let ((i 0))
              (dolist (k keys)
                (setq result (append result
                                     (funcall 'neovm--bt2-inorder (nth i children))))
                (setq result (append result (list k)))
                (setq i (1+ i)))
              ;; Last child
              (setq result (append result
                                   (funcall 'neovm--bt2-inorder (nth i children)))))
            result)))))

  (unwind-protect
      (let ((tree nil))
        ;; Insert elements that cause splits
        (setq tree (funcall 'neovm--bt2-insert tree 10))
        (let ((r1 (funcall 'neovm--bt2-inorder tree)))
          (setq tree (funcall 'neovm--bt2-insert tree 20))
          (let ((r2 (funcall 'neovm--bt2-inorder tree)))
            ;; Third insert causes first leaf split
            (setq tree (funcall 'neovm--bt2-insert tree 30))
            (let ((r3 (funcall 'neovm--bt2-inorder tree)))
              (setq tree (funcall 'neovm--bt2-insert tree 5))
              (setq tree (funcall 'neovm--bt2-insert tree 15))
              (let ((r4 (funcall 'neovm--bt2-inorder tree)))
                (list r1 r2 r3 r4
                      ;; Root should not be a leaf after splits
                      (funcall 'neovm--bt2-leaf-p tree)))))))
    (fmakunbound 'neovm--bt2-make-leaf)
    (fmakunbound 'neovm--bt2-make-node)
    (fmakunbound 'neovm--bt2-keys)
    (fmakunbound 'neovm--bt2-children)
    (fmakunbound 'neovm--bt2-leaf-p)
    (fmakunbound 'neovm--bt2-sorted-insert)
    (fmakunbound 'neovm--bt2-find-child-idx)
    (fmakunbound 'neovm--bt2-list-set)
    (fmakunbound 'neovm--bt2-list-insert)
    (fmakunbound 'neovm--bt2-insert-impl)
    (fmakunbound 'neovm--bt2-insert)
    (fmakunbound 'neovm--bt2-inorder)))"#;
    let expect = expect_test::expect![[r#""OK ((10) (10 20) (10 20 30) (5 10 15 20 30) nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// In-order traversal produces sorted output after various insertions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_btree_inorder_traversal_sorted() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Redefine the full B-tree implementation inline for this test
  (fset 'neovm--bt3-make-leaf (lambda (keys) (list keys nil t)))
  (fset 'neovm--bt3-make-node (lambda (keys children leaf-p) (list keys children leaf-p)))
  (fset 'neovm--bt3-keys (lambda (n) (car n)))
  (fset 'neovm--bt3-children (lambda (n) (cadr n)))
  (fset 'neovm--bt3-leaf-p (lambda (n) (caddr n)))

  (fset 'neovm--bt3-sorted-insert
    (lambda (lst val)
      (cond
        ((null lst) (list val))
        ((<= val (car lst)) (cons val lst))
        (t (cons (car lst) (funcall 'neovm--bt3-sorted-insert (cdr lst) val))))))

  (fset 'neovm--bt3-find-idx
    (lambda (keys val)
      (let ((idx 0))
        (dolist (k keys) (when (>= val k) (setq idx (1+ idx))))
        idx)))

  (fset 'neovm--bt3-lset
    (lambda (lst idx val)
      (if (= idx 0) (cons val (cdr lst))
        (cons (car lst) (funcall 'neovm--bt3-lset (cdr lst) (1- idx) val)))))

  (fset 'neovm--bt3-lins
    (lambda (lst idx val)
      (if (= idx 0) (cons val lst)
        (cons (car lst) (funcall 'neovm--bt3-lins (cdr lst) (1- idx) val)))))

  (fset 'neovm--bt3-ins-impl
    (lambda (node key)
      (if (funcall 'neovm--bt3-leaf-p node)
          (let ((nk (funcall 'neovm--bt3-sorted-insert (funcall 'neovm--bt3-keys node) key)))
            (if (<= (length nk) 2)
                (list (funcall 'neovm--bt3-make-leaf nk))
              (list (cadr nk)
                    (funcall 'neovm--bt3-make-leaf (list (car nk)))
                    (funcall 'neovm--bt3-make-leaf (list (caddr nk))))))
        (let* ((keys (funcall 'neovm--bt3-keys node))
               (ch (funcall 'neovm--bt3-children node))
               (idx (funcall 'neovm--bt3-find-idx keys key))
               (res (funcall 'neovm--bt3-ins-impl (nth idx ch) key)))
          (if (= (length res) 1)
              (list (funcall 'neovm--bt3-make-node keys (funcall 'neovm--bt3-lset ch idx (car res)) nil))
            (let* ((nk (funcall 'neovm--bt3-sorted-insert keys (car res)))
                   (nc (funcall 'neovm--bt3-lins (funcall 'neovm--bt3-lset ch idx (cadr res)) (1+ idx) (caddr res))))
              (if (<= (length nk) 2)
                  (list (funcall 'neovm--bt3-make-node nk nc nil))
                (list (cadr nk)
                      (funcall 'neovm--bt3-make-node (list (car nk)) (list (car nc) (cadr nc)) nil)
                      (funcall 'neovm--bt3-make-node (list (caddr nk)) (list (caddr nc) (cadddr nc)) nil)))))))))

  (fset 'neovm--bt3-insert
    (lambda (root key)
      (if (null root)
          (funcall 'neovm--bt3-make-leaf (list key))
        (let ((res (funcall 'neovm--bt3-ins-impl root key)))
          (if (= (length res) 1) (car res)
            (funcall 'neovm--bt3-make-node (list (car res)) (list (cadr res) (caddr res)) nil))))))

  (fset 'neovm--bt3-inorder
    (lambda (node)
      (if (null node) nil
        (if (funcall 'neovm--bt3-leaf-p node)
            (copy-sequence (funcall 'neovm--bt3-keys node))
          (let ((keys (funcall 'neovm--bt3-keys node))
                (ch (funcall 'neovm--bt3-children node))
                (res nil) (i 0))
            (dolist (k keys)
              (setq res (append res (funcall 'neovm--bt3-inorder (nth i ch))))
              (setq res (append res (list k)))
              (setq i (1+ i)))
            (setq res (append res (funcall 'neovm--bt3-inorder (nth i ch))))
            res)))))

  (unwind-protect
      (let ((tree nil))
        ;; Insert in reverse order: should still produce sorted traversal
        (dolist (k '(7 6 5 4 3 2 1))
          (setq tree (funcall 'neovm--bt3-insert tree k)))
        (let ((sorted-rev (funcall 'neovm--bt3-inorder tree)))
          ;; Insert in random order
          (setq tree nil)
          (dolist (k '(4 2 7 1 5 3 6))
            (setq tree (funcall 'neovm--bt3-insert tree k)))
          (let ((sorted-rand (funcall 'neovm--bt3-inorder tree)))
            ;; Insert already sorted
            (setq tree nil)
            (dolist (k '(1 2 3 4 5 6 7))
              (setq tree (funcall 'neovm--bt3-insert tree k)))
            (let ((sorted-asc (funcall 'neovm--bt3-inorder tree)))
              (list
                ;; All three should produce (1 2 3 4 5 6 7)
                sorted-rev
                sorted-rand
                sorted-asc
                (equal sorted-rev sorted-rand)
                (equal sorted-rand sorted-asc))))))
    (fmakunbound 'neovm--bt3-make-leaf)
    (fmakunbound 'neovm--bt3-make-node)
    (fmakunbound 'neovm--bt3-keys)
    (fmakunbound 'neovm--bt3-children)
    (fmakunbound 'neovm--bt3-leaf-p)
    (fmakunbound 'neovm--bt3-sorted-insert)
    (fmakunbound 'neovm--bt3-find-idx)
    (fmakunbound 'neovm--bt3-lset)
    (fmakunbound 'neovm--bt3-lins)
    (fmakunbound 'neovm--bt3-ins-impl)
    (fmakunbound 'neovm--bt3-insert)
    (fmakunbound 'neovm--bt3-inorder)))"#;
    let expect =
        expect_test::expect![[r#""OK ((1 2 3 4 5 6 7) (1 2 3 4 5 6 7) (1 2 3 4 5 6 7) t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Bulk insert and verify sorted order
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_btree_bulk_insert_sorted() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--bt4-make-leaf (lambda (keys) (list keys nil t)))
  (fset 'neovm--bt4-make-node (lambda (keys children leaf-p) (list keys children leaf-p)))
  (fset 'neovm--bt4-keys (lambda (n) (car n)))
  (fset 'neovm--bt4-children (lambda (n) (cadr n)))
  (fset 'neovm--bt4-leaf-p (lambda (n) (caddr n)))
  (fset 'neovm--bt4-sorted-insert
    (lambda (lst val)
      (cond ((null lst) (list val)) ((<= val (car lst)) (cons val lst))
            (t (cons (car lst) (funcall 'neovm--bt4-sorted-insert (cdr lst) val))))))
  (fset 'neovm--bt4-find-idx
    (lambda (keys val) (let ((idx 0)) (dolist (k keys) (when (>= val k) (setq idx (1+ idx)))) idx)))
  (fset 'neovm--bt4-lset
    (lambda (l i v) (if (= i 0) (cons v (cdr l)) (cons (car l) (funcall 'neovm--bt4-lset (cdr l) (1- i) v)))))
  (fset 'neovm--bt4-lins
    (lambda (l i v) (if (= i 0) (cons v l) (cons (car l) (funcall 'neovm--bt4-lins (cdr l) (1- i) v)))))
  (fset 'neovm--bt4-ins-impl
    (lambda (node key)
      (if (funcall 'neovm--bt4-leaf-p node)
          (let ((nk (funcall 'neovm--bt4-sorted-insert (funcall 'neovm--bt4-keys node) key)))
            (if (<= (length nk) 2) (list (funcall 'neovm--bt4-make-leaf nk))
              (list (cadr nk) (funcall 'neovm--bt4-make-leaf (list (car nk)))
                    (funcall 'neovm--bt4-make-leaf (list (caddr nk))))))
        (let* ((keys (funcall 'neovm--bt4-keys node))
               (ch (funcall 'neovm--bt4-children node))
               (idx (funcall 'neovm--bt4-find-idx keys key))
               (res (funcall 'neovm--bt4-ins-impl (nth idx ch) key)))
          (if (= (length res) 1)
              (list (funcall 'neovm--bt4-make-node keys (funcall 'neovm--bt4-lset ch idx (car res)) nil))
            (let* ((nk (funcall 'neovm--bt4-sorted-insert keys (car res)))
                   (nc (funcall 'neovm--bt4-lins (funcall 'neovm--bt4-lset ch idx (cadr res)) (1+ idx) (caddr res))))
              (if (<= (length nk) 2)
                  (list (funcall 'neovm--bt4-make-node nk nc nil))
                (list (cadr nk)
                      (funcall 'neovm--bt4-make-node (list (car nk)) (list (car nc) (cadr nc)) nil)
                      (funcall 'neovm--bt4-make-node (list (caddr nk)) (list (caddr nc) (cadddr nc)) nil)))))))))
  (fset 'neovm--bt4-insert
    (lambda (root key)
      (if (null root) (funcall 'neovm--bt4-make-leaf (list key))
        (let ((res (funcall 'neovm--bt4-ins-impl root key)))
          (if (= (length res) 1) (car res)
            (funcall 'neovm--bt4-make-node (list (car res)) (list (cadr res) (caddr res)) nil))))))
  (fset 'neovm--bt4-inorder
    (lambda (node)
      (if (null node) nil
        (if (funcall 'neovm--bt4-leaf-p node)
            (copy-sequence (funcall 'neovm--bt4-keys node))
          (let ((keys (funcall 'neovm--bt4-keys node))
                (ch (funcall 'neovm--bt4-children node))
                (res nil) (i 0))
            (dolist (k keys)
              (setq res (append res (funcall 'neovm--bt4-inorder (nth i ch))))
              (setq res (append res (list k)))
              (setq i (1+ i)))
            (setq res (append res (funcall 'neovm--bt4-inorder (nth i ch))))
            res)))))

  (fset 'neovm--bt4-search
    (lambda (node key)
      (if (null node) nil
        (let ((keys (funcall 'neovm--bt4-keys node)) (found nil))
          (dolist (k keys) (when (= k key) (setq found t)))
          (if found t
            (if (funcall 'neovm--bt4-leaf-p node) nil
              (let ((idx (funcall 'neovm--bt4-find-idx keys key)))
                (funcall 'neovm--bt4-search (nth idx (funcall 'neovm--bt4-children node)) key))))))))

  (unwind-protect
      (let ((tree nil)
            (input '(50 25 75 10 30 60 90 5 15 27 35 55 65 85 95)))
        ;; Bulk insert 15 elements
        (dolist (k input)
          (setq tree (funcall 'neovm--bt4-insert tree k)))
        (let ((traversal (funcall 'neovm--bt4-inorder tree))
              (sorted-input (sort (copy-sequence input) '<)))
          (list
            ;; Traversal should be sorted
            traversal
            (equal traversal sorted-input)
            (length traversal)
            ;; All elements searchable
            (cl-every (lambda (k) (funcall 'neovm--bt4-search tree k)) input)
            ;; Non-existent keys not found
            (funcall 'neovm--bt4-search tree 0)
            (funcall 'neovm--bt4-search tree 42)
            (funcall 'neovm--bt4-search tree 100))))
    (fmakunbound 'neovm--bt4-make-leaf)
    (fmakunbound 'neovm--bt4-make-node)
    (fmakunbound 'neovm--bt4-keys)
    (fmakunbound 'neovm--bt4-children)
    (fmakunbound 'neovm--bt4-leaf-p)
    (fmakunbound 'neovm--bt4-sorted-insert)
    (fmakunbound 'neovm--bt4-find-idx)
    (fmakunbound 'neovm--bt4-lset)
    (fmakunbound 'neovm--bt4-lins)
    (fmakunbound 'neovm--bt4-ins-impl)
    (fmakunbound 'neovm--bt4-insert)
    (fmakunbound 'neovm--bt4-inorder)
    (fmakunbound 'neovm--bt4-search)))"#;
    let expect = expect_test::expect![[r#""ERR (void-function cl-every)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Range query: find all keys between min and max (inclusive)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_btree_range_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--bt5-make-leaf (lambda (keys) (list keys nil t)))
  (fset 'neovm--bt5-make-node (lambda (keys children leaf-p) (list keys children leaf-p)))
  (fset 'neovm--bt5-keys (lambda (n) (car n)))
  (fset 'neovm--bt5-children (lambda (n) (cadr n)))
  (fset 'neovm--bt5-leaf-p (lambda (n) (caddr n)))
  (fset 'neovm--bt5-sorted-insert
    (lambda (lst val)
      (cond ((null lst) (list val)) ((<= val (car lst)) (cons val lst))
            (t (cons (car lst) (funcall 'neovm--bt5-sorted-insert (cdr lst) val))))))
  (fset 'neovm--bt5-find-idx
    (lambda (keys val) (let ((idx 0)) (dolist (k keys) (when (>= val k) (setq idx (1+ idx)))) idx)))
  (fset 'neovm--bt5-lset
    (lambda (l i v) (if (= i 0) (cons v (cdr l)) (cons (car l) (funcall 'neovm--bt5-lset (cdr l) (1- i) v)))))
  (fset 'neovm--bt5-lins
    (lambda (l i v) (if (= i 0) (cons v l) (cons (car l) (funcall 'neovm--bt5-lins (cdr l) (1- i) v)))))
  (fset 'neovm--bt5-ins-impl
    (lambda (node key)
      (if (funcall 'neovm--bt5-leaf-p node)
          (let ((nk (funcall 'neovm--bt5-sorted-insert (funcall 'neovm--bt5-keys node) key)))
            (if (<= (length nk) 2) (list (funcall 'neovm--bt5-make-leaf nk))
              (list (cadr nk) (funcall 'neovm--bt5-make-leaf (list (car nk)))
                    (funcall 'neovm--bt5-make-leaf (list (caddr nk))))))
        (let* ((keys (funcall 'neovm--bt5-keys node))
               (ch (funcall 'neovm--bt5-children node))
               (idx (funcall 'neovm--bt5-find-idx keys key))
               (res (funcall 'neovm--bt5-ins-impl (nth idx ch) key)))
          (if (= (length res) 1)
              (list (funcall 'neovm--bt5-make-node keys (funcall 'neovm--bt5-lset ch idx (car res)) nil))
            (let* ((nk (funcall 'neovm--bt5-sorted-insert keys (car res)))
                   (nc (funcall 'neovm--bt5-lins (funcall 'neovm--bt5-lset ch idx (cadr res)) (1+ idx) (caddr res))))
              (if (<= (length nk) 2)
                  (list (funcall 'neovm--bt5-make-node nk nc nil))
                (list (cadr nk)
                      (funcall 'neovm--bt5-make-node (list (car nk)) (list (car nc) (cadr nc)) nil)
                      (funcall 'neovm--bt5-make-node (list (caddr nk)) (list (caddr nc) (cadddr nc)) nil)))))))))
  (fset 'neovm--bt5-insert
    (lambda (root key)
      (if (null root) (funcall 'neovm--bt5-make-leaf (list key))
        (let ((res (funcall 'neovm--bt5-ins-impl root key)))
          (if (= (length res) 1) (car res)
            (funcall 'neovm--bt5-make-node (list (car res)) (list (cadr res) (caddr res)) nil))))))

  ;; Range query: collect all keys in [lo, hi] via in-order traversal with bounds check
  (fset 'neovm--bt5-range
    (lambda (node lo hi)
      "Collect all keys in NODE that are >= LO and <= HI."
      (if (null node) nil
        (if (funcall 'neovm--bt5-leaf-p node)
            (let ((result nil))
              (dolist (k (funcall 'neovm--bt5-keys node))
                (when (and (>= k lo) (<= k hi))
                  (setq result (cons k result))))
              (nreverse result))
          (let ((keys (funcall 'neovm--bt5-keys node))
                (ch (funcall 'neovm--bt5-children node))
                (result nil) (i 0))
            (dolist (k keys)
              ;; Visit left child if it might contain keys >= lo
              (when (<= lo k)
                (setq result (append result (funcall 'neovm--bt5-range (nth i ch) lo hi))))
              (when (and (>= k lo) (<= k hi))
                (setq result (append result (list k))))
              ;; Always need to check next child
              (when (> lo k)
                (setq result (append result (funcall 'neovm--bt5-range (nth i ch) lo hi))))
              (setq i (1+ i)))
            ;; Last child
            (let ((last-key (car (last keys))))
              (when (>= hi last-key)
                (setq result (append result (funcall 'neovm--bt5-range (nth i ch) lo hi)))))
            result)))))

  (unwind-protect
      (let ((tree nil))
        (dolist (k '(10 20 30 40 50 60 70 80 90 100))
          (setq tree (funcall 'neovm--bt5-insert tree k)))
        (list
          ;; Range [25, 65] should give (30 40 50 60)
          (funcall 'neovm--bt5-range tree 25 65)
          ;; Range [10, 30] should give (10 20 30)
          (funcall 'neovm--bt5-range tree 10 30)
          ;; Range [90, 100] should give (90 100)
          (funcall 'neovm--bt5-range tree 90 100)
          ;; Range [1, 5] should give nil (all below min)
          (funcall 'neovm--bt5-range tree 1 5)
          ;; Range [105, 200] should give nil (all above max)
          (funcall 'neovm--bt5-range tree 105 200)
          ;; Range [50, 50] should give (50) (single element)
          (funcall 'neovm--bt5-range tree 50 50)
          ;; Full range
          (funcall 'neovm--bt5-range tree 1 200)))
    (fmakunbound 'neovm--bt5-make-leaf)
    (fmakunbound 'neovm--bt5-make-node)
    (fmakunbound 'neovm--bt5-keys)
    (fmakunbound 'neovm--bt5-children)
    (fmakunbound 'neovm--bt5-leaf-p)
    (fmakunbound 'neovm--bt5-sorted-insert)
    (fmakunbound 'neovm--bt5-find-idx)
    (fmakunbound 'neovm--bt5-lset)
    (fmakunbound 'neovm--bt5-lins)
    (fmakunbound 'neovm--bt5-ins-impl)
    (fmakunbound 'neovm--bt5-insert)
    (fmakunbound 'neovm--bt5-range)))"#;
    let expect = expect_test::expect![[
        r#""OK ((30 40 50 60) (10 20 30) (90 100) nil nil (50) (10 20 30 40 50 60 70 80 90 100))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// B-tree height and structure verification
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_btree_height_and_structure() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--bt6-make-leaf (lambda (keys) (list keys nil t)))
  (fset 'neovm--bt6-make-node (lambda (keys children leaf-p) (list keys children leaf-p)))
  (fset 'neovm--bt6-keys (lambda (n) (car n)))
  (fset 'neovm--bt6-children (lambda (n) (cadr n)))
  (fset 'neovm--bt6-leaf-p (lambda (n) (caddr n)))
  (fset 'neovm--bt6-sorted-insert
    (lambda (lst val)
      (cond ((null lst) (list val)) ((<= val (car lst)) (cons val lst))
            (t (cons (car lst) (funcall 'neovm--bt6-sorted-insert (cdr lst) val))))))
  (fset 'neovm--bt6-find-idx
    (lambda (keys val) (let ((idx 0)) (dolist (k keys) (when (>= val k) (setq idx (1+ idx)))) idx)))
  (fset 'neovm--bt6-lset
    (lambda (l i v) (if (= i 0) (cons v (cdr l)) (cons (car l) (funcall 'neovm--bt6-lset (cdr l) (1- i) v)))))
  (fset 'neovm--bt6-lins
    (lambda (l i v) (if (= i 0) (cons v l) (cons (car l) (funcall 'neovm--bt6-lins (cdr l) (1- i) v)))))
  (fset 'neovm--bt6-ins-impl
    (lambda (node key)
      (if (funcall 'neovm--bt6-leaf-p node)
          (let ((nk (funcall 'neovm--bt6-sorted-insert (funcall 'neovm--bt6-keys node) key)))
            (if (<= (length nk) 2) (list (funcall 'neovm--bt6-make-leaf nk))
              (list (cadr nk) (funcall 'neovm--bt6-make-leaf (list (car nk)))
                    (funcall 'neovm--bt6-make-leaf (list (caddr nk))))))
        (let* ((keys (funcall 'neovm--bt6-keys node))
               (ch (funcall 'neovm--bt6-children node))
               (idx (funcall 'neovm--bt6-find-idx keys key))
               (res (funcall 'neovm--bt6-ins-impl (nth idx ch) key)))
          (if (= (length res) 1)
              (list (funcall 'neovm--bt6-make-node keys (funcall 'neovm--bt6-lset ch idx (car res)) nil))
            (let* ((nk (funcall 'neovm--bt6-sorted-insert keys (car res)))
                   (nc (funcall 'neovm--bt6-lins (funcall 'neovm--bt6-lset ch idx (cadr res)) (1+ idx) (caddr res))))
              (if (<= (length nk) 2)
                  (list (funcall 'neovm--bt6-make-node nk nc nil))
                (list (cadr nk)
                      (funcall 'neovm--bt6-make-node (list (car nk)) (list (car nc) (cadr nc)) nil)
                      (funcall 'neovm--bt6-make-node (list (caddr nk)) (list (caddr nc) (cadddr nc)) nil)))))))))
  (fset 'neovm--bt6-insert
    (lambda (root key)
      (if (null root) (funcall 'neovm--bt6-make-leaf (list key))
        (let ((res (funcall 'neovm--bt6-ins-impl root key)))
          (if (= (length res) 1) (car res)
            (funcall 'neovm--bt6-make-node (list (car res)) (list (cadr res) (caddr res)) nil))))))

  ;; Height of tree
  (fset 'neovm--bt6-height
    (lambda (node)
      (if (null node) 0
        (if (funcall 'neovm--bt6-leaf-p node) 1
          (1+ (funcall 'neovm--bt6-height (car (funcall 'neovm--bt6-children node))))))))

  ;; Count total number of keys in tree
  (fset 'neovm--bt6-count
    (lambda (node)
      (if (null node) 0
        (if (funcall 'neovm--bt6-leaf-p node)
            (length (funcall 'neovm--bt6-keys node))
          (let ((total (length (funcall 'neovm--bt6-keys node))))
            (dolist (ch (funcall 'neovm--bt6-children node))
              (setq total (+ total (funcall 'neovm--bt6-count ch))))
            total)))))

  ;; Verify B-tree invariant: all leaves at same depth
  (fset 'neovm--bt6-all-leaf-depths
    (lambda (node depth)
      (if (null node) nil
        (if (funcall 'neovm--bt6-leaf-p node)
            (list depth)
          (let ((depths nil))
            (dolist (ch (funcall 'neovm--bt6-children node))
              (setq depths (append depths (funcall 'neovm--bt6-all-leaf-depths ch (1+ depth)))))
            depths)))))

  (unwind-protect
      (let ((tree nil)
            (heights nil)
            (counts nil))
        ;; Insert 1 through 15 and track metrics
        (dotimes (i 15)
          (let ((k (1+ i)))
            (setq tree (funcall 'neovm--bt6-insert tree k))
            (setq heights (cons (funcall 'neovm--bt6-height tree) heights))
            (setq counts (cons (funcall 'neovm--bt6-count tree) counts))))
        (let ((leaf-depths (funcall 'neovm--bt6-all-leaf-depths tree 0)))
          (list
            ;; Heights should be non-decreasing
            (nreverse heights)
            ;; Counts should be 1..15
            (nreverse counts)
            ;; All leaves at same depth (B-tree invariant)
            (apply 'min leaf-depths)
            (apply 'max leaf-depths)
            (= (apply 'min leaf-depths) (apply 'max leaf-depths))
            ;; Root keys
            (funcall 'neovm--bt6-keys tree)
            ;; Root is not a leaf (we have 15 elements)
            (not (funcall 'neovm--bt6-leaf-p tree)))))
    (fmakunbound 'neovm--bt6-make-leaf)
    (fmakunbound 'neovm--bt6-make-node)
    (fmakunbound 'neovm--bt6-keys)
    (fmakunbound 'neovm--bt6-children)
    (fmakunbound 'neovm--bt6-leaf-p)
    (fmakunbound 'neovm--bt6-sorted-insert)
    (fmakunbound 'neovm--bt6-find-idx)
    (fmakunbound 'neovm--bt6-lset)
    (fmakunbound 'neovm--bt6-lins)
    (fmakunbound 'neovm--bt6-ins-impl)
    (fmakunbound 'neovm--bt6-insert)
    (fmakunbound 'neovm--bt6-height)
    (fmakunbound 'neovm--bt6-count)
    (fmakunbound 'neovm--bt6-all-leaf-depths)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 1 2 2 2 2 3 3 3 3 3 3 3 3 4) (1 2 3 4 5 6 7 8 9 10 11 12 13 14 15) 3 3 t (8) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
