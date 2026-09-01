//! Oracle parity tests for an advanced red-black tree in Elisp.
//!
//! Extends the basic RB-tree with deletion (with rebalancing), in-order
//! traversal, black-height validation, range queries, min/max extraction,
//! and bulk operations.  Node format: (key color left right).

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Shared RB-tree preamble used by all tests in this file.
// Provides: insert, delete, search, in-order, validate, range-query, min/max.
// Uses LLRB variant (left-leaning red-black tree) for simplicity.
// ---------------------------------------------------------------------------

const RB_PREAMBLE: &str = r#"
  ;; ---- Node representation: (key color left right) ----
  (fset 'neovm--rb2-key    (lambda (n) (car n)))
  (fset 'neovm--rb2-color  (lambda (n) (if n (cadr n) 'black)))
  (fset 'neovm--rb2-left   (lambda (n) (caddr n)))
  (fset 'neovm--rb2-right  (lambda (n) (cadddr n)))
  (fset 'neovm--rb2-node   (lambda (k c l r) (list k c l r)))
  (fset 'neovm--rb2-red-p  (lambda (n) (and n (eq (funcall 'neovm--rb2-color n) 'red))))

  ;; ---- Color flip ----
  (fset 'neovm--rb2-flip-color
    (lambda (c) (if (eq c 'red) 'black 'red)))

  (fset 'neovm--rb2-flip-colors
    (lambda (h)
      (funcall 'neovm--rb2-node
        (funcall 'neovm--rb2-key h)
        (funcall 'neovm--rb2-flip-color (funcall 'neovm--rb2-color h))
        (let ((l (funcall 'neovm--rb2-left h)))
          (if l (funcall 'neovm--rb2-node (funcall 'neovm--rb2-key l)
                  (funcall 'neovm--rb2-flip-color (funcall 'neovm--rb2-color l))
                  (funcall 'neovm--rb2-left l) (funcall 'neovm--rb2-right l))
            nil))
        (let ((r (funcall 'neovm--rb2-right h)))
          (if r (funcall 'neovm--rb2-node (funcall 'neovm--rb2-key r)
                  (funcall 'neovm--rb2-flip-color (funcall 'neovm--rb2-color r))
                  (funcall 'neovm--rb2-left r) (funcall 'neovm--rb2-right r))
            nil)))))

  ;; ---- Rotations ----
  (fset 'neovm--rb2-rotate-left
    (lambda (h)
      (let ((x (funcall 'neovm--rb2-right h)))
        (funcall 'neovm--rb2-node
          (funcall 'neovm--rb2-key x)
          (funcall 'neovm--rb2-color h)
          (funcall 'neovm--rb2-node
            (funcall 'neovm--rb2-key h)
            (funcall 'neovm--rb2-color x)
            (funcall 'neovm--rb2-left h)
            (funcall 'neovm--rb2-left x))
          (funcall 'neovm--rb2-right x)))))

  (fset 'neovm--rb2-rotate-right
    (lambda (h)
      (let ((x (funcall 'neovm--rb2-left h)))
        (funcall 'neovm--rb2-node
          (funcall 'neovm--rb2-key x)
          (funcall 'neovm--rb2-color h)
          (funcall 'neovm--rb2-left x)
          (funcall 'neovm--rb2-node
            (funcall 'neovm--rb2-key h)
            (funcall 'neovm--rb2-color x)
            (funcall 'neovm--rb2-right x)
            (funcall 'neovm--rb2-right h))))))

  ;; ---- Fix-up: restore LLRB invariants after insert ----
  (fset 'neovm--rb2-fixup
    (lambda (h)
      (let ((node h))
        ;; Right-leaning red => rotate left
        (when (and (funcall 'neovm--rb2-red-p (funcall 'neovm--rb2-right node))
                   (not (funcall 'neovm--rb2-red-p (funcall 'neovm--rb2-left node))))
          (setq node (funcall 'neovm--rb2-rotate-left node)))
        ;; Two consecutive left reds => rotate right
        (when (and (funcall 'neovm--rb2-red-p (funcall 'neovm--rb2-left node))
                   (funcall 'neovm--rb2-red-p
                     (funcall 'neovm--rb2-left (funcall 'neovm--rb2-left node))))
          (setq node (funcall 'neovm--rb2-rotate-right node)))
        ;; Both children red => flip colors
        (when (and (funcall 'neovm--rb2-red-p (funcall 'neovm--rb2-left node))
                   (funcall 'neovm--rb2-red-p (funcall 'neovm--rb2-right node)))
          (setq node (funcall 'neovm--rb2-flip-colors node)))
        node)))

  ;; ---- Insert ----
  (fset 'neovm--rb2-insert-rec
    (lambda (h key)
      (if (null h)
          (funcall 'neovm--rb2-node key 'red nil nil)
        (cond
         ((< key (funcall 'neovm--rb2-key h))
          (funcall 'neovm--rb2-fixup
            (funcall 'neovm--rb2-node
              (funcall 'neovm--rb2-key h)
              (funcall 'neovm--rb2-color h)
              (funcall 'neovm--rb2-insert-rec (funcall 'neovm--rb2-left h) key)
              (funcall 'neovm--rb2-right h))))
         ((> key (funcall 'neovm--rb2-key h))
          (funcall 'neovm--rb2-fixup
            (funcall 'neovm--rb2-node
              (funcall 'neovm--rb2-key h)
              (funcall 'neovm--rb2-color h)
              (funcall 'neovm--rb2-left h)
              (funcall 'neovm--rb2-insert-rec (funcall 'neovm--rb2-right h) key))))
         (t h)))))  ;; duplicate key, no change

  (fset 'neovm--rb2-insert
    (lambda (tree key)
      (let ((result (funcall 'neovm--rb2-insert-rec tree key)))
        ;; Root must be black
        (funcall 'neovm--rb2-node
          (funcall 'neovm--rb2-key result)
          'black
          (funcall 'neovm--rb2-left result)
          (funcall 'neovm--rb2-right result)))))

  ;; ---- Search ----
  (fset 'neovm--rb2-search
    (lambda (h key)
      (cond
       ((null h) nil)
       ((< key (funcall 'neovm--rb2-key h))
        (funcall 'neovm--rb2-search (funcall 'neovm--rb2-left h) key))
       ((> key (funcall 'neovm--rb2-key h))
        (funcall 'neovm--rb2-search (funcall 'neovm--rb2-right h) key))
       (t t))))

  ;; ---- In-order traversal ----
  (fset 'neovm--rb2-inorder
    (lambda (h)
      (if (null h) nil
        (append (funcall 'neovm--rb2-inorder (funcall 'neovm--rb2-left h))
                (list (funcall 'neovm--rb2-key h))
                (funcall 'neovm--rb2-inorder (funcall 'neovm--rb2-right h))))))

  ;; ---- Min / Max ----
  (fset 'neovm--rb2-min
    (lambda (h)
      (if (null (funcall 'neovm--rb2-left h))
          (funcall 'neovm--rb2-key h)
        (funcall 'neovm--rb2-min (funcall 'neovm--rb2-left h)))))

  (fset 'neovm--rb2-max
    (lambda (h)
      (if (null (funcall 'neovm--rb2-right h))
          (funcall 'neovm--rb2-key h)
        (funcall 'neovm--rb2-max (funcall 'neovm--rb2-right h)))))

  ;; ---- Size (count nodes) ----
  (fset 'neovm--rb2-size
    (lambda (h)
      (if (null h) 0
        (+ 1
           (funcall 'neovm--rb2-size (funcall 'neovm--rb2-left h))
           (funcall 'neovm--rb2-size (funcall 'neovm--rb2-right h))))))

  ;; ---- Black-height validation ----
  ;; Returns black-height if valid, nil if violated
  (fset 'neovm--rb2-black-height
    (lambda (h)
      (if (null h) 1  ;; nil leaves count as black
        (let ((lh (funcall 'neovm--rb2-black-height (funcall 'neovm--rb2-left h)))
              (rh (funcall 'neovm--rb2-black-height (funcall 'neovm--rb2-right h))))
          (if (or (null lh) (null rh) (/= lh rh))
              nil
            (+ lh (if (eq (funcall 'neovm--rb2-color h) 'black) 1 0)))))))

  ;; ---- No red-red violation ----
  (fset 'neovm--rb2-no-red-red-p
    (lambda (h)
      (if (null h) t
        (if (and (funcall 'neovm--rb2-red-p h)
                 (or (funcall 'neovm--rb2-red-p (funcall 'neovm--rb2-left h))
                     (funcall 'neovm--rb2-red-p (funcall 'neovm--rb2-right h))))
            nil
          (and (funcall 'neovm--rb2-no-red-red-p (funcall 'neovm--rb2-left h))
               (funcall 'neovm--rb2-no-red-red-p (funcall 'neovm--rb2-right h)))))))

  ;; ---- Full validation ----
  (fset 'neovm--rb2-valid-p
    (lambda (tree)
      (and (or (null tree) (eq (funcall 'neovm--rb2-color tree) 'black))  ;; root is black
           (not (null (funcall 'neovm--rb2-black-height tree)))           ;; uniform black-height
           (funcall 'neovm--rb2-no-red-red-p tree))))                    ;; no red-red

  ;; ---- Range query: keys in [lo, hi] ----
  (fset 'neovm--rb2-range
    (lambda (h lo hi)
      (if (null h) nil
        (let ((k (funcall 'neovm--rb2-key h))
              (result nil))
          (when (< lo k)
            (setq result (funcall 'neovm--rb2-range (funcall 'neovm--rb2-left h) lo hi)))
          (when (and (>= k lo) (<= k hi))
            (setq result (append result (list k))))
          (when (> hi k)
            (setq result (append result
                           (funcall 'neovm--rb2-range (funcall 'neovm--rb2-right h) lo hi))))
          result))))

  ;; ---- Delete minimum (helper for delete) ----
  (fset 'neovm--rb2-move-red-left
    (lambda (h)
      (let ((node (funcall 'neovm--rb2-flip-colors h)))
        (if (funcall 'neovm--rb2-red-p
              (funcall 'neovm--rb2-left (funcall 'neovm--rb2-right node)))
            (let ((node2 (funcall 'neovm--rb2-node
                           (funcall 'neovm--rb2-key node)
                           (funcall 'neovm--rb2-color node)
                           (funcall 'neovm--rb2-left node)
                           (funcall 'neovm--rb2-rotate-right (funcall 'neovm--rb2-right node)))))
              (funcall 'neovm--rb2-flip-colors
                (funcall 'neovm--rb2-rotate-left node2)))
          node))))

  (fset 'neovm--rb2-delete-min-rec
    (lambda (h)
      (if (null (funcall 'neovm--rb2-left h))
          nil
        (let ((node h))
          (when (and (not (funcall 'neovm--rb2-red-p (funcall 'neovm--rb2-left node)))
                     (not (funcall 'neovm--rb2-red-p
                            (funcall 'neovm--rb2-left (funcall 'neovm--rb2-left node)))))
            (setq node (funcall 'neovm--rb2-move-red-left node)))
          (funcall 'neovm--rb2-fixup
            (funcall 'neovm--rb2-node
              (funcall 'neovm--rb2-key node)
              (funcall 'neovm--rb2-color node)
              (funcall 'neovm--rb2-delete-min-rec (funcall 'neovm--rb2-left node))
              (funcall 'neovm--rb2-right node)))))))

  (fset 'neovm--rb2-delete-min
    (lambda (tree)
      (if (null tree) nil
        (let ((result (funcall 'neovm--rb2-delete-min-rec tree)))
          (if (null result) nil
            (funcall 'neovm--rb2-node
              (funcall 'neovm--rb2-key result) 'black
              (funcall 'neovm--rb2-left result)
              (funcall 'neovm--rb2-right result)))))))

  ;; ---- Build tree from list ----
  (fset 'neovm--rb2-from-list
    (lambda (keys)
      (let ((tree nil))
        (dolist (k keys) (setq tree (funcall 'neovm--rb2-insert tree k)))
        tree)))
"#;

const RB_CLEANUP: &str = r#"
    (fmakunbound 'neovm--rb2-key)
    (fmakunbound 'neovm--rb2-color)
    (fmakunbound 'neovm--rb2-left)
    (fmakunbound 'neovm--rb2-right)
    (fmakunbound 'neovm--rb2-node)
    (fmakunbound 'neovm--rb2-red-p)
    (fmakunbound 'neovm--rb2-flip-color)
    (fmakunbound 'neovm--rb2-flip-colors)
    (fmakunbound 'neovm--rb2-rotate-left)
    (fmakunbound 'neovm--rb2-rotate-right)
    (fmakunbound 'neovm--rb2-fixup)
    (fmakunbound 'neovm--rb2-insert-rec)
    (fmakunbound 'neovm--rb2-insert)
    (fmakunbound 'neovm--rb2-search)
    (fmakunbound 'neovm--rb2-inorder)
    (fmakunbound 'neovm--rb2-min)
    (fmakunbound 'neovm--rb2-max)
    (fmakunbound 'neovm--rb2-size)
    (fmakunbound 'neovm--rb2-black-height)
    (fmakunbound 'neovm--rb2-no-red-red-p)
    (fmakunbound 'neovm--rb2-valid-p)
    (fmakunbound 'neovm--rb2-range)
    (fmakunbound 'neovm--rb2-move-red-left)
    (fmakunbound 'neovm--rb2-delete-min-rec)
    (fmakunbound 'neovm--rb2-delete-min)
    (fmakunbound 'neovm--rb2-from-list)
"#;

// ---------------------------------------------------------------------------
// Insert with rebalancing — verify sorted order and invariants
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rbtree_adv_insert_rebalancing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {RB_PREAMBLE}
  (unwind-protect
      (let ((tree (funcall 'neovm--rb2-from-list '(50 30 70 20 40 60 80 10 25 35 45 55 65 75 90))))
        (list
         ;; In-order should be sorted
         (funcall 'neovm--rb2-inorder tree)
         ;; Size
         (funcall 'neovm--rb2-size tree)
         ;; Root is black
         (eq (funcall 'neovm--rb2-color tree) 'black)
         ;; Valid RB-tree
         (funcall 'neovm--rb2-valid-p tree)
         ;; Black-height exists (not nil)
         (not (null (funcall 'neovm--rb2-black-height tree)))))
    {RB_CLEANUP}))"#
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Insert ascending, descending, and random orders — all produce valid trees
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rbtree_adv_insertion_orders() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {RB_PREAMBLE}
  (unwind-protect
      (let ((asc (funcall 'neovm--rb2-from-list '(1 2 3 4 5 6 7 8 9 10)))
            (desc (funcall 'neovm--rb2-from-list '(10 9 8 7 6 5 4 3 2 1)))
            (zigzag (funcall 'neovm--rb2-from-list '(1 10 2 9 3 8 4 7 5 6)))
            (single (funcall 'neovm--rb2-from-list '(42)))
            (empty (funcall 'neovm--rb2-from-list nil)))
        (list
         ;; All have same sorted order
         (funcall 'neovm--rb2-inorder asc)
         (funcall 'neovm--rb2-inorder desc)
         (funcall 'neovm--rb2-inorder zigzag)
         ;; All valid
         (funcall 'neovm--rb2-valid-p asc)
         (funcall 'neovm--rb2-valid-p desc)
         (funcall 'neovm--rb2-valid-p zigzag)
         (funcall 'neovm--rb2-valid-p single)
         (funcall 'neovm--rb2-valid-p empty)
         ;; Sizes
         (funcall 'neovm--rb2-size asc)
         (funcall 'neovm--rb2-size single)
         (funcall 'neovm--rb2-size empty)
         ;; Duplicate insertion doesn't change size
         (let ((dup-tree (funcall 'neovm--rb2-from-list '(5 3 7 3 5 7))))
           (list (funcall 'neovm--rb2-size dup-tree)
                 (funcall 'neovm--rb2-inorder dup-tree)
                 (funcall 'neovm--rb2-valid-p dup-tree)))))
    {RB_CLEANUP}))"#
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Delete minimum — remove smallest keys and verify invariants
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rbtree_adv_delete_min() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {RB_PREAMBLE}
  (unwind-protect
      (let ((tree (funcall 'neovm--rb2-from-list '(50 30 70 20 40 60 80))))
        (let* ((min1 (funcall 'neovm--rb2-min tree))
               (tree2 (funcall 'neovm--rb2-delete-min tree))
               (min2 (funcall 'neovm--rb2-min tree2))
               (tree3 (funcall 'neovm--rb2-delete-min tree2))
               (min3 (funcall 'neovm--rb2-min tree3))
               (tree4 (funcall 'neovm--rb2-delete-min tree3)))
          (list
           min1 min2 min3
           (funcall 'neovm--rb2-inorder tree2)
           (funcall 'neovm--rb2-inorder tree3)
           (funcall 'neovm--rb2-inorder tree4)
           ;; All intermediate trees remain valid
           (funcall 'neovm--rb2-valid-p tree2)
           (funcall 'neovm--rb2-valid-p tree3)
           (funcall 'neovm--rb2-valid-p tree4)
           (funcall 'neovm--rb2-size tree4))))
    {RB_CLEANUP}))"#
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// In-order traversal — verify sorted output for various inputs
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rbtree_adv_inorder_traversal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {RB_PREAMBLE}
  (unwind-protect
      (list
       ;; Empty tree
       (funcall 'neovm--rb2-inorder nil)
       ;; Single element
       (funcall 'neovm--rb2-inorder (funcall 'neovm--rb2-from-list '(42)))
       ;; Two elements
       (funcall 'neovm--rb2-inorder (funcall 'neovm--rb2-from-list '(5 3)))
       (funcall 'neovm--rb2-inorder (funcall 'neovm--rb2-from-list '(3 5)))
       ;; Negative numbers
       (funcall 'neovm--rb2-inorder (funcall 'neovm--rb2-from-list '(-5 0 5 -10 10 -3 3)))
       ;; Large gap values
       (funcall 'neovm--rb2-inorder (funcall 'neovm--rb2-from-list '(1000 1 500 250 750 125 875)))
       ;; Consecutive values
       (funcall 'neovm--rb2-inorder (funcall 'neovm--rb2-from-list '(5 4 3 2 1 6 7 8 9 10)))
       ;; Verify sorted property: each element <= next
       (let* ((tree (funcall 'neovm--rb2-from-list '(42 17 99 3 28 55 71 8 63)))
              (sorted (funcall 'neovm--rb2-inorder tree))
              (is-sorted t))
         (let ((prev nil))
           (dolist (x sorted)
             (when (and prev (> prev x)) (setq is-sorted nil))
             (setq prev x)))
         (list is-sorted (length sorted))))
    {RB_CLEANUP}))"#
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Tree validation — black-height property across various trees
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rbtree_adv_black_height_validation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {RB_PREAMBLE}
  (unwind-protect
      (list
       ;; Empty tree has black-height 1
       (funcall 'neovm--rb2-black-height nil)
       ;; Single node (black root): black-height 2
       (funcall 'neovm--rb2-black-height (funcall 'neovm--rb2-from-list '(10)))
       ;; Various sizes — all should have non-nil black-height
       (not (null (funcall 'neovm--rb2-black-height (funcall 'neovm--rb2-from-list '(5 3 7)))))
       (not (null (funcall 'neovm--rb2-black-height (funcall 'neovm--rb2-from-list '(1 2 3 4 5 6 7)))))
       (not (null (funcall 'neovm--rb2-black-height (funcall 'neovm--rb2-from-list '(7 6 5 4 3 2 1)))))
       ;; Black-height grows logarithmically
       (let ((bh3 (funcall 'neovm--rb2-black-height (funcall 'neovm--rb2-from-list '(1 2 3))))
             (bh7 (funcall 'neovm--rb2-black-height (funcall 'neovm--rb2-from-list '(1 2 3 4 5 6 7))))
             (bh15 (funcall 'neovm--rb2-black-height
                     (funcall 'neovm--rb2-from-list '(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15)))))
         (list bh3 bh7 bh15
               (<= bh3 bh7)
               (<= bh7 bh15)))
       ;; After delete-min, black-height still valid
       (let* ((tree (funcall 'neovm--rb2-from-list '(10 5 15 3 7 12 18)))
              (bh-before (funcall 'neovm--rb2-black-height tree))
              (tree2 (funcall 'neovm--rb2-delete-min tree))
              (bh-after (funcall 'neovm--rb2-black-height tree2)))
         (list (not (null bh-before)) (not (null bh-after))
               (funcall 'neovm--rb2-valid-p tree2))))
    {RB_CLEANUP}))"#
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Range queries — find keys within [lo, hi]
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rbtree_adv_range_queries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {RB_PREAMBLE}
  (unwind-protect
      (let ((tree (funcall 'neovm--rb2-from-list '(10 20 30 40 50 60 70 80 90 100))))
        (list
         ;; Full range
         (funcall 'neovm--rb2-range tree 1 200)
         ;; Partial ranges
         (funcall 'neovm--rb2-range tree 25 75)
         (funcall 'neovm--rb2-range tree 30 30)  ;; single element
         (funcall 'neovm--rb2-range tree 31 39)  ;; no elements in range
         (funcall 'neovm--rb2-range tree 1 10)   ;; lower boundary
         (funcall 'neovm--rb2-range tree 90 200) ;; upper boundary
         ;; Range on empty tree
         (funcall 'neovm--rb2-range nil 1 100)
         ;; Range with negative bounds
         (let ((tree2 (funcall 'neovm--rb2-from-list '(-10 -5 0 5 10))))
           (list (funcall 'neovm--rb2-range tree2 -7 3)
                 (funcall 'neovm--rb2-range tree2 -100 100)))
         ;; Verify range results are sorted
         (let* ((result (funcall 'neovm--rb2-range tree 20 80))
                (is-sorted t)
                (prev nil))
           (dolist (x result)
             (when (and prev (> prev x)) (setq is-sorted nil))
             (setq prev x))
           (list is-sorted (length result)))
         ;; Min/max of tree
         (funcall 'neovm--rb2-min tree)
         (funcall 'neovm--rb2-max tree)))
    {RB_CLEANUP}))"#
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Bulk operations: build large tree, search, delete-min repeatedly
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rbtree_adv_bulk_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {RB_PREAMBLE}
  (unwind-protect
      (let ((tree (funcall 'neovm--rb2-from-list
                    '(50 25 75 12 37 62 87 6 18 31 43 56 68 81 93))))
        (list
         ;; Size and validity
         (funcall 'neovm--rb2-size tree)
         (funcall 'neovm--rb2-valid-p tree)
         ;; Search for existing and non-existing keys
         (funcall 'neovm--rb2-search tree 50)
         (funcall 'neovm--rb2-search tree 6)
         (funcall 'neovm--rb2-search tree 93)
         (funcall 'neovm--rb2-search tree 1)
         (funcall 'neovm--rb2-search tree 100)
         ;; Delete-min repeatedly, collecting mins and checking validity
         (let ((mins nil)
               (valid-all t)
               (t1 tree))
           (dotimes (_ 5)
             (setq mins (cons (funcall 'neovm--rb2-min t1) mins))
             (setq t1 (funcall 'neovm--rb2-delete-min t1))
             (unless (funcall 'neovm--rb2-valid-p t1) (setq valid-all nil)))
           (list (nreverse mins)
                 valid-all
                 (funcall 'neovm--rb2-size t1)
                 (funcall 'neovm--rb2-inorder t1)))
         ;; Insert into existing tree (add new keys)
         (let ((tree2 tree))
           (dolist (k '(1 2 3 99 100))
             (setq tree2 (funcall 'neovm--rb2-insert tree2 k)))
           (list (funcall 'neovm--rb2-size tree2)
                 (funcall 'neovm--rb2-valid-p tree2)
                 (funcall 'neovm--rb2-min tree2)
                 (funcall 'neovm--rb2-max tree2)
                 ;; Verify new keys are searchable
                 (funcall 'neovm--rb2-search tree2 1)
                 (funcall 'neovm--rb2-search tree2 100)))))
    {RB_CLEANUP}))"#
    );
    crate::common::assert_oracle_parity(&form);
}
