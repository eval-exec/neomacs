//! Oracle parity tests for a treap (tree + heap) data structure implemented in
//! Elisp. A treap maintains BST ordering on keys and max-heap ordering on
//! random priorities. Tests cover insert with rotations, search, in-order
//! traversal, split, merge, and range count operations.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Treap core: insert with rotations, search, in-order traversal
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_treap_insert_search_traverse() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Node: (key priority left right), nil = empty
    // Insert maintains BST on key, max-heap on priority (higher priority = closer to root)
    let form = r#"(progn
  ;; Node accessors
  (fset 'neovm--tr-key (lambda (n) (car n)))
  (fset 'neovm--tr-pri (lambda (n) (cadr n)))
  (fset 'neovm--tr-left (lambda (n) (caddr n)))
  (fset 'neovm--tr-right (lambda (n) (cadddr n)))

  ;; Constructor
  (fset 'neovm--tr-node
    (lambda (key pri left right)
      (list key pri left right)))

  ;; Right rotation: left child becomes root
  ;;      y           x
  ;;     / \         / \
  ;;    x   C  =>  A    y
  ;;   / \             / \
  ;;  A   B           B   C
  (fset 'neovm--tr-rot-right
    (lambda (y)
      (let ((x (funcall 'neovm--tr-left y)))
        (funcall 'neovm--tr-node
                 (funcall 'neovm--tr-key x)
                 (funcall 'neovm--tr-pri x)
                 (funcall 'neovm--tr-left x)
                 (funcall 'neovm--tr-node
                          (funcall 'neovm--tr-key y)
                          (funcall 'neovm--tr-pri y)
                          (funcall 'neovm--tr-right x)
                          (funcall 'neovm--tr-right y))))))

  ;; Left rotation: right child becomes root
  ;;    x              y
  ;;   / \            / \
  ;;  A   y    =>   x   C
  ;;     / \       / \
  ;;    B   C     A   B
  (fset 'neovm--tr-rot-left
    (lambda (x)
      (let ((y (funcall 'neovm--tr-right x)))
        (funcall 'neovm--tr-node
                 (funcall 'neovm--tr-key y)
                 (funcall 'neovm--tr-pri y)
                 (funcall 'neovm--tr-node
                          (funcall 'neovm--tr-key x)
                          (funcall 'neovm--tr-pri x)
                          (funcall 'neovm--tr-left x)
                          (funcall 'neovm--tr-left y))
                 (funcall 'neovm--tr-right y)))))

  ;; Insert: BST insert then rotate up if priority is higher
  (fset 'neovm--tr-insert
    (lambda (node key pri)
      (if (null node)
          (funcall 'neovm--tr-node key pri nil nil)
        (cond
         ((< key (funcall 'neovm--tr-key node))
          (let ((new-node (funcall 'neovm--tr-node
                                   (funcall 'neovm--tr-key node)
                                   (funcall 'neovm--tr-pri node)
                                   (funcall 'neovm--tr-insert
                                            (funcall 'neovm--tr-left node) key pri)
                                   (funcall 'neovm--tr-right node))))
            ;; If left child has higher priority, rotate right
            (if (and (funcall 'neovm--tr-left new-node)
                     (> (funcall 'neovm--tr-pri (funcall 'neovm--tr-left new-node))
                        (funcall 'neovm--tr-pri new-node)))
                (funcall 'neovm--tr-rot-right new-node)
              new-node)))
         ((> key (funcall 'neovm--tr-key node))
          (let ((new-node (funcall 'neovm--tr-node
                                   (funcall 'neovm--tr-key node)
                                   (funcall 'neovm--tr-pri node)
                                   (funcall 'neovm--tr-left node)
                                   (funcall 'neovm--tr-insert
                                            (funcall 'neovm--tr-right node) key pri))))
            ;; If right child has higher priority, rotate left
            (if (and (funcall 'neovm--tr-right new-node)
                     (> (funcall 'neovm--tr-pri (funcall 'neovm--tr-right new-node))
                        (funcall 'neovm--tr-pri new-node)))
                (funcall 'neovm--tr-rot-left new-node)
              new-node)))
         ;; Duplicate key: update priority
         (t (funcall 'neovm--tr-node key pri
                     (funcall 'neovm--tr-left node)
                     (funcall 'neovm--tr-right node)))))))

  ;; Search: BST search
  (fset 'neovm--tr-search
    (lambda (node key)
      (if (null node)
          nil
        (cond
         ((= key (funcall 'neovm--tr-key node)) t)
         ((< key (funcall 'neovm--tr-key node))
          (funcall 'neovm--tr-search (funcall 'neovm--tr-left node) key))
         (t
          (funcall 'neovm--tr-search (funcall 'neovm--tr-right node) key))))))

  ;; In-order traversal: returns sorted list of keys
  (fset 'neovm--tr-inorder
    (lambda (node)
      (if (null node)
          nil
        (append (funcall 'neovm--tr-inorder (funcall 'neovm--tr-left node))
                (list (funcall 'neovm--tr-key node))
                (funcall 'neovm--tr-inorder (funcall 'neovm--tr-right node))))))

  ;; Verify heap property: parent priority >= children priorities
  (fset 'neovm--tr-heap-valid
    (lambda (node)
      (if (null node)
          t
        (let ((lv (if (funcall 'neovm--tr-left node)
                      (and (<= (funcall 'neovm--tr-pri (funcall 'neovm--tr-left node))
                                (funcall 'neovm--tr-pri node))
                           (funcall 'neovm--tr-heap-valid (funcall 'neovm--tr-left node)))
                    t))
              (rv (if (funcall 'neovm--tr-right node)
                      (and (<= (funcall 'neovm--tr-pri (funcall 'neovm--tr-right node))
                                (funcall 'neovm--tr-pri node))
                           (funcall 'neovm--tr-heap-valid (funcall 'neovm--tr-right node)))
                    t)))
          (and lv rv)))))

  ;; Build a treap with deterministic priorities
  (let* ((t0 nil)
         (t1 (funcall 'neovm--tr-insert t0 50 80))
         (t2 (funcall 'neovm--tr-insert t1 30 90))
         (t3 (funcall 'neovm--tr-insert t2 70 60))
         (t4 (funcall 'neovm--tr-insert t3 20 50))
         (t5 (funcall 'neovm--tr-insert t4 40 75))
         (t6 (funcall 'neovm--tr-insert t5 60 40))
         (t7 (funcall 'neovm--tr-insert t6 80 30))
         (t8 (funcall 'neovm--tr-insert t7 10 20))
         (t9 (funcall 'neovm--tr-insert t8 90 10)))
    (list
      ;; In-order traversal should give sorted keys
      (funcall 'neovm--tr-inorder t9)
      ;; Search for existing keys
      (funcall 'neovm--tr-search t9 50)
      (funcall 'neovm--tr-search t9 30)
      (funcall 'neovm--tr-search t9 10)
      (funcall 'neovm--tr-search t9 90)
      ;; Search for non-existing keys
      (funcall 'neovm--tr-search t9 15)
      (funcall 'neovm--tr-search t9 55)
      (funcall 'neovm--tr-search t9 100)
      ;; Heap property valid
      (funcall 'neovm--tr-heap-valid t9)
      ;; Root should have highest priority (30 has pri=90)
      (funcall 'neovm--tr-key t9)
      (funcall 'neovm--tr-pri t9))))"#;
    let expect = expect_test::expect![[
        r#""OK ((10 20 30 40 50 60 70 80 90) t t t t nil nil nil t 30 90)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Treap: split operation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_treap_split() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Split treap into two: all keys < pivot go left, >= pivot go right
    let form = r#"(progn
  (fset 'neovm--tr-key (lambda (n) (car n)))
  (fset 'neovm--tr-pri (lambda (n) (cadr n)))
  (fset 'neovm--tr-left (lambda (n) (caddr n)))
  (fset 'neovm--tr-right (lambda (n) (cadddr n)))
  (fset 'neovm--tr-node
    (lambda (key pri left right) (list key pri left right)))

  ;; Split: returns (left-treap . right-treap)
  ;; left-treap has all keys < pivot, right-treap has all keys >= pivot
  (fset 'neovm--tr-split
    (lambda (node pivot)
      (if (null node)
          (cons nil nil)
        (if (< (funcall 'neovm--tr-key node) pivot)
            ;; Root goes to left part; split right subtree
            (let ((pair (funcall 'neovm--tr-split
                                 (funcall 'neovm--tr-right node) pivot)))
              (cons (funcall 'neovm--tr-node
                             (funcall 'neovm--tr-key node)
                             (funcall 'neovm--tr-pri node)
                             (funcall 'neovm--tr-left node)
                             (car pair))
                    (cdr pair)))
          ;; Root goes to right part; split left subtree
          (let ((pair (funcall 'neovm--tr-split
                               (funcall 'neovm--tr-left node) pivot)))
            (cons (car pair)
                  (funcall 'neovm--tr-node
                           (funcall 'neovm--tr-key node)
                           (funcall 'neovm--tr-pri node)
                           (cdr pair)
                           (funcall 'neovm--tr-right node))))))))

  ;; In-order traversal
  (fset 'neovm--tr-inorder
    (lambda (node)
      (if (null node) nil
        (append (funcall 'neovm--tr-inorder (funcall 'neovm--tr-left node))
                (list (funcall 'neovm--tr-key node))
                (funcall 'neovm--tr-inorder (funcall 'neovm--tr-right node))))))

  ;; Heap valid
  (fset 'neovm--tr-heap-valid
    (lambda (node)
      (if (null node) t
        (and (if (funcall 'neovm--tr-left node)
                 (and (<= (funcall 'neovm--tr-pri (funcall 'neovm--tr-left node))
                           (funcall 'neovm--tr-pri node))
                      (funcall 'neovm--tr-heap-valid (funcall 'neovm--tr-left node)))
               t)
             (if (funcall 'neovm--tr-right node)
                 (and (<= (funcall 'neovm--tr-pri (funcall 'neovm--tr-right node))
                           (funcall 'neovm--tr-pri node))
                      (funcall 'neovm--tr-heap-valid (funcall 'neovm--tr-right node)))
               t)))))

  ;; Insert helper (for building test tree)
  (fset 'neovm--tr-rot-right
    (lambda (y)
      (let ((x (funcall 'neovm--tr-left y)))
        (funcall 'neovm--tr-node
                 (funcall 'neovm--tr-key x) (funcall 'neovm--tr-pri x)
                 (funcall 'neovm--tr-left x)
                 (funcall 'neovm--tr-node
                          (funcall 'neovm--tr-key y) (funcall 'neovm--tr-pri y)
                          (funcall 'neovm--tr-right x) (funcall 'neovm--tr-right y))))))
  (fset 'neovm--tr-rot-left
    (lambda (x)
      (let ((y (funcall 'neovm--tr-right x)))
        (funcall 'neovm--tr-node
                 (funcall 'neovm--tr-key y) (funcall 'neovm--tr-pri y)
                 (funcall 'neovm--tr-node
                          (funcall 'neovm--tr-key x) (funcall 'neovm--tr-pri x)
                          (funcall 'neovm--tr-left x) (funcall 'neovm--tr-left y))
                 (funcall 'neovm--tr-right y)))))
  (fset 'neovm--tr-insert
    (lambda (node key pri)
      (if (null node)
          (funcall 'neovm--tr-node key pri nil nil)
        (cond
         ((< key (funcall 'neovm--tr-key node))
          (let ((nn (funcall 'neovm--tr-node
                             (funcall 'neovm--tr-key node) (funcall 'neovm--tr-pri node)
                             (funcall 'neovm--tr-insert (funcall 'neovm--tr-left node) key pri)
                             (funcall 'neovm--tr-right node))))
            (if (and (funcall 'neovm--tr-left nn)
                     (> (funcall 'neovm--tr-pri (funcall 'neovm--tr-left nn))
                        (funcall 'neovm--tr-pri nn)))
                (funcall 'neovm--tr-rot-right nn)
              nn)))
         ((> key (funcall 'neovm--tr-key node))
          (let ((nn (funcall 'neovm--tr-node
                             (funcall 'neovm--tr-key node) (funcall 'neovm--tr-pri node)
                             (funcall 'neovm--tr-left node)
                             (funcall 'neovm--tr-insert (funcall 'neovm--tr-right node) key pri))))
            (if (and (funcall 'neovm--tr-right nn)
                     (> (funcall 'neovm--tr-pri (funcall 'neovm--tr-right nn))
                        (funcall 'neovm--tr-pri nn)))
                (funcall 'neovm--tr-rot-left nn)
              nn)))
         (t (funcall 'neovm--tr-node key pri
                     (funcall 'neovm--tr-left node) (funcall 'neovm--tr-right node)))))))

  ;; Build treap: 10(p50), 20(p90), 30(p70), 40(p30), 50(p80), 60(p60), 70(p40)
  (let* ((tree nil)
         (tree (funcall 'neovm--tr-insert tree 10 50))
         (tree (funcall 'neovm--tr-insert tree 20 90))
         (tree (funcall 'neovm--tr-insert tree 30 70))
         (tree (funcall 'neovm--tr-insert tree 40 30))
         (tree (funcall 'neovm--tr-insert tree 50 80))
         (tree (funcall 'neovm--tr-insert tree 60 60))
         (tree (funcall 'neovm--tr-insert tree 70 40)))
    ;; Split at 35: left should have {10,20,30}, right should have {40,50,60,70}
    (let ((pair35 (funcall 'neovm--tr-split tree 35)))
      (let ((left35 (funcall 'neovm--tr-inorder (car pair35)))
            (right35 (funcall 'neovm--tr-inorder (cdr pair35))))
        ;; Split at 10: left should be empty, right should have all
        (let ((pair10 (funcall 'neovm--tr-split tree 10)))
          ;; Split at 80: left has all, right is empty
          (let ((pair80 (funcall 'neovm--tr-split tree 80)))
            ;; Split at 50: left={10,20,30,40}, right={50,60,70}
            (let ((pair50 (funcall 'neovm--tr-split tree 50)))
              (list
                ;; Split at 35
                left35
                right35
                ;; Split at 10 (nothing < 10)
                (funcall 'neovm--tr-inorder (car pair10))
                (funcall 'neovm--tr-inorder (cdr pair10))
                ;; Split at 80 (everything < 80)
                (funcall 'neovm--tr-inorder (car pair80))
                (funcall 'neovm--tr-inorder (cdr pair80))
                ;; Split at 50
                (funcall 'neovm--tr-inorder (car pair50))
                (funcall 'neovm--tr-inorder (cdr pair50))
                ;; Heap property preserved in split halves
                (funcall 'neovm--tr-heap-valid (car pair35))
                (funcall 'neovm--tr-heap-valid (cdr pair35))
                (funcall 'neovm--tr-heap-valid (car pair50))
                (funcall 'neovm--tr-heap-valid (cdr pair50))))))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((10 20 30) (40 50 60 70) nil (10 20 30 40 50 60 70) (10 20 30 40 50 60 70) nil (10 20 30 40) (50 60 70) t t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Treap: merge operation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_treap_merge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Merge two treaps where all keys in left < all keys in right
    let form = r#"(progn
  (fset 'neovm--tr-key (lambda (n) (car n)))
  (fset 'neovm--tr-pri (lambda (n) (cadr n)))
  (fset 'neovm--tr-left (lambda (n) (caddr n)))
  (fset 'neovm--tr-right (lambda (n) (cadddr n)))
  (fset 'neovm--tr-node
    (lambda (key pri left right) (list key pri left right)))

  ;; Merge: left treap has all keys < right treap keys
  ;; Root is whichever has higher priority
  (fset 'neovm--tr-merge
    (lambda (left right)
      (cond
       ((null left) right)
       ((null right) left)
       ((>= (funcall 'neovm--tr-pri left) (funcall 'neovm--tr-pri right))
        ;; Left root stays, merge left's right subtree with right
        (funcall 'neovm--tr-node
                 (funcall 'neovm--tr-key left)
                 (funcall 'neovm--tr-pri left)
                 (funcall 'neovm--tr-left left)
                 (funcall 'neovm--tr-merge (funcall 'neovm--tr-right left) right)))
       (t
        ;; Right root stays, merge left with right's left subtree
        (funcall 'neovm--tr-node
                 (funcall 'neovm--tr-key right)
                 (funcall 'neovm--tr-pri right)
                 (funcall 'neovm--tr-merge left (funcall 'neovm--tr-left right))
                 (funcall 'neovm--tr-right right))))))

  ;; Split
  (fset 'neovm--tr-split
    (lambda (node pivot)
      (if (null node)
          (cons nil nil)
        (if (< (funcall 'neovm--tr-key node) pivot)
            (let ((pair (funcall 'neovm--tr-split (funcall 'neovm--tr-right node) pivot)))
              (cons (funcall 'neovm--tr-node
                             (funcall 'neovm--tr-key node) (funcall 'neovm--tr-pri node)
                             (funcall 'neovm--tr-left node) (car pair))
                    (cdr pair)))
          (let ((pair (funcall 'neovm--tr-split (funcall 'neovm--tr-left node) pivot)))
            (cons (car pair)
                  (funcall 'neovm--tr-node
                           (funcall 'neovm--tr-key node) (funcall 'neovm--tr-pri node)
                           (cdr pair) (funcall 'neovm--tr-right node))))))))

  ;; Traversal and validation
  (fset 'neovm--tr-inorder
    (lambda (node)
      (if (null node) nil
        (append (funcall 'neovm--tr-inorder (funcall 'neovm--tr-left node))
                (list (funcall 'neovm--tr-key node))
                (funcall 'neovm--tr-inorder (funcall 'neovm--tr-right node))))))

  (fset 'neovm--tr-heap-valid
    (lambda (node)
      (if (null node) t
        (and (if (funcall 'neovm--tr-left node)
                 (and (<= (funcall 'neovm--tr-pri (funcall 'neovm--tr-left node))
                           (funcall 'neovm--tr-pri node))
                      (funcall 'neovm--tr-heap-valid (funcall 'neovm--tr-left node)))
               t)
             (if (funcall 'neovm--tr-right node)
                 (and (<= (funcall 'neovm--tr-pri (funcall 'neovm--tr-right node))
                           (funcall 'neovm--tr-pri node))
                      (funcall 'neovm--tr-heap-valid (funcall 'neovm--tr-right node)))
               t)))))

  ;; Build two separate treaps
  ;; Left treap: keys 10, 20, 30 with priorities 80, 60, 70
  (let* ((left nil)
         (left (funcall 'neovm--tr-node 20 80
                        (funcall 'neovm--tr-node 10 60 nil nil)
                        (funcall 'neovm--tr-node 30 70 nil nil)))
         ;; Right treap: keys 50, 60, 70 with priorities 90, 50, 75
         (right (funcall 'neovm--tr-node 50 90
                         nil
                         (funcall 'neovm--tr-node 70 75
                                  (funcall 'neovm--tr-node 60 50 nil nil)
                                  nil)))
         ;; Merge them
         (merged (funcall 'neovm--tr-merge left right)))
    (list
      ;; Merged in-order should be all keys sorted
      (funcall 'neovm--tr-inorder merged)
      ;; Heap property maintained
      (funcall 'neovm--tr-heap-valid merged)
      ;; Root should have the highest priority among all (50 has pri=90)
      (funcall 'neovm--tr-key merged)
      (funcall 'neovm--tr-pri merged)

      ;; Split and merge roundtrip: split at 45, then merge back
      (let* ((pair (funcall 'neovm--tr-split merged 45))
             (re-merged (funcall 'neovm--tr-merge (car pair) (cdr pair))))
        (list
          ;; Should have same sorted keys
          (funcall 'neovm--tr-inorder re-merged)
          ;; Heap still valid
          (funcall 'neovm--tr-heap-valid re-merged)))

      ;; Merge with empty
      (funcall 'neovm--tr-inorder (funcall 'neovm--tr-merge left nil))
      (funcall 'neovm--tr-inorder (funcall 'neovm--tr-merge nil right))

      ;; Merge single nodes
      (let ((single-merge (funcall 'neovm--tr-merge
                                   (funcall 'neovm--tr-node 5 100 nil nil)
                                   (funcall 'neovm--tr-node 95 50 nil nil))))
        (list
          (funcall 'neovm--tr-inorder single-merge)
          (funcall 'neovm--tr-heap-valid single-merge)
          ;; Root is 5 (pri 100 > 50)
          (funcall 'neovm--tr-key single-merge))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((10 20 30 50 60 70) t 50 90 ((10 20 30 50 60 70) t) (10 20 30) (50 60 70) ((5 95) t 5))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Treap: range count (count keys in [lo, hi])
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_treap_range_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Count keys within a range [lo, hi] using tree traversal
    let form = r#"(progn
  (fset 'neovm--tr-key (lambda (n) (car n)))
  (fset 'neovm--tr-pri (lambda (n) (cadr n)))
  (fset 'neovm--tr-left (lambda (n) (caddr n)))
  (fset 'neovm--tr-right (lambda (n) (cadddr n)))
  (fset 'neovm--tr-node
    (lambda (key pri left right) (list key pri left right)))

  ;; Count keys in range [lo, hi]
  (fset 'neovm--tr-range-count
    (lambda (node lo hi)
      (if (null node)
          0
        (let ((k (funcall 'neovm--tr-key node)))
          (cond
           ;; Key is too small: only check right subtree
           ((< k lo)
            (funcall 'neovm--tr-range-count (funcall 'neovm--tr-right node) lo hi))
           ;; Key is too big: only check left subtree
           ((> k hi)
            (funcall 'neovm--tr-range-count (funcall 'neovm--tr-left node) lo hi))
           ;; Key is in range: count it + both subtrees
           (t
            (+ 1
               (funcall 'neovm--tr-range-count (funcall 'neovm--tr-left node) lo hi)
               (funcall 'neovm--tr-range-count (funcall 'neovm--tr-right node) lo hi))))))))

  ;; Collect keys in range [lo, hi] (sorted)
  (fset 'neovm--tr-range-keys
    (lambda (node lo hi)
      (if (null node)
          nil
        (let ((k (funcall 'neovm--tr-key node)))
          (cond
           ((< k lo)
            (funcall 'neovm--tr-range-keys (funcall 'neovm--tr-right node) lo hi))
           ((> k hi)
            (funcall 'neovm--tr-range-keys (funcall 'neovm--tr-left node) lo hi))
           (t
            (append
             (funcall 'neovm--tr-range-keys (funcall 'neovm--tr-left node) lo hi)
             (list k)
             (funcall 'neovm--tr-range-keys (funcall 'neovm--tr-right node) lo hi))))))))

  ;; Size of treap
  (fset 'neovm--tr-size
    (lambda (node)
      (if (null node) 0
        (+ 1
           (funcall 'neovm--tr-size (funcall 'neovm--tr-left node))
           (funcall 'neovm--tr-size (funcall 'neovm--tr-right node))))))

  ;; Insert
  (fset 'neovm--tr-rot-right
    (lambda (y)
      (let ((x (funcall 'neovm--tr-left y)))
        (funcall 'neovm--tr-node
                 (funcall 'neovm--tr-key x) (funcall 'neovm--tr-pri x)
                 (funcall 'neovm--tr-left x)
                 (funcall 'neovm--tr-node
                          (funcall 'neovm--tr-key y) (funcall 'neovm--tr-pri y)
                          (funcall 'neovm--tr-right x) (funcall 'neovm--tr-right y))))))
  (fset 'neovm--tr-rot-left
    (lambda (x)
      (let ((y (funcall 'neovm--tr-right x)))
        (funcall 'neovm--tr-node
                 (funcall 'neovm--tr-key y) (funcall 'neovm--tr-pri y)
                 (funcall 'neovm--tr-node
                          (funcall 'neovm--tr-key x) (funcall 'neovm--tr-pri x)
                          (funcall 'neovm--tr-left x) (funcall 'neovm--tr-left y))
                 (funcall 'neovm--tr-right y)))))
  (fset 'neovm--tr-insert
    (lambda (node key pri)
      (if (null node)
          (funcall 'neovm--tr-node key pri nil nil)
        (cond
         ((< key (funcall 'neovm--tr-key node))
          (let ((nn (funcall 'neovm--tr-node
                             (funcall 'neovm--tr-key node) (funcall 'neovm--tr-pri node)
                             (funcall 'neovm--tr-insert (funcall 'neovm--tr-left node) key pri)
                             (funcall 'neovm--tr-right node))))
            (if (and (funcall 'neovm--tr-left nn)
                     (> (funcall 'neovm--tr-pri (funcall 'neovm--tr-left nn))
                        (funcall 'neovm--tr-pri nn)))
                (funcall 'neovm--tr-rot-right nn) nn)))
         ((> key (funcall 'neovm--tr-key node))
          (let ((nn (funcall 'neovm--tr-node
                             (funcall 'neovm--tr-key node) (funcall 'neovm--tr-pri node)
                             (funcall 'neovm--tr-left node)
                             (funcall 'neovm--tr-insert (funcall 'neovm--tr-right node) key pri))))
            (if (and (funcall 'neovm--tr-right nn)
                     (> (funcall 'neovm--tr-pri (funcall 'neovm--tr-right nn))
                        (funcall 'neovm--tr-pri nn)))
                (funcall 'neovm--tr-rot-left nn) nn)))
         (t (funcall 'neovm--tr-node key pri
                     (funcall 'neovm--tr-left node) (funcall 'neovm--tr-right node)))))))

  ;; Build treap with keys: 5, 15, 25, 35, 45, 55, 65, 75, 85, 95
  (let* ((tree nil)
         (tree (funcall 'neovm--tr-insert tree  5  50))
         (tree (funcall 'neovm--tr-insert tree 15  90))
         (tree (funcall 'neovm--tr-insert tree 25  70))
         (tree (funcall 'neovm--tr-insert tree 35  30))
         (tree (funcall 'neovm--tr-insert tree 45  80))
         (tree (funcall 'neovm--tr-insert tree 55  60))
         (tree (funcall 'neovm--tr-insert tree 65  40))
         (tree (funcall 'neovm--tr-insert tree 75  20))
         (tree (funcall 'neovm--tr-insert tree 85  10))
         (tree (funcall 'neovm--tr-insert tree 95  35)))
    (list
      ;; Total size
      (funcall 'neovm--tr-size tree)
      ;; Range [20, 60]: keys 25, 35, 45, 55
      (funcall 'neovm--tr-range-count tree 20 60)
      (funcall 'neovm--tr-range-keys tree 20 60)
      ;; Range [5, 95]: all 10 keys
      (funcall 'neovm--tr-range-count tree 5 95)
      ;; Range [0, 100]: all 10 keys
      (funcall 'neovm--tr-range-count tree 0 100)
      ;; Range [40, 50]: just 45
      (funcall 'neovm--tr-range-count tree 40 50)
      (funcall 'neovm--tr-range-keys tree 40 50)
      ;; Range [46, 54]: nothing
      (funcall 'neovm--tr-range-count tree 46 54)
      (funcall 'neovm--tr-range-keys tree 46 54)
      ;; Range [90, 100]: just 95
      (funcall 'neovm--tr-range-count tree 90 100)
      (funcall 'neovm--tr-range-keys tree 90 100)
      ;; Range [0, 4]: nothing
      (funcall 'neovm--tr-range-count tree 0 4)
      ;; Range [5, 5]: just 5
      (funcall 'neovm--tr-range-count tree 5 5)
      (funcall 'neovm--tr-range-keys tree 5 5)
      ;; Range [1, 99]: all keys
      (funcall 'neovm--tr-range-keys tree 1 99))))"#;
    let expect = expect_test::expect![[
        r#""OK (10 4 (25 35 45 55) 10 10 1 (45) 0 nil 1 (95) 0 1 (5) (5 15 25 35 45 55 65 75 85 95))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Treap: insert with split-merge approach
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_treap_split_merge_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Alternative insert using split + merge (more elegant approach)
    let form = r#"(progn
  (fset 'neovm--tr-key (lambda (n) (car n)))
  (fset 'neovm--tr-pri (lambda (n) (cadr n)))
  (fset 'neovm--tr-left (lambda (n) (caddr n)))
  (fset 'neovm--tr-right (lambda (n) (cadddr n)))
  (fset 'neovm--tr-node
    (lambda (key pri left right) (list key pri left right)))

  ;; Split
  (fset 'neovm--tr-split
    (lambda (node pivot)
      (if (null node) (cons nil nil)
        (if (< (funcall 'neovm--tr-key node) pivot)
            (let ((pair (funcall 'neovm--tr-split (funcall 'neovm--tr-right node) pivot)))
              (cons (funcall 'neovm--tr-node
                             (funcall 'neovm--tr-key node) (funcall 'neovm--tr-pri node)
                             (funcall 'neovm--tr-left node) (car pair))
                    (cdr pair)))
          (let ((pair (funcall 'neovm--tr-split (funcall 'neovm--tr-left node) pivot)))
            (cons (car pair)
                  (funcall 'neovm--tr-node
                           (funcall 'neovm--tr-key node) (funcall 'neovm--tr-pri node)
                           (cdr pair) (funcall 'neovm--tr-right node))))))))

  ;; Merge
  (fset 'neovm--tr-merge
    (lambda (left right)
      (cond
       ((null left) right)
       ((null right) left)
       ((>= (funcall 'neovm--tr-pri left) (funcall 'neovm--tr-pri right))
        (funcall 'neovm--tr-node
                 (funcall 'neovm--tr-key left) (funcall 'neovm--tr-pri left)
                 (funcall 'neovm--tr-left left)
                 (funcall 'neovm--tr-merge (funcall 'neovm--tr-right left) right)))
       (t
        (funcall 'neovm--tr-node
                 (funcall 'neovm--tr-key right) (funcall 'neovm--tr-pri right)
                 (funcall 'neovm--tr-merge left (funcall 'neovm--tr-left right))
                 (funcall 'neovm--tr-right right))))))

  ;; Insert via split-merge: split at key, merge left + new-node + right
  (fset 'neovm--tr-sm-insert
    (lambda (tree key pri)
      (let* ((pair (funcall 'neovm--tr-split tree key))
             ;; Also split right at key+1 to handle duplicates
             (pair2 (funcall 'neovm--tr-split (cdr pair) (1+ key)))
             (new-node (funcall 'neovm--tr-node key pri nil nil)))
        (funcall 'neovm--tr-merge
                 (funcall 'neovm--tr-merge (car pair) new-node)
                 (cdr pair2)))))

  ;; Delete via split-merge: split at key, split right at key+1, merge left + right2
  (fset 'neovm--tr-sm-delete
    (lambda (tree key)
      (let* ((pair (funcall 'neovm--tr-split tree key))
             (pair2 (funcall 'neovm--tr-split (cdr pair) (1+ key))))
        ;; Discard the middle (which is the node with the key)
        (funcall 'neovm--tr-merge (car pair) (cdr pair2)))))

  ;; Helpers
  (fset 'neovm--tr-inorder
    (lambda (node)
      (if (null node) nil
        (append (funcall 'neovm--tr-inorder (funcall 'neovm--tr-left node))
                (list (funcall 'neovm--tr-key node))
                (funcall 'neovm--tr-inorder (funcall 'neovm--tr-right node))))))

  (fset 'neovm--tr-heap-valid
    (lambda (node)
      (if (null node) t
        (and (if (funcall 'neovm--tr-left node)
                 (and (<= (funcall 'neovm--tr-pri (funcall 'neovm--tr-left node))
                           (funcall 'neovm--tr-pri node))
                      (funcall 'neovm--tr-heap-valid (funcall 'neovm--tr-left node)))
               t)
             (if (funcall 'neovm--tr-right node)
                 (and (<= (funcall 'neovm--tr-pri (funcall 'neovm--tr-right node))
                           (funcall 'neovm--tr-pri node))
                      (funcall 'neovm--tr-heap-valid (funcall 'neovm--tr-right node)))
               t)))))

  (fset 'neovm--tr-search
    (lambda (node key)
      (if (null node) nil
        (cond
         ((= key (funcall 'neovm--tr-key node)) t)
         ((< key (funcall 'neovm--tr-key node))
          (funcall 'neovm--tr-search (funcall 'neovm--tr-left node) key))
         (t (funcall 'neovm--tr-search (funcall 'neovm--tr-right node) key))))))

  ;; Build treap using split-merge insert
  (let* ((t0 nil)
         (t1 (funcall 'neovm--tr-sm-insert t0 40 70))
         (t2 (funcall 'neovm--tr-sm-insert t1 20 90))
         (t3 (funcall 'neovm--tr-sm-insert t2 60 50))
         (t4 (funcall 'neovm--tr-sm-insert t3 10 60))
         (t5 (funcall 'neovm--tr-sm-insert t4 30 80))
         (t6 (funcall 'neovm--tr-sm-insert t5 50 40))
         (t7 (funcall 'neovm--tr-sm-insert t6 70 30)))
    (list
      ;; Sorted keys
      (funcall 'neovm--tr-inorder t7)
      ;; Heap valid
      (funcall 'neovm--tr-heap-valid t7)
      ;; Search
      (funcall 'neovm--tr-search t7 40)
      (funcall 'neovm--tr-search t7 20)
      (funcall 'neovm--tr-search t7 70)
      (funcall 'neovm--tr-search t7 35)

      ;; Delete 40
      (let ((t8 (funcall 'neovm--tr-sm-delete t7 40)))
        (list
          (funcall 'neovm--tr-inorder t8)
          (funcall 'neovm--tr-heap-valid t8)
          (funcall 'neovm--tr-search t8 40)
          (funcall 'neovm--tr-search t8 30)))

      ;; Delete all one by one
      (let* ((d1 (funcall 'neovm--tr-sm-delete t7 10))
             (d2 (funcall 'neovm--tr-sm-delete d1 30))
             (d3 (funcall 'neovm--tr-sm-delete d2 50))
             (d4 (funcall 'neovm--tr-sm-delete d3 70)))
        (list
          (funcall 'neovm--tr-inorder d4)
          (funcall 'neovm--tr-heap-valid d4)))

      ;; Insert duplicate key replaces
      (let ((t-dup (funcall 'neovm--tr-sm-insert t7 40 99)))
        (list
          (funcall 'neovm--tr-inorder t-dup)
          ;; 40 should still be there exactly once
          (length (funcall 'neovm--tr-inorder t-dup)))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((10 20 30 40 50 60 70) t t t t nil ((10 20 30 50 60 70) t nil t) ((20 40 60) t) ((10 20 30 40 50 60 70) 7))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Treap: kth-smallest and size-augmented operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_treap_kth_smallest() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Find k-th smallest element using in-order traversal
    let form = r#"(progn
  (fset 'neovm--tr-key (lambda (n) (car n)))
  (fset 'neovm--tr-pri (lambda (n) (cadr n)))
  (fset 'neovm--tr-left (lambda (n) (caddr n)))
  (fset 'neovm--tr-right (lambda (n) (cadddr n)))
  (fset 'neovm--tr-node
    (lambda (key pri left right) (list key pri left right)))

  ;; In-order traversal
  (fset 'neovm--tr-inorder
    (lambda (node)
      (if (null node) nil
        (append (funcall 'neovm--tr-inorder (funcall 'neovm--tr-left node))
                (list (funcall 'neovm--tr-key node))
                (funcall 'neovm--tr-inorder (funcall 'neovm--tr-right node))))))

  ;; K-th smallest (1-indexed)
  (fset 'neovm--tr-kth
    (lambda (node k)
      (let ((sorted (funcall 'neovm--tr-inorder node)))
        (if (and (> k 0) (<= k (length sorted)))
            (nth (1- k) sorted)
          nil))))

  ;; Min and max
  (fset 'neovm--tr-min
    (lambda (node)
      (if (null node) nil
        (if (funcall 'neovm--tr-left node)
            (funcall 'neovm--tr-min (funcall 'neovm--tr-left node))
          (funcall 'neovm--tr-key node)))))

  (fset 'neovm--tr-max
    (lambda (node)
      (if (null node) nil
        (if (funcall 'neovm--tr-right node)
            (funcall 'neovm--tr-max (funcall 'neovm--tr-right node))
          (funcall 'neovm--tr-key node)))))

  ;; Height
  (fset 'neovm--tr-height
    (lambda (node)
      (if (null node) 0
        (1+ (max (funcall 'neovm--tr-height (funcall 'neovm--tr-left node))
                 (funcall 'neovm--tr-height (funcall 'neovm--tr-right node)))))))

  ;; Insert
  (fset 'neovm--tr-rot-right
    (lambda (y)
      (let ((x (funcall 'neovm--tr-left y)))
        (funcall 'neovm--tr-node
                 (funcall 'neovm--tr-key x) (funcall 'neovm--tr-pri x)
                 (funcall 'neovm--tr-left x)
                 (funcall 'neovm--tr-node
                          (funcall 'neovm--tr-key y) (funcall 'neovm--tr-pri y)
                          (funcall 'neovm--tr-right x) (funcall 'neovm--tr-right y))))))
  (fset 'neovm--tr-rot-left
    (lambda (x)
      (let ((y (funcall 'neovm--tr-right x)))
        (funcall 'neovm--tr-node
                 (funcall 'neovm--tr-key y) (funcall 'neovm--tr-pri y)
                 (funcall 'neovm--tr-node
                          (funcall 'neovm--tr-key x) (funcall 'neovm--tr-pri x)
                          (funcall 'neovm--tr-left x) (funcall 'neovm--tr-left y))
                 (funcall 'neovm--tr-right y)))))
  (fset 'neovm--tr-insert
    (lambda (node key pri)
      (if (null node) (funcall 'neovm--tr-node key pri nil nil)
        (cond
         ((< key (funcall 'neovm--tr-key node))
          (let ((nn (funcall 'neovm--tr-node
                             (funcall 'neovm--tr-key node) (funcall 'neovm--tr-pri node)
                             (funcall 'neovm--tr-insert (funcall 'neovm--tr-left node) key pri)
                             (funcall 'neovm--tr-right node))))
            (if (and (funcall 'neovm--tr-left nn)
                     (> (funcall 'neovm--tr-pri (funcall 'neovm--tr-left nn))
                        (funcall 'neovm--tr-pri nn)))
                (funcall 'neovm--tr-rot-right nn) nn)))
         ((> key (funcall 'neovm--tr-key node))
          (let ((nn (funcall 'neovm--tr-node
                             (funcall 'neovm--tr-key node) (funcall 'neovm--tr-pri node)
                             (funcall 'neovm--tr-left node)
                             (funcall 'neovm--tr-insert (funcall 'neovm--tr-right node) key pri))))
            (if (and (funcall 'neovm--tr-right nn)
                     (> (funcall 'neovm--tr-pri (funcall 'neovm--tr-right nn))
                        (funcall 'neovm--tr-pri nn)))
                (funcall 'neovm--tr-rot-left nn) nn)))
         (t (funcall 'neovm--tr-node key pri
                     (funcall 'neovm--tr-left node) (funcall 'neovm--tr-right node)))))))

  ;; Build treap: 3(p80), 1(p60), 4(p90), 1(dup->p60), 5(p70), 9(p40), 2(p50), 6(p85)
  (let* ((tree nil)
         (tree (funcall 'neovm--tr-insert tree 3 80))
         (tree (funcall 'neovm--tr-insert tree 1 60))
         (tree (funcall 'neovm--tr-insert tree 4 90))
         (tree (funcall 'neovm--tr-insert tree 5 70))
         (tree (funcall 'neovm--tr-insert tree 9 40))
         (tree (funcall 'neovm--tr-insert tree 2 50))
         (tree (funcall 'neovm--tr-insert tree 6 85)))
    (list
      ;; Sorted traversal
      (funcall 'neovm--tr-inorder tree)
      ;; K-th smallest
      (funcall 'neovm--tr-kth tree 1)   ;; 1
      (funcall 'neovm--tr-kth tree 2)   ;; 2
      (funcall 'neovm--tr-kth tree 3)   ;; 3
      (funcall 'neovm--tr-kth tree 4)   ;; 4
      (funcall 'neovm--tr-kth tree 5)   ;; 5
      (funcall 'neovm--tr-kth tree 6)   ;; 6
      (funcall 'neovm--tr-kth tree 7)   ;; 9
      ;; Out of bounds
      (funcall 'neovm--tr-kth tree 0)   ;; nil
      (funcall 'neovm--tr-kth tree 8)   ;; nil
      ;; Min and max
      (funcall 'neovm--tr-min tree)     ;; 1
      (funcall 'neovm--tr-max tree)     ;; 9
      ;; Height
      (funcall 'neovm--tr-height tree)
      ;; Min/max of empty
      (funcall 'neovm--tr-min nil)
      (funcall 'neovm--tr-max nil))))"#;
    let expect =
        expect_test::expect![[r#""OK ((1 2 3 4 5 6 9) 1 2 3 4 5 6 9 nil nil 1 9 4 nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
