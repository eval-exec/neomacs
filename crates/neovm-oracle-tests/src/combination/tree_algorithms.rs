//! Oracle parity tests for tree data structure algorithms:
//! binary search tree operations (insert, lookup, delete, traversal),
//! tree height and balance checking, lowest common ancestor,
//! serialize/deserialize trees, tree map/fold, and trie (prefix tree)
//! for string lookup.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Binary search tree: insert, lookup, delete, in-order traversal
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_tree_bst_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // BST node: (value left right) or nil for empty.
    // Implements insert, lookup, in-order traversal, min, max, and delete.
    let form = r#"(progn
  (fset 'neovm--test-bst-make
    (lambda (val left right)
      (list val left right)))

  (fset 'neovm--test-bst-val (lambda (node) (car node)))
  (fset 'neovm--test-bst-left (lambda (node) (cadr node)))
  (fset 'neovm--test-bst-right (lambda (node) (caddr node)))

  (fset 'neovm--test-bst-insert
    (lambda (tree val)
      (if (null tree)
          (funcall 'neovm--test-bst-make val nil nil)
        (let ((nv (funcall 'neovm--test-bst-val tree)))
          (cond
           ((< val nv)
            (funcall 'neovm--test-bst-make
                     nv
                     (funcall 'neovm--test-bst-insert
                              (funcall 'neovm--test-bst-left tree) val)
                     (funcall 'neovm--test-bst-right tree)))
           ((> val nv)
            (funcall 'neovm--test-bst-make
                     nv
                     (funcall 'neovm--test-bst-left tree)
                     (funcall 'neovm--test-bst-insert
                              (funcall 'neovm--test-bst-right tree) val)))
           (t tree))))))

  (fset 'neovm--test-bst-lookup
    (lambda (tree val)
      (if (null tree) nil
        (let ((nv (funcall 'neovm--test-bst-val tree)))
          (cond
           ((= val nv) t)
           ((< val nv)
            (funcall 'neovm--test-bst-lookup
                     (funcall 'neovm--test-bst-left tree) val))
           (t (funcall 'neovm--test-bst-lookup
                       (funcall 'neovm--test-bst-right tree) val)))))))

  (fset 'neovm--test-bst-inorder
    (lambda (tree)
      (if (null tree) nil
        (append (funcall 'neovm--test-bst-inorder
                         (funcall 'neovm--test-bst-left tree))
                (list (funcall 'neovm--test-bst-val tree))
                (funcall 'neovm--test-bst-inorder
                         (funcall 'neovm--test-bst-right tree))))))

  (fset 'neovm--test-bst-min
    (lambda (tree)
      (if (null (funcall 'neovm--test-bst-left tree))
          (funcall 'neovm--test-bst-val tree)
        (funcall 'neovm--test-bst-min
                 (funcall 'neovm--test-bst-left tree)))))

  (fset 'neovm--test-bst-delete
    (lambda (tree val)
      (if (null tree) nil
        (let ((nv (funcall 'neovm--test-bst-val tree))
              (left (funcall 'neovm--test-bst-left tree))
              (right (funcall 'neovm--test-bst-right tree)))
          (cond
           ((< val nv)
            (funcall 'neovm--test-bst-make
                     nv
                     (funcall 'neovm--test-bst-delete left val)
                     right))
           ((> val nv)
            (funcall 'neovm--test-bst-make
                     nv
                     left
                     (funcall 'neovm--test-bst-delete right val)))
           ;; val == nv: three cases
           ((null left) right)
           ((null right) left)
           (t
            ;; Replace with in-order successor (min of right subtree)
            (let ((successor (funcall 'neovm--test-bst-min right)))
              (funcall 'neovm--test-bst-make
                       successor
                       left
                       (funcall 'neovm--test-bst-delete
                                right successor)))))))))

  (unwind-protect
      (let ((tree nil))
        ;; Insert values: 5, 3, 7, 1, 4, 6, 8, 2
        (dolist (v '(5 3 7 1 4 6 8 2))
          (setq tree (funcall 'neovm--test-bst-insert tree v)))
        (let ((sorted (funcall 'neovm--test-bst-inorder tree))
              (found-4 (funcall 'neovm--test-bst-lookup tree 4))
              (found-9 (funcall 'neovm--test-bst-lookup tree 9))
              (min-val (funcall 'neovm--test-bst-min tree)))
          ;; Delete node with two children (5, the root)
          (let ((tree2 (funcall 'neovm--test-bst-delete tree 5)))
            (let ((sorted2 (funcall 'neovm--test-bst-inorder tree2))
                  (found-5 (funcall 'neovm--test-bst-lookup tree2 5)))
              ;; Delete leaf (2)
              (let ((tree3 (funcall 'neovm--test-bst-delete tree2 2)))
                (let ((sorted3 (funcall 'neovm--test-bst-inorder tree3)))
                  ;; Delete node with one child (1, only has right child after 2 deleted)
                  (let ((tree4 (funcall 'neovm--test-bst-delete tree3 1)))
                    (list sorted found-4 found-9 min-val
                          sorted2 found-5
                          sorted3
                          (funcall 'neovm--test-bst-inorder tree4)))))))))
    (fmakunbound 'neovm--test-bst-make)
    (fmakunbound 'neovm--test-bst-val)
    (fmakunbound 'neovm--test-bst-left)
    (fmakunbound 'neovm--test-bst-right)
    (fmakunbound 'neovm--test-bst-insert)
    (fmakunbound 'neovm--test-bst-lookup)
    (fmakunbound 'neovm--test-bst-inorder)
    (fmakunbound 'neovm--test-bst-min)
    (fmakunbound 'neovm--test-bst-delete)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5 6 7 8) t nil 1 (1 2 3 4 6 7 8) nil (1 3 4 6 7 8) (3 4 6 7 8))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Tree height and balance checking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_tree_height_balance() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compute tree height and check if a binary tree is height-balanced
    // (difference between left/right subtree heights <= 1 at every node).
    // Tree node: (value left right) or nil.
    let form = r#"(progn
  (fset 'neovm--test-tree-height
    (lambda (tree)
      (if (null tree) 0
        (1+ (max (funcall 'neovm--test-tree-height (cadr tree))
                 (funcall 'neovm--test-tree-height (caddr tree)))))))

  ;; Returns height if balanced, -1 if not balanced.
  ;; More efficient than separate height + balance check.
  (fset 'neovm--test-tree-check-balance
    (lambda (tree)
      (if (null tree) 0
        (let ((lh (funcall 'neovm--test-tree-check-balance (cadr tree)))
              (rh (funcall 'neovm--test-tree-check-balance (caddr tree))))
          (if (or (= lh -1) (= rh -1) (> (abs (- lh rh)) 1))
              -1
            (1+ (max lh rh)))))))

  (fset 'neovm--test-tree-balanced-p
    (lambda (tree)
      (/= (funcall 'neovm--test-tree-check-balance tree) -1)))

  ;; Count nodes
  (fset 'neovm--test-tree-count
    (lambda (tree)
      (if (null tree) 0
        (+ 1
           (funcall 'neovm--test-tree-count (cadr tree))
           (funcall 'neovm--test-tree-count (caddr tree))))))

  ;; Count leaves
  (fset 'neovm--test-tree-leaves
    (lambda (tree)
      (if (null tree) 0
        (if (and (null (cadr tree)) (null (caddr tree)))
            1
          (+ (funcall 'neovm--test-tree-leaves (cadr tree))
             (funcall 'neovm--test-tree-leaves (caddr tree)))))))

  (unwind-protect
      (let (;; Balanced tree:
            ;;        4
            ;;       / \
            ;;      2   6
            ;;     / \ / \
            ;;    1  3 5  7
            (balanced '(4 (2 (1 nil nil) (3 nil nil))
                          (6 (5 nil nil) (7 nil nil))))
            ;; Unbalanced tree:
            ;;    1
            ;;     \
            ;;      2
            ;;       \
            ;;        3
            ;;         \
            ;;          4
            (unbalanced '(1 nil (2 nil (3 nil (4 nil nil)))))
            ;; Single node
            (single '(42 nil nil))
            ;; Slightly unbalanced but still within tolerance
            ;;      3
            ;;     / \
            ;;    2   4
            ;;   /
            ;;  1
            (slight '(3 (2 (1 nil nil) nil) (4 nil nil))))
        (list
         ;; Heights
         (funcall 'neovm--test-tree-height balanced)
         (funcall 'neovm--test-tree-height unbalanced)
         (funcall 'neovm--test-tree-height single)
         (funcall 'neovm--test-tree-height nil)
         ;; Balance checks
         (funcall 'neovm--test-tree-balanced-p balanced)
         (funcall 'neovm--test-tree-balanced-p unbalanced)
         (funcall 'neovm--test-tree-balanced-p single)
         (funcall 'neovm--test-tree-balanced-p slight)
         ;; Node counts
         (funcall 'neovm--test-tree-count balanced)
         (funcall 'neovm--test-tree-count unbalanced)
         ;; Leaf counts
         (funcall 'neovm--test-tree-leaves balanced)
         (funcall 'neovm--test-tree-leaves unbalanced)))
    (fmakunbound 'neovm--test-tree-height)
    (fmakunbound 'neovm--test-tree-check-balance)
    (fmakunbound 'neovm--test-tree-balanced-p)
    (fmakunbound 'neovm--test-tree-count)
    (fmakunbound 'neovm--test-tree-leaves)))"#;
    let expect = expect_test::expect![[r#""OK (3 4 1 0 t nil t t 7 4 4 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Lowest common ancestor (LCA) algorithm
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_tree_lca() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Find lowest common ancestor in a binary tree (not necessarily BST).
    // Approach: for each node, check if target is in left/right subtree.
    // If targets are split across subtrees, current node is LCA.
    let form = r#"(progn
  ;; Check if a value exists in the tree
  (fset 'neovm--test-tree-contains
    (lambda (tree val)
      (if (null tree) nil
        (or (equal (car tree) val)
            (funcall 'neovm--test-tree-contains (cadr tree) val)
            (funcall 'neovm--test-tree-contains (caddr tree) val)))))

  ;; Find LCA of two values
  (fset 'neovm--test-tree-lca
    (lambda (tree a b)
      (if (null tree) nil
        (let ((val (car tree))
              (left (cadr tree))
              (right (caddr tree)))
          (cond
           ;; Current node is one of the targets
           ((or (equal val a) (equal val b)) val)
           ;; Check subtrees
           (t
            (let ((in-left-a (funcall 'neovm--test-tree-contains left a))
                  (in-left-b (funcall 'neovm--test-tree-contains left b))
                  (in-right-a (funcall 'neovm--test-tree-contains right a))
                  (in-right-b (funcall 'neovm--test-tree-contains right b)))
              (cond
               ;; Both in left subtree
               ((and in-left-a in-left-b)
                (funcall 'neovm--test-tree-lca left a b))
               ;; Both in right subtree
               ((and in-right-a in-right-b)
                (funcall 'neovm--test-tree-lca right a b))
               ;; Split across subtrees: current is LCA
               ((and (or in-left-a in-right-a)
                     (or in-left-b in-right-b))
                val)
               ;; Not found
               (t nil)))))))))

  ;; Also find path from root to a node
  (fset 'neovm--test-tree-path
    (lambda (tree target)
      (if (null tree) nil
        (if (equal (car tree) target)
            (list target)
          (let ((left-path (funcall 'neovm--test-tree-path
                                     (cadr tree) target))
                (right-path (funcall 'neovm--test-tree-path
                                      (caddr tree) target)))
            (cond
             (left-path (cons (car tree) left-path))
             (right-path (cons (car tree) right-path))
             (t nil)))))))

  (unwind-protect
      (let ((tree '(1 (2 (4 nil nil)
                         (5 (8 nil nil) (9 nil nil)))
                      (3 (6 nil nil)
                         (7 nil nil)))))
        (list
         ;; LCA of 4 and 5 should be 2
         (funcall 'neovm--test-tree-lca tree 4 5)
         ;; LCA of 4 and 9 should be 2
         (funcall 'neovm--test-tree-lca tree 4 9)
         ;; LCA of 8 and 7 should be 1 (root)
         (funcall 'neovm--test-tree-lca tree 8 7)
         ;; LCA of 6 and 7 should be 3
         (funcall 'neovm--test-tree-lca tree 6 7)
         ;; LCA of node with itself should be that node
         (funcall 'neovm--test-tree-lca tree 5 5)
         ;; LCA of root and any node should be root
         (funcall 'neovm--test-tree-lca tree 1 9)
         ;; Paths for verification
         (funcall 'neovm--test-tree-path tree 8)
         (funcall 'neovm--test-tree-path tree 7)
         (funcall 'neovm--test-tree-path tree 99)))
    (fmakunbound 'neovm--test-tree-contains)
    (fmakunbound 'neovm--test-tree-lca)
    (fmakunbound 'neovm--test-tree-path)))"#;
    let expect = expect_test::expect![[r#""OK (2 2 1 3 5 1 (1 2 5 8) (1 3 7) nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Serialize/deserialize a tree to/from list representation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_tree_serialize_deserialize() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Serialize a tree to a flat list using preorder traversal with
    // sentinel nil markers for empty subtrees.
    // Deserialize reconstructs the tree from the flat list.
    let form = r#"(progn
  ;; Serialize: preorder traversal with nil markers
  (fset 'neovm--test-tree-serialize
    (lambda (tree)
      (if (null tree)
          (list nil)
        (append (list (car tree))
                (funcall 'neovm--test-tree-serialize (cadr tree))
                (funcall 'neovm--test-tree-serialize (caddr tree))))))

  ;; Deserialize: returns (tree . remaining-tokens)
  (fset 'neovm--test-tree-deser-helper
    (lambda (tokens)
      (if (or (null tokens) (null (car tokens)))
          (cons nil (cdr tokens))
        (let* ((val (car tokens))
               (rest1 (cdr tokens))
               (left-result (funcall 'neovm--test-tree-deser-helper rest1))
               (left-tree (car left-result))
               (rest2 (cdr left-result))
               (right-result (funcall 'neovm--test-tree-deser-helper rest2))
               (right-tree (car right-result))
               (rest3 (cdr right-result)))
          (cons (list val left-tree right-tree) rest3)))))

  (fset 'neovm--test-tree-deserialize
    (lambda (tokens)
      (car (funcall 'neovm--test-tree-deser-helper tokens))))

  ;; In-order for verification
  (fset 'neovm--test-tree-inorder
    (lambda (tree)
      (if (null tree) nil
        (append (funcall 'neovm--test-tree-inorder (cadr tree))
                (list (car tree))
                (funcall 'neovm--test-tree-inorder (caddr tree))))))

  ;; Preorder for verification
  (fset 'neovm--test-tree-preorder
    (lambda (tree)
      (if (null tree) nil
        (append (list (car tree))
                (funcall 'neovm--test-tree-preorder (cadr tree))
                (funcall 'neovm--test-tree-preorder (caddr tree))))))

  (unwind-protect
      (let ((tree1 '(1 (2 (4 nil nil) (5 nil nil))
                       (3 nil (6 nil nil))))
            (tree2 '(10 nil nil))
            (tree3 nil))
        (let ((s1 (funcall 'neovm--test-tree-serialize tree1))
              (s2 (funcall 'neovm--test-tree-serialize tree2))
              (s3 (funcall 'neovm--test-tree-serialize tree3)))
          ;; Roundtrip: serialize then deserialize should give same tree
          (let ((rt1 (funcall 'neovm--test-tree-deserialize s1))
                (rt2 (funcall 'neovm--test-tree-deserialize s2))
                (rt3 (funcall 'neovm--test-tree-deserialize s3)))
            (list
             ;; Serialized forms
             s1 s2 s3
             ;; Roundtrip equality
             (equal tree1 rt1)
             (equal tree2 rt2)
             (equal tree3 rt3)
             ;; Traversals match after roundtrip
             (equal (funcall 'neovm--test-tree-inorder tree1)
                    (funcall 'neovm--test-tree-inorder rt1))
             (equal (funcall 'neovm--test-tree-preorder tree1)
                    (funcall 'neovm--test-tree-preorder rt1))
             ;; Actual traversal values
             (funcall 'neovm--test-tree-inorder tree1)
             (funcall 'neovm--test-tree-preorder tree1)))))
    (fmakunbound 'neovm--test-tree-serialize)
    (fmakunbound 'neovm--test-tree-deser-helper)
    (fmakunbound 'neovm--test-tree-deserialize)
    (fmakunbound 'neovm--test-tree-inorder)
    (fmakunbound 'neovm--test-tree-preorder)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 4 nil nil 5 nil nil 3 nil 6 nil nil) (10 nil nil) (nil) t t t t t (4 2 5 1 3 6) (1 2 4 5 3 6))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Tree map/fold operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_tree_map_fold() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement map (apply function to each node value) and fold
    // (reduce tree to a single value) for binary trees.
    // Also: filter (keep only nodes matching predicate, collect to list),
    // and zip (combine two trees element-wise).
    let form = r#"(progn
  ;; Map: apply f to each node value, preserve structure
  (fset 'neovm--test-tree-map
    (lambda (tree f)
      (if (null tree) nil
        (list (funcall f (car tree))
              (funcall 'neovm--test-tree-map (cadr tree) f)
              (funcall 'neovm--test-tree-map (caddr tree) f)))))

  ;; Fold left (in-order): accumulate with function (acc, val) -> acc
  (fset 'neovm--test-tree-fold
    (lambda (tree f acc)
      (if (null tree) acc
        (let* ((left-acc (funcall 'neovm--test-tree-fold
                                   (cadr tree) f acc))
               (mid-acc (funcall f left-acc (car tree)))
               (right-acc (funcall 'neovm--test-tree-fold
                                    (caddr tree) f mid-acc)))
          right-acc))))

  ;; Filter: collect values matching predicate (in-order)
  (fset 'neovm--test-tree-filter
    (lambda (tree pred)
      (if (null tree) nil
        (let ((left-result (funcall 'neovm--test-tree-filter
                                     (cadr tree) pred))
              (right-result (funcall 'neovm--test-tree-filter
                                      (caddr tree) pred)))
          (if (funcall pred (car tree))
              (append left-result (list (car tree)) right-result)
            (append left-result right-result))))))

  ;; Zip: combine two trees element-wise with function f
  ;; If one tree is nil where the other is not, use default value
  (fset 'neovm--test-tree-zip
    (lambda (tree-a tree-b f default-val)
      (cond
       ((and (null tree-a) (null tree-b)) nil)
       ((null tree-a)
        (funcall 'neovm--test-tree-map tree-b
                 (lambda (v) (funcall f default-val v))))
       ((null tree-b)
        (funcall 'neovm--test-tree-map tree-a
                 (lambda (v) (funcall f v default-val))))
       (t
        (list (funcall f (car tree-a) (car tree-b))
              (funcall 'neovm--test-tree-zip
                       (cadr tree-a) (cadr tree-b) f default-val)
              (funcall 'neovm--test-tree-zip
                       (caddr tree-a) (caddr tree-b) f default-val))))))

  ;; Flatten: convert tree to sorted list (in-order)
  (fset 'neovm--test-tree-flatten
    (lambda (tree)
      (if (null tree) nil
        (append (funcall 'neovm--test-tree-flatten (cadr tree))
                (list (car tree))
                (funcall 'neovm--test-tree-flatten (caddr tree))))))

  (unwind-protect
      (let ((tree '(5 (3 (1 nil nil) (4 nil nil))
                      (8 (7 nil nil) (9 nil nil)))))
        (list
         ;; Map: double all values
         (funcall 'neovm--test-tree-flatten
                  (funcall 'neovm--test-tree-map tree
                           (lambda (x) (* x 2))))
         ;; Map: square all values
         (funcall 'neovm--test-tree-flatten
                  (funcall 'neovm--test-tree-map tree
                           (lambda (x) (* x x))))
         ;; Fold: sum all values (1+3+4+5+7+8+9 = 37)
         (funcall 'neovm--test-tree-fold tree
                  (lambda (acc v) (+ acc v)) 0)
         ;; Fold: product of all values
         (funcall 'neovm--test-tree-fold tree
                  (lambda (acc v) (* acc v)) 1)
         ;; Fold: max value
         (funcall 'neovm--test-tree-fold tree
                  (lambda (acc v) (max acc v)) 0)
         ;; Fold: count nodes
         (funcall 'neovm--test-tree-fold tree
                  (lambda (acc _v) (1+ acc)) 0)
         ;; Filter: only even values
         (funcall 'neovm--test-tree-filter tree
                  (lambda (x) (= (% x 2) 0)))
         ;; Filter: values > 4
         (funcall 'neovm--test-tree-filter tree
                  (lambda (x) (> x 4)))
         ;; Zip: add corresponding nodes of tree with itself
         (funcall 'neovm--test-tree-flatten
                  (funcall 'neovm--test-tree-zip
                           tree tree (lambda (a b) (+ a b)) 0))
         ;; Original flattened for reference
         (funcall 'neovm--test-tree-flatten tree)))
    (fmakunbound 'neovm--test-tree-map)
    (fmakunbound 'neovm--test-tree-fold)
    (fmakunbound 'neovm--test-tree-filter)
    (fmakunbound 'neovm--test-tree-zip)
    (fmakunbound 'neovm--test-tree-flatten)))"#;
    let expect = expect_test::expect![[
        r#""OK ((2 6 8 10 14 16 18) (1 9 16 25 49 64 81) 37 30240 9 7 (4 8) (5 7 8 9) (2 6 8 10 14 16 18) (1 3 4 5 7 8 9))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Trie (prefix tree) for string lookup
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_tree_trie_string_lookup() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Trie using hash tables: each node is a hash table where keys are
    // characters and 'end marks terminal nodes.  Supports insert, search,
    // prefix search, autocomplete (collect all words with given prefix),
    // delete, and word count.
    let form = r#"(progn
  (fset 'neovm--test-trie-new
    (lambda () (make-hash-table)))

  (fset 'neovm--test-trie-insert
    (lambda (trie word)
      (let ((node trie))
        (dotimes (i (length word))
          (let ((ch (aref word i)))
            (unless (gethash ch node)
              (puthash ch (make-hash-table) node))
            (setq node (gethash ch node))))
        (puthash 'end t node))))

  (fset 'neovm--test-trie-search
    (lambda (trie word)
      (let ((node trie)
            (found t))
        (dotimes (i (length word))
          (let ((ch (aref word i)))
            (if (gethash ch node)
                (setq node (gethash ch node))
              (setq found nil))))
        (and found (gethash 'end node nil)))))

  (fset 'neovm--test-trie-starts-with
    (lambda (trie prefix)
      (let ((node trie)
            (found t))
        (dotimes (i (length prefix))
          (let ((ch (aref prefix i)))
            (if (gethash ch node)
                (setq node (gethash ch node))
              (setq found nil))))
        found)))

  ;; Collect all words under a node with given prefix
  (fset 'neovm--test-trie-collect
    (lambda (node prefix)
      (let ((results nil))
        (when (gethash 'end node nil)
          (setq results (list prefix)))
        (maphash (lambda (k v)
                   (unless (eq k 'end)
                     (setq results
                           (append results
                                   (funcall 'neovm--test-trie-collect
                                            v (concat prefix
                                                      (char-to-string k)))))))
                 node)
        results)))

  ;; Autocomplete: find all words starting with prefix
  (fset 'neovm--test-trie-autocomplete
    (lambda (trie prefix)
      (let ((node trie)
            (found t))
        (dotimes (i (length prefix))
          (let ((ch (aref prefix i)))
            (if (gethash ch node)
                (setq node (gethash ch node))
              (setq found nil))))
        (if found
            (sort (funcall 'neovm--test-trie-collect node prefix)
                  #'string<)
          nil))))

  ;; Delete a word from trie (just remove 'end marker)
  (fset 'neovm--test-trie-delete
    (lambda (trie word)
      (let ((node trie)
            (found t))
        (dotimes (i (length word))
          (let ((ch (aref word i)))
            (if (gethash ch node)
                (setq node (gethash ch node))
              (setq found nil))))
        (when (and found (gethash 'end node nil))
          (remhash 'end node)
          t))))

  ;; Count words in trie
  (fset 'neovm--test-trie-count-words
    (lambda (node)
      (let ((count (if (gethash 'end node nil) 1 0)))
        (maphash (lambda (k v)
                   (unless (eq k 'end)
                     (setq count
                           (+ count
                              (funcall 'neovm--test-trie-count-words v)))))
                 node)
        count)))

  (unwind-protect
      (let ((trie (funcall 'neovm--test-trie-new)))
        ;; Insert words
        (dolist (w '("apple" "app" "application" "apply" "ape"
                     "banana" "band" "bandana" "ban"))
          (funcall 'neovm--test-trie-insert trie w))
        (let ((count-before (funcall 'neovm--test-trie-count-words trie))
              ;; Exact searches
              (s1 (funcall 'neovm--test-trie-search trie "apple"))
              (s2 (funcall 'neovm--test-trie-search trie "app"))
              (s3 (funcall 'neovm--test-trie-search trie "ap"))
              (s4 (funcall 'neovm--test-trie-search trie "xyz"))
              ;; Prefix checks
              (p1 (funcall 'neovm--test-trie-starts-with trie "app"))
              (p2 (funcall 'neovm--test-trie-starts-with trie "ban"))
              (p3 (funcall 'neovm--test-trie-starts-with trie "cat"))
              ;; Autocomplete
              (ac1 (funcall 'neovm--test-trie-autocomplete trie "app"))
              (ac2 (funcall 'neovm--test-trie-autocomplete trie "ban"))
              (ac3 (funcall 'neovm--test-trie-autocomplete trie "z")))
          ;; Delete "app" and verify
          (funcall 'neovm--test-trie-delete trie "app")
          (let ((s5 (funcall 'neovm--test-trie-search trie "app"))
                (s6 (funcall 'neovm--test-trie-search trie "apple"))
                (count-after (funcall 'neovm--test-trie-count-words trie))
                (ac4 (funcall 'neovm--test-trie-autocomplete trie "app")))
            (list count-before
                  s1 s2 s3 s4
                  p1 p2 p3
                  ac1 ac2 ac3
                  s5 s6 count-after ac4))))
    (fmakunbound 'neovm--test-trie-new)
    (fmakunbound 'neovm--test-trie-insert)
    (fmakunbound 'neovm--test-trie-search)
    (fmakunbound 'neovm--test-trie-starts-with)
    (fmakunbound 'neovm--test-trie-collect)
    (fmakunbound 'neovm--test-trie-autocomplete)
    (fmakunbound 'neovm--test-trie-delete)
    (fmakunbound 'neovm--test-trie-count-words)))"#;
    let expect = expect_test::expect![[
        r#""OK (9 t t nil nil t t nil (\"app\" \"apple\" \"application\" \"apply\") (\"ban\" \"banana\" \"band\" \"bandana\") nil nil t 8 (\"apple\" \"application\" \"apply\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Tree: level-order traversal (BFS) and mirror/invert
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_tree_level_order_and_mirror() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Level-order (BFS) traversal of a binary tree, returning values
    // grouped by level.  Also mirror/invert a tree and verify properties.
    let form = r#"(progn
  ;; Level-order traversal: returns list of lists, one per level
  (fset 'neovm--test-tree-levels
    (lambda (tree)
      (if (null tree) nil
        (let ((queue (list tree))
              (result nil))
          (while queue
            (let ((level-size (length queue))
                  (level-vals nil)
                  (next-queue nil))
              (dotimes (_ level-size)
                (let ((node (car queue)))
                  (setq queue (cdr queue))
                  (setq level-vals (cons (car node) level-vals))
                  (when (cadr node)
                    (setq next-queue (append next-queue (list (cadr node)))))
                  (when (caddr node)
                    (setq next-queue (append next-queue (list (caddr node)))))))
              (setq result (cons (nreverse level-vals) result))
              (setq queue next-queue)))
          (nreverse result)))))

  ;; Mirror/invert: swap left and right children recursively
  (fset 'neovm--test-tree-mirror
    (lambda (tree)
      (if (null tree) nil
        (list (car tree)
              (funcall 'neovm--test-tree-mirror (caddr tree))
              (funcall 'neovm--test-tree-mirror (cadr tree))))))

  ;; In-order for verification
  (fset 'neovm--test-tree-inorder2
    (lambda (tree)
      (if (null tree) nil
        (append (funcall 'neovm--test-tree-inorder2 (cadr tree))
                (list (car tree))
                (funcall 'neovm--test-tree-inorder2 (caddr tree))))))

  ;; Check if two trees have the same structure (ignoring values)
  (fset 'neovm--test-tree-same-shape
    (lambda (a b)
      (cond
       ((and (null a) (null b)) t)
       ((or (null a) (null b)) nil)
       (t (and (funcall 'neovm--test-tree-same-shape (cadr a) (cadr b))
               (funcall 'neovm--test-tree-same-shape (caddr a) (caddr b)))))))

  (unwind-protect
      (let ((tree '(1 (2 (4 nil nil) (5 nil nil))
                      (3 (6 nil nil) (7 nil nil)))))
        (let ((levels (funcall 'neovm--test-tree-levels tree))
              (mirrored (funcall 'neovm--test-tree-mirror tree)))
          (let ((mirror-levels (funcall 'neovm--test-tree-levels mirrored))
                (mirror-inorder (funcall 'neovm--test-tree-inorder2 mirrored))
                (orig-inorder (funcall 'neovm--test-tree-inorder2 tree)))
            ;; Double mirror should give back original
            (let ((double-mirror (funcall 'neovm--test-tree-mirror mirrored)))
              (list
               ;; Level order of original
               levels
               ;; Level order of mirror (same levels but reversed within each)
               mirror-levels
               ;; In-order of original vs mirror (should be reverse)
               orig-inorder
               mirror-inorder
               (equal orig-inorder (nreverse (copy-sequence mirror-inorder)))
               ;; Double mirror equals original
               (equal tree double-mirror)
               ;; Same shape between tree and its mirror
               (funcall 'neovm--test-tree-same-shape tree mirrored))))))
    (fmakunbound 'neovm--test-tree-levels)
    (fmakunbound 'neovm--test-tree-mirror)
    (fmakunbound 'neovm--test-tree-inorder2)
    (fmakunbound 'neovm--test-tree-same-shape)))"#;
    let expect = expect_test::expect![[
        r#""OK (((1) (2 3) (4 5 6 7)) ((1) (3 2) (7 6 5 4)) (4 2 5 1 6 3 7) (7 3 6 1 5 2 4) t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
