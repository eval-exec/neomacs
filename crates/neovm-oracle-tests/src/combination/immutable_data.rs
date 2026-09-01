//! Oracle parity tests for immutable/persistent data structure patterns in Elisp.
//!
//! Covers: persistent list (cons-based, structural sharing), persistent association
//! list (functional update), persistent vector simulation via tree-of-vectors,
//! undo/redo via persistent data, structural sharing verification with `eq`,
//! and a transaction log with snapshots.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Persistent list: cons-based with structural sharing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_immutable_persistent_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Demonstrate that cons-based lists naturally share structure.
    // Multiple "versions" of a list share common tails. Operations
    // (prepend, functional append, functional remove) produce new lists
    // without mutating the originals.
    let form = r#"(progn
  (fset 'neovm--iml-prepend
    (lambda (lst val) (cons val lst)))

  (fset 'neovm--iml-remove-first
    (lambda (lst val)
      "Return a new list with the first occurrence of VAL removed."
      (cond
       ((null lst) nil)
       ((equal (car lst) val) (cdr lst))
       (t (cons (car lst) (funcall 'neovm--iml-remove-first (cdr lst) val))))))

  (fset 'neovm--iml-functional-append
    (lambda (lst val)
      "Append VAL to end of LST, returning new list. Shares nothing with original."
      (if (null lst)
          (list val)
        (cons (car lst) (funcall 'neovm--iml-functional-append (cdr lst) val)))))

  (fset 'neovm--iml-take
    (lambda (lst n)
      "Return first N elements as a new list."
      (if (or (null lst) (<= n 0))
          nil
        (cons (car lst) (funcall 'neovm--iml-take (cdr lst) (1- n))))))

  (unwind-protect
      (let* ((base '(3 4 5))
             ;; v1 prepends two elements: shares tail with base
             (v1 (funcall 'neovm--iml-prepend
                          (funcall 'neovm--iml-prepend base 2) 1))
             ;; v2 prepends different elements: also shares tail with base
             (v2 (funcall 'neovm--iml-prepend
                          (funcall 'neovm--iml-prepend base 20) 10))
             ;; v3 removes element from v1
             (v3 (funcall 'neovm--iml-remove-first v1 3))
             ;; v4 appends to base (no sharing with original)
             (v4 (funcall 'neovm--iml-functional-append base 6))
             ;; v5 take first 2 from v1
             (v5 (funcall 'neovm--iml-take v1 2)))
        (list
         ;; All versions exist simultaneously
         base v1 v2 v3 v4 v5
         ;; Structural sharing: cddr of v1 IS base
         (eq (cddr v1) base)
         ;; cddr of v2 IS base
         (eq (cddr v2) base)
         ;; v3 removed 3: (1 2 4 5) -- cdr shares with (cdr base) = (4 5)
         (eq (cddr v3) (cdr base))
         ;; v4 shares nothing (fully rebuilt)
         (not (eq v4 base))
         ;; Original base completely unchanged
         (equal base '(3 4 5))
         ;; Lengths
         (length v1) (length v2) (length v3) (length v4) (length v5)))
    (fmakunbound 'neovm--iml-prepend)
    (fmakunbound 'neovm--iml-remove-first)
    (fmakunbound 'neovm--iml-functional-append)
    (fmakunbound 'neovm--iml-take)))"#;
    let expect = expect_test::expect![[
        r#""OK ((3 4 5) (1 2 3 4 5) (10 20 3 4 5) (1 2 4 5) (3 4 5 6) (1 2) t t t t t 5 5 4 4 2)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Persistent association list: functional update
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_immutable_persistent_alist() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a persistent alist where "update" conses a new pair on front.
    // "Delete" rebuilds without the key. Multiple versions coexist.
    // assoc finds the most recent binding (shadowing).
    let form = r#"(progn
  (fset 'neovm--ial-put
    (lambda (alist key val)
      "Return new alist with KEY bound to VAL. Shadows any existing binding."
      (cons (cons key val) alist)))

  (fset 'neovm--ial-get
    (lambda (alist key)
      "Get value for KEY, or nil."
      (let ((pair (assoc key alist)))
        (if pair (cdr pair) nil))))

  (fset 'neovm--ial-delete
    (lambda (alist key)
      "Return new alist with all bindings for KEY removed."
      (let ((result nil))
        (dolist (pair alist)
          (unless (equal (car pair) key)
            (setq result (cons pair result))))
        (nreverse result))))

  (fset 'neovm--ial-keys
    (lambda (alist)
      "Return unique keys in order of first appearance."
      (let ((seen (make-hash-table :test 'equal))
            (result nil))
        (dolist (pair alist)
          (unless (gethash (car pair) seen)
            (puthash (car pair) t seen)
            (setq result (cons (car pair) result))))
        (nreverse result))))

  (fset 'neovm--ial-to-unique-alist
    (lambda (alist)
      "Return alist with only the most recent binding per key."
      (let ((keys (funcall 'neovm--ial-keys alist))
            (result nil))
        (dolist (k keys)
          (setq result (cons (cons k (funcall 'neovm--ial-get alist k)) result)))
        (nreverse result))))

  (unwind-protect
      (let* ((v0 nil)
             ;; Build up bindings
             (v1 (funcall 'neovm--ial-put v0 "name" "Alice"))
             (v2 (funcall 'neovm--ial-put v1 "age" 30))
             (v3 (funcall 'neovm--ial-put v2 "city" "NYC"))
             ;; Update: shadow "age" with new value
             (v4 (funcall 'neovm--ial-put v3 "age" 31))
             ;; Delete: remove "city"
             (v5 (funcall 'neovm--ial-delete v4 "city")))
        (list
         ;; v1 has only name
         (funcall 'neovm--ial-get v1 "name")
         (funcall 'neovm--ial-get v1 "age")
         ;; v3 has all three
         (funcall 'neovm--ial-get v3 "name")
         (funcall 'neovm--ial-get v3 "age")
         (funcall 'neovm--ial-get v3 "city")
         ;; v4 shadows age: new value visible, old versions unchanged
         (funcall 'neovm--ial-get v4 "age")
         (funcall 'neovm--ial-get v3 "age")
         ;; v5 has city removed
         (funcall 'neovm--ial-get v5 "city")
         (funcall 'neovm--ial-get v5 "name")
         ;; Keys
         (funcall 'neovm--ial-keys v4)
         ;; Unique alist for v4
         (funcall 'neovm--ial-to-unique-alist v4)
         ;; Structural sharing: v1 is the tail of v2
         (eq (cdr v2) v1)
         ;; v3 shares tail chain back to v1
         (eq (cddr v3) v1)
         ;; All previous versions unchanged
         (equal (funcall 'neovm--ial-to-unique-alist v1)
                '(("name" . "Alice")))))
    (fmakunbound 'neovm--ial-put)
    (fmakunbound 'neovm--ial-get)
    (fmakunbound 'neovm--ial-delete)
    (fmakunbound 'neovm--ial-keys)
    (fmakunbound 'neovm--ial-to-unique-alist)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Alice\" nil \"Alice\" 30 \"NYC\" 31 30 nil \"Alice\" (\"age\" \"city\" \"name\") ((\"age\" . 31) (\"city\" . \"NYC\") (\"name\" . \"Alice\")) t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Persistent vector simulation using tree of vectors
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_immutable_persistent_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate a persistent vector using a balanced tree of small vectors.
    // Each node is either a leaf vector or an internal node (left . right).
    // Functional update: path-copy from root to leaf, sharing siblings.
    let form = r#"(progn
  ;; Leaf size: 4 elements per leaf
  (fset 'neovm--ipv-leaf-size (lambda () 4))

  (fset 'neovm--ipv-from-list
    (lambda (lst)
      "Build a persistent vector from a list."
      (let ((leaves nil) (chunk nil) (i 0))
        (dolist (x lst)
          (setq chunk (cons x chunk))
          (setq i (1+ i))
          (when (= i (funcall 'neovm--ipv-leaf-size))
            (setq leaves (cons (apply #'vector (nreverse chunk)) leaves))
            (setq chunk nil)
            (setq i 0)))
        ;; Handle remaining
        (when chunk
          (setq leaves (cons (apply #'vector (nreverse chunk)) leaves)))
        (setq leaves (nreverse leaves))
        ;; Build balanced tree from leaves
        (funcall 'neovm--ipv-build-tree leaves))))

  (fset 'neovm--ipv-build-tree
    (lambda (nodes)
      (cond
       ((null nodes) nil)
       ((= (length nodes) 1) (car nodes))
       (t (let ((mid (/ (length nodes) 2))
                (left nil) (right nil) (i 0))
            (dolist (n nodes)
              (if (< i mid)
                  (setq left (cons n left))
                (setq right (cons n right)))
              (setq i (1+ i)))
            (cons (funcall 'neovm--ipv-build-tree (nreverse left))
                  (funcall 'neovm--ipv-build-tree (nreverse right))))))))

  (fset 'neovm--ipv-size
    (lambda (pv)
      (cond
       ((null pv) 0)
       ((vectorp pv) (length pv))
       ((consp pv)
        (+ (funcall 'neovm--ipv-size (car pv))
           (funcall 'neovm--ipv-size (cdr pv))))
       (t 0))))

  (fset 'neovm--ipv-get
    (lambda (pv idx)
      (cond
       ((null pv) nil)
       ((vectorp pv)
        (if (< idx (length pv)) (aref pv idx) nil))
       ((consp pv)
        (let ((left-sz (funcall 'neovm--ipv-size (car pv))))
          (if (< idx left-sz)
              (funcall 'neovm--ipv-get (car pv) idx)
            (funcall 'neovm--ipv-get (cdr pv) (- idx left-sz))))))))

  (fset 'neovm--ipv-set
    (lambda (pv idx val)
      "Return a new persistent vector with element at IDX set to VAL."
      (cond
       ((null pv) nil)
       ((vectorp pv)
        (let ((new-v (copy-sequence pv)))
          (aset new-v idx val)
          new-v))
       ((consp pv)
        (let ((left-sz (funcall 'neovm--ipv-size (car pv))))
          (if (< idx left-sz)
              (cons (funcall 'neovm--ipv-set (car pv) idx val) (cdr pv))
            (cons (car pv) (funcall 'neovm--ipv-set (cdr pv) (- idx left-sz) val))))))))

  (fset 'neovm--ipv-to-list
    (lambda (pv)
      (cond
       ((null pv) nil)
       ((vectorp pv) (append pv nil))
       ((consp pv)
        (append (funcall 'neovm--ipv-to-list (car pv))
                (funcall 'neovm--ipv-to-list (cdr pv)))))))

  (unwind-protect
      (let* ((v0 (funcall 'neovm--ipv-from-list '(0 1 2 3 4 5 6 7 8 9)))
             ;; Functional update: set index 3 to 99
             (v1 (funcall 'neovm--ipv-set v0 3 99))
             ;; Another update on v0: set index 7 to 77
             (v2 (funcall 'neovm--ipv-set v0 7 77))
             ;; Chain: update v1 at index 0
             (v3 (funcall 'neovm--ipv-set v1 0 -1)))
        (list
         ;; Original v0 unchanged
         (funcall 'neovm--ipv-to-list v0)
         ;; v1: index 3 is 99
         (funcall 'neovm--ipv-to-list v1)
         ;; v2: index 7 is 77, index 3 is original 3
         (funcall 'neovm--ipv-to-list v2)
         ;; v3: index 0 is -1, index 3 is 99
         (funcall 'neovm--ipv-to-list v3)
         ;; Sizes all the same
         (funcall 'neovm--ipv-size v0)
         (funcall 'neovm--ipv-size v1)
         ;; Specific lookups
         (funcall 'neovm--ipv-get v0 3)
         (funcall 'neovm--ipv-get v1 3)
         (funcall 'neovm--ipv-get v2 7)
         (funcall 'neovm--ipv-get v3 0)
         ;; Structural sharing: right subtree of v1 is eq to right of v0
         ;; (because update at index 3 only touches left subtree)
         (eq (cdr v1) (cdr v0))))
    (fmakunbound 'neovm--ipv-leaf-size)
    (fmakunbound 'neovm--ipv-from-list)
    (fmakunbound 'neovm--ipv-build-tree)
    (fmakunbound 'neovm--ipv-size)
    (fmakunbound 'neovm--ipv-get)
    (fmakunbound 'neovm--ipv-set)
    (fmakunbound 'neovm--ipv-to-list)))"#;
    let expect = expect_test::expect![[
        r#""OK ((0 1 2 3 4 5 6 7 8 9) (0 1 2 99 4 5 6 7 8 9) (0 1 2 3 4 5 6 77 8 9) (-1 1 2 99 4 5 6 7 8 9) 10 10 3 99 77 -1 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Undo/redo via persistent data: just keep old versions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_immutable_undo_redo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Model a simple document editor with undo/redo using persistent data.
    // State is an alist. Each edit produces a new state. Undo pops from
    // history, pushes to redo stack. Redo pops from redo stack.
    let form = r#"(progn
  ;; Editor state: (current-state undo-stack redo-stack)
  (fset 'neovm--iur-make
    (lambda (initial)
      (list initial nil nil)))

  (fset 'neovm--iur-edit
    (lambda (editor new-state)
      "Apply an edit. Clears redo stack."
      (let ((current (car editor))
            (undo-stack (cadr editor)))
        (list new-state (cons current undo-stack) nil))))

  (fset 'neovm--iur-undo
    (lambda (editor)
      "Undo: move current to redo, pop undo to current."
      (let ((current (car editor))
            (undo-stack (cadr editor))
            (redo-stack (caddr editor)))
        (if (null undo-stack)
            editor
          (list (car undo-stack)
                (cdr undo-stack)
                (cons current redo-stack))))))

  (fset 'neovm--iur-redo
    (lambda (editor)
      "Redo: move current to undo, pop redo to current."
      (let ((current (car editor))
            (undo-stack (cadr editor))
            (redo-stack (caddr editor)))
        (if (null redo-stack)
            editor
          (list (car redo-stack)
                (cons current undo-stack)
                (cdr redo-stack))))))

  (fset 'neovm--iur-current
    (lambda (editor) (car editor)))

  (fset 'neovm--iur-can-undo
    (lambda (editor) (not (null (cadr editor)))))

  (fset 'neovm--iur-can-redo
    (lambda (editor) (not (null (caddr editor)))))

  (unwind-protect
      (let* (;; Start with initial document
             (e0 (funcall 'neovm--iur-make '((title . "Draft") (body . ""))))
             ;; Edit 1: set body
             (e1 (funcall 'neovm--iur-edit e0
                   '((title . "Draft") (body . "Hello world"))))
             ;; Edit 2: update title
             (e2 (funcall 'neovm--iur-edit e1
                   '((title . "Final") (body . "Hello world"))))
             ;; Edit 3: update body
             (e3 (funcall 'neovm--iur-edit e2
                   '((title . "Final") (body . "Hello brave world")))))
        (let* ((current-3 (funcall 'neovm--iur-current e3))
               ;; Undo once
               (e4 (funcall 'neovm--iur-undo e3))
               (current-4 (funcall 'neovm--iur-current e4))
               ;; Undo again
               (e5 (funcall 'neovm--iur-undo e4))
               (current-5 (funcall 'neovm--iur-current e5))
               ;; Redo once
               (e6 (funcall 'neovm--iur-redo e5))
               (current-6 (funcall 'neovm--iur-current e6))
               ;; Redo again
               (e7 (funcall 'neovm--iur-redo e6))
               (current-7 (funcall 'neovm--iur-current e7))
               ;; Undo, then new edit clears redo
               (e8 (funcall 'neovm--iur-undo e7))
               (e9 (funcall 'neovm--iur-edit e8
                     '((title . "Revised") (body . "New content")))))
          (list
           current-3
           current-4
           current-5
           current-6
           current-7
           (funcall 'neovm--iur-current e9)
           ;; Can-undo/redo checks
           (funcall 'neovm--iur-can-undo e3)
           (funcall 'neovm--iur-can-redo e3)
           (funcall 'neovm--iur-can-undo e4)
           (funcall 'neovm--iur-can-redo e4)
           ;; After new edit on undone state, redo is gone
           (funcall 'neovm--iur-can-redo e9)
           (funcall 'neovm--iur-can-undo e9)
           ;; Old editor objects still valid (persistence)
           (equal (funcall 'neovm--iur-current e1)
                  '((title . "Draft") (body . "Hello world"))))))
    (fmakunbound 'neovm--iur-make)
    (fmakunbound 'neovm--iur-edit)
    (fmakunbound 'neovm--iur-undo)
    (fmakunbound 'neovm--iur-redo)
    (fmakunbound 'neovm--iur-current)
    (fmakunbound 'neovm--iur-can-undo)
    (fmakunbound 'neovm--iur-can-redo)))"#;
    let expect = expect_test::expect![[
        r#""OK (((title . \"Final\") (body . \"Hello brave world\")) ((title . \"Final\") (body . \"Hello world\")) ((title . \"Draft\") (body . \"Hello world\")) ((title . \"Final\") (body . \"Hello world\")) ((title . \"Final\") (body . \"Hello brave world\")) ((title . \"Revised\") (body . \"New content\")) t nil t t nil t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Structural sharing verification via eq on shared tails
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_immutable_structural_sharing_eq() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Construct lists that share structure and verify sharing with eq.
    // Test that functional operations preserve sharing where possible
    // and break it only where necessary.
    let form = r#"(let* (;; Shared tail
            (tail '(d e f))
            ;; Two lists sharing the same tail
            (list-a (cons 'a (cons 'b (cons 'c tail))))
            (list-b (cons 'x (cons 'y tail)))
            ;; Functional cons on list-a: new head, shares everything from list-a
            (list-c (cons 'z list-a))
            ;; Functional update: replace head of list-a
            ;; This creates a new cons but shares the cdr
            (list-d (cons 'A (cdr list-a)))
            ;; Deep sharing: nested structures
            (inner '(1 2 3))
            (outer-a (list 'frame-a inner))
            (outer-b (list 'frame-b inner))
            ;; Functional "set-car" on outer-a
            (outer-c (cons 'frame-c (cdr outer-a))))
       (list
        ;; Tail sharing
        (eq (cdddr list-a) tail)
        (eq (cddr list-b) tail)
        ;; list-c shares all of list-a
        (eq (cdr list-c) list-a)
        ;; list-d shares cdr of list-a
        (eq (cdr list-d) (cdr list-a))
        ;; Transitive: list-d also shares tail
        (eq (cdddr list-d) tail)
        ;; Deep sharing: inner is shared by outer-a and outer-b
        (eq (cadr outer-a) inner)
        (eq (cadr outer-b) inner)
        (eq (cadr outer-a) (cadr outer-b))
        ;; outer-c shares cdr with outer-a
        (eq (cdr outer-c) (cdr outer-a))
        ;; Therefore outer-c shares inner too
        (eq (cadr outer-c) inner)
        ;; Content equality
        (equal list-a '(a b c d e f))
        (equal list-b '(x y d e f))
        (equal list-c '(z a b c d e f))
        (equal list-d '(A b c d e f))
        ;; Lengths
        (length list-a) (length list-b) (length list-c) (length list-d)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t t t t t 6 5 7 6)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Transaction log with snapshots
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_immutable_transaction_log() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A transaction log that records operations and maintains snapshots.
    // Each transaction produces a new state. Named snapshots can be taken
    // and restored. The log itself is immutable (append-only via cons).
    let form = r#"(progn
  ;; State: alist of key-value pairs
  ;; Transaction: (op key value) where op is 'set or 'delete
  ;; System: (state log snapshots)
  ;;   snapshots is alist of (name . state)

  (fset 'neovm--itl-make
    (lambda () (list nil nil nil)))

  (fset 'neovm--itl-state (lambda (sys) (car sys)))
  (fset 'neovm--itl-log (lambda (sys) (cadr sys)))
  (fset 'neovm--itl-snapshots (lambda (sys) (caddr sys)))

  (fset 'neovm--itl-apply-tx
    (lambda (state tx)
      "Apply transaction TX to STATE, return new state."
      (let ((op (car tx)) (key (cadr tx)) (val (caddr tx)))
        (cond
         ((eq op 'set)
          ;; Functional alist update: cons new pair, shadowing old
          (cons (cons key val) state))
         ((eq op 'delete)
          ;; Remove all bindings for key
          (let ((result nil))
            (dolist (pair state)
              (unless (equal (car pair) key)
                (setq result (cons pair result))))
            (nreverse result)))
         (t state)))))

  (fset 'neovm--itl-transact
    (lambda (sys tx)
      "Execute transaction TX, returning new system."
      (let ((new-state (funcall 'neovm--itl-apply-tx
                                (funcall 'neovm--itl-state sys) tx))
            (new-log (cons tx (funcall 'neovm--itl-log sys))))
        (list new-state new-log (funcall 'neovm--itl-snapshots sys)))))

  (fset 'neovm--itl-snapshot
    (lambda (sys name)
      "Take a named snapshot of current state."
      (list (funcall 'neovm--itl-state sys)
            (funcall 'neovm--itl-log sys)
            (cons (cons name (funcall 'neovm--itl-state sys))
                  (funcall 'neovm--itl-snapshots sys)))))

  (fset 'neovm--itl-restore
    (lambda (sys name)
      "Restore to named snapshot. Log a restore event."
      (let ((snap (assoc name (funcall 'neovm--itl-snapshots sys))))
        (if snap
            (list (cdr snap)
                  (cons (list 'restore name) (funcall 'neovm--itl-log sys))
                  (funcall 'neovm--itl-snapshots sys))
          sys))))

  (fset 'neovm--itl-get
    (lambda (sys key)
      (let ((pair (assoc key (funcall 'neovm--itl-state sys))))
        (if pair (cdr pair) nil))))

  (fset 'neovm--itl-log-length
    (lambda (sys) (length (funcall 'neovm--itl-log sys))))

  (unwind-protect
      (let ((s0 (funcall 'neovm--itl-make)))
        ;; Transaction 1: set user=Alice
        (let ((s1 (funcall 'neovm--itl-transact s0 '(set "user" "Alice"))))
          ;; Transaction 2: set role=admin
          (let ((s2 (funcall 'neovm--itl-transact s1 '(set "role" "admin"))))
            ;; Take snapshot "before-change"
            (let ((s3 (funcall 'neovm--itl-snapshot s2 "before-change")))
              ;; Transaction 3: change user=Bob
              (let ((s4 (funcall 'neovm--itl-transact s3 '(set "user" "Bob"))))
                ;; Transaction 4: delete role
                (let ((s5 (funcall 'neovm--itl-transact s4 '(delete "role" nil))))
                  ;; Transaction 5: set email
                  (let ((s6 (funcall 'neovm--itl-transact s5 '(set "email" "bob@example.com"))))
                    ;; Check current state
                    (let ((cur-user (funcall 'neovm--itl-get s6 "user"))
                          (cur-role (funcall 'neovm--itl-get s6 "role"))
                          (cur-email (funcall 'neovm--itl-get s6 "email"))
                          (log-len (funcall 'neovm--itl-log-length s6)))
                      ;; Restore to "before-change"
                      (let ((s7 (funcall 'neovm--itl-restore s6 "before-change")))
                        (let ((rest-user (funcall 'neovm--itl-get s7 "user"))
                              (rest-role (funcall 'neovm--itl-get s7 "role"))
                              (rest-email (funcall 'neovm--itl-get s7 "email"))
                              (rest-log-len (funcall 'neovm--itl-log-length s7)))
                          ;; s2 still unchanged (persistence)
                          (let ((s2-user (funcall 'neovm--itl-get s2 "user"))
                                (s2-role (funcall 'neovm--itl-get s2 "role")))
                            (list
                             cur-user cur-role cur-email log-len
                             rest-user rest-role rest-email rest-log-len
                             s2-user s2-role
                             ;; Old system objects still valid
                             (funcall 'neovm--itl-get s0 "user")
                             (funcall 'neovm--itl-get s1 "user")
                             ;; Snapshot still available after restore
                             (length (funcall 'neovm--itl-snapshots s7)))))))))))))
    (fmakunbound 'neovm--itl-make)
    (fmakunbound 'neovm--itl-state)
    (fmakunbound 'neovm--itl-log)
    (fmakunbound 'neovm--itl-snapshots)
    (fmakunbound 'neovm--itl-apply-tx)
    (fmakunbound 'neovm--itl-transact)
    (fmakunbound 'neovm--itl-snapshot)
    (fmakunbound 'neovm--itl-restore)
    (fmakunbound 'neovm--itl-get)
    (fmakunbound 'neovm--itl-log-length)))"#;
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
