//! Oracle parity tests for a balanced AVL tree implemented in Elisp:
//! insert with LL/RR/LR/RL rotations, search, in-order traversal,
//! delete with rebalancing, height tracking, and balance factor computation.
//! Tests sequential and random insertion orders.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// AVL tree core: insert with rotations, height, balance factor
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_avl_tree_insert_and_rotations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // AVL node: (value height left right) or nil.
    // Implements left/right rotations, insert with rebalancing.
    let form = r#"(progn
  ;; Node accessors
  (fset 'neovm--avl-val (lambda (n) (car n)))
  (fset 'neovm--avl-h (lambda (n) (if n (cadr n) 0)))
  (fset 'neovm--avl-left (lambda (n) (caddr n)))
  (fset 'neovm--avl-right (lambda (n) (cadddr n)))

  ;; Constructor: recomputes height from children
  (fset 'neovm--avl-node
    (lambda (val left right)
      (list val
            (1+ (max (funcall 'neovm--avl-h left)
                     (funcall 'neovm--avl-h right)))
            left right)))

  ;; Balance factor: left height - right height
  (fset 'neovm--avl-bf
    (lambda (n)
      (if n
          (- (funcall 'neovm--avl-h (funcall 'neovm--avl-left n))
             (funcall 'neovm--avl-h (funcall 'neovm--avl-right n)))
        0)))

  ;; Right rotation (for left-heavy, LL case)
  ;;       y            x
  ;;      / \          / \
  ;;     x   C  =>   A   y
  ;;    / \              / \
  ;;   A   B            B   C
  (fset 'neovm--avl-rot-right
    (lambda (y)
      (let ((x (funcall 'neovm--avl-left y))
            (c (funcall 'neovm--avl-right y)))
        (let ((b (funcall 'neovm--avl-right x))
              (a (funcall 'neovm--avl-left x)))
          (let ((new-y (funcall 'neovm--avl-node
                                (funcall 'neovm--avl-val y) b c)))
            (funcall 'neovm--avl-node
                     (funcall 'neovm--avl-val x) a new-y))))))

  ;; Left rotation (for right-heavy, RR case)
  ;;     x              y
  ;;    / \            / \
  ;;   A   y    =>   x   C
  ;;      / \       / \
  ;;     B   C     A   B
  (fset 'neovm--avl-rot-left
    (lambda (x)
      (let ((y (funcall 'neovm--avl-right x))
            (a (funcall 'neovm--avl-left x)))
        (let ((b (funcall 'neovm--avl-left y))
              (c (funcall 'neovm--avl-right y)))
          (let ((new-x (funcall 'neovm--avl-node
                                (funcall 'neovm--avl-val x) a b)))
            (funcall 'neovm--avl-node
                     (funcall 'neovm--avl-val y) new-x c))))))

  ;; Rebalance a node after insert/delete
  (fset 'neovm--avl-balance
    (lambda (node)
      (let ((bf (funcall 'neovm--avl-bf node)))
        (cond
         ;; Left-heavy (bf > 1)
         ((> bf 1)
          (if (< (funcall 'neovm--avl-bf (funcall 'neovm--avl-left node)) 0)
              ;; LR case: left-rotate left child, then right-rotate
              (funcall 'neovm--avl-rot-right
                       (funcall 'neovm--avl-node
                                (funcall 'neovm--avl-val node)
                                (funcall 'neovm--avl-rot-left
                                         (funcall 'neovm--avl-left node))
                                (funcall 'neovm--avl-right node)))
            ;; LL case: right-rotate
            (funcall 'neovm--avl-rot-right node)))
         ;; Right-heavy (bf < -1)
         ((< bf -1)
          (if (> (funcall 'neovm--avl-bf (funcall 'neovm--avl-right node)) 0)
              ;; RL case: right-rotate right child, then left-rotate
              (funcall 'neovm--avl-rot-left
                       (funcall 'neovm--avl-node
                                (funcall 'neovm--avl-val node)
                                (funcall 'neovm--avl-left node)
                                (funcall 'neovm--avl-rot-right
                                         (funcall 'neovm--avl-right node))))
            ;; RR case: left-rotate
            (funcall 'neovm--avl-rot-left node)))
         ;; Balanced
         (t node)))))

  ;; Insert
  (fset 'neovm--avl-insert
    (lambda (tree val)
      (if (null tree)
          (funcall 'neovm--avl-node val nil nil)
        (let ((nv (funcall 'neovm--avl-val tree)))
          (cond
           ((< val nv)
            (funcall 'neovm--avl-balance
                     (funcall 'neovm--avl-node
                              nv
                              (funcall 'neovm--avl-insert
                                       (funcall 'neovm--avl-left tree) val)
                              (funcall 'neovm--avl-right tree))))
           ((> val nv)
            (funcall 'neovm--avl-balance
                     (funcall 'neovm--avl-node
                              nv
                              (funcall 'neovm--avl-left tree)
                              (funcall 'neovm--avl-insert
                                       (funcall 'neovm--avl-right tree) val))))
           (t tree))))))

  ;; In-order traversal
  (fset 'neovm--avl-inorder
    (lambda (tree)
      (if (null tree) nil
        (append (funcall 'neovm--avl-inorder (funcall 'neovm--avl-left tree))
                (list (funcall 'neovm--avl-val tree))
                (funcall 'neovm--avl-inorder (funcall 'neovm--avl-right tree))))))

  ;; Search
  (fset 'neovm--avl-search
    (lambda (tree val)
      (if (null tree) nil
        (let ((nv (funcall 'neovm--avl-val tree)))
          (cond
           ((= val nv) t)
           ((< val nv) (funcall 'neovm--avl-search (funcall 'neovm--avl-left tree) val))
           (t (funcall 'neovm--avl-search (funcall 'neovm--avl-right tree) val)))))))

  (unwind-protect
      (let ((tree nil))
        ;; Insert sequential (worst case for plain BST, triggers rotations)
        (dolist (v '(1 2 3 4 5 6 7))
          (setq tree (funcall 'neovm--avl-insert tree v)))
        (let ((sorted (funcall 'neovm--avl-inorder tree))
              (height (funcall 'neovm--avl-h tree))
              (root-val (funcall 'neovm--avl-val tree))
              (root-bf (funcall 'neovm--avl-bf tree)))
          (list
           sorted
           height
           root-val
           root-bf
           ;; Height should be O(log n) = 3 for 7 nodes
           (<= height 4)
           ;; Balance factor at root should be -1, 0, or 1
           (<= (abs root-bf) 1)
           ;; Search
           (funcall 'neovm--avl-search tree 4)
           (funcall 'neovm--avl-search tree 7)
           (funcall 'neovm--avl-search tree 8))))
    (fmakunbound 'neovm--avl-val)
    (fmakunbound 'neovm--avl-h)
    (fmakunbound 'neovm--avl-left)
    (fmakunbound 'neovm--avl-right)
    (fmakunbound 'neovm--avl-node)
    (fmakunbound 'neovm--avl-bf)
    (fmakunbound 'neovm--avl-rot-right)
    (fmakunbound 'neovm--avl-rot-left)
    (fmakunbound 'neovm--avl-balance)
    (fmakunbound 'neovm--avl-insert)
    (fmakunbound 'neovm--avl-inorder)
    (fmakunbound 'neovm--avl-search)))"#;
    let expect = expect_test::expect![[r#""OK ((1 2 3 4 5 6 7) 3 4 0 t t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// AVL tree: reverse sequential insertion (LL rotation chain)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_avl_tree_reverse_sequential() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Inserting in reverse order triggers left-heavy (LL) rotations.
    let form = r#"(progn
  (fset 'neovm--avl-val (lambda (n) (car n)))
  (fset 'neovm--avl-h (lambda (n) (if n (cadr n) 0)))
  (fset 'neovm--avl-left (lambda (n) (caddr n)))
  (fset 'neovm--avl-right (lambda (n) (cadddr n)))
  (fset 'neovm--avl-node
    (lambda (val left right)
      (list val (1+ (max (funcall 'neovm--avl-h left)
                         (funcall 'neovm--avl-h right)))
            left right)))
  (fset 'neovm--avl-bf
    (lambda (n) (if n (- (funcall 'neovm--avl-h (funcall 'neovm--avl-left n))
                         (funcall 'neovm--avl-h (funcall 'neovm--avl-right n))) 0)))
  (fset 'neovm--avl-rot-right
    (lambda (y)
      (let* ((x (funcall 'neovm--avl-left y))
             (b (funcall 'neovm--avl-right x)))
        (funcall 'neovm--avl-node (funcall 'neovm--avl-val x)
                 (funcall 'neovm--avl-left x)
                 (funcall 'neovm--avl-node (funcall 'neovm--avl-val y)
                          b (funcall 'neovm--avl-right y))))))
  (fset 'neovm--avl-rot-left
    (lambda (x)
      (let* ((y (funcall 'neovm--avl-right x))
             (b (funcall 'neovm--avl-left y)))
        (funcall 'neovm--avl-node (funcall 'neovm--avl-val y)
                 (funcall 'neovm--avl-node (funcall 'neovm--avl-val x)
                          (funcall 'neovm--avl-left x) b)
                 (funcall 'neovm--avl-right y)))))
  (fset 'neovm--avl-balance
    (lambda (node)
      (let ((bf (funcall 'neovm--avl-bf node)))
        (cond
         ((> bf 1)
          (if (< (funcall 'neovm--avl-bf (funcall 'neovm--avl-left node)) 0)
              (funcall 'neovm--avl-rot-right
                       (funcall 'neovm--avl-node (funcall 'neovm--avl-val node)
                                (funcall 'neovm--avl-rot-left (funcall 'neovm--avl-left node))
                                (funcall 'neovm--avl-right node)))
            (funcall 'neovm--avl-rot-right node)))
         ((< bf -1)
          (if (> (funcall 'neovm--avl-bf (funcall 'neovm--avl-right node)) 0)
              (funcall 'neovm--avl-rot-left
                       (funcall 'neovm--avl-node (funcall 'neovm--avl-val node)
                                (funcall 'neovm--avl-left node)
                                (funcall 'neovm--avl-rot-right (funcall 'neovm--avl-right node))))
            (funcall 'neovm--avl-rot-left node)))
         (t node)))))
  (fset 'neovm--avl-insert
    (lambda (tree val)
      (if (null tree)
          (funcall 'neovm--avl-node val nil nil)
        (let ((nv (funcall 'neovm--avl-val tree)))
          (cond
           ((< val nv)
            (funcall 'neovm--avl-balance
                     (funcall 'neovm--avl-node nv
                              (funcall 'neovm--avl-insert (funcall 'neovm--avl-left tree) val)
                              (funcall 'neovm--avl-right tree))))
           ((> val nv)
            (funcall 'neovm--avl-balance
                     (funcall 'neovm--avl-node nv
                              (funcall 'neovm--avl-left tree)
                              (funcall 'neovm--avl-insert (funcall 'neovm--avl-right tree) val))))
           (t tree))))))
  (fset 'neovm--avl-inorder
    (lambda (tree)
      (if (null tree) nil
        (append (funcall 'neovm--avl-inorder (funcall 'neovm--avl-left tree))
                (list (funcall 'neovm--avl-val tree))
                (funcall 'neovm--avl-inorder (funcall 'neovm--avl-right tree))))))
  ;; Check balance at every node
  (fset 'neovm--avl-all-balanced
    (lambda (tree)
      (if (null tree) t
        (and (<= (abs (funcall 'neovm--avl-bf tree)) 1)
             (funcall 'neovm--avl-all-balanced (funcall 'neovm--avl-left tree))
             (funcall 'neovm--avl-all-balanced (funcall 'neovm--avl-right tree))))))

  (unwind-protect
      (let ((tree nil))
        ;; Insert in reverse: 10, 9, 8, ..., 1
        (dolist (v '(10 9 8 7 6 5 4 3 2 1))
          (setq tree (funcall 'neovm--avl-insert tree v)))
        (list
         (funcall 'neovm--avl-inorder tree)
         (funcall 'neovm--avl-h tree)
         (funcall 'neovm--avl-val tree)
         ;; ALL nodes should be balanced
         (funcall 'neovm--avl-all-balanced tree)
         ;; Height should be O(log 10) <= 4
         (<= (funcall 'neovm--avl-h tree) 5)))
    (fmakunbound 'neovm--avl-val)
    (fmakunbound 'neovm--avl-h)
    (fmakunbound 'neovm--avl-left)
    (fmakunbound 'neovm--avl-right)
    (fmakunbound 'neovm--avl-node)
    (fmakunbound 'neovm--avl-bf)
    (fmakunbound 'neovm--avl-rot-right)
    (fmakunbound 'neovm--avl-rot-left)
    (fmakunbound 'neovm--avl-balance)
    (fmakunbound 'neovm--avl-insert)
    (fmakunbound 'neovm--avl-inorder)
    (fmakunbound 'neovm--avl-all-balanced)))"#;
    let expect = expect_test::expect![[r#""OK ((1 2 3 4 5 6 7 8 9 10) 4 7 t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// AVL tree: LR and RL rotation scenarios
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_avl_tree_lr_rl_rotations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Specific insertion orders that force LR and RL double rotations.
    let form = r#"(progn
  (fset 'neovm--avl-val (lambda (n) (car n)))
  (fset 'neovm--avl-h (lambda (n) (if n (cadr n) 0)))
  (fset 'neovm--avl-left (lambda (n) (caddr n)))
  (fset 'neovm--avl-right (lambda (n) (cadddr n)))
  (fset 'neovm--avl-node
    (lambda (val left right)
      (list val (1+ (max (funcall 'neovm--avl-h left)
                         (funcall 'neovm--avl-h right)))
            left right)))
  (fset 'neovm--avl-bf
    (lambda (n) (if n (- (funcall 'neovm--avl-h (funcall 'neovm--avl-left n))
                         (funcall 'neovm--avl-h (funcall 'neovm--avl-right n))) 0)))
  (fset 'neovm--avl-rot-right
    (lambda (y)
      (let* ((x (funcall 'neovm--avl-left y))
             (b (funcall 'neovm--avl-right x)))
        (funcall 'neovm--avl-node (funcall 'neovm--avl-val x)
                 (funcall 'neovm--avl-left x)
                 (funcall 'neovm--avl-node (funcall 'neovm--avl-val y)
                          b (funcall 'neovm--avl-right y))))))
  (fset 'neovm--avl-rot-left
    (lambda (x)
      (let* ((y (funcall 'neovm--avl-right x))
             (b (funcall 'neovm--avl-left y)))
        (funcall 'neovm--avl-node (funcall 'neovm--avl-val y)
                 (funcall 'neovm--avl-node (funcall 'neovm--avl-val x)
                          (funcall 'neovm--avl-left x) b)
                 (funcall 'neovm--avl-right y)))))
  (fset 'neovm--avl-balance
    (lambda (node)
      (let ((bf (funcall 'neovm--avl-bf node)))
        (cond
         ((> bf 1)
          (if (< (funcall 'neovm--avl-bf (funcall 'neovm--avl-left node)) 0)
              (funcall 'neovm--avl-rot-right
                       (funcall 'neovm--avl-node (funcall 'neovm--avl-val node)
                                (funcall 'neovm--avl-rot-left (funcall 'neovm--avl-left node))
                                (funcall 'neovm--avl-right node)))
            (funcall 'neovm--avl-rot-right node)))
         ((< bf -1)
          (if (> (funcall 'neovm--avl-bf (funcall 'neovm--avl-right node)) 0)
              (funcall 'neovm--avl-rot-left
                       (funcall 'neovm--avl-node (funcall 'neovm--avl-val node)
                                (funcall 'neovm--avl-left node)
                                (funcall 'neovm--avl-rot-right (funcall 'neovm--avl-right node))))
            (funcall 'neovm--avl-rot-left node)))
         (t node)))))
  (fset 'neovm--avl-insert
    (lambda (tree val)
      (if (null tree)
          (funcall 'neovm--avl-node val nil nil)
        (let ((nv (funcall 'neovm--avl-val tree)))
          (cond
           ((< val nv)
            (funcall 'neovm--avl-balance
                     (funcall 'neovm--avl-node nv
                              (funcall 'neovm--avl-insert (funcall 'neovm--avl-left tree) val)
                              (funcall 'neovm--avl-right tree))))
           ((> val nv)
            (funcall 'neovm--avl-balance
                     (funcall 'neovm--avl-node nv
                              (funcall 'neovm--avl-left tree)
                              (funcall 'neovm--avl-insert (funcall 'neovm--avl-right tree) val))))
           (t tree))))))
  (fset 'neovm--avl-inorder
    (lambda (tree)
      (if (null tree) nil
        (append (funcall 'neovm--avl-inorder (funcall 'neovm--avl-left tree))
                (list (funcall 'neovm--avl-val tree))
                (funcall 'neovm--avl-inorder (funcall 'neovm--avl-right tree))))))
  (fset 'neovm--avl-all-balanced
    (lambda (tree)
      (if (null tree) t
        (and (<= (abs (funcall 'neovm--avl-bf tree)) 1)
             (funcall 'neovm--avl-all-balanced (funcall 'neovm--avl-left tree))
             (funcall 'neovm--avl-all-balanced (funcall 'neovm--avl-right tree))))))

  (unwind-protect
      (let ()
        ;; LR case: insert 30, 10, 20 (left child has right-heavy subtree)
        (let ((t1 nil))
          (dolist (v '(30 10 20))
            (setq t1 (funcall 'neovm--avl-insert t1 v)))
          ;; RL case: insert 10, 30, 20 (right child has left-heavy subtree)
          (let ((t2 nil))
            (dolist (v '(10 30 20))
              (setq t2 (funcall 'neovm--avl-insert t2 v)))
            ;; Mixed: zigzag insertion pattern
            (let ((t3 nil))
              (dolist (v '(50 20 70 10 30 60 80 25 35))
                (setq t3 (funcall 'neovm--avl-insert t3 v)))
              (list
               ;; LR tree
               (funcall 'neovm--avl-inorder t1)
               (funcall 'neovm--avl-val t1)
               (funcall 'neovm--avl-h t1)
               (funcall 'neovm--avl-all-balanced t1)
               ;; RL tree
               (funcall 'neovm--avl-inorder t2)
               (funcall 'neovm--avl-val t2)
               (funcall 'neovm--avl-h t2)
               (funcall 'neovm--avl-all-balanced t2)
               ;; Mixed tree
               (funcall 'neovm--avl-inorder t3)
               (funcall 'neovm--avl-h t3)
               (funcall 'neovm--avl-all-balanced t3))))))
    (fmakunbound 'neovm--avl-val)
    (fmakunbound 'neovm--avl-h)
    (fmakunbound 'neovm--avl-left)
    (fmakunbound 'neovm--avl-right)
    (fmakunbound 'neovm--avl-node)
    (fmakunbound 'neovm--avl-bf)
    (fmakunbound 'neovm--avl-rot-right)
    (fmakunbound 'neovm--avl-rot-left)
    (fmakunbound 'neovm--avl-balance)
    (fmakunbound 'neovm--avl-insert)
    (fmakunbound 'neovm--avl-inorder)
    (fmakunbound 'neovm--avl-all-balanced)))"#;
    let expect = expect_test::expect![[
        r#""OK ((10 20 30) 20 2 t (10 20 30) 20 2 t (10 20 25 30 35 50 60 70 80) 4 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// AVL tree: delete with rebalancing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_avl_tree_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Delete from AVL tree: find in-order successor for two-child nodes,
    // rebalance on the way up.
    let form = r#"(progn
  (fset 'neovm--avl-val (lambda (n) (car n)))
  (fset 'neovm--avl-h (lambda (n) (if n (cadr n) 0)))
  (fset 'neovm--avl-left (lambda (n) (caddr n)))
  (fset 'neovm--avl-right (lambda (n) (cadddr n)))
  (fset 'neovm--avl-node
    (lambda (val left right)
      (list val (1+ (max (funcall 'neovm--avl-h left)
                         (funcall 'neovm--avl-h right)))
            left right)))
  (fset 'neovm--avl-bf
    (lambda (n) (if n (- (funcall 'neovm--avl-h (funcall 'neovm--avl-left n))
                         (funcall 'neovm--avl-h (funcall 'neovm--avl-right n))) 0)))
  (fset 'neovm--avl-rot-right
    (lambda (y)
      (let* ((x (funcall 'neovm--avl-left y))
             (b (funcall 'neovm--avl-right x)))
        (funcall 'neovm--avl-node (funcall 'neovm--avl-val x)
                 (funcall 'neovm--avl-left x)
                 (funcall 'neovm--avl-node (funcall 'neovm--avl-val y)
                          b (funcall 'neovm--avl-right y))))))
  (fset 'neovm--avl-rot-left
    (lambda (x)
      (let* ((y (funcall 'neovm--avl-right x))
             (b (funcall 'neovm--avl-left y)))
        (funcall 'neovm--avl-node (funcall 'neovm--avl-val y)
                 (funcall 'neovm--avl-node (funcall 'neovm--avl-val x)
                          (funcall 'neovm--avl-left x) b)
                 (funcall 'neovm--avl-right y)))))
  (fset 'neovm--avl-balance
    (lambda (node)
      (let ((bf (funcall 'neovm--avl-bf node)))
        (cond
         ((> bf 1)
          (if (< (funcall 'neovm--avl-bf (funcall 'neovm--avl-left node)) 0)
              (funcall 'neovm--avl-rot-right
                       (funcall 'neovm--avl-node (funcall 'neovm--avl-val node)
                                (funcall 'neovm--avl-rot-left (funcall 'neovm--avl-left node))
                                (funcall 'neovm--avl-right node)))
            (funcall 'neovm--avl-rot-right node)))
         ((< bf -1)
          (if (> (funcall 'neovm--avl-bf (funcall 'neovm--avl-right node)) 0)
              (funcall 'neovm--avl-rot-left
                       (funcall 'neovm--avl-node (funcall 'neovm--avl-val node)
                                (funcall 'neovm--avl-left node)
                                (funcall 'neovm--avl-rot-right (funcall 'neovm--avl-right node))))
            (funcall 'neovm--avl-rot-left node)))
         (t node)))))
  (fset 'neovm--avl-insert
    (lambda (tree val)
      (if (null tree)
          (funcall 'neovm--avl-node val nil nil)
        (let ((nv (funcall 'neovm--avl-val tree)))
          (cond
           ((< val nv)
            (funcall 'neovm--avl-balance
                     (funcall 'neovm--avl-node nv
                              (funcall 'neovm--avl-insert (funcall 'neovm--avl-left tree) val)
                              (funcall 'neovm--avl-right tree))))
           ((> val nv)
            (funcall 'neovm--avl-balance
                     (funcall 'neovm--avl-node nv
                              (funcall 'neovm--avl-left tree)
                              (funcall 'neovm--avl-insert (funcall 'neovm--avl-right tree) val))))
           (t tree))))))
  (fset 'neovm--avl-inorder
    (lambda (tree)
      (if (null tree) nil
        (append (funcall 'neovm--avl-inorder (funcall 'neovm--avl-left tree))
                (list (funcall 'neovm--avl-val tree))
                (funcall 'neovm--avl-inorder (funcall 'neovm--avl-right tree))))))
  (fset 'neovm--avl-search
    (lambda (tree val)
      (if (null tree) nil
        (let ((nv (funcall 'neovm--avl-val tree)))
          (cond
           ((= val nv) t)
           ((< val nv) (funcall 'neovm--avl-search (funcall 'neovm--avl-left tree) val))
           (t (funcall 'neovm--avl-search (funcall 'neovm--avl-right tree) val)))))))
  (fset 'neovm--avl-all-balanced
    (lambda (tree)
      (if (null tree) t
        (and (<= (abs (funcall 'neovm--avl-bf tree)) 1)
             (funcall 'neovm--avl-all-balanced (funcall 'neovm--avl-left tree))
             (funcall 'neovm--avl-all-balanced (funcall 'neovm--avl-right tree))))))
  ;; Find minimum value
  (fset 'neovm--avl-min
    (lambda (tree)
      (if (null (funcall 'neovm--avl-left tree))
          (funcall 'neovm--avl-val tree)
        (funcall 'neovm--avl-min (funcall 'neovm--avl-left tree)))))
  ;; Delete
  (fset 'neovm--avl-delete
    (lambda (tree val)
      (if (null tree) nil
        (let ((nv (funcall 'neovm--avl-val tree))
              (left (funcall 'neovm--avl-left tree))
              (right (funcall 'neovm--avl-right tree)))
          (cond
           ((< val nv)
            (funcall 'neovm--avl-balance
                     (funcall 'neovm--avl-node nv
                              (funcall 'neovm--avl-delete left val)
                              right)))
           ((> val nv)
            (funcall 'neovm--avl-balance
                     (funcall 'neovm--avl-node nv
                              left
                              (funcall 'neovm--avl-delete right val))))
           ;; Found: three cases
           ((null left) right)
           ((null right) left)
           (t
            (let ((succ (funcall 'neovm--avl-min right)))
              (funcall 'neovm--avl-balance
                       (funcall 'neovm--avl-node succ
                                left
                                (funcall 'neovm--avl-delete right succ))))))))))

  (unwind-protect
      (let ((tree nil))
        ;; Build a tree with 15 elements
        (dolist (v '(8 4 12 2 6 10 14 1 3 5 7 9 11 13 15))
          (setq tree (funcall 'neovm--avl-insert tree v)))
        (let ((before (funcall 'neovm--avl-inorder tree))
              (before-balanced (funcall 'neovm--avl-all-balanced tree)))
          ;; Delete leaf
          (setq tree (funcall 'neovm--avl-delete tree 1))
          (let ((after-del-1 (funcall 'neovm--avl-inorder tree))
                (bal-1 (funcall 'neovm--avl-all-balanced tree)))
            ;; Delete node with one child
            (setq tree (funcall 'neovm--avl-delete tree 2))
            (let ((after-del-2 (funcall 'neovm--avl-inorder tree))
                  (bal-2 (funcall 'neovm--avl-all-balanced tree)))
              ;; Delete node with two children (root)
              (setq tree (funcall 'neovm--avl-delete tree 8))
              (let ((after-del-8 (funcall 'neovm--avl-inorder tree))
                    (bal-8 (funcall 'neovm--avl-all-balanced tree)))
                ;; Delete several more
                (dolist (v '(12 4 14))
                  (setq tree (funcall 'neovm--avl-delete tree v)))
                (list
                 before
                 before-balanced
                 after-del-1
                 bal-1
                 after-del-2
                 bal-2
                 after-del-8
                 bal-8
                 (funcall 'neovm--avl-inorder tree)
                 (funcall 'neovm--avl-all-balanced tree)
                 ;; Deleted elements should not be found
                 (funcall 'neovm--avl-search tree 1)
                 (funcall 'neovm--avl-search tree 8)
                 ;; Remaining elements should be found
                 (funcall 'neovm--avl-search tree 5)
                 (funcall 'neovm--avl-search tree 15)))))))
    (fmakunbound 'neovm--avl-val)
    (fmakunbound 'neovm--avl-h)
    (fmakunbound 'neovm--avl-left)
    (fmakunbound 'neovm--avl-right)
    (fmakunbound 'neovm--avl-node)
    (fmakunbound 'neovm--avl-bf)
    (fmakunbound 'neovm--avl-rot-right)
    (fmakunbound 'neovm--avl-rot-left)
    (fmakunbound 'neovm--avl-balance)
    (fmakunbound 'neovm--avl-insert)
    (fmakunbound 'neovm--avl-inorder)
    (fmakunbound 'neovm--avl-search)
    (fmakunbound 'neovm--avl-all-balanced)
    (fmakunbound 'neovm--avl-min)
    (fmakunbound 'neovm--avl-delete)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5 6 7 8 9 10 11 12 13 14 15) t (2 3 4 5 6 7 8 9 10 11 12 13 14 15) t (3 4 5 6 7 8 9 10 11 12 13 14 15) t (3 4 5 6 7 9 10 11 12 13 14 15) t (3 5 6 7 9 10 11 13 15) t nil nil t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// AVL tree: pseudo-random insertion order and bulk operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_avl_tree_pseudorandom_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use a simple linear congruential generator to produce a
    // pseudo-random insertion order, then verify AVL invariants.
    let form = r#"(progn
  (fset 'neovm--avl-val (lambda (n) (car n)))
  (fset 'neovm--avl-h (lambda (n) (if n (cadr n) 0)))
  (fset 'neovm--avl-left (lambda (n) (caddr n)))
  (fset 'neovm--avl-right (lambda (n) (cadddr n)))
  (fset 'neovm--avl-node
    (lambda (val left right)
      (list val (1+ (max (funcall 'neovm--avl-h left)
                         (funcall 'neovm--avl-h right)))
            left right)))
  (fset 'neovm--avl-bf
    (lambda (n) (if n (- (funcall 'neovm--avl-h (funcall 'neovm--avl-left n))
                         (funcall 'neovm--avl-h (funcall 'neovm--avl-right n))) 0)))
  (fset 'neovm--avl-rot-right
    (lambda (y)
      (let* ((x (funcall 'neovm--avl-left y))
             (b (funcall 'neovm--avl-right x)))
        (funcall 'neovm--avl-node (funcall 'neovm--avl-val x)
                 (funcall 'neovm--avl-left x)
                 (funcall 'neovm--avl-node (funcall 'neovm--avl-val y)
                          b (funcall 'neovm--avl-right y))))))
  (fset 'neovm--avl-rot-left
    (lambda (x)
      (let* ((y (funcall 'neovm--avl-right x))
             (b (funcall 'neovm--avl-left y)))
        (funcall 'neovm--avl-node (funcall 'neovm--avl-val y)
                 (funcall 'neovm--avl-node (funcall 'neovm--avl-val x)
                          (funcall 'neovm--avl-left x) b)
                 (funcall 'neovm--avl-right y)))))
  (fset 'neovm--avl-balance
    (lambda (node)
      (let ((bf (funcall 'neovm--avl-bf node)))
        (cond
         ((> bf 1)
          (if (< (funcall 'neovm--avl-bf (funcall 'neovm--avl-left node)) 0)
              (funcall 'neovm--avl-rot-right
                       (funcall 'neovm--avl-node (funcall 'neovm--avl-val node)
                                (funcall 'neovm--avl-rot-left (funcall 'neovm--avl-left node))
                                (funcall 'neovm--avl-right node)))
            (funcall 'neovm--avl-rot-right node)))
         ((< bf -1)
          (if (> (funcall 'neovm--avl-bf (funcall 'neovm--avl-right node)) 0)
              (funcall 'neovm--avl-rot-left
                       (funcall 'neovm--avl-node (funcall 'neovm--avl-val node)
                                (funcall 'neovm--avl-left node)
                                (funcall 'neovm--avl-rot-right (funcall 'neovm--avl-right node))))
            (funcall 'neovm--avl-rot-left node)))
         (t node)))))
  (fset 'neovm--avl-insert
    (lambda (tree val)
      (if (null tree)
          (funcall 'neovm--avl-node val nil nil)
        (let ((nv (funcall 'neovm--avl-val tree)))
          (cond
           ((< val nv)
            (funcall 'neovm--avl-balance
                     (funcall 'neovm--avl-node nv
                              (funcall 'neovm--avl-insert (funcall 'neovm--avl-left tree) val)
                              (funcall 'neovm--avl-right tree))))
           ((> val nv)
            (funcall 'neovm--avl-balance
                     (funcall 'neovm--avl-node nv
                              (funcall 'neovm--avl-left tree)
                              (funcall 'neovm--avl-insert (funcall 'neovm--avl-right tree) val))))
           (t tree))))))
  (fset 'neovm--avl-inorder
    (lambda (tree)
      (if (null tree) nil
        (append (funcall 'neovm--avl-inorder (funcall 'neovm--avl-left tree))
                (list (funcall 'neovm--avl-val tree))
                (funcall 'neovm--avl-inorder (funcall 'neovm--avl-right tree))))))
  (fset 'neovm--avl-all-balanced
    (lambda (tree)
      (if (null tree) t
        (and (<= (abs (funcall 'neovm--avl-bf tree)) 1)
             (funcall 'neovm--avl-all-balanced (funcall 'neovm--avl-left tree))
             (funcall 'neovm--avl-all-balanced (funcall 'neovm--avl-right tree))))))
  (fset 'neovm--avl-count
    (lambda (tree)
      (if (null tree) 0
        (+ 1 (funcall 'neovm--avl-count (funcall 'neovm--avl-left tree))
             (funcall 'neovm--avl-count (funcall 'neovm--avl-right tree))))))

  (unwind-protect
      (let ((tree nil)
            (seed 7)
            (inserted nil))
        ;; LCG: next = (seed * 13 + 5) mod 101
        (dotimes (_ 20)
          (setq seed (% (+ (* seed 13) 5) 101))
          (unless (member seed inserted)
            (setq inserted (cons seed inserted))
            (setq tree (funcall 'neovm--avl-insert tree seed))))
        (let ((sorted (funcall 'neovm--avl-inorder tree))
              (count (funcall 'neovm--avl-count tree))
              (height (funcall 'neovm--avl-h tree))
              (balanced (funcall 'neovm--avl-all-balanced tree)))
          (list
           ;; In-order should be sorted
           (equal sorted (sort (copy-sequence sorted) #'<))
           ;; Count should match unique inserted
           (= count (length inserted))
           ;; Tree is balanced at every node
           balanced
           ;; Height bounded by 1.44 * log2(count+2)
           height
           count
           ;; All inserted values should be found
           sorted)))
    (fmakunbound 'neovm--avl-val)
    (fmakunbound 'neovm--avl-h)
    (fmakunbound 'neovm--avl-left)
    (fmakunbound 'neovm--avl-right)
    (fmakunbound 'neovm--avl-node)
    (fmakunbound 'neovm--avl-bf)
    (fmakunbound 'neovm--avl-rot-right)
    (fmakunbound 'neovm--avl-rot-left)
    (fmakunbound 'neovm--avl-balance)
    (fmakunbound 'neovm--avl-insert)
    (fmakunbound 'neovm--avl-inorder)
    (fmakunbound 'neovm--avl-all-balanced)
    (fmakunbound 'neovm--avl-count)))"#;
    let expect = expect_test::expect![[
        r#""OK (t t t 5 20 (3 14 17 22 24 29 30 32 33 41 44 51 62 64 72 79 89 90 92 96))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// AVL tree: insert, delete interleaved, then rebuild
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_avl_tree_insert_delete_interleaved() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Interleave inserts and deletes to stress rebalancing.
    let form = r#"(progn
  (fset 'neovm--avl-val (lambda (n) (car n)))
  (fset 'neovm--avl-h (lambda (n) (if n (cadr n) 0)))
  (fset 'neovm--avl-left (lambda (n) (caddr n)))
  (fset 'neovm--avl-right (lambda (n) (cadddr n)))
  (fset 'neovm--avl-node
    (lambda (val left right)
      (list val (1+ (max (funcall 'neovm--avl-h left)
                         (funcall 'neovm--avl-h right)))
            left right)))
  (fset 'neovm--avl-bf
    (lambda (n) (if n (- (funcall 'neovm--avl-h (funcall 'neovm--avl-left n))
                         (funcall 'neovm--avl-h (funcall 'neovm--avl-right n))) 0)))
  (fset 'neovm--avl-rot-right
    (lambda (y)
      (let* ((x (funcall 'neovm--avl-left y))
             (b (funcall 'neovm--avl-right x)))
        (funcall 'neovm--avl-node (funcall 'neovm--avl-val x)
                 (funcall 'neovm--avl-left x)
                 (funcall 'neovm--avl-node (funcall 'neovm--avl-val y)
                          b (funcall 'neovm--avl-right y))))))
  (fset 'neovm--avl-rot-left
    (lambda (x)
      (let* ((y (funcall 'neovm--avl-right x))
             (b (funcall 'neovm--avl-left y)))
        (funcall 'neovm--avl-node (funcall 'neovm--avl-val y)
                 (funcall 'neovm--avl-node (funcall 'neovm--avl-val x)
                          (funcall 'neovm--avl-left x) b)
                 (funcall 'neovm--avl-right y)))))
  (fset 'neovm--avl-balance
    (lambda (node)
      (let ((bf (funcall 'neovm--avl-bf node)))
        (cond
         ((> bf 1)
          (if (< (funcall 'neovm--avl-bf (funcall 'neovm--avl-left node)) 0)
              (funcall 'neovm--avl-rot-right
                       (funcall 'neovm--avl-node (funcall 'neovm--avl-val node)
                                (funcall 'neovm--avl-rot-left (funcall 'neovm--avl-left node))
                                (funcall 'neovm--avl-right node)))
            (funcall 'neovm--avl-rot-right node)))
         ((< bf -1)
          (if (> (funcall 'neovm--avl-bf (funcall 'neovm--avl-right node)) 0)
              (funcall 'neovm--avl-rot-left
                       (funcall 'neovm--avl-node (funcall 'neovm--avl-val node)
                                (funcall 'neovm--avl-left node)
                                (funcall 'neovm--avl-rot-right (funcall 'neovm--avl-right node))))
            (funcall 'neovm--avl-rot-left node)))
         (t node)))))
  (fset 'neovm--avl-insert
    (lambda (tree val)
      (if (null tree)
          (funcall 'neovm--avl-node val nil nil)
        (let ((nv (funcall 'neovm--avl-val tree)))
          (cond
           ((< val nv)
            (funcall 'neovm--avl-balance
                     (funcall 'neovm--avl-node nv
                              (funcall 'neovm--avl-insert (funcall 'neovm--avl-left tree) val)
                              (funcall 'neovm--avl-right tree))))
           ((> val nv)
            (funcall 'neovm--avl-balance
                     (funcall 'neovm--avl-node nv
                              (funcall 'neovm--avl-left tree)
                              (funcall 'neovm--avl-insert (funcall 'neovm--avl-right tree) val))))
           (t tree))))))
  (fset 'neovm--avl-min
    (lambda (tree)
      (if (null (funcall 'neovm--avl-left tree))
          (funcall 'neovm--avl-val tree)
        (funcall 'neovm--avl-min (funcall 'neovm--avl-left tree)))))
  (fset 'neovm--avl-delete
    (lambda (tree val)
      (if (null tree) nil
        (let ((nv (funcall 'neovm--avl-val tree))
              (left (funcall 'neovm--avl-left tree))
              (right (funcall 'neovm--avl-right tree)))
          (cond
           ((< val nv)
            (funcall 'neovm--avl-balance
                     (funcall 'neovm--avl-node nv
                              (funcall 'neovm--avl-delete left val) right)))
           ((> val nv)
            (funcall 'neovm--avl-balance
                     (funcall 'neovm--avl-node nv
                              left (funcall 'neovm--avl-delete right val))))
           ((null left) right)
           ((null right) left)
           (t (let ((succ (funcall 'neovm--avl-min right)))
                (funcall 'neovm--avl-balance
                         (funcall 'neovm--avl-node succ left
                                  (funcall 'neovm--avl-delete right succ))))))))))
  (fset 'neovm--avl-inorder
    (lambda (tree)
      (if (null tree) nil
        (append (funcall 'neovm--avl-inorder (funcall 'neovm--avl-left tree))
                (list (funcall 'neovm--avl-val tree))
                (funcall 'neovm--avl-inorder (funcall 'neovm--avl-right tree))))))
  (fset 'neovm--avl-search
    (lambda (tree val)
      (if (null tree) nil
        (let ((nv (funcall 'neovm--avl-val tree)))
          (cond
           ((= val nv) t)
           ((< val nv) (funcall 'neovm--avl-search (funcall 'neovm--avl-left tree) val))
           (t (funcall 'neovm--avl-search (funcall 'neovm--avl-right tree) val)))))))
  (fset 'neovm--avl-all-balanced
    (lambda (tree)
      (if (null tree) t
        (and (<= (abs (funcall 'neovm--avl-bf tree)) 1)
             (funcall 'neovm--avl-all-balanced (funcall 'neovm--avl-left tree))
             (funcall 'neovm--avl-all-balanced (funcall 'neovm--avl-right tree))))))

  (unwind-protect
      (let ((tree nil))
        ;; Phase 1: insert 1..10
        (dolist (v '(1 2 3 4 5 6 7 8 9 10))
          (setq tree (funcall 'neovm--avl-insert tree v)))
        (let ((after-insert (funcall 'neovm--avl-inorder tree))
              (bal-1 (funcall 'neovm--avl-all-balanced tree)))
          ;; Phase 2: delete even numbers
          (dolist (v '(2 4 6 8 10))
            (setq tree (funcall 'neovm--avl-delete tree v)))
          (let ((after-del-evens (funcall 'neovm--avl-inorder tree))
                (bal-2 (funcall 'neovm--avl-all-balanced tree)))
            ;; Phase 3: insert 20..25
            (dolist (v '(20 21 22 23 24 25))
              (setq tree (funcall 'neovm--avl-insert tree v)))
            (let ((after-add-more (funcall 'neovm--avl-inorder tree))
                  (bal-3 (funcall 'neovm--avl-all-balanced tree)))
              ;; Phase 4: delete odds and some new values
              (dolist (v '(1 3 5 7 9 22))
                (setq tree (funcall 'neovm--avl-delete tree v)))
              (list
               after-insert
               bal-1
               after-del-evens
               bal-2
               after-add-more
               bal-3
               (funcall 'neovm--avl-inorder tree)
               (funcall 'neovm--avl-all-balanced tree)
               ;; Verify specific elements
               (funcall 'neovm--avl-search tree 20)
               (funcall 'neovm--avl-search tree 1)
               (funcall 'neovm--avl-search tree 25)
               (funcall 'neovm--avl-search tree 22))))))
    (fmakunbound 'neovm--avl-val)
    (fmakunbound 'neovm--avl-h)
    (fmakunbound 'neovm--avl-left)
    (fmakunbound 'neovm--avl-right)
    (fmakunbound 'neovm--avl-node)
    (fmakunbound 'neovm--avl-bf)
    (fmakunbound 'neovm--avl-rot-right)
    (fmakunbound 'neovm--avl-rot-left)
    (fmakunbound 'neovm--avl-balance)
    (fmakunbound 'neovm--avl-insert)
    (fmakunbound 'neovm--avl-min)
    (fmakunbound 'neovm--avl-delete)
    (fmakunbound 'neovm--avl-inorder)
    (fmakunbound 'neovm--avl-search)
    (fmakunbound 'neovm--avl-all-balanced)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5 6 7 8 9 10) t (1 3 5 7 9) t (1 3 5 7 9 20 21 22 23 24 25) t (20 21 23 24 25) t t nil t nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// AVL tree: delete all nodes one by one, verify empty
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_avl_tree_delete_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Insert several elements, then delete them all one by one,
    // checking balance after each deletion.
    let form = r#"(progn
  (fset 'neovm--avl-val (lambda (n) (car n)))
  (fset 'neovm--avl-h (lambda (n) (if n (cadr n) 0)))
  (fset 'neovm--avl-left (lambda (n) (caddr n)))
  (fset 'neovm--avl-right (lambda (n) (cadddr n)))
  (fset 'neovm--avl-node
    (lambda (val left right)
      (list val (1+ (max (funcall 'neovm--avl-h left)
                         (funcall 'neovm--avl-h right)))
            left right)))
  (fset 'neovm--avl-bf
    (lambda (n) (if n (- (funcall 'neovm--avl-h (funcall 'neovm--avl-left n))
                         (funcall 'neovm--avl-h (funcall 'neovm--avl-right n))) 0)))
  (fset 'neovm--avl-rot-right
    (lambda (y)
      (let* ((x (funcall 'neovm--avl-left y))
             (b (funcall 'neovm--avl-right x)))
        (funcall 'neovm--avl-node (funcall 'neovm--avl-val x)
                 (funcall 'neovm--avl-left x)
                 (funcall 'neovm--avl-node (funcall 'neovm--avl-val y)
                          b (funcall 'neovm--avl-right y))))))
  (fset 'neovm--avl-rot-left
    (lambda (x)
      (let* ((y (funcall 'neovm--avl-right x))
             (b (funcall 'neovm--avl-left y)))
        (funcall 'neovm--avl-node (funcall 'neovm--avl-val y)
                 (funcall 'neovm--avl-node (funcall 'neovm--avl-val x)
                          (funcall 'neovm--avl-left x) b)
                 (funcall 'neovm--avl-right y)))))
  (fset 'neovm--avl-balance
    (lambda (node)
      (let ((bf (funcall 'neovm--avl-bf node)))
        (cond
         ((> bf 1)
          (if (< (funcall 'neovm--avl-bf (funcall 'neovm--avl-left node)) 0)
              (funcall 'neovm--avl-rot-right
                       (funcall 'neovm--avl-node (funcall 'neovm--avl-val node)
                                (funcall 'neovm--avl-rot-left (funcall 'neovm--avl-left node))
                                (funcall 'neovm--avl-right node)))
            (funcall 'neovm--avl-rot-right node)))
         ((< bf -1)
          (if (> (funcall 'neovm--avl-bf (funcall 'neovm--avl-right node)) 0)
              (funcall 'neovm--avl-rot-left
                       (funcall 'neovm--avl-node (funcall 'neovm--avl-val node)
                                (funcall 'neovm--avl-left node)
                                (funcall 'neovm--avl-rot-right (funcall 'neovm--avl-right node))))
            (funcall 'neovm--avl-rot-left node)))
         (t node)))))
  (fset 'neovm--avl-insert
    (lambda (tree val)
      (if (null tree)
          (funcall 'neovm--avl-node val nil nil)
        (let ((nv (funcall 'neovm--avl-val tree)))
          (cond
           ((< val nv)
            (funcall 'neovm--avl-balance
                     (funcall 'neovm--avl-node nv
                              (funcall 'neovm--avl-insert (funcall 'neovm--avl-left tree) val)
                              (funcall 'neovm--avl-right tree))))
           ((> val nv)
            (funcall 'neovm--avl-balance
                     (funcall 'neovm--avl-node nv
                              (funcall 'neovm--avl-left tree)
                              (funcall 'neovm--avl-insert (funcall 'neovm--avl-right tree) val))))
           (t tree))))))
  (fset 'neovm--avl-min
    (lambda (tree)
      (if (null (funcall 'neovm--avl-left tree))
          (funcall 'neovm--avl-val tree)
        (funcall 'neovm--avl-min (funcall 'neovm--avl-left tree)))))
  (fset 'neovm--avl-delete
    (lambda (tree val)
      (if (null tree) nil
        (let ((nv (funcall 'neovm--avl-val tree))
              (left (funcall 'neovm--avl-left tree))
              (right (funcall 'neovm--avl-right tree)))
          (cond
           ((< val nv)
            (funcall 'neovm--avl-balance
                     (funcall 'neovm--avl-node nv
                              (funcall 'neovm--avl-delete left val) right)))
           ((> val nv)
            (funcall 'neovm--avl-balance
                     (funcall 'neovm--avl-node nv
                              left (funcall 'neovm--avl-delete right val))))
           ((null left) right)
           ((null right) left)
           (t (let ((succ (funcall 'neovm--avl-min right)))
                (funcall 'neovm--avl-balance
                         (funcall 'neovm--avl-node succ left
                                  (funcall 'neovm--avl-delete right succ))))))))))
  (fset 'neovm--avl-inorder
    (lambda (tree)
      (if (null tree) nil
        (append (funcall 'neovm--avl-inorder (funcall 'neovm--avl-left tree))
                (list (funcall 'neovm--avl-val tree))
                (funcall 'neovm--avl-inorder (funcall 'neovm--avl-right tree))))))
  (fset 'neovm--avl-all-balanced
    (lambda (tree)
      (if (null tree) t
        (and (<= (abs (funcall 'neovm--avl-bf tree)) 1)
             (funcall 'neovm--avl-all-balanced (funcall 'neovm--avl-left tree))
             (funcall 'neovm--avl-all-balanced (funcall 'neovm--avl-right tree))))))
  (fset 'neovm--avl-count
    (lambda (tree)
      (if (null tree) 0
        (+ 1 (funcall 'neovm--avl-count (funcall 'neovm--avl-left tree))
             (funcall 'neovm--avl-count (funcall 'neovm--avl-right tree))))))

  (unwind-protect
      (let ((tree nil)
            (elements '(5 3 8 1 4 7 10 2 6 9))
            (results nil))
        ;; Insert all
        (dolist (v elements)
          (setq tree (funcall 'neovm--avl-insert tree v)))
        (setq results (list (funcall 'neovm--avl-inorder tree)
                            (funcall 'neovm--avl-count tree)
                            (funcall 'neovm--avl-all-balanced tree)))
        ;; Delete all in a different order, checking balance after each
        (let ((all-balanced t))
          (dolist (v '(8 3 10 1 5 7 2 9 4 6))
            (setq tree (funcall 'neovm--avl-delete tree v))
            (unless (funcall 'neovm--avl-all-balanced tree)
              (setq all-balanced nil)))
          (append results
                  (list
                   ;; Tree should be nil after deleting everything
                   (null tree)
                   ;; All intermediate states were balanced
                   all-balanced
                   ;; Deleting from empty tree
                   (null (funcall 'neovm--avl-delete nil 42))))))
    (fmakunbound 'neovm--avl-val)
    (fmakunbound 'neovm--avl-h)
    (fmakunbound 'neovm--avl-left)
    (fmakunbound 'neovm--avl-right)
    (fmakunbound 'neovm--avl-node)
    (fmakunbound 'neovm--avl-bf)
    (fmakunbound 'neovm--avl-rot-right)
    (fmakunbound 'neovm--avl-rot-left)
    (fmakunbound 'neovm--avl-balance)
    (fmakunbound 'neovm--avl-insert)
    (fmakunbound 'neovm--avl-min)
    (fmakunbound 'neovm--avl-delete)
    (fmakunbound 'neovm--avl-inorder)
    (fmakunbound 'neovm--avl-all-balanced)
    (fmakunbound 'neovm--avl-count)))"#;
    let expect = expect_test::expect![[r#""OK ((1 2 3 4 5 6 7 8 9 10) 10 t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
