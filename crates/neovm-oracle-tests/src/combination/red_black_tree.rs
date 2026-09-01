//! Oracle parity tests for a red-black tree implemented in Elisp.
//!
//! Implements nodes with color (red/black), insert with rotations and
//! fix-up (rebalancing to maintain RB invariants), search, in-order
//! traversal, and verification of RB-tree properties: root is black,
//! no red-red parent-child, and equal black-height on all paths.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// RB-tree core: node representation, rotations, insert with fix-up
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rbtree_insert_and_fixup() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Node: (key color left right) where color is 'red or 'black, nil = black leaf
    // Insert uses standard left-leaning red-black tree algorithm (LLRB)
    let form = r#"(progn
  ;; Node accessors
  (fset 'neovm--rb-key (lambda (n) (car n)))
  (fset 'neovm--rb-color (lambda (n) (if n (cadr n) 'black)))
  (fset 'neovm--rb-left (lambda (n) (caddr n)))
  (fset 'neovm--rb-right (lambda (n) (cadddr n)))

  ;; Constructor
  (fset 'neovm--rb-node
    (lambda (key color left right)
      (list key color left right)))

  ;; Color predicates
  (fset 'neovm--rb-red-p
    (lambda (n) (and n (eq (funcall 'neovm--rb-color n) 'red))))

  ;; Rotations
  (fset 'neovm--rb-rotate-left
    (lambda (h)
      "Rotate left: h's right child becomes parent."
      (let ((x (funcall 'neovm--rb-right h)))
        (funcall 'neovm--rb-node
                 (funcall 'neovm--rb-key x)
                 (funcall 'neovm--rb-color h)
                 (funcall 'neovm--rb-node
                          (funcall 'neovm--rb-key h)
                          'red
                          (funcall 'neovm--rb-left h)
                          (funcall 'neovm--rb-left x))
                 (funcall 'neovm--rb-right x)))))

  (fset 'neovm--rb-rotate-right
    (lambda (h)
      "Rotate right: h's left child becomes parent."
      (let ((x (funcall 'neovm--rb-left h)))
        (funcall 'neovm--rb-node
                 (funcall 'neovm--rb-key x)
                 (funcall 'neovm--rb-color h)
                 (funcall 'neovm--rb-left x)
                 (funcall 'neovm--rb-node
                          (funcall 'neovm--rb-key h)
                          'red
                          (funcall 'neovm--rb-right x)
                          (funcall 'neovm--rb-right h))))))

  ;; Flip colors: parent becomes red, children become black (or vice versa)
  (fset 'neovm--rb-flip-colors
    (lambda (h)
      (let ((new-color (if (eq (funcall 'neovm--rb-color h) 'red) 'black 'red)))
        (let ((left-color (if (eq new-color 'red) 'black 'red))
              (right-color (if (eq new-color 'red) 'black 'red)))
          (funcall 'neovm--rb-node
                   (funcall 'neovm--rb-key h)
                   new-color
                   (if (funcall 'neovm--rb-left h)
                       (funcall 'neovm--rb-node
                                (funcall 'neovm--rb-key (funcall 'neovm--rb-left h))
                                left-color
                                (funcall 'neovm--rb-left (funcall 'neovm--rb-left h))
                                (funcall 'neovm--rb-right (funcall 'neovm--rb-left h)))
                     nil)
                   (if (funcall 'neovm--rb-right h)
                       (funcall 'neovm--rb-node
                                (funcall 'neovm--rb-key (funcall 'neovm--rb-right h))
                                right-color
                                (funcall 'neovm--rb-left (funcall 'neovm--rb-right h))
                                (funcall 'neovm--rb-right (funcall 'neovm--rb-right h)))
                     nil))))))

  ;; LLRB insert helper
  (fset 'neovm--rb-insert-rec
    (lambda (h key)
      (if (null h)
          ;; New node is always red
          (funcall 'neovm--rb-node key 'red nil nil)
        (let ((cmp (cond ((< key (funcall 'neovm--rb-key h)) -1)
                         ((> key (funcall 'neovm--rb-key h)) 1)
                         (t 0))))
          (let ((result
                 (cond
                   ((= cmp 0) h)  ;; duplicate, no insert
                   ((< cmp 0)
                    (funcall 'neovm--rb-node
                             (funcall 'neovm--rb-key h)
                             (funcall 'neovm--rb-color h)
                             (funcall 'neovm--rb-insert-rec
                                      (funcall 'neovm--rb-left h) key)
                             (funcall 'neovm--rb-right h)))
                   (t
                    (funcall 'neovm--rb-node
                             (funcall 'neovm--rb-key h)
                             (funcall 'neovm--rb-color h)
                             (funcall 'neovm--rb-left h)
                             (funcall 'neovm--rb-insert-rec
                                      (funcall 'neovm--rb-right h) key))))))
            ;; Fix-up: LLRB invariant enforcement
            ;; 1. Right-leaning red link -> rotate left
            (let ((r result))
              (when (and (funcall 'neovm--rb-red-p (funcall 'neovm--rb-right r))
                         (not (funcall 'neovm--rb-red-p (funcall 'neovm--rb-left r))))
                (setq r (funcall 'neovm--rb-rotate-left r)))
              ;; 2. Two consecutive left red links -> rotate right
              (when (and (funcall 'neovm--rb-red-p (funcall 'neovm--rb-left r))
                         (funcall 'neovm--rb-red-p
                                  (funcall 'neovm--rb-left (funcall 'neovm--rb-left r))))
                (setq r (funcall 'neovm--rb-rotate-right r)))
              ;; 3. Both children red -> flip colors
              (when (and (funcall 'neovm--rb-red-p (funcall 'neovm--rb-left r))
                         (funcall 'neovm--rb-red-p (funcall 'neovm--rb-right r)))
                (setq r (funcall 'neovm--rb-flip-colors r)))
              r))))))

  ;; Public insert: insert then make root black
  (fset 'neovm--rb-insert
    (lambda (tree key)
      (let ((result (funcall 'neovm--rb-insert-rec tree key)))
        (funcall 'neovm--rb-node
                 (funcall 'neovm--rb-key result)
                 'black
                 (funcall 'neovm--rb-left result)
                 (funcall 'neovm--rb-right result)))))

  ;; In-order traversal
  (fset 'neovm--rb-inorder
    (lambda (tree)
      (if (null tree) nil
        (append (funcall 'neovm--rb-inorder (funcall 'neovm--rb-left tree))
                (list (funcall 'neovm--rb-key tree))
                (funcall 'neovm--rb-inorder (funcall 'neovm--rb-right tree))))))

  (unwind-protect
      (let ((tree nil))
        ;; Insert: 7 2 11 1 5 8 14 4
        (setq tree (funcall 'neovm--rb-insert tree 7))
        (setq tree (funcall 'neovm--rb-insert tree 2))
        (setq tree (funcall 'neovm--rb-insert tree 11))
        (setq tree (funcall 'neovm--rb-insert tree 1))
        (setq tree (funcall 'neovm--rb-insert tree 5))
        (setq tree (funcall 'neovm--rb-insert tree 8))
        (setq tree (funcall 'neovm--rb-insert tree 14))
        (setq tree (funcall 'neovm--rb-insert tree 4))
        (list
          ;; In-order should be sorted
          (funcall 'neovm--rb-inorder tree)
          ;; Root must be black
          (funcall 'neovm--rb-color tree)
          ;; Root key
          (funcall 'neovm--rb-key tree)
          ;; Duplicate insert should not change tree
          (let ((tree2 (funcall 'neovm--rb-insert tree 5)))
            (funcall 'neovm--rb-inorder tree2))))
    (fmakunbound 'neovm--rb-key)
    (fmakunbound 'neovm--rb-color)
    (fmakunbound 'neovm--rb-left)
    (fmakunbound 'neovm--rb-right)
    (fmakunbound 'neovm--rb-node)
    (fmakunbound 'neovm--rb-red-p)
    (fmakunbound 'neovm--rb-rotate-left)
    (fmakunbound 'neovm--rb-rotate-right)
    (fmakunbound 'neovm--rb-flip-colors)
    (fmakunbound 'neovm--rb-insert-rec)
    (fmakunbound 'neovm--rb-insert)
    (fmakunbound 'neovm--rb-inorder)))"#;
    let expect =
        expect_test::expect![[r#""OK ((1 2 4 5 7 8 11 14) black 7 (1 2 4 5 7 8 11 14))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// RB-tree search and membership
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rbtree_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--rb-key (lambda (n) (car n)))
  (fset 'neovm--rb-color (lambda (n) (if n (cadr n) 'black)))
  (fset 'neovm--rb-left (lambda (n) (caddr n)))
  (fset 'neovm--rb-right (lambda (n) (cadddr n)))
  (fset 'neovm--rb-node (lambda (key color left right) (list key color left right)))
  (fset 'neovm--rb-red-p (lambda (n) (and n (eq (funcall 'neovm--rb-color n) 'red))))

  (fset 'neovm--rb-rotate-left
    (lambda (h)
      (let ((x (funcall 'neovm--rb-right h)))
        (funcall 'neovm--rb-node (funcall 'neovm--rb-key x) (funcall 'neovm--rb-color h)
                 (funcall 'neovm--rb-node (funcall 'neovm--rb-key h) 'red
                          (funcall 'neovm--rb-left h) (funcall 'neovm--rb-left x))
                 (funcall 'neovm--rb-right x)))))

  (fset 'neovm--rb-rotate-right
    (lambda (h)
      (let ((x (funcall 'neovm--rb-left h)))
        (funcall 'neovm--rb-node (funcall 'neovm--rb-key x) (funcall 'neovm--rb-color h)
                 (funcall 'neovm--rb-left x)
                 (funcall 'neovm--rb-node (funcall 'neovm--rb-key h) 'red
                          (funcall 'neovm--rb-right x) (funcall 'neovm--rb-right h))))))

  (fset 'neovm--rb-flip-colors
    (lambda (h)
      (let ((nc (if (eq (funcall 'neovm--rb-color h) 'red) 'black 'red))
            (lc (if (eq (funcall 'neovm--rb-color h) 'red) 'red 'black)))
        (funcall 'neovm--rb-node (funcall 'neovm--rb-key h) nc
                 (if (funcall 'neovm--rb-left h)
                     (funcall 'neovm--rb-node (funcall 'neovm--rb-key (funcall 'neovm--rb-left h))
                              lc (funcall 'neovm--rb-left (funcall 'neovm--rb-left h))
                              (funcall 'neovm--rb-right (funcall 'neovm--rb-left h))) nil)
                 (if (funcall 'neovm--rb-right h)
                     (funcall 'neovm--rb-node (funcall 'neovm--rb-key (funcall 'neovm--rb-right h))
                              lc (funcall 'neovm--rb-left (funcall 'neovm--rb-right h))
                              (funcall 'neovm--rb-right (funcall 'neovm--rb-right h))) nil)))))

  (fset 'neovm--rb-insert-rec
    (lambda (h key)
      (if (null h)
          (funcall 'neovm--rb-node key 'red nil nil)
        (let ((cmp (cond ((< key (funcall 'neovm--rb-key h)) -1)
                         ((> key (funcall 'neovm--rb-key h)) 1) (t 0))))
          (let ((r (cond
                     ((= cmp 0) h)
                     ((< cmp 0) (funcall 'neovm--rb-node (funcall 'neovm--rb-key h)
                                         (funcall 'neovm--rb-color h)
                                         (funcall 'neovm--rb-insert-rec (funcall 'neovm--rb-left h) key)
                                         (funcall 'neovm--rb-right h)))
                     (t (funcall 'neovm--rb-node (funcall 'neovm--rb-key h)
                                 (funcall 'neovm--rb-color h)
                                 (funcall 'neovm--rb-left h)
                                 (funcall 'neovm--rb-insert-rec (funcall 'neovm--rb-right h) key))))))
            (when (and (funcall 'neovm--rb-red-p (funcall 'neovm--rb-right r))
                       (not (funcall 'neovm--rb-red-p (funcall 'neovm--rb-left r))))
              (setq r (funcall 'neovm--rb-rotate-left r)))
            (when (and (funcall 'neovm--rb-red-p (funcall 'neovm--rb-left r))
                       (funcall 'neovm--rb-red-p (funcall 'neovm--rb-left (funcall 'neovm--rb-left r))))
              (setq r (funcall 'neovm--rb-rotate-right r)))
            (when (and (funcall 'neovm--rb-red-p (funcall 'neovm--rb-left r))
                       (funcall 'neovm--rb-red-p (funcall 'neovm--rb-right r)))
              (setq r (funcall 'neovm--rb-flip-colors r)))
            r)))))

  (fset 'neovm--rb-insert
    (lambda (tree key)
      (let ((r (funcall 'neovm--rb-insert-rec tree key)))
        (funcall 'neovm--rb-node (funcall 'neovm--rb-key r) 'black
                 (funcall 'neovm--rb-left r) (funcall 'neovm--rb-right r)))))

  ;; Search
  (fset 'neovm--rb-search
    (lambda (tree key)
      "Search for KEY in RB tree. Return t if found, nil otherwise."
      (if (null tree) nil
        (let ((k (funcall 'neovm--rb-key tree)))
          (cond ((= key k) t)
                ((< key k) (funcall 'neovm--rb-search (funcall 'neovm--rb-left tree) key))
                (t (funcall 'neovm--rb-search (funcall 'neovm--rb-right tree) key)))))))

  ;; Min and Max
  (fset 'neovm--rb-min
    (lambda (tree)
      (if (null tree) nil
        (if (null (funcall 'neovm--rb-left tree))
            (funcall 'neovm--rb-key tree)
          (funcall 'neovm--rb-min (funcall 'neovm--rb-left tree))))))

  (fset 'neovm--rb-max
    (lambda (tree)
      (if (null tree) nil
        (if (null (funcall 'neovm--rb-right tree))
            (funcall 'neovm--rb-key tree)
          (funcall 'neovm--rb-max (funcall 'neovm--rb-right tree))))))

  ;; Size
  (fset 'neovm--rb-size
    (lambda (tree)
      (if (null tree) 0
        (+ 1 (funcall 'neovm--rb-size (funcall 'neovm--rb-left tree))
           (funcall 'neovm--rb-size (funcall 'neovm--rb-right tree))))))

  (unwind-protect
      (let ((tree nil))
        (dolist (k '(50 25 75 12 37 62 87 6 18 31 43 56 68 81 93))
          (setq tree (funcall 'neovm--rb-insert tree k)))
        (list
          ;; Search for present keys
          (mapcar (lambda (k) (funcall 'neovm--rb-search tree k))
                  '(50 25 75 6 93 43 68))
          ;; Search for absent keys
          (mapcar (lambda (k) (funcall 'neovm--rb-search tree k))
                  '(0 100 44 51 99 1))
          ;; Min, Max, Size
          (funcall 'neovm--rb-min tree)
          (funcall 'neovm--rb-max tree)
          (funcall 'neovm--rb-size tree)
          ;; Search empty tree
          (funcall 'neovm--rb-search nil 42)))
    (fmakunbound 'neovm--rb-key)
    (fmakunbound 'neovm--rb-color)
    (fmakunbound 'neovm--rb-left)
    (fmakunbound 'neovm--rb-right)
    (fmakunbound 'neovm--rb-node)
    (fmakunbound 'neovm--rb-red-p)
    (fmakunbound 'neovm--rb-rotate-left)
    (fmakunbound 'neovm--rb-rotate-right)
    (fmakunbound 'neovm--rb-flip-colors)
    (fmakunbound 'neovm--rb-insert-rec)
    (fmakunbound 'neovm--rb-insert)
    (fmakunbound 'neovm--rb-search)
    (fmakunbound 'neovm--rb-min)
    (fmakunbound 'neovm--rb-max)
    (fmakunbound 'neovm--rb-size)))"#;
    let expect =
        expect_test::expect![[r#""OK ((t t t t t t t) (nil nil nil nil nil nil) 6 93 15 nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Verify RB-tree properties
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rbtree_verify_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify: root is black, no red-red, equal black-height on all paths
    let form = r#"(progn
  (fset 'neovm--rb-key (lambda (n) (car n)))
  (fset 'neovm--rb-color (lambda (n) (if n (cadr n) 'black)))
  (fset 'neovm--rb-left (lambda (n) (caddr n)))
  (fset 'neovm--rb-right (lambda (n) (cadddr n)))
  (fset 'neovm--rb-node (lambda (key color left right) (list key color left right)))
  (fset 'neovm--rb-red-p (lambda (n) (and n (eq (funcall 'neovm--rb-color n) 'red))))

  (fset 'neovm--rb-rotate-left
    (lambda (h)
      (let ((x (funcall 'neovm--rb-right h)))
        (funcall 'neovm--rb-node (funcall 'neovm--rb-key x) (funcall 'neovm--rb-color h)
                 (funcall 'neovm--rb-node (funcall 'neovm--rb-key h) 'red
                          (funcall 'neovm--rb-left h) (funcall 'neovm--rb-left x))
                 (funcall 'neovm--rb-right x)))))

  (fset 'neovm--rb-rotate-right
    (lambda (h)
      (let ((x (funcall 'neovm--rb-left h)))
        (funcall 'neovm--rb-node (funcall 'neovm--rb-key x) (funcall 'neovm--rb-color h)
                 (funcall 'neovm--rb-left x)
                 (funcall 'neovm--rb-node (funcall 'neovm--rb-key h) 'red
                          (funcall 'neovm--rb-right x) (funcall 'neovm--rb-right h))))))

  (fset 'neovm--rb-flip-colors
    (lambda (h)
      (let ((nc (if (eq (funcall 'neovm--rb-color h) 'red) 'black 'red))
            (lc (if (eq (funcall 'neovm--rb-color h) 'red) 'red 'black)))
        (funcall 'neovm--rb-node (funcall 'neovm--rb-key h) nc
                 (if (funcall 'neovm--rb-left h)
                     (funcall 'neovm--rb-node (funcall 'neovm--rb-key (funcall 'neovm--rb-left h))
                              lc (funcall 'neovm--rb-left (funcall 'neovm--rb-left h))
                              (funcall 'neovm--rb-right (funcall 'neovm--rb-left h))) nil)
                 (if (funcall 'neovm--rb-right h)
                     (funcall 'neovm--rb-node (funcall 'neovm--rb-key (funcall 'neovm--rb-right h))
                              lc (funcall 'neovm--rb-left (funcall 'neovm--rb-right h))
                              (funcall 'neovm--rb-right (funcall 'neovm--rb-right h))) nil)))))

  (fset 'neovm--rb-insert-rec
    (lambda (h key)
      (if (null h) (funcall 'neovm--rb-node key 'red nil nil)
        (let ((cmp (cond ((< key (funcall 'neovm--rb-key h)) -1)
                         ((> key (funcall 'neovm--rb-key h)) 1) (t 0))))
          (let ((r (cond ((= cmp 0) h)
                         ((< cmp 0) (funcall 'neovm--rb-node (funcall 'neovm--rb-key h)
                                             (funcall 'neovm--rb-color h)
                                             (funcall 'neovm--rb-insert-rec (funcall 'neovm--rb-left h) key)
                                             (funcall 'neovm--rb-right h)))
                         (t (funcall 'neovm--rb-node (funcall 'neovm--rb-key h)
                                     (funcall 'neovm--rb-color h) (funcall 'neovm--rb-left h)
                                     (funcall 'neovm--rb-insert-rec (funcall 'neovm--rb-right h) key))))))
            (when (and (funcall 'neovm--rb-red-p (funcall 'neovm--rb-right r))
                       (not (funcall 'neovm--rb-red-p (funcall 'neovm--rb-left r))))
              (setq r (funcall 'neovm--rb-rotate-left r)))
            (when (and (funcall 'neovm--rb-red-p (funcall 'neovm--rb-left r))
                       (funcall 'neovm--rb-red-p (funcall 'neovm--rb-left (funcall 'neovm--rb-left r))))
              (setq r (funcall 'neovm--rb-rotate-right r)))
            (when (and (funcall 'neovm--rb-red-p (funcall 'neovm--rb-left r))
                       (funcall 'neovm--rb-red-p (funcall 'neovm--rb-right r)))
              (setq r (funcall 'neovm--rb-flip-colors r)))
            r)))))

  (fset 'neovm--rb-insert
    (lambda (tree key)
      (let ((r (funcall 'neovm--rb-insert-rec tree key)))
        (funcall 'neovm--rb-node (funcall 'neovm--rb-key r) 'black
                 (funcall 'neovm--rb-left r) (funcall 'neovm--rb-right r)))))

  (fset 'neovm--rb-inorder
    (lambda (tree)
      (if (null tree) nil
        (append (funcall 'neovm--rb-inorder (funcall 'neovm--rb-left tree))
                (list (funcall 'neovm--rb-key tree))
                (funcall 'neovm--rb-inorder (funcall 'neovm--rb-right tree))))))

  ;; Property 1: Root is black
  (fset 'neovm--rb-prop-root-black
    (lambda (tree)
      (or (null tree) (eq (funcall 'neovm--rb-color tree) 'black))))

  ;; Property 2: No red node has a red child
  (fset 'neovm--rb-prop-no-red-red
    (lambda (tree)
      (if (null tree) t
        (if (funcall 'neovm--rb-red-p tree)
            (and (not (funcall 'neovm--rb-red-p (funcall 'neovm--rb-left tree)))
                 (not (funcall 'neovm--rb-red-p (funcall 'neovm--rb-right tree)))
                 (funcall 'neovm--rb-prop-no-red-red (funcall 'neovm--rb-left tree))
                 (funcall 'neovm--rb-prop-no-red-red (funcall 'neovm--rb-right tree)))
          (and (funcall 'neovm--rb-prop-no-red-red (funcall 'neovm--rb-left tree))
               (funcall 'neovm--rb-prop-no-red-red (funcall 'neovm--rb-right tree)))))))

  ;; Property 3: All paths from root to nil have same number of black nodes
  (fset 'neovm--rb-black-height
    (lambda (tree)
      "Return black height or -1 if inconsistent."
      (if (null tree) 0
        (let ((lh (funcall 'neovm--rb-black-height (funcall 'neovm--rb-left tree)))
              (rh (funcall 'neovm--rb-black-height (funcall 'neovm--rb-right tree))))
          (if (or (= lh -1) (= rh -1) (/= lh rh))
              -1
            (+ lh (if (eq (funcall 'neovm--rb-color tree) 'black) 1 0)))))))

  ;; Property 4: BST ordering
  (fset 'neovm--rb-prop-bst
    (lambda (tree)
      (let ((inorder (funcall 'neovm--rb-inorder tree)))
        (let ((sorted t) (prev nil))
          (dolist (k inorder)
            (when (and prev (>= prev k))
              (setq sorted nil))
            (setq prev k))
          sorted))))

  ;; Verify all properties
  (fset 'neovm--rb-verify
    (lambda (tree)
      (list
        (funcall 'neovm--rb-prop-root-black tree)
        (funcall 'neovm--rb-prop-no-red-red tree)
        (let ((bh (funcall 'neovm--rb-black-height tree)))
          (list (>= bh 0) bh))
        (funcall 'neovm--rb-prop-bst tree))))

  (unwind-protect
      (list
        ;; Test 1: Sequential insertion (worst case for BST)
        (let ((tree nil))
          (dolist (k '(1 2 3 4 5 6 7 8 9 10))
            (setq tree (funcall 'neovm--rb-insert tree k)))
          (list (funcall 'neovm--rb-verify tree)
                (funcall 'neovm--rb-inorder tree)))

        ;; Test 2: Reverse sequential insertion
        (let ((tree nil))
          (dolist (k '(10 9 8 7 6 5 4 3 2 1))
            (setq tree (funcall 'neovm--rb-insert tree k)))
          (list (funcall 'neovm--rb-verify tree)
                (funcall 'neovm--rb-inorder tree)))

        ;; Test 3: Random-ish insertion order
        (let ((tree nil))
          (dolist (k '(42 17 93 8 56 71 33 25 88 3 64 49 12 77 61))
            (setq tree (funcall 'neovm--rb-insert tree k)))
          (list (funcall 'neovm--rb-verify tree)
                (funcall 'neovm--rb-inorder tree)))

        ;; Test 4: Single element
        (let ((tree (funcall 'neovm--rb-insert nil 42)))
          (funcall 'neovm--rb-verify tree))

        ;; Test 5: Empty tree
        (funcall 'neovm--rb-verify nil)

        ;; Test 6: Duplicates should not break invariants
        (let ((tree nil))
          (dolist (k '(5 3 7 3 5 7 1 1 9 9))
            (setq tree (funcall 'neovm--rb-insert tree k)))
          (list (funcall 'neovm--rb-verify tree)
                (funcall 'neovm--rb-inorder tree))))
    (fmakunbound 'neovm--rb-key)
    (fmakunbound 'neovm--rb-color)
    (fmakunbound 'neovm--rb-left)
    (fmakunbound 'neovm--rb-right)
    (fmakunbound 'neovm--rb-node)
    (fmakunbound 'neovm--rb-red-p)
    (fmakunbound 'neovm--rb-rotate-left)
    (fmakunbound 'neovm--rb-rotate-right)
    (fmakunbound 'neovm--rb-flip-colors)
    (fmakunbound 'neovm--rb-insert-rec)
    (fmakunbound 'neovm--rb-insert)
    (fmakunbound 'neovm--rb-inorder)
    (fmakunbound 'neovm--rb-prop-root-black)
    (fmakunbound 'neovm--rb-prop-no-red-red)
    (fmakunbound 'neovm--rb-black-height)
    (fmakunbound 'neovm--rb-prop-bst)
    (fmakunbound 'neovm--rb-verify)))"#;
    let expect = expect_test::expect![[
        r#""OK (((t t (t 3) t) (1 2 3 4 5 6 7 8 9 10)) ((t t (t 3) t) (1 2 3 4 5 6 7 8 9 10)) ((t t (t 3) t) (3 8 12 17 25 33 42 49 56 61 64 71 77 88 93)) (t t (t 1) t) (t t (t 0) t) ((t t (t 2) t) (1 3 5 7 9)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// RB-tree: level-order traversal and tree structure visualization
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rbtree_level_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--rb-key (lambda (n) (car n)))
  (fset 'neovm--rb-color (lambda (n) (if n (cadr n) 'black)))
  (fset 'neovm--rb-left (lambda (n) (caddr n)))
  (fset 'neovm--rb-right (lambda (n) (cadddr n)))
  (fset 'neovm--rb-node (lambda (key color left right) (list key color left right)))
  (fset 'neovm--rb-red-p (lambda (n) (and n (eq (funcall 'neovm--rb-color n) 'red))))

  (fset 'neovm--rb-rotate-left
    (lambda (h)
      (let ((x (funcall 'neovm--rb-right h)))
        (funcall 'neovm--rb-node (funcall 'neovm--rb-key x) (funcall 'neovm--rb-color h)
                 (funcall 'neovm--rb-node (funcall 'neovm--rb-key h) 'red
                          (funcall 'neovm--rb-left h) (funcall 'neovm--rb-left x))
                 (funcall 'neovm--rb-right x)))))
  (fset 'neovm--rb-rotate-right
    (lambda (h)
      (let ((x (funcall 'neovm--rb-left h)))
        (funcall 'neovm--rb-node (funcall 'neovm--rb-key x) (funcall 'neovm--rb-color h)
                 (funcall 'neovm--rb-left x)
                 (funcall 'neovm--rb-node (funcall 'neovm--rb-key h) 'red
                          (funcall 'neovm--rb-right x) (funcall 'neovm--rb-right h))))))
  (fset 'neovm--rb-flip-colors
    (lambda (h)
      (let ((nc (if (eq (funcall 'neovm--rb-color h) 'red) 'black 'red))
            (lc (if (eq (funcall 'neovm--rb-color h) 'red) 'red 'black)))
        (funcall 'neovm--rb-node (funcall 'neovm--rb-key h) nc
                 (if (funcall 'neovm--rb-left h)
                     (funcall 'neovm--rb-node (funcall 'neovm--rb-key (funcall 'neovm--rb-left h))
                              lc (funcall 'neovm--rb-left (funcall 'neovm--rb-left h))
                              (funcall 'neovm--rb-right (funcall 'neovm--rb-left h))) nil)
                 (if (funcall 'neovm--rb-right h)
                     (funcall 'neovm--rb-node (funcall 'neovm--rb-key (funcall 'neovm--rb-right h))
                              lc (funcall 'neovm--rb-left (funcall 'neovm--rb-right h))
                              (funcall 'neovm--rb-right (funcall 'neovm--rb-right h))) nil)))))
  (fset 'neovm--rb-insert-rec
    (lambda (h key)
      (if (null h) (funcall 'neovm--rb-node key 'red nil nil)
        (let ((cmp (cond ((< key (funcall 'neovm--rb-key h)) -1)
                         ((> key (funcall 'neovm--rb-key h)) 1) (t 0))))
          (let ((r (cond ((= cmp 0) h)
                         ((< cmp 0) (funcall 'neovm--rb-node (funcall 'neovm--rb-key h)
                                             (funcall 'neovm--rb-color h)
                                             (funcall 'neovm--rb-insert-rec (funcall 'neovm--rb-left h) key)
                                             (funcall 'neovm--rb-right h)))
                         (t (funcall 'neovm--rb-node (funcall 'neovm--rb-key h)
                                     (funcall 'neovm--rb-color h) (funcall 'neovm--rb-left h)
                                     (funcall 'neovm--rb-insert-rec (funcall 'neovm--rb-right h) key))))))
            (when (and (funcall 'neovm--rb-red-p (funcall 'neovm--rb-right r))
                       (not (funcall 'neovm--rb-red-p (funcall 'neovm--rb-left r))))
              (setq r (funcall 'neovm--rb-rotate-left r)))
            (when (and (funcall 'neovm--rb-red-p (funcall 'neovm--rb-left r))
                       (funcall 'neovm--rb-red-p (funcall 'neovm--rb-left (funcall 'neovm--rb-left r))))
              (setq r (funcall 'neovm--rb-rotate-right r)))
            (when (and (funcall 'neovm--rb-red-p (funcall 'neovm--rb-left r))
                       (funcall 'neovm--rb-red-p (funcall 'neovm--rb-right r)))
              (setq r (funcall 'neovm--rb-flip-colors r)))
            r)))))
  (fset 'neovm--rb-insert
    (lambda (tree key)
      (let ((r (funcall 'neovm--rb-insert-rec tree key)))
        (funcall 'neovm--rb-node (funcall 'neovm--rb-key r) 'black
                 (funcall 'neovm--rb-left r) (funcall 'neovm--rb-right r)))))

  ;; Level-order (BFS) traversal
  (fset 'neovm--rb-level-order
    (lambda (tree)
      "Return list of levels, each level is list of (key color)."
      (if (null tree) nil
        (let ((queue (list tree))
              (levels nil))
          (while queue
            (let ((level nil)
                  (next-queue nil)
                  (q queue))
              (while q
                (let ((node (car q)))
                  (setq q (cdr q))
                  (when node
                    (setq level (cons (list (funcall 'neovm--rb-key node)
                                           (funcall 'neovm--rb-color node))
                                     level))
                    (when (funcall 'neovm--rb-left node)
                      (setq next-queue (append next-queue (list (funcall 'neovm--rb-left node)))))
                    (when (funcall 'neovm--rb-right node)
                      (setq next-queue (append next-queue (list (funcall 'neovm--rb-right node))))))))
              (when level
                (setq levels (cons (nreverse level) levels)))
              (setq queue next-queue)))
          (nreverse levels)))))

  ;; Height (max depth)
  (fset 'neovm--rb-height
    (lambda (tree)
      (if (null tree) 0
        (1+ (max (funcall 'neovm--rb-height (funcall 'neovm--rb-left tree))
                 (funcall 'neovm--rb-height (funcall 'neovm--rb-right tree)))))))

  ;; Count red and black nodes
  (fset 'neovm--rb-count-colors
    (lambda (tree)
      (if (null tree) '(0 . 0)
        (let ((left (funcall 'neovm--rb-count-colors (funcall 'neovm--rb-left tree)))
              (right (funcall 'neovm--rb-count-colors (funcall 'neovm--rb-right tree))))
          (if (funcall 'neovm--rb-red-p tree)
              (cons (+ 1 (car left) (car right))
                    (+ (cdr left) (cdr right)))
            (cons (+ (car left) (car right))
                  (+ 1 (cdr left) (cdr right))))))))

  (unwind-protect
      (let ((tree nil))
        (dolist (k '(10 20 30 15 25 5 1 8))
          (setq tree (funcall 'neovm--rb-insert tree k)))
        (list
          ;; Level order with colors
          (funcall 'neovm--rb-level-order tree)
          ;; Height
          (funcall 'neovm--rb-height tree)
          ;; Color counts (red . black)
          (funcall 'neovm--rb-count-colors tree)
          ;; Build larger tree and check height bound: h <= 2*log2(n+1)
          (let ((big nil))
            (dolist (k '(50 25 75 12 37 62 87 6 18 31 43 56 68 81 93
                         3 9 15 21 28 34 40 46 53 59 65 71 78 84 90 96))
              (setq big (funcall 'neovm--rb-insert big k)))
            (let ((h (funcall 'neovm--rb-height big))
                  (n 31))
              ;; Height should be reasonable (<=2*log2(32)=10)
              (list h (<= h 10) (funcall 'neovm--rb-count-colors big))))))
    (fmakunbound 'neovm--rb-key)
    (fmakunbound 'neovm--rb-color)
    (fmakunbound 'neovm--rb-left)
    (fmakunbound 'neovm--rb-right)
    (fmakunbound 'neovm--rb-node)
    (fmakunbound 'neovm--rb-red-p)
    (fmakunbound 'neovm--rb-rotate-left)
    (fmakunbound 'neovm--rb-rotate-right)
    (fmakunbound 'neovm--rb-flip-colors)
    (fmakunbound 'neovm--rb-insert-rec)
    (fmakunbound 'neovm--rb-insert)
    (fmakunbound 'neovm--rb-level-order)
    (fmakunbound 'neovm--rb-height)
    (fmakunbound 'neovm--rb-count-colors)))"#;
    let expect = expect_test::expect![[
        r#""OK ((((10 black)) ((5 black) (20 black)) ((1 black) (8 black) (15 black) (30 black)) ((25 red))) 4 (1 . 7) (5 t (0 . 31)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// RB-tree: range queries and floor/ceiling operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rbtree_range_queries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--rb-key (lambda (n) (car n)))
  (fset 'neovm--rb-color (lambda (n) (if n (cadr n) 'black)))
  (fset 'neovm--rb-left (lambda (n) (caddr n)))
  (fset 'neovm--rb-right (lambda (n) (cadddr n)))
  (fset 'neovm--rb-node (lambda (key color left right) (list key color left right)))
  (fset 'neovm--rb-red-p (lambda (n) (and n (eq (funcall 'neovm--rb-color n) 'red))))
  (fset 'neovm--rb-rotate-left
    (lambda (h)
      (let ((x (funcall 'neovm--rb-right h)))
        (funcall 'neovm--rb-node (funcall 'neovm--rb-key x) (funcall 'neovm--rb-color h)
                 (funcall 'neovm--rb-node (funcall 'neovm--rb-key h) 'red
                          (funcall 'neovm--rb-left h) (funcall 'neovm--rb-left x))
                 (funcall 'neovm--rb-right x)))))
  (fset 'neovm--rb-rotate-right
    (lambda (h)
      (let ((x (funcall 'neovm--rb-left h)))
        (funcall 'neovm--rb-node (funcall 'neovm--rb-key x) (funcall 'neovm--rb-color h)
                 (funcall 'neovm--rb-left x)
                 (funcall 'neovm--rb-node (funcall 'neovm--rb-key h) 'red
                          (funcall 'neovm--rb-right x) (funcall 'neovm--rb-right h))))))
  (fset 'neovm--rb-flip-colors
    (lambda (h)
      (let ((nc (if (eq (funcall 'neovm--rb-color h) 'red) 'black 'red))
            (lc (if (eq (funcall 'neovm--rb-color h) 'red) 'red 'black)))
        (funcall 'neovm--rb-node (funcall 'neovm--rb-key h) nc
                 (if (funcall 'neovm--rb-left h)
                     (funcall 'neovm--rb-node (funcall 'neovm--rb-key (funcall 'neovm--rb-left h))
                              lc (funcall 'neovm--rb-left (funcall 'neovm--rb-left h))
                              (funcall 'neovm--rb-right (funcall 'neovm--rb-left h))) nil)
                 (if (funcall 'neovm--rb-right h)
                     (funcall 'neovm--rb-node (funcall 'neovm--rb-key (funcall 'neovm--rb-right h))
                              lc (funcall 'neovm--rb-left (funcall 'neovm--rb-right h))
                              (funcall 'neovm--rb-right (funcall 'neovm--rb-right h))) nil)))))
  (fset 'neovm--rb-insert-rec
    (lambda (h key)
      (if (null h) (funcall 'neovm--rb-node key 'red nil nil)
        (let ((cmp (cond ((< key (funcall 'neovm--rb-key h)) -1)
                         ((> key (funcall 'neovm--rb-key h)) 1) (t 0))))
          (let ((r (cond ((= cmp 0) h)
                         ((< cmp 0) (funcall 'neovm--rb-node (funcall 'neovm--rb-key h)
                                             (funcall 'neovm--rb-color h)
                                             (funcall 'neovm--rb-insert-rec (funcall 'neovm--rb-left h) key)
                                             (funcall 'neovm--rb-right h)))
                         (t (funcall 'neovm--rb-node (funcall 'neovm--rb-key h)
                                     (funcall 'neovm--rb-color h) (funcall 'neovm--rb-left h)
                                     (funcall 'neovm--rb-insert-rec (funcall 'neovm--rb-right h) key))))))
            (when (and (funcall 'neovm--rb-red-p (funcall 'neovm--rb-right r))
                       (not (funcall 'neovm--rb-red-p (funcall 'neovm--rb-left r))))
              (setq r (funcall 'neovm--rb-rotate-left r)))
            (when (and (funcall 'neovm--rb-red-p (funcall 'neovm--rb-left r))
                       (funcall 'neovm--rb-red-p (funcall 'neovm--rb-left (funcall 'neovm--rb-left r))))
              (setq r (funcall 'neovm--rb-rotate-right r)))
            (when (and (funcall 'neovm--rb-red-p (funcall 'neovm--rb-left r))
                       (funcall 'neovm--rb-red-p (funcall 'neovm--rb-right r)))
              (setq r (funcall 'neovm--rb-flip-colors r)))
            r)))))
  (fset 'neovm--rb-insert
    (lambda (tree key)
      (let ((r (funcall 'neovm--rb-insert-rec tree key)))
        (funcall 'neovm--rb-node (funcall 'neovm--rb-key r) 'black
                 (funcall 'neovm--rb-left r) (funcall 'neovm--rb-right r)))))

  ;; Range query: collect all keys in [lo, hi]
  (fset 'neovm--rb-range
    (lambda (tree lo hi)
      (if (null tree) nil
        (let ((k (funcall 'neovm--rb-key tree))
              (result nil))
          (when (> k lo)
            (setq result (funcall 'neovm--rb-range (funcall 'neovm--rb-left tree) lo hi)))
          (when (and (>= k lo) (<= k hi))
            (setq result (append result (list k))))
          (when (< k hi)
            (setq result (append result
                                 (funcall 'neovm--rb-range (funcall 'neovm--rb-right tree) lo hi))))
          result))))

  ;; Floor: largest key <= given key
  (fset 'neovm--rb-floor
    (lambda (tree key)
      (if (null tree) nil
        (let ((k (funcall 'neovm--rb-key tree)))
          (cond ((= key k) k)
                ((< key k) (funcall 'neovm--rb-floor (funcall 'neovm--rb-left tree) key))
                (t (let ((right-floor (funcall 'neovm--rb-floor (funcall 'neovm--rb-right tree) key)))
                     (if right-floor right-floor k))))))))

  ;; Ceiling: smallest key >= given key
  (fset 'neovm--rb-ceiling
    (lambda (tree key)
      (if (null tree) nil
        (let ((k (funcall 'neovm--rb-key tree)))
          (cond ((= key k) k)
                ((> key k) (funcall 'neovm--rb-ceiling (funcall 'neovm--rb-right tree) key))
                (t (let ((left-ceil (funcall 'neovm--rb-ceiling (funcall 'neovm--rb-left tree) key)))
                     (if left-ceil left-ceil k))))))))

  ;; Rank: number of keys less than given key
  (fset 'neovm--rb-size
    (lambda (tree)
      (if (null tree) 0
        (+ 1 (funcall 'neovm--rb-size (funcall 'neovm--rb-left tree))
           (funcall 'neovm--rb-size (funcall 'neovm--rb-right tree))))))

  (fset 'neovm--rb-rank
    (lambda (tree key)
      (if (null tree) 0
        (let ((k (funcall 'neovm--rb-key tree)))
          (cond ((< key k) (funcall 'neovm--rb-rank (funcall 'neovm--rb-left tree) key))
                ((= key k) (funcall 'neovm--rb-size (funcall 'neovm--rb-left tree)))
                (t (+ 1
                      (funcall 'neovm--rb-size (funcall 'neovm--rb-left tree))
                      (funcall 'neovm--rb-rank (funcall 'neovm--rb-right tree) key))))))))

  (unwind-protect
      (let ((tree nil))
        ;; Insert: 20 40 60 80 10 30 50 70 90
        (dolist (k '(20 40 60 80 10 30 50 70 90))
          (setq tree (funcall 'neovm--rb-insert tree k)))
        (list
          ;; Range queries
          (funcall 'neovm--rb-range tree 25 75)
          (funcall 'neovm--rb-range tree 10 90)
          (funcall 'neovm--rb-range tree 1 9)
          (funcall 'neovm--rb-range tree 45 55)
          ;; Floor
          (funcall 'neovm--rb-floor tree 25)   ;; 20
          (funcall 'neovm--rb-floor tree 50)   ;; 50 (exact)
          (funcall 'neovm--rb-floor tree 5)    ;; nil (nothing <=5)
          (funcall 'neovm--rb-floor tree 95)   ;; 90
          ;; Ceiling
          (funcall 'neovm--rb-ceiling tree 25)  ;; 30
          (funcall 'neovm--rb-ceiling tree 50)  ;; 50 (exact)
          (funcall 'neovm--rb-ceiling tree 95)  ;; nil (nothing >=95)
          (funcall 'neovm--rb-ceiling tree 5)   ;; 10
          ;; Rank
          (funcall 'neovm--rb-rank tree 10)   ;; 0 (nothing less)
          (funcall 'neovm--rb-rank tree 50)   ;; 4
          (funcall 'neovm--rb-rank tree 90)   ;; 8
          (funcall 'neovm--rb-rank tree 55))) ;; 5
    (fmakunbound 'neovm--rb-key)
    (fmakunbound 'neovm--rb-color)
    (fmakunbound 'neovm--rb-left)
    (fmakunbound 'neovm--rb-right)
    (fmakunbound 'neovm--rb-node)
    (fmakunbound 'neovm--rb-red-p)
    (fmakunbound 'neovm--rb-rotate-left)
    (fmakunbound 'neovm--rb-rotate-right)
    (fmakunbound 'neovm--rb-flip-colors)
    (fmakunbound 'neovm--rb-insert-rec)
    (fmakunbound 'neovm--rb-insert)
    (fmakunbound 'neovm--rb-range)
    (fmakunbound 'neovm--rb-floor)
    (fmakunbound 'neovm--rb-ceiling)
    (fmakunbound 'neovm--rb-size)
    (fmakunbound 'neovm--rb-rank)))"#;
    let expect = expect_test::expect![[
        r#""OK ((30 40 50 60 70) (10 20 30 40 50 60 70 80 90) nil (50) 20 50 nil 90 30 50 nil 10 0 4 8 5)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// RB-tree: bulk operations and set-like operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rbtree_set_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--rb-key (lambda (n) (car n)))
  (fset 'neovm--rb-color (lambda (n) (if n (cadr n) 'black)))
  (fset 'neovm--rb-left (lambda (n) (caddr n)))
  (fset 'neovm--rb-right (lambda (n) (cadddr n)))
  (fset 'neovm--rb-node (lambda (key color left right) (list key color left right)))
  (fset 'neovm--rb-red-p (lambda (n) (and n (eq (funcall 'neovm--rb-color n) 'red))))
  (fset 'neovm--rb-rotate-left
    (lambda (h)
      (let ((x (funcall 'neovm--rb-right h)))
        (funcall 'neovm--rb-node (funcall 'neovm--rb-key x) (funcall 'neovm--rb-color h)
                 (funcall 'neovm--rb-node (funcall 'neovm--rb-key h) 'red
                          (funcall 'neovm--rb-left h) (funcall 'neovm--rb-left x))
                 (funcall 'neovm--rb-right x)))))
  (fset 'neovm--rb-rotate-right
    (lambda (h)
      (let ((x (funcall 'neovm--rb-left h)))
        (funcall 'neovm--rb-node (funcall 'neovm--rb-key x) (funcall 'neovm--rb-color h)
                 (funcall 'neovm--rb-left x)
                 (funcall 'neovm--rb-node (funcall 'neovm--rb-key h) 'red
                          (funcall 'neovm--rb-right x) (funcall 'neovm--rb-right h))))))
  (fset 'neovm--rb-flip-colors
    (lambda (h)
      (let ((nc (if (eq (funcall 'neovm--rb-color h) 'red) 'black 'red))
            (lc (if (eq (funcall 'neovm--rb-color h) 'red) 'red 'black)))
        (funcall 'neovm--rb-node (funcall 'neovm--rb-key h) nc
                 (if (funcall 'neovm--rb-left h)
                     (funcall 'neovm--rb-node (funcall 'neovm--rb-key (funcall 'neovm--rb-left h))
                              lc (funcall 'neovm--rb-left (funcall 'neovm--rb-left h))
                              (funcall 'neovm--rb-right (funcall 'neovm--rb-left h))) nil)
                 (if (funcall 'neovm--rb-right h)
                     (funcall 'neovm--rb-node (funcall 'neovm--rb-key (funcall 'neovm--rb-right h))
                              lc (funcall 'neovm--rb-left (funcall 'neovm--rb-right h))
                              (funcall 'neovm--rb-right (funcall 'neovm--rb-right h))) nil)))))
  (fset 'neovm--rb-insert-rec
    (lambda (h key)
      (if (null h) (funcall 'neovm--rb-node key 'red nil nil)
        (let ((cmp (cond ((< key (funcall 'neovm--rb-key h)) -1)
                         ((> key (funcall 'neovm--rb-key h)) 1) (t 0))))
          (let ((r (cond ((= cmp 0) h)
                         ((< cmp 0) (funcall 'neovm--rb-node (funcall 'neovm--rb-key h)
                                             (funcall 'neovm--rb-color h)
                                             (funcall 'neovm--rb-insert-rec (funcall 'neovm--rb-left h) key)
                                             (funcall 'neovm--rb-right h)))
                         (t (funcall 'neovm--rb-node (funcall 'neovm--rb-key h)
                                     (funcall 'neovm--rb-color h) (funcall 'neovm--rb-left h)
                                     (funcall 'neovm--rb-insert-rec (funcall 'neovm--rb-right h) key))))))
            (when (and (funcall 'neovm--rb-red-p (funcall 'neovm--rb-right r))
                       (not (funcall 'neovm--rb-red-p (funcall 'neovm--rb-left r))))
              (setq r (funcall 'neovm--rb-rotate-left r)))
            (when (and (funcall 'neovm--rb-red-p (funcall 'neovm--rb-left r))
                       (funcall 'neovm--rb-red-p (funcall 'neovm--rb-left (funcall 'neovm--rb-left r))))
              (setq r (funcall 'neovm--rb-rotate-right r)))
            (when (and (funcall 'neovm--rb-red-p (funcall 'neovm--rb-left r))
                       (funcall 'neovm--rb-red-p (funcall 'neovm--rb-right r)))
              (setq r (funcall 'neovm--rb-flip-colors r)))
            r)))))
  (fset 'neovm--rb-insert
    (lambda (tree key)
      (let ((r (funcall 'neovm--rb-insert-rec tree key)))
        (funcall 'neovm--rb-node (funcall 'neovm--rb-key r) 'black
                 (funcall 'neovm--rb-left r) (funcall 'neovm--rb-right r)))))
  (fset 'neovm--rb-inorder
    (lambda (tree)
      (if (null tree) nil
        (append (funcall 'neovm--rb-inorder (funcall 'neovm--rb-left tree))
                (list (funcall 'neovm--rb-key tree))
                (funcall 'neovm--rb-inorder (funcall 'neovm--rb-right tree))))))
  (fset 'neovm--rb-search
    (lambda (tree key)
      (if (null tree) nil
        (let ((k (funcall 'neovm--rb-key tree)))
          (cond ((= key k) t)
                ((< key k) (funcall 'neovm--rb-search (funcall 'neovm--rb-left tree) key))
                (t (funcall 'neovm--rb-search (funcall 'neovm--rb-right tree) key)))))))

  ;; Build tree from list
  (fset 'neovm--rb-from-list
    (lambda (lst)
      (let ((tree nil))
        (dolist (k lst) (setq tree (funcall 'neovm--rb-insert tree k)))
        tree)))

  ;; Set intersection via tree
  (fset 'neovm--rb-intersection
    (lambda (tree1 tree2)
      "Return sorted list of keys in both trees."
      (let ((result nil))
        (dolist (k (funcall 'neovm--rb-inorder tree1))
          (when (funcall 'neovm--rb-search tree2 k)
            (setq result (cons k result))))
        (nreverse result))))

  ;; Set union via tree
  (fset 'neovm--rb-union
    (lambda (tree1 tree2)
      (let ((merged tree1))
        (dolist (k (funcall 'neovm--rb-inorder tree2))
          (setq merged (funcall 'neovm--rb-insert merged k)))
        (funcall 'neovm--rb-inorder merged))))

  ;; Set difference: keys in tree1 but not in tree2
  (fset 'neovm--rb-difference
    (lambda (tree1 tree2)
      (let ((result nil))
        (dolist (k (funcall 'neovm--rb-inorder tree1))
          (unless (funcall 'neovm--rb-search tree2 k)
            (setq result (cons k result))))
        (nreverse result))))

  (unwind-protect
      (let ((t1 (funcall 'neovm--rb-from-list '(1 3 5 7 9 11 13 15)))
            (t2 (funcall 'neovm--rb-from-list '(2 4 6 8 10 12 14)))
            (t3 (funcall 'neovm--rb-from-list '(5 10 15 20 25)))
            (t4 (funcall 'neovm--rb-from-list '(3 6 9 12 15))))
        (list
          ;; Intersection of disjoint sets
          (funcall 'neovm--rb-intersection t1 t2)
          ;; Intersection with overlap
          (funcall 'neovm--rb-intersection t1 t3)
          ;; Union of disjoint
          (funcall 'neovm--rb-union t1 t2)
          ;; Union with overlap
          (funcall 'neovm--rb-union t3 t4)
          ;; Difference
          (funcall 'neovm--rb-difference t1 t3)
          ;; Symmetric difference (A-B) union (B-A)
          (let ((a-minus-b (funcall 'neovm--rb-difference t3 t4))
                (b-minus-a (funcall 'neovm--rb-difference t4 t3)))
            (sort (append a-minus-b b-minus-a) #'<))
          ;; Intersection of a set with itself
          (funcall 'neovm--rb-intersection t1 t1)
          ;; Empty intersection
          (funcall 'neovm--rb-intersection
                   (funcall 'neovm--rb-from-list '(1 2 3))
                   (funcall 'neovm--rb-from-list '(4 5 6)))))
    (fmakunbound 'neovm--rb-key)
    (fmakunbound 'neovm--rb-color)
    (fmakunbound 'neovm--rb-left)
    (fmakunbound 'neovm--rb-right)
    (fmakunbound 'neovm--rb-node)
    (fmakunbound 'neovm--rb-red-p)
    (fmakunbound 'neovm--rb-rotate-left)
    (fmakunbound 'neovm--rb-rotate-right)
    (fmakunbound 'neovm--rb-flip-colors)
    (fmakunbound 'neovm--rb-insert-rec)
    (fmakunbound 'neovm--rb-insert)
    (fmakunbound 'neovm--rb-inorder)
    (fmakunbound 'neovm--rb-search)
    (fmakunbound 'neovm--rb-from-list)
    (fmakunbound 'neovm--rb-intersection)
    (fmakunbound 'neovm--rb-union)
    (fmakunbound 'neovm--rb-difference)))"#;
    let expect = expect_test::expect![[
        r#""OK (nil (5 15) (1 2 3 4 5 6 7 8 9 10 11 12 13 14 15) (3 5 6 9 10 12 15 20 25) (1 3 7 9 11 13) (3 5 6 9 10 12 20 25) (1 3 5 7 9 11 13 15) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
