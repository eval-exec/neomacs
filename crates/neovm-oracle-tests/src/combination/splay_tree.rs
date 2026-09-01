//! Oracle parity tests for a splay tree implemented in Elisp.
//!
//! Implements a top-down splay tree with zig, zig-zig, and zig-zag
//! rotations. Tests insert with splaying, search with splaying
//! (recently accessed elements move to root), delete, in-order
//! traversal, and sequential access pattern optimization.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Splay tree core: rotations, splay, insert
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_splay_tree_insert_and_splay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Node: (key left right) or nil
    // Splay brings accessed key to root via zig/zig-zig/zig-zag
    let form = r#"(progn
  ;; Node accessors
  (fset 'neovm--st-key (lambda (n) (car n)))
  (fset 'neovm--st-left (lambda (n) (cadr n)))
  (fset 'neovm--st-right (lambda (n) (caddr n)))
  (fset 'neovm--st-node (lambda (key left right) (list key left right)))

  ;; Right rotation (zig): left child becomes root
  (fset 'neovm--st-rotate-right
    (lambda (t0)
      (if (null t0) nil
        (let ((l (funcall 'neovm--st-left t0)))
          (if (null l) t0
            (funcall 'neovm--st-node
                     (funcall 'neovm--st-key l)
                     (funcall 'neovm--st-left l)
                     (funcall 'neovm--st-node
                              (funcall 'neovm--st-key t0)
                              (funcall 'neovm--st-right l)
                              (funcall 'neovm--st-right t0))))))))

  ;; Left rotation (zig): right child becomes root
  (fset 'neovm--st-rotate-left
    (lambda (t0)
      (if (null t0) nil
        (let ((r (funcall 'neovm--st-right t0)))
          (if (null r) t0
            (funcall 'neovm--st-node
                     (funcall 'neovm--st-key r)
                     (funcall 'neovm--st-node
                              (funcall 'neovm--st-key t0)
                              (funcall 'neovm--st-left t0)
                              (funcall 'neovm--st-left r))
                     (funcall 'neovm--st-right r)))))))

  ;; Top-down splay: bring key to root (or nearest)
  ;; Returns splayed tree
  (fset 'neovm--st-splay
    (lambda (key t0)
      (if (null t0) nil
        (cond
         ((= key (funcall 'neovm--st-key t0)) t0)
         ((< key (funcall 'neovm--st-key t0))
          (let ((l (funcall 'neovm--st-left t0)))
            (if (null l) t0
              (cond
               ;; Zig: key is at left child
               ((= key (funcall 'neovm--st-key l))
                (funcall 'neovm--st-rotate-right t0))
               ;; Zig-zig: key is in left-left subtree
               ((< key (funcall 'neovm--st-key l))
                (let ((ll (funcall 'neovm--st-splay key (funcall 'neovm--st-left l))))
                  (let ((new-l (funcall 'neovm--st-node
                                        (funcall 'neovm--st-key l)
                                        ll
                                        (funcall 'neovm--st-right l))))
                    (let ((new-t (funcall 'neovm--st-node
                                          (funcall 'neovm--st-key t0)
                                          new-l
                                          (funcall 'neovm--st-right t0))))
                      (funcall 'neovm--st-rotate-right
                               (funcall 'neovm--st-rotate-right new-t))))))
               ;; Zig-zag: key is in left-right subtree
               (t
                (let ((lr (funcall 'neovm--st-splay key (funcall 'neovm--st-right l))))
                  (let ((new-l (funcall 'neovm--st-node
                                        (funcall 'neovm--st-key l)
                                        (funcall 'neovm--st-left l)
                                        lr)))
                    (funcall 'neovm--st-rotate-right
                             (funcall 'neovm--st-node
                                      (funcall 'neovm--st-key t0)
                                      (funcall 'neovm--st-rotate-left new-l)
                                      (funcall 'neovm--st-right t0))))))))))
         (t  ;; key > root key
          (let ((r (funcall 'neovm--st-right t0)))
            (if (null r) t0
              (cond
               ;; Zig: key is at right child
               ((= key (funcall 'neovm--st-key r))
                (funcall 'neovm--st-rotate-left t0))
               ;; Zig-zig: key is in right-right subtree
               ((> key (funcall 'neovm--st-key r))
                (let ((rr (funcall 'neovm--st-splay key (funcall 'neovm--st-right r))))
                  (let ((new-r (funcall 'neovm--st-node
                                        (funcall 'neovm--st-key r)
                                        (funcall 'neovm--st-left r)
                                        rr)))
                    (let ((new-t (funcall 'neovm--st-node
                                          (funcall 'neovm--st-key t0)
                                          (funcall 'neovm--st-left t0)
                                          new-r)))
                      (funcall 'neovm--st-rotate-left
                               (funcall 'neovm--st-rotate-left new-t))))))
               ;; Zig-zag: key is in right-left subtree
               (t
                (let ((rl (funcall 'neovm--st-splay key (funcall 'neovm--st-left r))))
                  (let ((new-r (funcall 'neovm--st-node
                                        (funcall 'neovm--st-key r)
                                        rl
                                        (funcall 'neovm--st-right r))))
                    (funcall 'neovm--st-rotate-left
                             (funcall 'neovm--st-node
                                      (funcall 'neovm--st-key t0)
                                      (funcall 'neovm--st-left t0)
                                      (funcall 'neovm--st-rotate-right new-r))))))))))))))

  ;; Insert: splay then split
  (fset 'neovm--st-insert
    (lambda (key t0)
      (if (null t0)
          (funcall 'neovm--st-node key nil nil)
        (let ((splayed (funcall 'neovm--st-splay key t0)))
          (let ((root-key (funcall 'neovm--st-key splayed)))
            (cond
             ((= key root-key) splayed)  ;; duplicate, no-op
             ((< key root-key)
              (funcall 'neovm--st-node
                       key
                       (funcall 'neovm--st-left splayed)
                       (funcall 'neovm--st-node
                                root-key
                                nil
                                (funcall 'neovm--st-right splayed))))
             (t
              (funcall 'neovm--st-node
                       key
                       (funcall 'neovm--st-node
                                root-key
                                (funcall 'neovm--st-left splayed)
                                nil)
                       (funcall 'neovm--st-right splayed)))))))))

  ;; In-order traversal
  (fset 'neovm--st-inorder
    (lambda (t0)
      (if (null t0) nil
        (append (funcall 'neovm--st-inorder (funcall 'neovm--st-left t0))
                (list (funcall 'neovm--st-key t0))
                (funcall 'neovm--st-inorder (funcall 'neovm--st-right t0))))))

  ;; Build tree from list
  (let ((tree nil))
    (dolist (k '(5 3 7 1 4 6 8 2 9 0))
      (setq tree (funcall 'neovm--st-insert k tree)))
    ;; After inserting 0 last, 0 should be at root
    (let ((root-key (funcall 'neovm--st-key tree))
          (sorted (funcall 'neovm--st-inorder tree)))
      (unwind-protect
          (list root-key sorted
                (equal sorted '(0 1 2 3 4 5 6 7 8 9)))
        (progn
          (fmakunbound 'neovm--st-key) (fmakunbound 'neovm--st-left)
          (fmakunbound 'neovm--st-right) (fmakunbound 'neovm--st-node)
          (fmakunbound 'neovm--st-rotate-right) (fmakunbound 'neovm--st-rotate-left)
          (fmakunbound 'neovm--st-splay) (fmakunbound 'neovm--st-insert)
          (fmakunbound 'neovm--st-inorder))))))"#;
    let expect = expect_test::expect![[r#""OK (0 (0 1 2 3 4 5 6 7 8 9) t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Search with splaying: recently accessed moves to root
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_splay_tree_search_moves_to_root() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--st-key (lambda (n) (car n)))
  (fset 'neovm--st-left (lambda (n) (cadr n)))
  (fset 'neovm--st-right (lambda (n) (caddr n)))
  (fset 'neovm--st-node (lambda (key left right) (list key left right)))

  (fset 'neovm--st-rotate-right
    (lambda (t0)
      (let ((l (funcall 'neovm--st-left t0)))
        (if (null l) t0
          (funcall 'neovm--st-node (funcall 'neovm--st-key l)
                   (funcall 'neovm--st-left l)
                   (funcall 'neovm--st-node (funcall 'neovm--st-key t0)
                            (funcall 'neovm--st-right l)
                            (funcall 'neovm--st-right t0)))))))

  (fset 'neovm--st-rotate-left
    (lambda (t0)
      (let ((r (funcall 'neovm--st-right t0)))
        (if (null r) t0
          (funcall 'neovm--st-node (funcall 'neovm--st-key r)
                   (funcall 'neovm--st-node (funcall 'neovm--st-key t0)
                            (funcall 'neovm--st-left t0)
                            (funcall 'neovm--st-left r))
                   (funcall 'neovm--st-right r))))))

  (fset 'neovm--st-splay
    (lambda (key t0)
      (if (null t0) nil
        (cond
         ((= key (funcall 'neovm--st-key t0)) t0)
         ((< key (funcall 'neovm--st-key t0))
          (let ((l (funcall 'neovm--st-left t0)))
            (if (null l) t0
              (if (= key (funcall 'neovm--st-key l))
                  (funcall 'neovm--st-rotate-right t0)
                (if (< key (funcall 'neovm--st-key l))
                    (let* ((ll (funcall 'neovm--st-splay key (funcall 'neovm--st-left l)))
                           (new-l (funcall 'neovm--st-node (funcall 'neovm--st-key l) ll (funcall 'neovm--st-right l)))
                           (new-t (funcall 'neovm--st-node (funcall 'neovm--st-key t0) new-l (funcall 'neovm--st-right t0))))
                      (funcall 'neovm--st-rotate-right (funcall 'neovm--st-rotate-right new-t)))
                  (let* ((lr (funcall 'neovm--st-splay key (funcall 'neovm--st-right l)))
                         (new-l (funcall 'neovm--st-node (funcall 'neovm--st-key l) (funcall 'neovm--st-left l) lr)))
                    (funcall 'neovm--st-rotate-right
                             (funcall 'neovm--st-node (funcall 'neovm--st-key t0)
                                      (funcall 'neovm--st-rotate-left new-l)
                                      (funcall 'neovm--st-right t0)))))))))
         (t
          (let ((r (funcall 'neovm--st-right t0)))
            (if (null r) t0
              (if (= key (funcall 'neovm--st-key r))
                  (funcall 'neovm--st-rotate-left t0)
                (if (> key (funcall 'neovm--st-key r))
                    (let* ((rr (funcall 'neovm--st-splay key (funcall 'neovm--st-right r)))
                           (new-r (funcall 'neovm--st-node (funcall 'neovm--st-key r) (funcall 'neovm--st-left r) rr))
                           (new-t (funcall 'neovm--st-node (funcall 'neovm--st-key t0) (funcall 'neovm--st-left t0) new-r)))
                      (funcall 'neovm--st-rotate-left (funcall 'neovm--st-rotate-left new-t)))
                  (let* ((rl (funcall 'neovm--st-splay key (funcall 'neovm--st-left r)))
                         (new-r (funcall 'neovm--st-node (funcall 'neovm--st-key r) rl (funcall 'neovm--st-right r))))
                    (funcall 'neovm--st-rotate-left
                             (funcall 'neovm--st-node (funcall 'neovm--st-key t0)
                                      (funcall 'neovm--st-left t0)
                                      (funcall 'neovm--st-rotate-right new-r)))))))))))))

  (fset 'neovm--st-insert
    (lambda (key t0)
      (if (null t0) (funcall 'neovm--st-node key nil nil)
        (let ((splayed (funcall 'neovm--st-splay key t0)))
          (let ((rk (funcall 'neovm--st-key splayed)))
            (cond ((= key rk) splayed)
                  ((< key rk)
                   (funcall 'neovm--st-node key (funcall 'neovm--st-left splayed)
                            (funcall 'neovm--st-node rk nil (funcall 'neovm--st-right splayed))))
                  (t (funcall 'neovm--st-node key
                              (funcall 'neovm--st-node rk (funcall 'neovm--st-left splayed) nil)
                              (funcall 'neovm--st-right splayed)))))))))

  ;; Search: splay and check root
  (fset 'neovm--st-search
    (lambda (key t0)
      (if (null t0) (list nil nil)
        (let ((splayed (funcall 'neovm--st-splay key t0)))
          (list (= key (funcall 'neovm--st-key splayed)) splayed)))))

  ;; Build a tree
  (let ((tree nil))
    (dolist (k '(10 5 15 3 7 12 18 1 4 6 8))
      (setq tree (funcall 'neovm--st-insert k tree)))
    ;; Search for 3: should move 3 to root
    (let* ((result1 (funcall 'neovm--st-search 3 tree))
           (found1 (car result1))
           (tree1 (cadr result1))
           (root1 (funcall 'neovm--st-key tree1))
           ;; Search for 18: should move 18 to root
           (result2 (funcall 'neovm--st-search 18 tree1))
           (found2 (car result2))
           (tree2 (cadr result2))
           (root2 (funcall 'neovm--st-key tree2))
           ;; Search for 99 (not found): root changes to nearest
           (result3 (funcall 'neovm--st-search 99 tree2))
           (found3 (car result3)))
      (unwind-protect
          (list found1 root1 found2 root2 found3)
        (progn
          (fmakunbound 'neovm--st-key) (fmakunbound 'neovm--st-left)
          (fmakunbound 'neovm--st-right) (fmakunbound 'neovm--st-node)
          (fmakunbound 'neovm--st-rotate-right) (fmakunbound 'neovm--st-rotate-left)
          (fmakunbound 'neovm--st-splay) (fmakunbound 'neovm--st-insert)
          (fmakunbound 'neovm--st-search))))))"#;
    let expect = expect_test::expect![[r#""OK (t 3 t 18 nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Delete: remove a key and maintain BST invariant
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_splay_tree_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--st-key (lambda (n) (car n)))
  (fset 'neovm--st-left (lambda (n) (cadr n)))
  (fset 'neovm--st-right (lambda (n) (caddr n)))
  (fset 'neovm--st-node (lambda (key left right) (list key left right)))
  (fset 'neovm--st-rotate-right
    (lambda (t0) (let ((l (funcall 'neovm--st-left t0)))
      (if (null l) t0
        (funcall 'neovm--st-node (funcall 'neovm--st-key l) (funcall 'neovm--st-left l)
                 (funcall 'neovm--st-node (funcall 'neovm--st-key t0) (funcall 'neovm--st-right l) (funcall 'neovm--st-right t0)))))))
  (fset 'neovm--st-rotate-left
    (lambda (t0) (let ((r (funcall 'neovm--st-right t0)))
      (if (null r) t0
        (funcall 'neovm--st-node (funcall 'neovm--st-key r)
                 (funcall 'neovm--st-node (funcall 'neovm--st-key t0) (funcall 'neovm--st-left t0) (funcall 'neovm--st-left r))
                 (funcall 'neovm--st-right r))))))
  (fset 'neovm--st-splay
    (lambda (key t0)
      (if (null t0) nil
        (cond
         ((= key (funcall 'neovm--st-key t0)) t0)
         ((< key (funcall 'neovm--st-key t0))
          (let ((l (funcall 'neovm--st-left t0)))
            (if (null l) t0
              (if (= key (funcall 'neovm--st-key l)) (funcall 'neovm--st-rotate-right t0)
                (if (< key (funcall 'neovm--st-key l))
                    (let* ((ll (funcall 'neovm--st-splay key (funcall 'neovm--st-left l)))
                           (nl (funcall 'neovm--st-node (funcall 'neovm--st-key l) ll (funcall 'neovm--st-right l)))
                           (nt (funcall 'neovm--st-node (funcall 'neovm--st-key t0) nl (funcall 'neovm--st-right t0))))
                      (funcall 'neovm--st-rotate-right (funcall 'neovm--st-rotate-right nt)))
                  (let* ((lr (funcall 'neovm--st-splay key (funcall 'neovm--st-right l)))
                         (nl (funcall 'neovm--st-node (funcall 'neovm--st-key l) (funcall 'neovm--st-left l) lr)))
                    (funcall 'neovm--st-rotate-right
                             (funcall 'neovm--st-node (funcall 'neovm--st-key t0)
                                      (funcall 'neovm--st-rotate-left nl) (funcall 'neovm--st-right t0)))))))))
         (t
          (let ((r (funcall 'neovm--st-right t0)))
            (if (null r) t0
              (if (= key (funcall 'neovm--st-key r)) (funcall 'neovm--st-rotate-left t0)
                (if (> key (funcall 'neovm--st-key r))
                    (let* ((rr (funcall 'neovm--st-splay key (funcall 'neovm--st-right r)))
                           (nr (funcall 'neovm--st-node (funcall 'neovm--st-key r) (funcall 'neovm--st-left r) rr))
                           (nt (funcall 'neovm--st-node (funcall 'neovm--st-key t0) (funcall 'neovm--st-left t0) nr)))
                      (funcall 'neovm--st-rotate-left (funcall 'neovm--st-rotate-left nt)))
                  (let* ((rl (funcall 'neovm--st-splay key (funcall 'neovm--st-left r)))
                         (nr (funcall 'neovm--st-node (funcall 'neovm--st-key r) rl (funcall 'neovm--st-right r))))
                    (funcall 'neovm--st-rotate-left
                             (funcall 'neovm--st-node (funcall 'neovm--st-key t0)
                                      (funcall 'neovm--st-left t0) (funcall 'neovm--st-rotate-right nr)))))))))))))
  (fset 'neovm--st-insert
    (lambda (key t0)
      (if (null t0) (funcall 'neovm--st-node key nil nil)
        (let ((s (funcall 'neovm--st-splay key t0)))
          (let ((rk (funcall 'neovm--st-key s)))
            (cond ((= key rk) s)
                  ((< key rk) (funcall 'neovm--st-node key (funcall 'neovm--st-left s)
                                       (funcall 'neovm--st-node rk nil (funcall 'neovm--st-right s))))
                  (t (funcall 'neovm--st-node key (funcall 'neovm--st-node rk (funcall 'neovm--st-left s) nil)
                              (funcall 'neovm--st-right s)))))))))
  (fset 'neovm--st-inorder
    (lambda (t0) (if (null t0) nil
      (append (funcall 'neovm--st-inorder (funcall 'neovm--st-left t0))
              (list (funcall 'neovm--st-key t0))
              (funcall 'neovm--st-inorder (funcall 'neovm--st-right t0))))))

  ;; Find max in tree (rightmost)
  (fset 'neovm--st-max
    (lambda (t0)
      (if (null (funcall 'neovm--st-right t0))
          (funcall 'neovm--st-key t0)
        (funcall 'neovm--st-max (funcall 'neovm--st-right t0)))))

  ;; Delete: splay key to root, then join left and right subtrees
  (fset 'neovm--st-delete
    (lambda (key t0)
      (if (null t0) nil
        (let ((s (funcall 'neovm--st-splay key t0)))
          (if (not (= key (funcall 'neovm--st-key s)))
              s  ;; key not found
            (let ((l (funcall 'neovm--st-left s))
                  (r (funcall 'neovm--st-right s)))
              (if (null l) r
                ;; Splay max of left subtree, then attach right
                (let ((sl (funcall 'neovm--st-splay (funcall 'neovm--st-max l) l)))
                  (funcall 'neovm--st-node (funcall 'neovm--st-key sl)
                           (funcall 'neovm--st-left sl) r)))))))))

  (let ((tree nil))
    (dolist (k '(5 3 7 1 4 6 8 2 9 0))
      (setq tree (funcall 'neovm--st-insert k tree)))
    (let ((before (funcall 'neovm--st-inorder tree)))
      ;; Delete root (0)
      (setq tree (funcall 'neovm--st-delete 0 tree))
      (let ((after-del-0 (funcall 'neovm--st-inorder tree)))
        ;; Delete middle (5)
        (setq tree (funcall 'neovm--st-delete 5 tree))
        (let ((after-del-5 (funcall 'neovm--st-inorder tree)))
          ;; Delete leaf (9)
          (setq tree (funcall 'neovm--st-delete 9 tree))
          (let ((after-del-9 (funcall 'neovm--st-inorder tree)))
            ;; Delete non-existent (99)
            (setq tree (funcall 'neovm--st-delete 99 tree))
            (let ((after-del-99 (funcall 'neovm--st-inorder tree)))
              (unwind-protect
                  (list before after-del-0 after-del-5 after-del-9 after-del-99)
                (progn
                  (fmakunbound 'neovm--st-key) (fmakunbound 'neovm--st-left)
                  (fmakunbound 'neovm--st-right) (fmakunbound 'neovm--st-node)
                  (fmakunbound 'neovm--st-rotate-right) (fmakunbound 'neovm--st-rotate-left)
                  (fmakunbound 'neovm--st-splay) (fmakunbound 'neovm--st-insert)
                  (fmakunbound 'neovm--st-inorder) (fmakunbound 'neovm--st-max)
                  (fmakunbound 'neovm--st-delete))))))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((0 1 2 3 4 5 6 7 8 9) (1 2 3 4 5 6 7 8 9) (1 2 3 4 6 7 8 9) (1 2 3 4 6 7 8) (1 2 3 4 6 7 8))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// In-order traversal correctness after mixed operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_splay_tree_inorder_after_mixed_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--st-key (lambda (n) (car n)))
  (fset 'neovm--st-left (lambda (n) (cadr n)))
  (fset 'neovm--st-right (lambda (n) (caddr n)))
  (fset 'neovm--st-node (lambda (key left right) (list key left right)))
  (fset 'neovm--st-rotate-right
    (lambda (t0) (let ((l (funcall 'neovm--st-left t0)))
      (if (null l) t0 (funcall 'neovm--st-node (funcall 'neovm--st-key l) (funcall 'neovm--st-left l)
                                (funcall 'neovm--st-node (funcall 'neovm--st-key t0) (funcall 'neovm--st-right l) (funcall 'neovm--st-right t0)))))))
  (fset 'neovm--st-rotate-left
    (lambda (t0) (let ((r (funcall 'neovm--st-right t0)))
      (if (null r) t0 (funcall 'neovm--st-node (funcall 'neovm--st-key r)
                                (funcall 'neovm--st-node (funcall 'neovm--st-key t0) (funcall 'neovm--st-left t0) (funcall 'neovm--st-left r))
                                (funcall 'neovm--st-right r))))))
  (fset 'neovm--st-splay
    (lambda (key t0)
      (if (null t0) nil
        (cond
         ((= key (funcall 'neovm--st-key t0)) t0)
         ((< key (funcall 'neovm--st-key t0))
          (let ((l (funcall 'neovm--st-left t0)))
            (if (null l) t0
              (if (= key (funcall 'neovm--st-key l)) (funcall 'neovm--st-rotate-right t0)
                (if (< key (funcall 'neovm--st-key l))
                    (let* ((ll (funcall 'neovm--st-splay key (funcall 'neovm--st-left l)))
                           (nl (funcall 'neovm--st-node (funcall 'neovm--st-key l) ll (funcall 'neovm--st-right l)))
                           (nt (funcall 'neovm--st-node (funcall 'neovm--st-key t0) nl (funcall 'neovm--st-right t0))))
                      (funcall 'neovm--st-rotate-right (funcall 'neovm--st-rotate-right nt)))
                  (let* ((lr (funcall 'neovm--st-splay key (funcall 'neovm--st-right l)))
                         (nl (funcall 'neovm--st-node (funcall 'neovm--st-key l) (funcall 'neovm--st-left l) lr)))
                    (funcall 'neovm--st-rotate-right
                             (funcall 'neovm--st-node (funcall 'neovm--st-key t0) (funcall 'neovm--st-rotate-left nl) (funcall 'neovm--st-right t0)))))))))
         (t (let ((r (funcall 'neovm--st-right t0)))
              (if (null r) t0
                (if (= key (funcall 'neovm--st-key r)) (funcall 'neovm--st-rotate-left t0)
                  (if (> key (funcall 'neovm--st-key r))
                      (let* ((rr (funcall 'neovm--st-splay key (funcall 'neovm--st-right r)))
                             (nr (funcall 'neovm--st-node (funcall 'neovm--st-key r) (funcall 'neovm--st-left r) rr))
                             (nt (funcall 'neovm--st-node (funcall 'neovm--st-key t0) (funcall 'neovm--st-left t0) nr)))
                        (funcall 'neovm--st-rotate-left (funcall 'neovm--st-rotate-left nt)))
                    (let* ((rl (funcall 'neovm--st-splay key (funcall 'neovm--st-left r)))
                           (nr (funcall 'neovm--st-node (funcall 'neovm--st-key r) rl (funcall 'neovm--st-right r))))
                      (funcall 'neovm--st-rotate-left
                               (funcall 'neovm--st-node (funcall 'neovm--st-key t0) (funcall 'neovm--st-left t0)
                                        (funcall 'neovm--st-rotate-right nr)))))))))))))
  (fset 'neovm--st-insert
    (lambda (key t0)
      (if (null t0) (funcall 'neovm--st-node key nil nil)
        (let ((s (funcall 'neovm--st-splay key t0)))
          (let ((rk (funcall 'neovm--st-key s)))
            (cond ((= key rk) s)
                  ((< key rk) (funcall 'neovm--st-node key (funcall 'neovm--st-left s)
                                       (funcall 'neovm--st-node rk nil (funcall 'neovm--st-right s))))
                  (t (funcall 'neovm--st-node key (funcall 'neovm--st-node rk (funcall 'neovm--st-left s) nil)
                              (funcall 'neovm--st-right s)))))))))
  (fset 'neovm--st-inorder
    (lambda (t0) (if (null t0) nil
      (append (funcall 'neovm--st-inorder (funcall 'neovm--st-left t0))
              (list (funcall 'neovm--st-key t0))
              (funcall 'neovm--st-inorder (funcall 'neovm--st-right t0))))))
  (fset 'neovm--st-max
    (lambda (t0) (if (null (funcall 'neovm--st-right t0)) (funcall 'neovm--st-key t0)
      (funcall 'neovm--st-max (funcall 'neovm--st-right t0)))))
  (fset 'neovm--st-delete
    (lambda (key t0)
      (if (null t0) nil
        (let ((s (funcall 'neovm--st-splay key t0)))
          (if (not (= key (funcall 'neovm--st-key s))) s
            (let ((l (funcall 'neovm--st-left s)) (r (funcall 'neovm--st-right s)))
              (if (null l) r
                (let ((sl (funcall 'neovm--st-splay (funcall 'neovm--st-max l) l)))
                  (funcall 'neovm--st-node (funcall 'neovm--st-key sl) (funcall 'neovm--st-left sl) r)))))))))

  ;; Mixed operations: insert, search (splay), delete, insert more
  (let ((tree nil))
    ;; Phase 1: insert descending
    (dolist (k '(20 15 10 5 25 30))
      (setq tree (funcall 'neovm--st-insert k tree)))
    (let ((phase1 (funcall 'neovm--st-inorder tree)))
      ;; Phase 2: search accesses (splay to root)
      (setq tree (cadr (list (= 10 (funcall 'neovm--st-key (funcall 'neovm--st-splay 10 tree)))
                             (funcall 'neovm--st-splay 10 tree))))
      (let ((root-after-search (funcall 'neovm--st-key tree))
            (phase2 (funcall 'neovm--st-inorder tree)))
        ;; Phase 3: delete some
        (setq tree (funcall 'neovm--st-delete 15 tree))
        (setq tree (funcall 'neovm--st-delete 25 tree))
        (let ((phase3 (funcall 'neovm--st-inorder tree)))
          ;; Phase 4: insert more, including duplicates
          (setq tree (funcall 'neovm--st-insert 12 tree))
          (setq tree (funcall 'neovm--st-insert 10 tree))  ;; duplicate
          (setq tree (funcall 'neovm--st-insert 22 tree))
          (let ((phase4 (funcall 'neovm--st-inorder tree)))
            (unwind-protect
                (list phase1 root-after-search phase2 phase3 phase4)
              (progn
                (fmakunbound 'neovm--st-key) (fmakunbound 'neovm--st-left)
                (fmakunbound 'neovm--st-right) (fmakunbound 'neovm--st-node)
                (fmakunbound 'neovm--st-rotate-right) (fmakunbound 'neovm--st-rotate-left)
                (fmakunbound 'neovm--st-splay) (fmakunbound 'neovm--st-insert)
                (fmakunbound 'neovm--st-inorder) (fmakunbound 'neovm--st-max)
                (fmakunbound 'neovm--st-delete))))))))))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 102 56)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Sequential access pattern: amortized O(1) for repeated access
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_splay_tree_sequential_access() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // After accessing elements sequentially, verify the tree structure
    // reflects the temporal locality property
    let form = r#"(progn
  (fset 'neovm--st-key (lambda (n) (car n)))
  (fset 'neovm--st-left (lambda (n) (cadr n)))
  (fset 'neovm--st-right (lambda (n) (caddr n)))
  (fset 'neovm--st-node (lambda (key left right) (list key left right)))
  (fset 'neovm--st-rotate-right
    (lambda (t0) (let ((l (funcall 'neovm--st-left t0)))
      (if (null l) t0 (funcall 'neovm--st-node (funcall 'neovm--st-key l) (funcall 'neovm--st-left l)
                                (funcall 'neovm--st-node (funcall 'neovm--st-key t0) (funcall 'neovm--st-right l) (funcall 'neovm--st-right t0)))))))
  (fset 'neovm--st-rotate-left
    (lambda (t0) (let ((r (funcall 'neovm--st-right t0)))
      (if (null r) t0 (funcall 'neovm--st-node (funcall 'neovm--st-key r)
                                (funcall 'neovm--st-node (funcall 'neovm--st-key t0) (funcall 'neovm--st-left t0) (funcall 'neovm--st-left r))
                                (funcall 'neovm--st-right r))))))
  (fset 'neovm--st-splay
    (lambda (key t0)
      (if (null t0) nil
        (cond
         ((= key (funcall 'neovm--st-key t0)) t0)
         ((< key (funcall 'neovm--st-key t0))
          (let ((l (funcall 'neovm--st-left t0)))
            (if (null l) t0
              (if (= key (funcall 'neovm--st-key l)) (funcall 'neovm--st-rotate-right t0)
                (if (< key (funcall 'neovm--st-key l))
                    (let* ((ll (funcall 'neovm--st-splay key (funcall 'neovm--st-left l)))
                           (nl (funcall 'neovm--st-node (funcall 'neovm--st-key l) ll (funcall 'neovm--st-right l)))
                           (nt (funcall 'neovm--st-node (funcall 'neovm--st-key t0) nl (funcall 'neovm--st-right t0))))
                      (funcall 'neovm--st-rotate-right (funcall 'neovm--st-rotate-right nt)))
                  (let* ((lr (funcall 'neovm--st-splay key (funcall 'neovm--st-right l)))
                         (nl (funcall 'neovm--st-node (funcall 'neovm--st-key l) (funcall 'neovm--st-left l) lr)))
                    (funcall 'neovm--st-rotate-right
                             (funcall 'neovm--st-node (funcall 'neovm--st-key t0) (funcall 'neovm--st-rotate-left nl) (funcall 'neovm--st-right t0)))))))))
         (t (let ((r (funcall 'neovm--st-right t0)))
              (if (null r) t0
                (if (= key (funcall 'neovm--st-key r)) (funcall 'neovm--st-rotate-left t0)
                  (if (> key (funcall 'neovm--st-key r))
                      (let* ((rr (funcall 'neovm--st-splay key (funcall 'neovm--st-right r)))
                             (nr (funcall 'neovm--st-node (funcall 'neovm--st-key r) (funcall 'neovm--st-left r) rr))
                             (nt (funcall 'neovm--st-node (funcall 'neovm--st-key t0) (funcall 'neovm--st-left t0) nr)))
                        (funcall 'neovm--st-rotate-left (funcall 'neovm--st-rotate-left nt)))
                    (let* ((rl (funcall 'neovm--st-splay key (funcall 'neovm--st-left r)))
                           (nr (funcall 'neovm--st-node (funcall 'neovm--st-key r) rl (funcall 'neovm--st-right r))))
                      (funcall 'neovm--st-rotate-left
                               (funcall 'neovm--st-node (funcall 'neovm--st-key t0) (funcall 'neovm--st-left t0)
                                        (funcall 'neovm--st-rotate-right nr)))))))))))))
  (fset 'neovm--st-insert
    (lambda (key t0)
      (if (null t0) (funcall 'neovm--st-node key nil nil)
        (let ((s (funcall 'neovm--st-splay key t0)))
          (let ((rk (funcall 'neovm--st-key s)))
            (cond ((= key rk) s)
                  ((< key rk) (funcall 'neovm--st-node key (funcall 'neovm--st-left s)
                                       (funcall 'neovm--st-node rk nil (funcall 'neovm--st-right s))))
                  (t (funcall 'neovm--st-node key (funcall 'neovm--st-node rk (funcall 'neovm--st-left s) nil)
                              (funcall 'neovm--st-right s)))))))))
  (fset 'neovm--st-inorder
    (lambda (t0) (if (null t0) nil
      (append (funcall 'neovm--st-inorder (funcall 'neovm--st-left t0))
              (list (funcall 'neovm--st-key t0))
              (funcall 'neovm--st-inorder (funcall 'neovm--st-right t0))))))
  ;; Tree height
  (fset 'neovm--st-height
    (lambda (t0)
      (if (null t0) 0
        (1+ (max (funcall 'neovm--st-height (funcall 'neovm--st-left t0))
                 (funcall 'neovm--st-height (funcall 'neovm--st-right t0)))))))

  ;; Build a tree with 1..15
  (let ((tree nil))
    (dolist (k '(8 4 12 2 6 10 14 1 3 5 7 9 11 13 15))
      (setq tree (funcall 'neovm--st-insert k tree)))
    ;; Access 1, 2, 3 sequentially: each becomes root after access
    (let ((roots nil))
      (dolist (k '(1 2 3 4 5))
        (setq tree (funcall 'neovm--st-splay k tree))
        (setq roots (cons (funcall 'neovm--st-key tree) roots)))
      (let ((seq-roots (nreverse roots))
            (final-inorder (funcall 'neovm--st-inorder tree))
            (final-height (funcall 'neovm--st-height tree)))
        (unwind-protect
            (list seq-roots
                  (equal final-inorder '(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15))
                  final-height
                  ;; After splaying 5, it should be root
                  (funcall 'neovm--st-key tree))
          (progn
            (fmakunbound 'neovm--st-key) (fmakunbound 'neovm--st-left)
            (fmakunbound 'neovm--st-right) (fmakunbound 'neovm--st-node)
            (fmakunbound 'neovm--st-rotate-right) (fmakunbound 'neovm--st-rotate-left)
            (fmakunbound 'neovm--st-splay) (fmakunbound 'neovm--st-insert)
            (fmakunbound 'neovm--st-inorder) (fmakunbound 'neovm--st-height)))))))"#;
    let expect = expect_test::expect![[r#""OK ((1 2 3 4 5) t 6 5)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Duplicate insert and size tracking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_splay_tree_duplicates_and_size() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--st-key (lambda (n) (car n)))
  (fset 'neovm--st-left (lambda (n) (cadr n)))
  (fset 'neovm--st-right (lambda (n) (caddr n)))
  (fset 'neovm--st-node (lambda (key left right) (list key left right)))
  (fset 'neovm--st-rotate-right
    (lambda (t0) (let ((l (funcall 'neovm--st-left t0)))
      (if (null l) t0 (funcall 'neovm--st-node (funcall 'neovm--st-key l) (funcall 'neovm--st-left l)
                                (funcall 'neovm--st-node (funcall 'neovm--st-key t0) (funcall 'neovm--st-right l) (funcall 'neovm--st-right t0)))))))
  (fset 'neovm--st-rotate-left
    (lambda (t0) (let ((r (funcall 'neovm--st-right t0)))
      (if (null r) t0 (funcall 'neovm--st-node (funcall 'neovm--st-key r)
                                (funcall 'neovm--st-node (funcall 'neovm--st-key t0) (funcall 'neovm--st-left t0) (funcall 'neovm--st-left r))
                                (funcall 'neovm--st-right r))))))
  (fset 'neovm--st-splay
    (lambda (key t0)
      (if (null t0) nil
        (cond
         ((= key (funcall 'neovm--st-key t0)) t0)
         ((< key (funcall 'neovm--st-key t0))
          (let ((l (funcall 'neovm--st-left t0)))
            (if (null l) t0
              (if (= key (funcall 'neovm--st-key l)) (funcall 'neovm--st-rotate-right t0)
                (if (< key (funcall 'neovm--st-key l))
                    (let* ((ll (funcall 'neovm--st-splay key (funcall 'neovm--st-left l)))
                           (nl (funcall 'neovm--st-node (funcall 'neovm--st-key l) ll (funcall 'neovm--st-right l)))
                           (nt (funcall 'neovm--st-node (funcall 'neovm--st-key t0) nl (funcall 'neovm--st-right t0))))
                      (funcall 'neovm--st-rotate-right (funcall 'neovm--st-rotate-right nt)))
                  (let* ((lr (funcall 'neovm--st-splay key (funcall 'neovm--st-right l)))
                         (nl (funcall 'neovm--st-node (funcall 'neovm--st-key l) (funcall 'neovm--st-left l) lr)))
                    (funcall 'neovm--st-rotate-right
                             (funcall 'neovm--st-node (funcall 'neovm--st-key t0) (funcall 'neovm--st-rotate-left nl) (funcall 'neovm--st-right t0)))))))))
         (t (let ((r (funcall 'neovm--st-right t0)))
              (if (null r) t0
                (if (= key (funcall 'neovm--st-key r)) (funcall 'neovm--st-rotate-left t0)
                  (if (> key (funcall 'neovm--st-key r))
                      (let* ((rr (funcall 'neovm--st-splay key (funcall 'neovm--st-right r)))
                             (nr (funcall 'neovm--st-node (funcall 'neovm--st-key r) (funcall 'neovm--st-left r) rr))
                             (nt (funcall 'neovm--st-node (funcall 'neovm--st-key t0) (funcall 'neovm--st-left t0) nr)))
                        (funcall 'neovm--st-rotate-left (funcall 'neovm--st-rotate-left nt)))
                    (let* ((rl (funcall 'neovm--st-splay key (funcall 'neovm--st-left r)))
                           (nr (funcall 'neovm--st-node (funcall 'neovm--st-key r) rl (funcall 'neovm--st-right r))))
                      (funcall 'neovm--st-rotate-left
                               (funcall 'neovm--st-node (funcall 'neovm--st-key t0) (funcall 'neovm--st-left t0)
                                        (funcall 'neovm--st-rotate-right nr)))))))))))))
  (fset 'neovm--st-insert
    (lambda (key t0)
      (if (null t0) (funcall 'neovm--st-node key nil nil)
        (let ((s (funcall 'neovm--st-splay key t0)))
          (let ((rk (funcall 'neovm--st-key s)))
            (cond ((= key rk) s)  ;; no duplicate
                  ((< key rk) (funcall 'neovm--st-node key (funcall 'neovm--st-left s)
                                       (funcall 'neovm--st-node rk nil (funcall 'neovm--st-right s))))
                  (t (funcall 'neovm--st-node key (funcall 'neovm--st-node rk (funcall 'neovm--st-left s) nil)
                              (funcall 'neovm--st-right s)))))))))
  (fset 'neovm--st-inorder
    (lambda (t0) (if (null t0) nil
      (append (funcall 'neovm--st-inorder (funcall 'neovm--st-left t0))
              (list (funcall 'neovm--st-key t0))
              (funcall 'neovm--st-inorder (funcall 'neovm--st-right t0))))))
  (fset 'neovm--st-size
    (lambda (t0) (if (null t0) 0
      (+ 1 (funcall 'neovm--st-size (funcall 'neovm--st-left t0))
           (funcall 'neovm--st-size (funcall 'neovm--st-right t0))))))

  ;; Insert with duplicates: size should not grow
  (let ((tree nil))
    (dolist (k '(5 3 7 1 4 6 8))
      (setq tree (funcall 'neovm--st-insert k tree)))
    (let ((size-before (funcall 'neovm--st-size tree))
          (inorder-before (funcall 'neovm--st-inorder tree)))
      ;; Insert duplicates
      (dolist (k '(5 3 7 1 4 6 8))
        (setq tree (funcall 'neovm--st-insert k tree)))
      (let ((size-after (funcall 'neovm--st-size tree))
            (inorder-after (funcall 'neovm--st-inorder tree)))
        ;; Insert some new ones
        (dolist (k '(2 9 0))
          (setq tree (funcall 'neovm--st-insert k tree)))
        (let ((size-final (funcall 'neovm--st-size tree))
              (inorder-final (funcall 'neovm--st-inorder tree)))
          (unwind-protect
              (list size-before size-after
                    (equal inorder-before inorder-after)
                    size-final inorder-final)
            (progn
              (fmakunbound 'neovm--st-key) (fmakunbound 'neovm--st-left)
              (fmakunbound 'neovm--st-right) (fmakunbound 'neovm--st-node)
              (fmakunbound 'neovm--st-rotate-right) (fmakunbound 'neovm--st-rotate-left)
              (fmakunbound 'neovm--st-splay) (fmakunbound 'neovm--st-insert)
              (fmakunbound 'neovm--st-inorder) (fmakunbound 'neovm--st-size))))))))"#;
    let expect = expect_test::expect![[r#""OK (7 7 t 10 (0 1 2 3 4 5 6 7 8 9))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Single-element and two-element trees: edge cases for rotations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_splay_tree_small_trees() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--st-key (lambda (n) (car n)))
  (fset 'neovm--st-left (lambda (n) (cadr n)))
  (fset 'neovm--st-right (lambda (n) (caddr n)))
  (fset 'neovm--st-node (lambda (key left right) (list key left right)))
  (fset 'neovm--st-rotate-right
    (lambda (t0) (let ((l (funcall 'neovm--st-left t0)))
      (if (null l) t0 (funcall 'neovm--st-node (funcall 'neovm--st-key l) (funcall 'neovm--st-left l)
                                (funcall 'neovm--st-node (funcall 'neovm--st-key t0) (funcall 'neovm--st-right l) (funcall 'neovm--st-right t0)))))))
  (fset 'neovm--st-rotate-left
    (lambda (t0) (let ((r (funcall 'neovm--st-right t0)))
      (if (null r) t0 (funcall 'neovm--st-node (funcall 'neovm--st-key r)
                                (funcall 'neovm--st-node (funcall 'neovm--st-key t0) (funcall 'neovm--st-left t0) (funcall 'neovm--st-left r))
                                (funcall 'neovm--st-right r))))))
  (fset 'neovm--st-splay
    (lambda (key t0)
      (if (null t0) nil
        (cond
         ((= key (funcall 'neovm--st-key t0)) t0)
         ((< key (funcall 'neovm--st-key t0))
          (let ((l (funcall 'neovm--st-left t0)))
            (if (null l) t0
              (if (= key (funcall 'neovm--st-key l)) (funcall 'neovm--st-rotate-right t0)
                (if (< key (funcall 'neovm--st-key l))
                    (let* ((ll (funcall 'neovm--st-splay key (funcall 'neovm--st-left l)))
                           (nl (funcall 'neovm--st-node (funcall 'neovm--st-key l) ll (funcall 'neovm--st-right l)))
                           (nt (funcall 'neovm--st-node (funcall 'neovm--st-key t0) nl (funcall 'neovm--st-right t0))))
                      (funcall 'neovm--st-rotate-right (funcall 'neovm--st-rotate-right nt)))
                  (let* ((lr (funcall 'neovm--st-splay key (funcall 'neovm--st-right l)))
                         (nl (funcall 'neovm--st-node (funcall 'neovm--st-key l) (funcall 'neovm--st-left l) lr)))
                    (funcall 'neovm--st-rotate-right
                             (funcall 'neovm--st-node (funcall 'neovm--st-key t0) (funcall 'neovm--st-rotate-left nl) (funcall 'neovm--st-right t0)))))))))
         (t (let ((r (funcall 'neovm--st-right t0)))
              (if (null r) t0
                (if (= key (funcall 'neovm--st-key r)) (funcall 'neovm--st-rotate-left t0)
                  (if (> key (funcall 'neovm--st-key r))
                      (let* ((rr (funcall 'neovm--st-splay key (funcall 'neovm--st-right r)))
                             (nr (funcall 'neovm--st-node (funcall 'neovm--st-key r) (funcall 'neovm--st-left r) rr))
                             (nt (funcall 'neovm--st-node (funcall 'neovm--st-key t0) (funcall 'neovm--st-left t0) nr)))
                        (funcall 'neovm--st-rotate-left (funcall 'neovm--st-rotate-left nt)))
                    (let* ((rl (funcall 'neovm--st-splay key (funcall 'neovm--st-left r)))
                           (nr (funcall 'neovm--st-node (funcall 'neovm--st-key r) rl (funcall 'neovm--st-right r))))
                      (funcall 'neovm--st-rotate-left
                               (funcall 'neovm--st-node (funcall 'neovm--st-key t0) (funcall 'neovm--st-left t0)
                                        (funcall 'neovm--st-rotate-right nr)))))))))))))
  (fset 'neovm--st-insert
    (lambda (key t0)
      (if (null t0) (funcall 'neovm--st-node key nil nil)
        (let ((s (funcall 'neovm--st-splay key t0)))
          (let ((rk (funcall 'neovm--st-key s)))
            (cond ((= key rk) s)
                  ((< key rk) (funcall 'neovm--st-node key (funcall 'neovm--st-left s)
                                       (funcall 'neovm--st-node rk nil (funcall 'neovm--st-right s))))
                  (t (funcall 'neovm--st-node key (funcall 'neovm--st-node rk (funcall 'neovm--st-left s) nil)
                              (funcall 'neovm--st-right s)))))))))
  (fset 'neovm--st-inorder
    (lambda (t0) (if (null t0) nil
      (append (funcall 'neovm--st-inorder (funcall 'neovm--st-left t0))
              (list (funcall 'neovm--st-key t0))
              (funcall 'neovm--st-inorder (funcall 'neovm--st-right t0))))))

  (let ((results nil))
    ;; Empty tree splay
    (setq results (cons (list 'empty-splay (funcall 'neovm--st-splay 5 nil)) results))

    ;; Single element
    (let ((t1 (funcall 'neovm--st-insert 42 nil)))
      (setq results (cons (list 'single (funcall 'neovm--st-key t1)
                                (funcall 'neovm--st-inorder t1)) results))
      ;; Splay same key
      (let ((t1s (funcall 'neovm--st-splay 42 t1)))
        (setq results (cons (list 'single-splay-same (funcall 'neovm--st-key t1s)) results)))
      ;; Splay non-existent: root stays
      (let ((t1m (funcall 'neovm--st-splay 99 t1)))
        (setq results (cons (list 'single-splay-miss (funcall 'neovm--st-key t1m)) results))))

    ;; Two elements: ascending
    (let* ((t2 (funcall 'neovm--st-insert 1 nil))
           (t2 (funcall 'neovm--st-insert 2 t2)))
      (setq results (cons (list 'two-asc (funcall 'neovm--st-key t2)
                                (funcall 'neovm--st-inorder t2)) results))
      ;; Splay smaller: should become root
      (let ((t2s (funcall 'neovm--st-splay 1 t2)))
        (setq results (cons (list 'two-splay-small (funcall 'neovm--st-key t2s)
                                  (funcall 'neovm--st-inorder t2s)) results))))

    ;; Two elements: descending
    (let* ((t3 (funcall 'neovm--st-insert 2 nil))
           (t3 (funcall 'neovm--st-insert 1 t3)))
      (setq results (cons (list 'two-desc (funcall 'neovm--st-key t3)
                                (funcall 'neovm--st-inorder t3)) results))
      ;; Splay larger: should become root
      (let ((t3s (funcall 'neovm--st-splay 2 t3)))
        (setq results (cons (list 'two-splay-large (funcall 'neovm--st-key t3s)
                                  (funcall 'neovm--st-inorder t3s)) results))))

    (unwind-protect
        (nreverse results)
      (progn
        (fmakunbound 'neovm--st-key) (fmakunbound 'neovm--st-left)
        (fmakunbound 'neovm--st-right) (fmakunbound 'neovm--st-node)
        (fmakunbound 'neovm--st-rotate-right) (fmakunbound 'neovm--st-rotate-left)
        (fmakunbound 'neovm--st-splay) (fmakunbound 'neovm--st-insert)
        (fmakunbound 'neovm--st-inorder)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((empty-splay nil) (single 42 (42)) (single-splay-same 42) (single-splay-miss 42) (two-asc 2 (1 2)) (two-splay-small 1 (1 2)) (two-desc 1 (1 2)) (two-splay-large 2 (1 2)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
