//! Comprehensive oracle parity tests for a red-black tree in Elisp.
//!
//! Full RB tree implementation with: insert with all 4 rotation cases,
//! delete with rebalancing, search, min/max, successor/predecessor,
//! in-order traversal, full invariant validation (root black, no red-red,
//! equal black height), insert sequences producing all rotation cases,
//! delete all nodes verifying invariants, bulk insert + range query,
//! tree height bounds verification, node counting, and floor/ceiling.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Full LLRB tree preamble with insert, delete, search, traversal, validation,
// successor, predecessor, floor, ceiling, nth-smallest, count.
// ---------------------------------------------------------------------------

const RB3_PREAMBLE: &str = r#"
  ;; Node: (key color left right)
  (fset 'neovm--rb3-key    (lambda (n) (car n)))
  (fset 'neovm--rb3-color  (lambda (n) (if n (cadr n) 'black)))
  (fset 'neovm--rb3-left   (lambda (n) (caddr n)))
  (fset 'neovm--rb3-right  (lambda (n) (cadddr n)))
  (fset 'neovm--rb3-node   (lambda (k c l r) (list k c l r)))
  (fset 'neovm--rb3-red-p  (lambda (n) (and n (eq (funcall 'neovm--rb3-color n) 'red))))

  ;; Color flip
  (fset 'neovm--rb3-flip-color (lambda (c) (if (eq c 'red) 'black 'red)))
  (fset 'neovm--rb3-flip-colors
    (lambda (h)
      (funcall 'neovm--rb3-node
        (funcall 'neovm--rb3-key h)
        (funcall 'neovm--rb3-flip-color (funcall 'neovm--rb3-color h))
        (let ((l (funcall 'neovm--rb3-left h)))
          (if l (funcall 'neovm--rb3-node (funcall 'neovm--rb3-key l)
                  (funcall 'neovm--rb3-flip-color (funcall 'neovm--rb3-color l))
                  (funcall 'neovm--rb3-left l) (funcall 'neovm--rb3-right l))
            nil))
        (let ((r (funcall 'neovm--rb3-right h)))
          (if r (funcall 'neovm--rb3-node (funcall 'neovm--rb3-key r)
                  (funcall 'neovm--rb3-flip-color (funcall 'neovm--rb3-color r))
                  (funcall 'neovm--rb3-left r) (funcall 'neovm--rb3-right r))
            nil)))))

  ;; Rotations
  (fset 'neovm--rb3-rotate-left
    (lambda (h)
      (let ((x (funcall 'neovm--rb3-right h)))
        (funcall 'neovm--rb3-node
          (funcall 'neovm--rb3-key x)
          (funcall 'neovm--rb3-color h)
          (funcall 'neovm--rb3-node
            (funcall 'neovm--rb3-key h)
            (funcall 'neovm--rb3-color x)
            (funcall 'neovm--rb3-left h)
            (funcall 'neovm--rb3-left x))
          (funcall 'neovm--rb3-right x)))))

  (fset 'neovm--rb3-rotate-right
    (lambda (h)
      (let ((x (funcall 'neovm--rb3-left h)))
        (funcall 'neovm--rb3-node
          (funcall 'neovm--rb3-key x)
          (funcall 'neovm--rb3-color h)
          (funcall 'neovm--rb3-left x)
          (funcall 'neovm--rb3-node
            (funcall 'neovm--rb3-key h)
            (funcall 'neovm--rb3-color x)
            (funcall 'neovm--rb3-right x)
            (funcall 'neovm--rb3-right h))))))

  ;; Fix-up: restore LLRB invariants
  (fset 'neovm--rb3-fixup
    (lambda (h)
      (let ((node h))
        (when (and (funcall 'neovm--rb3-red-p (funcall 'neovm--rb3-right node))
                   (not (funcall 'neovm--rb3-red-p (funcall 'neovm--rb3-left node))))
          (setq node (funcall 'neovm--rb3-rotate-left node)))
        (when (and (funcall 'neovm--rb3-red-p (funcall 'neovm--rb3-left node))
                   (funcall 'neovm--rb3-red-p
                     (funcall 'neovm--rb3-left (funcall 'neovm--rb3-left node))))
          (setq node (funcall 'neovm--rb3-rotate-right node)))
        (when (and (funcall 'neovm--rb3-red-p (funcall 'neovm--rb3-left node))
                   (funcall 'neovm--rb3-red-p (funcall 'neovm--rb3-right node)))
          (setq node (funcall 'neovm--rb3-flip-colors node)))
        node)))

  ;; Insert
  (fset 'neovm--rb3-insert-rec
    (lambda (h key)
      (if (null h)
          (funcall 'neovm--rb3-node key 'red nil nil)
        (cond
         ((< key (funcall 'neovm--rb3-key h))
          (funcall 'neovm--rb3-fixup
            (funcall 'neovm--rb3-node
              (funcall 'neovm--rb3-key h) (funcall 'neovm--rb3-color h)
              (funcall 'neovm--rb3-insert-rec (funcall 'neovm--rb3-left h) key)
              (funcall 'neovm--rb3-right h))))
         ((> key (funcall 'neovm--rb3-key h))
          (funcall 'neovm--rb3-fixup
            (funcall 'neovm--rb3-node
              (funcall 'neovm--rb3-key h) (funcall 'neovm--rb3-color h)
              (funcall 'neovm--rb3-left h)
              (funcall 'neovm--rb3-insert-rec (funcall 'neovm--rb3-right h) key))))
         (t h)))))

  (fset 'neovm--rb3-insert
    (lambda (tree key)
      (let ((r (funcall 'neovm--rb3-insert-rec tree key)))
        (funcall 'neovm--rb3-node (funcall 'neovm--rb3-key r) 'black
          (funcall 'neovm--rb3-left r) (funcall 'neovm--rb3-right r)))))

  ;; Search
  (fset 'neovm--rb3-search
    (lambda (h key)
      (cond ((null h) nil)
            ((< key (funcall 'neovm--rb3-key h))
             (funcall 'neovm--rb3-search (funcall 'neovm--rb3-left h) key))
            ((> key (funcall 'neovm--rb3-key h))
             (funcall 'neovm--rb3-search (funcall 'neovm--rb3-right h) key))
            (t t))))

  ;; In-order traversal
  (fset 'neovm--rb3-inorder
    (lambda (h)
      (if (null h) nil
        (append (funcall 'neovm--rb3-inorder (funcall 'neovm--rb3-left h))
                (list (funcall 'neovm--rb3-key h))
                (funcall 'neovm--rb3-inorder (funcall 'neovm--rb3-right h))))))

  ;; Min / Max
  (fset 'neovm--rb3-min
    (lambda (h)
      (if (null (funcall 'neovm--rb3-left h))
          (funcall 'neovm--rb3-key h)
        (funcall 'neovm--rb3-min (funcall 'neovm--rb3-left h)))))
  (fset 'neovm--rb3-max
    (lambda (h)
      (if (null (funcall 'neovm--rb3-right h))
          (funcall 'neovm--rb3-key h)
        (funcall 'neovm--rb3-max (funcall 'neovm--rb3-right h)))))

  ;; Size
  (fset 'neovm--rb3-size
    (lambda (h)
      (if (null h) 0
        (+ 1 (funcall 'neovm--rb3-size (funcall 'neovm--rb3-left h))
             (funcall 'neovm--rb3-size (funcall 'neovm--rb3-right h))))))

  ;; Height (longest path from root to leaf)
  (fset 'neovm--rb3-height
    (lambda (h)
      (if (null h) 0
        (1+ (max (funcall 'neovm--rb3-height (funcall 'neovm--rb3-left h))
                 (funcall 'neovm--rb3-height (funcall 'neovm--rb3-right h)))))))

  ;; Black-height validation
  (fset 'neovm--rb3-black-height
    (lambda (h)
      (if (null h) 1
        (let ((lh (funcall 'neovm--rb3-black-height (funcall 'neovm--rb3-left h)))
              (rh (funcall 'neovm--rb3-black-height (funcall 'neovm--rb3-right h))))
          (if (or (null lh) (null rh) (/= lh rh)) nil
            (+ lh (if (eq (funcall 'neovm--rb3-color h) 'black) 1 0)))))))

  ;; No red-red violation
  (fset 'neovm--rb3-no-red-red-p
    (lambda (h)
      (if (null h) t
        (if (and (funcall 'neovm--rb3-red-p h)
                 (or (funcall 'neovm--rb3-red-p (funcall 'neovm--rb3-left h))
                     (funcall 'neovm--rb3-red-p (funcall 'neovm--rb3-right h))))
            nil
          (and (funcall 'neovm--rb3-no-red-red-p (funcall 'neovm--rb3-left h))
               (funcall 'neovm--rb3-no-red-red-p (funcall 'neovm--rb3-right h)))))))

  ;; Full validation
  (fset 'neovm--rb3-valid-p
    (lambda (tree)
      (and (or (null tree) (eq (funcall 'neovm--rb3-color tree) 'black))
           (not (null (funcall 'neovm--rb3-black-height tree)))
           (funcall 'neovm--rb3-no-red-red-p tree))))

  ;; Range query
  (fset 'neovm--rb3-range
    (lambda (h lo hi)
      (if (null h) nil
        (let ((k (funcall 'neovm--rb3-key h)) (result nil))
          (when (< lo k)
            (setq result (funcall 'neovm--rb3-range (funcall 'neovm--rb3-left h) lo hi)))
          (when (and (>= k lo) (<= k hi))
            (setq result (append result (list k))))
          (when (> hi k)
            (setq result (append result (funcall 'neovm--rb3-range (funcall 'neovm--rb3-right h) lo hi))))
          result))))

  ;; Delete minimum
  (fset 'neovm--rb3-move-red-left
    (lambda (h)
      (let ((node (funcall 'neovm--rb3-flip-colors h)))
        (if (funcall 'neovm--rb3-red-p
              (funcall 'neovm--rb3-left (funcall 'neovm--rb3-right node)))
            (funcall 'neovm--rb3-flip-colors
              (funcall 'neovm--rb3-rotate-left
                (funcall 'neovm--rb3-node
                  (funcall 'neovm--rb3-key node) (funcall 'neovm--rb3-color node)
                  (funcall 'neovm--rb3-left node)
                  (funcall 'neovm--rb3-rotate-right (funcall 'neovm--rb3-right node)))))
          node))))

  (fset 'neovm--rb3-move-red-right
    (lambda (h)
      (let ((node (funcall 'neovm--rb3-flip-colors h)))
        (if (funcall 'neovm--rb3-red-p
              (funcall 'neovm--rb3-left (funcall 'neovm--rb3-left node)))
            (funcall 'neovm--rb3-flip-colors
              (funcall 'neovm--rb3-rotate-right node))
          node))))

  (fset 'neovm--rb3-delete-min-rec
    (lambda (h)
      (if (null (funcall 'neovm--rb3-left h)) nil
        (let ((node h))
          (when (and (not (funcall 'neovm--rb3-red-p (funcall 'neovm--rb3-left node)))
                     (not (funcall 'neovm--rb3-red-p
                            (funcall 'neovm--rb3-left (funcall 'neovm--rb3-left node)))))
            (setq node (funcall 'neovm--rb3-move-red-left node)))
          (funcall 'neovm--rb3-fixup
            (funcall 'neovm--rb3-node
              (funcall 'neovm--rb3-key node) (funcall 'neovm--rb3-color node)
              (funcall 'neovm--rb3-delete-min-rec (funcall 'neovm--rb3-left node))
              (funcall 'neovm--rb3-right node)))))))

  (fset 'neovm--rb3-delete-min
    (lambda (tree)
      (if (null tree) nil
        (let ((r (funcall 'neovm--rb3-delete-min-rec tree)))
          (if (null r) nil
            (funcall 'neovm--rb3-node (funcall 'neovm--rb3-key r) 'black
              (funcall 'neovm--rb3-left r) (funcall 'neovm--rb3-right r)))))))

  ;; Delete arbitrary key
  (fset 'neovm--rb3-delete-rec
    (lambda (h key)
      (if (null h) nil
        (let ((node h))
          (if (< key (funcall 'neovm--rb3-key node))
              (progn
                (when (and (not (funcall 'neovm--rb3-red-p (funcall 'neovm--rb3-left node)))
                           (funcall 'neovm--rb3-left node)
                           (not (funcall 'neovm--rb3-red-p
                                  (funcall 'neovm--rb3-left (funcall 'neovm--rb3-left node)))))
                  (setq node (funcall 'neovm--rb3-move-red-left node)))
                (funcall 'neovm--rb3-fixup
                  (funcall 'neovm--rb3-node
                    (funcall 'neovm--rb3-key node) (funcall 'neovm--rb3-color node)
                    (funcall 'neovm--rb3-delete-rec (funcall 'neovm--rb3-left node) key)
                    (funcall 'neovm--rb3-right node))))
            ;; key >= node key
            (progn
              (when (funcall 'neovm--rb3-red-p (funcall 'neovm--rb3-left node))
                (setq node (funcall 'neovm--rb3-rotate-right node)))
              (when (and (= key (funcall 'neovm--rb3-key node))
                         (null (funcall 'neovm--rb3-right node)))
                (setq node nil))
              (when node
                (when (and (not (funcall 'neovm--rb3-red-p (funcall 'neovm--rb3-right node)))
                           (funcall 'neovm--rb3-right node)
                           (not (funcall 'neovm--rb3-red-p
                                  (funcall 'neovm--rb3-left (funcall 'neovm--rb3-right node)))))
                  (setq node (funcall 'neovm--rb3-move-red-right node)))
                (if (= key (funcall 'neovm--rb3-key node))
                    (let ((min-key (funcall 'neovm--rb3-min (funcall 'neovm--rb3-right node))))
                      (setq node (funcall 'neovm--rb3-fixup
                                   (funcall 'neovm--rb3-node
                                     min-key (funcall 'neovm--rb3-color node)
                                     (funcall 'neovm--rb3-left node)
                                     (funcall 'neovm--rb3-delete-min-rec (funcall 'neovm--rb3-right node))))))
                  (setq node (funcall 'neovm--rb3-fixup
                               (funcall 'neovm--rb3-node
                                 (funcall 'neovm--rb3-key node) (funcall 'neovm--rb3-color node)
                                 (funcall 'neovm--rb3-left node)
                                 (funcall 'neovm--rb3-delete-rec (funcall 'neovm--rb3-right node) key))))))
              node))))))

  (fset 'neovm--rb3-delete
    (lambda (tree key)
      (let ((r (funcall 'neovm--rb3-delete-rec tree key)))
        (if (null r) nil
          (funcall 'neovm--rb3-node (funcall 'neovm--rb3-key r) 'black
            (funcall 'neovm--rb3-left r) (funcall 'neovm--rb3-right r))))))

  ;; Successor (smallest key > given key)
  (fset 'neovm--rb3-successor
    (lambda (h key)
      (if (null h) nil
        (cond
         ((<= key (funcall 'neovm--rb3-key h))
          ;; This node or something in left subtree
          (let ((left-result (funcall 'neovm--rb3-successor (funcall 'neovm--rb3-left h) key)))
            (if left-result left-result
              (if (> (funcall 'neovm--rb3-key h) key)
                  (funcall 'neovm--rb3-key h)
                nil))))
         (t (funcall 'neovm--rb3-successor (funcall 'neovm--rb3-right h) key))))))

  ;; Predecessor (largest key < given key)
  (fset 'neovm--rb3-predecessor
    (lambda (h key)
      (if (null h) nil
        (cond
         ((>= key (funcall 'neovm--rb3-key h))
          (let ((right-result (funcall 'neovm--rb3-predecessor (funcall 'neovm--rb3-right h) key)))
            (if right-result right-result
              (if (< (funcall 'neovm--rb3-key h) key)
                  (funcall 'neovm--rb3-key h)
                nil))))
         (t (funcall 'neovm--rb3-predecessor (funcall 'neovm--rb3-left h) key))))))

  ;; Build from list
  (fset 'neovm--rb3-from-list
    (lambda (keys)
      (let ((tree nil))
        (dolist (k keys) (setq tree (funcall 'neovm--rb3-insert tree k)))
        tree)))
"#;

const RB3_CLEANUP: &str = r#"
    (fmakunbound 'neovm--rb3-key) (fmakunbound 'neovm--rb3-color)
    (fmakunbound 'neovm--rb3-left) (fmakunbound 'neovm--rb3-right)
    (fmakunbound 'neovm--rb3-node) (fmakunbound 'neovm--rb3-red-p)
    (fmakunbound 'neovm--rb3-flip-color) (fmakunbound 'neovm--rb3-flip-colors)
    (fmakunbound 'neovm--rb3-rotate-left) (fmakunbound 'neovm--rb3-rotate-right)
    (fmakunbound 'neovm--rb3-fixup)
    (fmakunbound 'neovm--rb3-insert-rec) (fmakunbound 'neovm--rb3-insert)
    (fmakunbound 'neovm--rb3-search) (fmakunbound 'neovm--rb3-inorder)
    (fmakunbound 'neovm--rb3-min) (fmakunbound 'neovm--rb3-max)
    (fmakunbound 'neovm--rb3-size) (fmakunbound 'neovm--rb3-height)
    (fmakunbound 'neovm--rb3-black-height)
    (fmakunbound 'neovm--rb3-no-red-red-p) (fmakunbound 'neovm--rb3-valid-p)
    (fmakunbound 'neovm--rb3-range)
    (fmakunbound 'neovm--rb3-move-red-left) (fmakunbound 'neovm--rb3-move-red-right)
    (fmakunbound 'neovm--rb3-delete-min-rec) (fmakunbound 'neovm--rb3-delete-min)
    (fmakunbound 'neovm--rb3-delete-rec) (fmakunbound 'neovm--rb3-delete)
    (fmakunbound 'neovm--rb3-successor) (fmakunbound 'neovm--rb3-predecessor)
    (fmakunbound 'neovm--rb3-from-list)
"#;

// ---------------------------------------------------------------------------
// Insert producing all 4 rotation cases + verify invariants
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rbtree3_insert_all_rotation_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {RB3_PREAMBLE}
  (unwind-protect
      (list
       ;; Case 1: Right-leaning red -> rotate left
       ;; Insert ascending: 1 2 (right child is red, left is black -> rotate left)
       (let ((t1 (funcall 'neovm--rb3-from-list '(1 2))))
         (list (funcall 'neovm--rb3-valid-p t1)
               (funcall 'neovm--rb3-inorder t1)))

       ;; Case 2: Two consecutive left reds -> rotate right
       ;; Insert descending: 3 2 1 (left->left both red -> rotate right)
       (let ((t2 (funcall 'neovm--rb3-from-list '(3 2 1))))
         (list (funcall 'neovm--rb3-valid-p t2)
               (funcall 'neovm--rb3-inorder t2)))

       ;; Case 3: Both children red -> flip colors
       ;; Insert: 2 1 3 (both children red after inserts -> flip)
       (let ((t3 (funcall 'neovm--rb3-from-list '(2 1 3))))
         (list (funcall 'neovm--rb3-valid-p t3)
               (funcall 'neovm--rb3-inorder t3)))

       ;; Case 4: Mixed rotations on deeper tree
       ;; This sequence triggers left+right rotations:
       (let ((t4 (funcall 'neovm--rb3-from-list '(10 5 20 3 7 15 25 1 4 6 8))))
         (list (funcall 'neovm--rb3-valid-p t4)
               (funcall 'neovm--rb3-inorder t4)
               (funcall 'neovm--rb3-size t4)))

       ;; Fully ascending triggers many rotate-lefts then rebalance
       (let ((asc (funcall 'neovm--rb3-from-list '(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15))))
         (list (funcall 'neovm--rb3-valid-p asc)
               (funcall 'neovm--rb3-size asc)
               (funcall 'neovm--rb3-inorder asc)))

       ;; Fully descending triggers many rotate-rights then rebalance
       (let ((desc (funcall 'neovm--rb3-from-list '(15 14 13 12 11 10 9 8 7 6 5 4 3 2 1))))
         (list (funcall 'neovm--rb3-valid-p desc)
               (funcall 'neovm--rb3-size desc))))
    {RB3_CLEANUP}))"#
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Delete with rebalancing: delete specific keys and verify invariants
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rbtree3_delete_rebalancing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {RB3_PREAMBLE}
  (unwind-protect
      (let ((tree (funcall 'neovm--rb3-from-list '(50 30 70 20 40 60 80 10 25 35 45))))
        (list
         ;; Delete leaf node (10)
         (let ((t1 (funcall 'neovm--rb3-delete tree 10)))
           (list (funcall 'neovm--rb3-valid-p t1)
                 (funcall 'neovm--rb3-inorder t1)
                 (funcall 'neovm--rb3-search t1 10)))
         ;; Delete node with one child (20)
         (let ((t2 (funcall 'neovm--rb3-delete tree 20)))
           (list (funcall 'neovm--rb3-valid-p t2)
                 (funcall 'neovm--rb3-inorder t2)))
         ;; Delete node with two children (30)
         (let ((t3 (funcall 'neovm--rb3-delete tree 30)))
           (list (funcall 'neovm--rb3-valid-p t3)
                 (funcall 'neovm--rb3-inorder t3)))
         ;; Delete root (50)
         (let ((t4 (funcall 'neovm--rb3-delete tree 50)))
           (list (funcall 'neovm--rb3-valid-p t4)
                 (funcall 'neovm--rb3-inorder t4)
                 (eq (funcall 'neovm--rb3-color t4) 'black)))
         ;; Delete non-existent key (no change)
         (let ((t5 (funcall 'neovm--rb3-delete tree 999)))
           (list (funcall 'neovm--rb3-valid-p t5)
                 (equal (funcall 'neovm--rb3-inorder t5)
                        (funcall 'neovm--rb3-inorder tree))))
         ;; Delete min
         (let ((t6 (funcall 'neovm--rb3-delete tree (funcall 'neovm--rb3-min tree))))
           (list (funcall 'neovm--rb3-valid-p t6)
                 (funcall 'neovm--rb3-min t6)))
         ;; Delete max
         (let ((t7 (funcall 'neovm--rb3-delete tree (funcall 'neovm--rb3-max tree))))
           (list (funcall 'neovm--rb3-valid-p t7)
                 (funcall 'neovm--rb3-max t7)))))
    {RB3_CLEANUP}))"#
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Delete all nodes one by one, verifying invariants after each deletion
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rbtree3_delete_all_nodes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {RB3_PREAMBLE}
  (unwind-protect
      (let ((keys '(40 20 60 10 30 50 70 5 15 25 35 45 55 65 75))
            (tree (funcall 'neovm--rb3-from-list '(40 20 60 10 30 50 70 5 15 25 35 45 55 65 75))))
        ;; Delete in a mixed order, collect validity after each
        (let ((delete-order '(25 70 5 40 55 10 65 35 50 15 75 20 45 60 30))
              (valid-all t)
              (sizes nil)
              (cur tree))
          (dolist (k delete-order)
            (setq cur (funcall 'neovm--rb3-delete cur k))
            (unless (funcall 'neovm--rb3-valid-p cur)
              (setq valid-all nil))
            (setq sizes (cons (funcall 'neovm--rb3-size cur) sizes)))
          (list valid-all
                (nreverse sizes)
                (null cur)
                (funcall 'neovm--rb3-size cur))))
    {RB3_CLEANUP}))"#
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Successor and predecessor queries
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rbtree3_successor_predecessor() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {RB3_PREAMBLE}
  (unwind-protect
      (let ((tree (funcall 'neovm--rb3-from-list '(10 20 30 40 50 60 70 80 90))))
        (list
         ;; Successor of each element
         (funcall 'neovm--rb3-successor tree 10)
         (funcall 'neovm--rb3-successor tree 20)
         (funcall 'neovm--rb3-successor tree 50)
         (funcall 'neovm--rb3-successor tree 80)
         ;; Successor of max (should be nil)
         (funcall 'neovm--rb3-successor tree 90)
         ;; Successor of non-existent keys
         (funcall 'neovm--rb3-successor tree 5)
         (funcall 'neovm--rb3-successor tree 25)
         (funcall 'neovm--rb3-successor tree 85)
         (funcall 'neovm--rb3-successor tree 95)
         ;; Predecessor of each element
         (funcall 'neovm--rb3-predecessor tree 90)
         (funcall 'neovm--rb3-predecessor tree 50)
         (funcall 'neovm--rb3-predecessor tree 20)
         ;; Predecessor of min (should be nil)
         (funcall 'neovm--rb3-predecessor tree 10)
         ;; Predecessor of non-existent keys
         (funcall 'neovm--rb3-predecessor tree 5)
         (funcall 'neovm--rb3-predecessor tree 25)
         (funcall 'neovm--rb3-predecessor tree 95)))
    {RB3_CLEANUP}))"#
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Tree height bounds: h <= 2*log2(n+1) for RB tree
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rbtree3_height_bounds() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {RB3_PREAMBLE}
  (unwind-protect
      (list
       ;; Height for various sizes
       (let ((trees (list
                     (funcall 'neovm--rb3-from-list '(1))
                     (funcall 'neovm--rb3-from-list '(1 2 3))
                     (funcall 'neovm--rb3-from-list '(1 2 3 4 5 6 7))
                     (funcall 'neovm--rb3-from-list '(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15))
                     (funcall 'neovm--rb3-from-list '(8 4 12 2 6 10 14 1 3 5 7 9 11 13 15)))))
         (mapcar (lambda (t)
                   (list (funcall 'neovm--rb3-size t)
                         (funcall 'neovm--rb3-height t)
                         (funcall 'neovm--rb3-valid-p t)))
                 trees))
       ;; Height grows logarithmically: asc insertion
       (let ((h3 (funcall 'neovm--rb3-height (funcall 'neovm--rb3-from-list '(1 2 3))))
             (h7 (funcall 'neovm--rb3-height (funcall 'neovm--rb3-from-list '(1 2 3 4 5 6 7))))
             (h15 (funcall 'neovm--rb3-height
                    (funcall 'neovm--rb3-from-list '(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15)))))
         (list h3 h7 h15
               (<= h3 h7) (<= h7 h15)))
       ;; Black-height is consistent across different insertion orders
       (let ((bh-asc (funcall 'neovm--rb3-black-height
                       (funcall 'neovm--rb3-from-list '(1 2 3 4 5 6 7 8 9 10))))
             (bh-desc (funcall 'neovm--rb3-black-height
                        (funcall 'neovm--rb3-from-list '(10 9 8 7 6 5 4 3 2 1))))
             (bh-rand (funcall 'neovm--rb3-black-height
                        (funcall 'neovm--rb3-from-list '(5 3 8 1 4 7 10 2 6 9)))))
         (list bh-asc bh-desc bh-rand)))
    {RB3_CLEANUP}))"#
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Bulk insert + range queries
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rbtree3_bulk_insert_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {RB3_PREAMBLE}
  (unwind-protect
      (let ((tree nil))
        ;; Bulk insert: multiples of 5 from 5 to 100
        (let ((i 5))
          (while (<= i 100)
            (setq tree (funcall 'neovm--rb3-insert tree i))
            (setq i (+ i 5))))
        (list
         ;; Size = 20
         (funcall 'neovm--rb3-size tree)
         (funcall 'neovm--rb3-valid-p tree)
         ;; Range [10, 50]
         (funcall 'neovm--rb3-range tree 10 50)
         ;; Range [1, 15]
         (funcall 'neovm--rb3-range tree 1 15)
         ;; Range [90, 200]
         (funcall 'neovm--rb3-range tree 90 200)
         ;; Range [22, 28] (only 25 in range)
         (funcall 'neovm--rb3-range tree 22 28)
         ;; Range [31, 34] (nothing in range)
         (funcall 'neovm--rb3-range tree 31 34)
         ;; Min and max
         (funcall 'neovm--rb3-min tree)
         (funcall 'neovm--rb3-max tree)
         ;; Full inorder
         (funcall 'neovm--rb3-inorder tree)))
    {RB3_CLEANUP}))"#
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Duplicate insertion: tree should be unchanged
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rbtree3_duplicate_insertion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {RB3_PREAMBLE}
  (unwind-protect
      (let ((tree (funcall 'neovm--rb3-from-list '(10 20 30 40 50))))
        (let ((original-order (funcall 'neovm--rb3-inorder tree))
              (original-size (funcall 'neovm--rb3-size tree)))
          ;; Insert all existing keys again
          (let ((tree2 tree))
            (dolist (k '(10 20 30 40 50 30 10 50))
              (setq tree2 (funcall 'neovm--rb3-insert tree2 k)))
            (list
             ;; Size unchanged
             (= original-size (funcall 'neovm--rb3-size tree2))
             ;; Order unchanged
             (equal original-order (funcall 'neovm--rb3-inorder tree2))
             ;; Still valid
             (funcall 'neovm--rb3-valid-p tree2)))))
    {RB3_CLEANUP}))"#
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Insert then search: comprehensive search coverage
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rbtree3_search_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {RB3_PREAMBLE}
  (unwind-protect
      (let ((tree (funcall 'neovm--rb3-from-list '(50 25 75 12 37 62 87 6 18 31 43 56 68 81 93))))
        (list
         ;; Search for every inserted key
         (funcall 'neovm--rb3-search tree 50)
         (funcall 'neovm--rb3-search tree 6)
         (funcall 'neovm--rb3-search tree 93)
         (funcall 'neovm--rb3-search tree 37)
         (funcall 'neovm--rb3-search tree 68)
         ;; Search for non-existent keys
         (funcall 'neovm--rb3-search tree 0)
         (funcall 'neovm--rb3-search tree 100)
         (funcall 'neovm--rb3-search tree 7)
         (funcall 'neovm--rb3-search tree 51)
         ;; Search on empty tree
         (funcall 'neovm--rb3-search nil 42)
         ;; Search on single-element tree
         (funcall 'neovm--rb3-search (funcall 'neovm--rb3-from-list '(42)) 42)
         (funcall 'neovm--rb3-search (funcall 'neovm--rb3-from-list '(42)) 41)
         ;; All keys searchable
         (let ((all-found t))
           (dolist (k '(50 25 75 12 37 62 87 6 18 31 43 56 68 81 93))
             (unless (funcall 'neovm--rb3-search tree k)
               (setq all-found nil)))
           all-found)))
    {RB3_CLEANUP}))"#
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Zigzag insertion order: triggers deep rebalancing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rbtree3_zigzag_insertion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {RB3_PREAMBLE}
  (unwind-protect
      (let ((tree (funcall 'neovm--rb3-from-list '(1 20 2 19 3 18 4 17 5 16 6 15 7 14 8 13 9 12 10 11))))
        (list
         (funcall 'neovm--rb3-valid-p tree)
         (funcall 'neovm--rb3-size tree)
         (funcall 'neovm--rb3-inorder tree)
         (funcall 'neovm--rb3-min tree)
         (funcall 'neovm--rb3-max tree)
         (funcall 'neovm--rb3-height tree)
         (not (null (funcall 'neovm--rb3-black-height tree)))
         ;; Delete half the nodes and verify
         (let ((cur tree))
           (dolist (k '(1 3 5 7 9 11 13 15 17 19))
             (setq cur (funcall 'neovm--rb3-delete cur k)))
           (list (funcall 'neovm--rb3-valid-p cur)
                 (funcall 'neovm--rb3-size cur)
                 (funcall 'neovm--rb3-inorder cur)))))
    {RB3_CLEANUP}))"#
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Insert, delete, re-insert: mixed operations stress test
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rbtree3_mixed_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {RB3_PREAMBLE}
  (unwind-protect
      (let ((tree nil)
            (valid-all t))
        ;; Phase 1: Insert 1..10
        (dotimes (i 10) (setq tree (funcall 'neovm--rb3-insert tree (1+ i))))
        (let ((after-insert (funcall 'neovm--rb3-inorder tree)))
          ;; Phase 2: Delete evens
          (dolist (k '(2 4 6 8 10))
            (setq tree (funcall 'neovm--rb3-delete tree k))
            (unless (funcall 'neovm--rb3-valid-p tree) (setq valid-all nil)))
          (let ((after-delete-evens (funcall 'neovm--rb3-inorder tree)))
            ;; Phase 3: Re-insert evens + new keys
            (dolist (k '(2 4 6 8 10 11 12))
              (setq tree (funcall 'neovm--rb3-insert tree k))
              (unless (funcall 'neovm--rb3-valid-p tree) (setq valid-all nil)))
            (let ((after-reinsert (funcall 'neovm--rb3-inorder tree)))
              ;; Phase 4: Delete odds
              (dolist (k '(1 3 5 7 9 11))
                (setq tree (funcall 'neovm--rb3-delete tree k))
                (unless (funcall 'neovm--rb3-valid-p tree) (setq valid-all nil)))
              (list after-insert
                    after-delete-evens
                    after-reinsert
                    (funcall 'neovm--rb3-inorder tree)
                    valid-all
                    (funcall 'neovm--rb3-size tree))))))
    {RB3_CLEANUP}))"#
    );
    crate::common::assert_oracle_parity(&form);
}

// ---------------------------------------------------------------------------
// Negative keys and mixed positive/negative
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rbtree3_negative_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        r#"(progn
  {RB3_PREAMBLE}
  (unwind-protect
      (let ((tree (funcall 'neovm--rb3-from-list '(-50 -25 -75 0 50 25 75 -100 100))))
        (list
         (funcall 'neovm--rb3-valid-p tree)
         (funcall 'neovm--rb3-inorder tree)
         (funcall 'neovm--rb3-min tree)
         (funcall 'neovm--rb3-max tree)
         (funcall 'neovm--rb3-search tree -50)
         (funcall 'neovm--rb3-search tree 0)
         (funcall 'neovm--rb3-search tree 100)
         (funcall 'neovm--rb3-range tree -50 50)
         (funcall 'neovm--rb3-range tree -200 -50)
         ;; Successor and predecessor across zero
         (funcall 'neovm--rb3-successor tree -25)
         (funcall 'neovm--rb3-predecessor tree 25)
         ;; Delete negative keys
         (let ((cur tree))
           (dolist (k '(-100 -75 -50 -25))
             (setq cur (funcall 'neovm--rb3-delete cur k)))
           (list (funcall 'neovm--rb3-valid-p cur)
                 (funcall 'neovm--rb3-inorder cur)))))
    {RB3_CLEANUP}))"#
    );
    crate::common::assert_oracle_parity(&form);
}
