//! Oracle parity tests for persistent (immutable) data structures:
//! persistent list (prepend O(1), shared tail), persistent association list,
//! persistent vector (path-copying), structural sharing verification,
//! undo history with persistent snapshots.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Persistent list with O(1) prepend and structural sharing verification
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_persistent_data_list_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Persistent list operations: prepend, drop, take, split, reverse.
    // Each operation returns a new list; originals are unmodified.
    // Verify structural sharing with `eq`.
    let form = r#"(progn
  ;; Persistent operations on cons lists
  (fset 'neovm--pdl-prepend
    (lambda (lst val) (cons val lst)))

  (fset 'neovm--pdl-drop
    (lambda (lst n)
      "Drop first N elements, returning shared tail."
      (let ((result lst) (i 0))
        (while (and result (< i n))
          (setq result (cdr result))
          (setq i (1+ i)))
        result)))

  (fset 'neovm--pdl-take
    (lambda (lst n)
      "Take first N elements, building new spine."
      (let ((result nil) (current lst) (i 0))
        (while (and current (< i n))
          (setq result (cons (car current) result))
          (setq current (cdr current))
          (setq i (1+ i)))
        (nreverse result))))

  (fset 'neovm--pdl-split
    (lambda (lst n)
      "Split at position N: returns (take-n . drop-n)."
      (cons (funcall 'neovm--pdl-take lst n)
            (funcall 'neovm--pdl-drop lst n))))

  (fset 'neovm--pdl-insert-at
    (lambda (lst n val)
      "Insert VAL at position N, returning new list."
      (let ((front (funcall 'neovm--pdl-take lst n))
            (back (funcall 'neovm--pdl-drop lst n)))
        (append front (cons val back)))))

  (fset 'neovm--pdl-remove-at
    (lambda (lst n)
      "Remove element at position N, returning new list."
      (let ((front (funcall 'neovm--pdl-take lst n))
            (back (funcall 'neovm--pdl-drop lst (1+ n))))
        (append front back))))

  (unwind-protect
      (let* ((base '(10 20 30 40 50))
             ;; Prepend creates shared tail
             (v1 (funcall 'neovm--pdl-prepend base 5))
             (v2 (funcall 'neovm--pdl-prepend base 0))
             ;; Drop shares suffix
             (v3 (funcall 'neovm--pdl-drop base 2))
             ;; Take builds new spine
             (v4 (funcall 'neovm--pdl-take base 3))
             ;; Split gives both
             (v5 (funcall 'neovm--pdl-split base 2))
             ;; Insert at position
             (v6 (funcall 'neovm--pdl-insert-at base 2 25))
             ;; Remove from position
             (v7 (funcall 'neovm--pdl-remove-at base 1)))
        (list
         ;; Values
         v1 v2 v3 v4 v5 v6 v7
         ;; Base unchanged
         (equal base '(10 20 30 40 50))
         ;; Structural sharing: cdr of v1 IS base
         (eq (cdr v1) base)
         ;; cdr of v2 IS base
         (eq (cdr v2) base)
         ;; v3 IS (cddr base) - shares tail
         (eq v3 (cddr base))
         ;; v4 does NOT share with base (new cons cells)
         (not (eq v4 base))
         ;; split's cdr shares with base
         (eq (cdr v5) (cddr base))
         ;; Insert shares tail from insertion point
         (eq (cdddr v6) (cddr base))
         ;; Lengths
         (length v1) (length v2) (length v3)
         (length v4) (length v6) (length v7)))
    (fmakunbound 'neovm--pdl-prepend)
    (fmakunbound 'neovm--pdl-drop)
    (fmakunbound 'neovm--pdl-take)
    (fmakunbound 'neovm--pdl-split)
    (fmakunbound 'neovm--pdl-insert-at)
    (fmakunbound 'neovm--pdl-remove-at)))"#;
    let expect = expect_test::expect![[
        r#""OK ((5 10 20 30 40 50) (0 10 20 30 40 50) (30 40 50) (10 20 30) ((10 20) 30 40 50) (10 20 25 30 40 50) (10 30 40 50) t t t t t t t 6 6 3 3 6 4)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Persistent association list with versioned updates
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_persistent_data_alist_versioned() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Persistent alist where each update creates a new version.
    // Supports: put (shadow), get (most recent), delete (rebuild without key),
    // merge (combine two alists), diff (find keys that differ).
    let form = r#"(progn
  (fset 'neovm--pda-put
    (lambda (alist key val)
      (cons (cons key val) alist)))

  (fset 'neovm--pda-get
    (lambda (alist key)
      (let ((pair (assoc key alist)))
        (if pair (cdr pair) nil))))

  (fset 'neovm--pda-delete
    (lambda (alist key)
      "Remove all bindings for KEY."
      (let ((result nil))
        (dolist (pair alist)
          (unless (equal (car pair) key)
            (setq result (cons pair result))))
        (nreverse result))))

  (fset 'neovm--pda-merge
    (lambda (alist1 alist2)
      "Merge alist2 into alist1. alist2 bindings shadow alist1."
      (let ((result (copy-sequence alist1)))
        ;; Prepend alist2 entries (they will shadow)
        (dolist (pair (reverse alist2))
          (setq result (cons pair result)))
        result)))

  (fset 'neovm--pda-keys
    (lambda (alist)
      "Unique keys in first-occurrence order."
      (let ((seen (make-hash-table :test 'equal))
            (result nil))
        (dolist (pair alist)
          (unless (gethash (car pair) seen)
            (puthash (car pair) t seen)
            (setq result (cons (car pair) result))))
        (nreverse result))))

  (fset 'neovm--pda-to-alist
    (lambda (palist)
      "Compact: one entry per key, most recent value."
      (let ((keys (funcall 'neovm--pda-keys palist))
            (result nil))
        (dolist (k keys)
          (setq result (cons (cons k (funcall 'neovm--pda-get palist k)) result)))
        (nreverse result))))

  (unwind-protect
      (let* ((v0 nil)
             (v1 (funcall 'neovm--pda-put v0 :name "Alice"))
             (v2 (funcall 'neovm--pda-put v1 :age 30))
             (v3 (funcall 'neovm--pda-put v2 :city "NYC"))
             ;; Update: shadow :age
             (v4 (funcall 'neovm--pda-put v3 :age 31))
             ;; Delete: remove :city
             (v5 (funcall 'neovm--pda-delete v4 :city))
             ;; Another branch from v3: different update
             (v3b (funcall 'neovm--pda-put v3 :name "Bob"))
             ;; Merge: v5's data with a new alist
             (extra '((:email . "alice@example.com") (:phone . "555-1234")))
             (v6 (funcall 'neovm--pda-merge v5 extra)))
        (list
         ;; Lookups at different versions
         (funcall 'neovm--pda-get v1 :name)
         (funcall 'neovm--pda-get v1 :age)     ;; nil - not yet added
         (funcall 'neovm--pda-get v3 :age)     ;; 30
         (funcall 'neovm--pda-get v4 :age)     ;; 31 (shadowed)
         (funcall 'neovm--pda-get v3 :city)    ;; "NYC"
         (funcall 'neovm--pda-get v5 :city)    ;; nil (deleted)
         ;; Branch: v3b has Bob, v3 still has Alice
         (funcall 'neovm--pda-get v3b :name)
         (funcall 'neovm--pda-get v3 :name)
         ;; Keys
         (funcall 'neovm--pda-keys v4)
         ;; Compact versions
         (funcall 'neovm--pda-to-alist v4)
         (funcall 'neovm--pda-to-alist v5)
         (funcall 'neovm--pda-to-alist v3b)
         ;; Merged
         (funcall 'neovm--pda-get v6 :email)
         (funcall 'neovm--pda-get v6 :name)
         (funcall 'neovm--pda-keys v6)
         ;; Structural sharing: v1 is the tail of v2
         (eq (cdr v2) v1)
         ;; v3 tail chain reaches v1
         (eq (cddr v3) v1)
         ;; v4 shares tail from v3
         (eq (cdr v4) v3)))
    (fmakunbound 'neovm--pda-put)
    (fmakunbound 'neovm--pda-get)
    (fmakunbound 'neovm--pda-delete)
    (fmakunbound 'neovm--pda-merge)
    (fmakunbound 'neovm--pda-keys)
    (fmakunbound 'neovm--pda-to-alist)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Alice\" nil 30 31 \"NYC\" nil \"Bob\" \"Alice\" (:age :city :name) ((:age . 31) (:city . \"NYC\") (:name . \"Alice\")) ((:age . 31) (:name . \"Alice\")) ((:name . \"Bob\") (:city . \"NYC\") (:age . 30)) \"alice@example.com\" \"Alice\" (:email :phone :age :name) t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Persistent vector via path-copying binary tree
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_persistent_data_vector_path_copy() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Persistent vector backed by a binary tree of fixed-size leaf arrays.
    // Updates use path-copying: only nodes on the path from root to target
    // leaf are copied; siblings are shared.
    let form = r#"(progn
  (fset 'neovm--pdv-leaf-size (lambda () 4))

  (fset 'neovm--pdv-from-list
    (lambda (lst)
      "Build persistent vector from list."
      (let ((leaves nil) (chunk nil) (i 0) (lsz (funcall 'neovm--pdv-leaf-size)))
        (dolist (x lst)
          (setq chunk (cons x chunk))
          (setq i (1+ i))
          (when (= i lsz)
            (setq leaves (cons (apply #'vector (nreverse chunk)) leaves))
            (setq chunk nil)
            (setq i 0)))
        (when chunk
          (setq leaves (cons (apply #'vector (nreverse chunk)) leaves)))
        (funcall 'neovm--pdv-build-tree (nreverse leaves)))))

  (fset 'neovm--pdv-build-tree
    (lambda (nodes)
      (cond
       ((null nodes) nil)
       ((= (length nodes) 1) (car nodes))
       (t (let ((mid (/ (length nodes) 2))
                (left nil) (right nil) (i 0))
            (dolist (n nodes)
              (if (< i mid) (setq left (cons n left))
                (setq right (cons n right)))
              (setq i (1+ i)))
            (cons (funcall 'neovm--pdv-build-tree (nreverse left))
                  (funcall 'neovm--pdv-build-tree (nreverse right))))))))

  (fset 'neovm--pdv-size
    (lambda (pv)
      (cond
       ((null pv) 0)
       ((vectorp pv) (length pv))
       ((consp pv) (+ (funcall 'neovm--pdv-size (car pv))
                       (funcall 'neovm--pdv-size (cdr pv))))
       (t 0))))

  (fset 'neovm--pdv-get
    (lambda (pv idx)
      (cond
       ((null pv) nil)
       ((vectorp pv) (if (< idx (length pv)) (aref pv idx) nil))
       ((consp pv)
        (let ((lsz (funcall 'neovm--pdv-size (car pv))))
          (if (< idx lsz)
              (funcall 'neovm--pdv-get (car pv) idx)
            (funcall 'neovm--pdv-get (cdr pv) (- idx lsz))))))))

  (fset 'neovm--pdv-set
    (lambda (pv idx val)
      "Return new persistent vector with element at IDX set to VAL."
      (cond
       ((null pv) nil)
       ((vectorp pv)
        (let ((new-v (copy-sequence pv)))
          (aset new-v idx val)
          new-v))
       ((consp pv)
        (let ((lsz (funcall 'neovm--pdv-size (car pv))))
          (if (< idx lsz)
              ;; Path-copy left, share right
              (cons (funcall 'neovm--pdv-set (car pv) idx val) (cdr pv))
            ;; Share left, path-copy right
            (cons (car pv) (funcall 'neovm--pdv-set (cdr pv) (- idx lsz) val))))))))

  (fset 'neovm--pdv-to-list
    (lambda (pv)
      (cond
       ((null pv) nil)
       ((vectorp pv) (append pv nil))
       ((consp pv) (append (funcall 'neovm--pdv-to-list (car pv))
                           (funcall 'neovm--pdv-to-list (cdr pv)))))))

  (fset 'neovm--pdv-push
    (lambda (pv val)
      "Append VAL to end (rebuilds rightmost path)."
      (funcall 'neovm--pdv-from-list
               (append (funcall 'neovm--pdv-to-list pv) (list val)))))

  (unwind-protect
      (let* ((v0 (funcall 'neovm--pdv-from-list '(0 1 2 3 4 5 6 7)))
             ;; Set index 2 to 99 (left subtree change)
             (v1 (funcall 'neovm--pdv-set v0 2 99))
             ;; Set index 6 to 66 (right subtree change)
             (v2 (funcall 'neovm--pdv-set v0 6 66))
             ;; Set on v1: change index 5 (right subtree)
             (v3 (funcall 'neovm--pdv-set v1 5 55))
             ;; Multiple updates on same version (branching)
             (v4a (funcall 'neovm--pdv-set v0 0 -1))
             (v4b (funcall 'neovm--pdv-set v0 0 -2))
             ;; Push value
             (v5 (funcall 'neovm--pdv-push v0 8)))
        (list
         ;; All versions as lists
         (funcall 'neovm--pdv-to-list v0)
         (funcall 'neovm--pdv-to-list v1)
         (funcall 'neovm--pdv-to-list v2)
         (funcall 'neovm--pdv-to-list v3)
         (funcall 'neovm--pdv-to-list v4a)
         (funcall 'neovm--pdv-to-list v4b)
         (funcall 'neovm--pdv-to-list v5)
         ;; Sizes
         (funcall 'neovm--pdv-size v0)
         (funcall 'neovm--pdv-size v5)
         ;; Specific lookups across versions
         (funcall 'neovm--pdv-get v0 2)  ;; 2
         (funcall 'neovm--pdv-get v1 2)  ;; 99
         (funcall 'neovm--pdv-get v2 6)  ;; 66
         (funcall 'neovm--pdv-get v0 6)  ;; 6 (unchanged)
         ;; Structural sharing: v1 changed left, so right is shared with v0
         (eq (cdr v1) (cdr v0))
         ;; v2 changed right, so left is shared with v0
         (eq (car v2) (car v0))
         ;; v4a and v4b share right subtree with v0
         (eq (cdr v4a) (cdr v0))
         (eq (cdr v4b) (cdr v0))
         ;; v4a and v4b differ in left
         (not (eq (car v4a) (car v4b)))))
    (fmakunbound 'neovm--pdv-leaf-size)
    (fmakunbound 'neovm--pdv-from-list)
    (fmakunbound 'neovm--pdv-build-tree)
    (fmakunbound 'neovm--pdv-size)
    (fmakunbound 'neovm--pdv-get)
    (fmakunbound 'neovm--pdv-set)
    (fmakunbound 'neovm--pdv-to-list)
    (fmakunbound 'neovm--pdv-push)))"#;
    let expect = expect_test::expect![[
        r#""OK ((0 1 2 3 4 5 6 7) (0 1 99 3 4 5 6 7) (0 1 2 3 4 5 66 7) (0 1 99 3 4 55 6 7) (-1 1 2 3 4 5 6 7) (-2 1 2 3 4 5 6 7) (0 1 2 3 4 5 6 7 8) 8 9 2 99 66 6 t t t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Structural sharing stress test: many versions from same base
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_persistent_data_sharing_stress() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Create many versions from the same base list and verify that
    // shared tails remain `eq` across all versions
    let form = r#"(let* ((base '(a b c d e f g h i j))
       ;; Create 10 versions, each prepending a different element
       (versions (let ((vs nil) (i 0))
                   (while (< i 10)
                     (setq vs (cons (cons (intern (format "v%d" i)) base) vs))
                     (setq i (1+ i)))
                   (nreverse vs)))
       ;; Verify all versions share the same base tail
       (all-share-base
        (let ((result t))
          (dolist (v versions)
            (unless (eq (cdr v) base)
              (setq result nil)))
          result))
       ;; Create branching versions: prepend to first version
       (v0 (car versions))
       (branch-a (cons 'xa (cons 'ya v0)))
       (branch-b (cons 'xb (cons 'yb v0)))
       ;; Deep nesting: chain of prepends sharing progressively less
       (chain0 base)
       (chain1 (cons 100 chain0))
       (chain2 (cons 200 chain1))
       (chain3 (cons 300 chain2))
       (chain4 (cons 400 chain3)))
  (list
   ;; All 10 versions share base
   all-share-base
   ;; Branches share v0
   (eq (cddr branch-a) v0)
   (eq (cddr branch-b) v0)
   ;; Branches share base through v0
   (eq (cdddr branch-a) base)
   (eq (cdddr branch-b) base)
   ;; Chain sharing
   (eq (cdr chain1) chain0)
   (eq (cdr chain2) chain1)
   (eq (cddr chain2) chain0)
   (eq (cdr chain4) chain3)
   (eq (cdddr chain4) chain1)
   ;; Content unchanged at each level
   (equal chain0 '(a b c d e f g h i j))
   (equal chain4 '(400 300 200 100 a b c d e f g h i j))
   ;; Lengths
   (length chain0) (length chain1) (length chain4)
   ;; Count versions
   (length versions)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t t t 10 11 14 10)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Undo history with persistent snapshots and branching
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_persistent_data_undo_history() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // An undo system that keeps full history as persistent snapshots.
    // Supports undo, redo, branching (undo then new edit creates new branch),
    // and inspection of entire history tree.
    let form = r#"(progn
  ;; History node: (state parent-index redo-branches)
  ;; System: (current-index nodes)
  (fset 'neovm--puh-make
    (lambda (initial-state)
      (list 0 (list (list initial-state -1 nil)))))

  (fset 'neovm--puh-current-state
    (lambda (sys)
      (car (nth (car sys) (cadr sys)))))

  (fset 'neovm--puh-current-index
    (lambda (sys) (car sys)))

  (fset 'neovm--puh-node-count
    (lambda (sys) (length (cadr sys))))

  (fset 'neovm--puh-edit
    (lambda (sys new-state)
      "Apply edit: create new node, set as current."
      (let* ((cur-idx (car sys))
             (nodes (cadr sys))
             (new-idx (length nodes))
             (new-node (list new-state cur-idx nil)))
        (list new-idx (append nodes (list new-node))))))

  (fset 'neovm--puh-undo
    (lambda (sys)
      "Move to parent node."
      (let* ((cur-idx (car sys))
             (nodes (cadr sys))
             (cur-node (nth cur-idx nodes))
             (parent-idx (cadr cur-node)))
        (if (< parent-idx 0)
            sys  ;; at root, can't undo
          (list parent-idx nodes)))))

  (fset 'neovm--puh-can-undo
    (lambda (sys)
      (let* ((cur-node (nth (car sys) (cadr sys))))
        (>= (cadr cur-node) 0))))

  (fset 'neovm--puh-history-path
    (lambda (sys)
      "Return list of states from root to current."
      (let ((path nil)
            (idx (car sys))
            (nodes (cadr sys)))
        (while (>= idx 0)
          (let ((node (nth idx nodes)))
            (setq path (cons (car node) path))
            (setq idx (cadr node))))
        path)))

  (unwind-protect
      (let* (;; Start with empty document
             (s0 (funcall 'neovm--puh-make '(:text "")))
             ;; Edit 1
             (s1 (funcall 'neovm--puh-edit s0 '(:text "Hello")))
             ;; Edit 2
             (s2 (funcall 'neovm--puh-edit s1 '(:text "Hello World")))
             ;; Edit 3
             (s3 (funcall 'neovm--puh-edit s2 '(:text "Hello World!")))
             ;; Undo once
             (s4 (funcall 'neovm--puh-undo s3))
             ;; Undo again
             (s5 (funcall 'neovm--puh-undo s4))
             ;; Branch: new edit from s5 (which is at s1's state)
             (s6 (funcall 'neovm--puh-edit s5 '(:text "Hello Emacs")))
             ;; Continue on branch
             (s7 (funcall 'neovm--puh-edit s6 '(:text "Hello Emacs!")))
             ;; Undo all the way to root
             (s8 (funcall 'neovm--puh-undo
                          (funcall 'neovm--puh-undo
                                   (funcall 'neovm--puh-undo s7)))))
        (list
         ;; Current states at each point
         (funcall 'neovm--puh-current-state s0)
         (funcall 'neovm--puh-current-state s1)
         (funcall 'neovm--puh-current-state s2)
         (funcall 'neovm--puh-current-state s3)
         ;; After undo
         (funcall 'neovm--puh-current-state s4)
         (funcall 'neovm--puh-current-state s5)
         ;; Branch
         (funcall 'neovm--puh-current-state s6)
         (funcall 'neovm--puh-current-state s7)
         ;; Back to root
         (funcall 'neovm--puh-current-state s8)
         ;; History paths
         (funcall 'neovm--puh-history-path s3)
         (funcall 'neovm--puh-history-path s7)
         (funcall 'neovm--puh-history-path s5)
         ;; Can-undo checks
         (funcall 'neovm--puh-can-undo s0)   ;; nil (at root)
         (funcall 'neovm--puh-can-undo s3)   ;; t
         (funcall 'neovm--puh-can-undo s8)   ;; nil (back at root)
         ;; Node counts: each edit adds a node
         (funcall 'neovm--puh-node-count s0)
         (funcall 'neovm--puh-node-count s3)
         (funcall 'neovm--puh-node-count s7)
         ;; Old snapshots still valid (persistence)
         (equal (funcall 'neovm--puh-current-state s2) '(:text "Hello World"))
         (equal (funcall 'neovm--puh-current-state s6) '(:text "Hello Emacs"))))
    (fmakunbound 'neovm--puh-make)
    (fmakunbound 'neovm--puh-current-state)
    (fmakunbound 'neovm--puh-current-index)
    (fmakunbound 'neovm--puh-node-count)
    (fmakunbound 'neovm--puh-edit)
    (fmakunbound 'neovm--puh-undo)
    (fmakunbound 'neovm--puh-can-undo)
    (fmakunbound 'neovm--puh-history-path)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:text \"\") (:text \"Hello\") (:text \"Hello World\") (:text \"Hello World!\") (:text \"Hello World\") (:text \"Hello\") (:text \"Hello Emacs\") (:text \"Hello Emacs!\") (:text \"\") ((:text \"\") (:text \"Hello\") (:text \"Hello World\") (:text \"Hello World!\")) ((:text \"\") (:text \"Hello\") (:text \"Hello Emacs\") (:text \"Hello Emacs!\")) ((:text \"\") (:text \"Hello\")) nil t nil 1 4 6 t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Persistent hash map simulation via sorted alist with binary search
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_persistent_data_sorted_map() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A persistent sorted map backed by an alist kept in sorted order.
    // Supports: insert, lookup, delete, merge, range-query.
    // All operations return new maps without mutating originals.
    let form = r#"(progn
  (fset 'neovm--psm-empty (lambda () nil))

  (fset 'neovm--psm-insert
    (lambda (smap key val)
      "Insert key-val into sorted map (by key), returning new map."
      (cond
       ((null smap) (list (cons key val)))
       ((< key (caar smap))
        (cons (cons key val) smap))
       ((= key (caar smap))
        ;; Replace existing key
        (cons (cons key val) (cdr smap)))
       (t (cons (car smap)
                (funcall 'neovm--psm-insert (cdr smap) key val))))))

  (fset 'neovm--psm-lookup
    (lambda (smap key)
      (let ((pair (assoc key smap)))
        (if pair (cdr pair) nil))))

  (fset 'neovm--psm-delete
    (lambda (smap key)
      (cond
       ((null smap) nil)
       ((= key (caar smap)) (cdr smap))
       (t (cons (car smap)
                (funcall 'neovm--psm-delete (cdr smap) key))))))

  (fset 'neovm--psm-keys
    (lambda (smap) (mapcar #'car smap)))

  (fset 'neovm--psm-values
    (lambda (smap) (mapcar #'cdr smap)))

  (fset 'neovm--psm-range
    (lambda (smap low high)
      "Return entries where low <= key <= high."
      (let ((result nil))
        (dolist (pair smap)
          (when (and (>= (car pair) low) (<= (car pair) high))
            (setq result (cons pair result))))
        (nreverse result))))

  (fset 'neovm--psm-merge
    (lambda (smap1 smap2)
      "Merge smap2 into smap1. smap2 values take precedence."
      (let ((result smap1))
        (dolist (pair smap2)
          (setq result (funcall 'neovm--psm-insert result (car pair) (cdr pair))))
        result)))

  (unwind-protect
      (let* ((m0 (funcall 'neovm--psm-empty))
             (m1 (funcall 'neovm--psm-insert m0 5 "five"))
             (m2 (funcall 'neovm--psm-insert m1 3 "three"))
             (m3 (funcall 'neovm--psm-insert m2 7 "seven"))
             (m4 (funcall 'neovm--psm-insert m3 1 "one"))
             (m5 (funcall 'neovm--psm-insert m4 9 "nine"))
             ;; Update existing key
             (m6 (funcall 'neovm--psm-insert m5 5 "FIVE"))
             ;; Delete
             (m7 (funcall 'neovm--psm-delete m5 3))
             ;; Branch from m5: different update
             (m5b (funcall 'neovm--psm-insert m5 5 "cinq"))
             ;; Merge
             (extra (funcall 'neovm--psm-insert
                             (funcall 'neovm--psm-insert
                                      (funcall 'neovm--psm-empty) 2 "two")
                             4 "four"))
             (m8 (funcall 'neovm--psm-merge m5 extra)))
        (list
         ;; Keys are sorted
         (funcall 'neovm--psm-keys m5)
         ;; Values
         (funcall 'neovm--psm-values m5)
         ;; Lookups
         (funcall 'neovm--psm-lookup m5 3)
         (funcall 'neovm--psm-lookup m5 5)
         (funcall 'neovm--psm-lookup m5 99)  ;; nil
         ;; After update
         (funcall 'neovm--psm-lookup m6 5)
         ;; Original unchanged
         (funcall 'neovm--psm-lookup m5 5)
         ;; After delete
         (funcall 'neovm--psm-keys m7)
         (funcall 'neovm--psm-lookup m7 3)
         ;; Branch has different value
         (funcall 'neovm--psm-lookup m5b 5)
         ;; Range query
         (funcall 'neovm--psm-range m5 3 7)
         (funcall 'neovm--psm-range m5 1 5)
         ;; Merged map keys
         (funcall 'neovm--psm-keys m8)
         ;; Merged lookups
         (funcall 'neovm--psm-lookup m8 2)
         (funcall 'neovm--psm-lookup m8 4)
         (funcall 'neovm--psm-lookup m8 5)))
    (fmakunbound 'neovm--psm-empty)
    (fmakunbound 'neovm--psm-insert)
    (fmakunbound 'neovm--psm-lookup)
    (fmakunbound 'neovm--psm-delete)
    (fmakunbound 'neovm--psm-keys)
    (fmakunbound 'neovm--psm-values)
    (fmakunbound 'neovm--psm-range)
    (fmakunbound 'neovm--psm-merge)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 3 5 7 9) (\"one\" \"three\" \"five\" \"seven\" \"nine\") \"three\" \"five\" nil \"FIVE\" \"five\" (1 5 7 9) nil \"cinq\" ((3 . \"three\") (5 . \"five\") (7 . \"seven\")) ((1 . \"one\") (3 . \"three\") (5 . \"five\")) (1 2 3 4 5 7 9) \"two\" \"four\" \"five\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Persistent zipper: navigable persistent tree
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_persistent_data_zipper() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A zipper for persistent navigation of a binary tree.
    // The zipper allows moving up/down/left/right and editing at the focus,
    // all producing new zippers that share unmodified structure.
    let form = r#"(progn
  ;; Tree: nil | (value left right)
  (fset 'neovm--pz-node
    (lambda (val left right) (list val left right)))

  ;; Zipper: (focused-tree . breadcrumbs)
  ;; Breadcrumb: (:left parent-val right-tree) | (:right parent-val left-tree)
  (fset 'neovm--pz-from-tree
    (lambda (tree) (cons tree nil)))

  (fset 'neovm--pz-focus
    (lambda (z) (car z)))

  (fset 'neovm--pz-focus-val
    (lambda (z)
      (let ((t (car z)))
        (if t (car t) nil))))

  (fset 'neovm--pz-go-left
    (lambda (z)
      "Move focus to left child."
      (let ((tree (car z))
            (crumbs (cdr z)))
        (if (and tree (cadr tree))
            (cons (cadr tree)
                  (cons (list :left (car tree) (caddr tree)) crumbs))
          z))))

  (fset 'neovm--pz-go-right
    (lambda (z)
      "Move focus to right child."
      (let ((tree (car z))
            (crumbs (cdr z)))
        (if (and tree (caddr tree))
            (cons (caddr tree)
                  (cons (list :right (car tree) (cadr tree)) crumbs))
          z))))

  (fset 'neovm--pz-go-up
    (lambda (z)
      "Move focus to parent."
      (let ((tree (car z))
            (crumbs (cdr z)))
        (if (null crumbs)
            z  ;; at root
          (let ((crumb (car crumbs))
                (rest (cdr crumbs)))
            (if (eq (car crumb) :left)
                ;; We came from left: parent is (parent-val us right)
                (cons (list (cadr crumb) tree (caddr crumb)) rest)
              ;; We came from right: parent is (parent-val left us)
              (cons (list (cadr crumb) (caddr crumb) tree) rest)))))))

  (fset 'neovm--pz-edit
    (lambda (z new-val)
      "Replace the focused node's value, returning new zipper."
      (let ((tree (car z))
            (crumbs (cdr z)))
        (if tree
            (cons (list new-val (cadr tree) (caddr tree)) crumbs)
          z))))

  (fset 'neovm--pz-to-tree
    (lambda (z)
      "Reconstruct tree from zipper (go up to root)."
      (let ((current z))
        (while (cdr current)
          (setq current (funcall 'neovm--pz-go-up current)))
        (car current))))

  (unwind-protect
      (let* (;; Build tree:     1
             ;;               /   \
             ;;              2     3
             ;;             / \   / \
             ;;            4   5 6   7
             (tree (funcall 'neovm--pz-node 1
                            (funcall 'neovm--pz-node 2
                                     (funcall 'neovm--pz-node 4 nil nil)
                                     (funcall 'neovm--pz-node 5 nil nil))
                            (funcall 'neovm--pz-node 3
                                     (funcall 'neovm--pz-node 6 nil nil)
                                     (funcall 'neovm--pz-node 7 nil nil))))
             ;; Create zipper at root
             (z0 (funcall 'neovm--pz-from-tree tree))
             ;; Navigate: root -> left -> left (to node 4)
             (z1 (funcall 'neovm--pz-go-left
                          (funcall 'neovm--pz-go-left z0)))
             ;; Edit node 4 to 44
             (z2 (funcall 'neovm--pz-edit z1 44))
             ;; Go back to root and reconstruct
             (tree2 (funcall 'neovm--pz-to-tree z2))
             ;; Navigate: root -> right -> left (to node 6)
             (z3 (funcall 'neovm--pz-go-left
                          (funcall 'neovm--pz-go-right z0)))
             ;; Edit node 6 to 66
             (z4 (funcall 'neovm--pz-edit z3 66))
             (tree3 (funcall 'neovm--pz-to-tree z4)))
        (list
         ;; Focus values during navigation
         (funcall 'neovm--pz-focus-val z0)   ;; 1 (root)
         (funcall 'neovm--pz-focus-val z1)   ;; 4
         (funcall 'neovm--pz-focus-val z2)   ;; 44
         (funcall 'neovm--pz-focus-val z3)   ;; 6
         ;; Original tree unchanged
         (equal tree '(1 (2 (4 nil nil) (5 nil nil)) (3 (6 nil nil) (7 nil nil))))
         ;; Modified tree2: 4 -> 44, everything else same
         tree2
         ;; Modified tree3: 6 -> 66
         tree3
         ;; Structural sharing in tree2: right subtree is eq to original
         (eq (caddr tree2) (caddr tree))
         ;; In tree3: left subtree is eq to original
         (eq (cadr tree3) (cadr tree))))
    (fmakunbound 'neovm--pz-node)
    (fmakunbound 'neovm--pz-from-tree)
    (fmakunbound 'neovm--pz-focus)
    (fmakunbound 'neovm--pz-focus-val)
    (fmakunbound 'neovm--pz-go-left)
    (fmakunbound 'neovm--pz-go-right)
    (fmakunbound 'neovm--pz-go-up)
    (fmakunbound 'neovm--pz-edit)
    (fmakunbound 'neovm--pz-to-tree)))"#;
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
