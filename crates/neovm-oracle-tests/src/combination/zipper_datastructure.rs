//! Oracle parity tests for a zipper data structure implemented in Elisp.
//!
//! A zipper is a functional cursor into a data structure, allowing efficient
//! navigation and local modification. Tests include: list zipper creation,
//! movement, insertion, deletion, modification, tree zipper navigation,
//! and buffer-like editing simulation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// List zipper: creation, move left/right, convert back to list
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_zipper_list_navigation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; List zipper: (left-reversed focus right)
  ;; left-reversed is the elements to the left, in reverse order
  (fset 'neovm--zip-from-list
    (lambda (lst)
      "Create zipper focused on first element of LST."
      (if (null lst)
          (list nil nil nil)
        (list nil (car lst) (cdr lst)))))

  (fset 'neovm--zip-left  (lambda (z) (nth 0 z)))
  (fset 'neovm--zip-focus (lambda (z) (nth 1 z)))
  (fset 'neovm--zip-right (lambda (z) (nth 2 z)))

  (fset 'neovm--zip-move-right
    (lambda (z)
      "Move focus one step right."
      (let ((left (funcall 'neovm--zip-left z))
            (focus (funcall 'neovm--zip-focus z))
            (right (funcall 'neovm--zip-right z)))
        (if (null right)
            nil  ;; can't move right
          (list (cons focus left) (car right) (cdr right))))))

  (fset 'neovm--zip-move-left
    (lambda (z)
      "Move focus one step left."
      (let ((left (funcall 'neovm--zip-left z))
            (focus (funcall 'neovm--zip-focus z))
            (right (funcall 'neovm--zip-right z)))
        (if (null left)
            nil  ;; can't move left
          (list (cdr left) (car left) (cons focus right))))))

  (fset 'neovm--zip-to-list
    (lambda (z)
      "Convert zipper back to a flat list."
      (let ((left (funcall 'neovm--zip-left z))
            (focus (funcall 'neovm--zip-focus z))
            (right (funcall 'neovm--zip-right z)))
        (append (reverse left) (list focus) right))))

  (fset 'neovm--zip-at-start-p
    (lambda (z) (null (funcall 'neovm--zip-left z))))

  (fset 'neovm--zip-at-end-p
    (lambda (z) (null (funcall 'neovm--zip-right z))))

  (unwind-protect
      (let ((z (funcall 'neovm--zip-from-list '(a b c d e))))
        (list
          ;; Initial state: focus on 'a'
          (funcall 'neovm--zip-focus z)
          (funcall 'neovm--zip-at-start-p z)
          (funcall 'neovm--zip-at-end-p z)
          ;; Move right to 'b'
          (let ((z1 (funcall 'neovm--zip-move-right z)))
            (list (funcall 'neovm--zip-focus z1)
                  (funcall 'neovm--zip-at-start-p z1)))
          ;; Move right twice to 'c'
          (let ((z2 (funcall 'neovm--zip-move-right
                              (funcall 'neovm--zip-move-right z))))
            (funcall 'neovm--zip-focus z2))
          ;; Move to end
          (let* ((z1 (funcall 'neovm--zip-move-right z))
                 (z2 (funcall 'neovm--zip-move-right z1))
                 (z3 (funcall 'neovm--zip-move-right z2))
                 (z4 (funcall 'neovm--zip-move-right z3)))
            (list (funcall 'neovm--zip-focus z4)
                  (funcall 'neovm--zip-at-end-p z4)
                  ;; Can't move further right
                  (funcall 'neovm--zip-move-right z4)))
          ;; Move right then left returns to same focus
          (let* ((z1 (funcall 'neovm--zip-move-right z))
                 (z2 (funcall 'neovm--zip-move-left z1)))
            (funcall 'neovm--zip-focus z2))
          ;; Can't move left from start
          (funcall 'neovm--zip-move-left z)
          ;; Convert back to list preserves order
          (funcall 'neovm--zip-to-list z)
          ;; Convert from middle preserves order
          (let ((z-mid (funcall 'neovm--zip-move-right
                                 (funcall 'neovm--zip-move-right z))))
            (funcall 'neovm--zip-to-list z-mid))))
    (fmakunbound 'neovm--zip-from-list)
    (fmakunbound 'neovm--zip-left)
    (fmakunbound 'neovm--zip-focus)
    (fmakunbound 'neovm--zip-right)
    (fmakunbound 'neovm--zip-move-right)
    (fmakunbound 'neovm--zip-move-left)
    (fmakunbound 'neovm--zip-to-list)
    (fmakunbound 'neovm--zip-at-start-p)
    (fmakunbound 'neovm--zip-at-end-p)))"#;
    let expect = expect_test::expect![[
        r#""OK (a t nil (b nil) c (e t nil) a nil (a b c d e) (a b c d e))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// List zipper: insert and delete at focus
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_zipper_insert_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--zid-from-list
    (lambda (lst)
      (if (null lst) (list nil nil nil)
        (list nil (car lst) (cdr lst)))))

  (fset 'neovm--zid-move-right
    (lambda (z)
      (if (null (nth 2 z)) nil
        (list (cons (nth 1 z) (nth 0 z)) (car (nth 2 z)) (cdr (nth 2 z))))))

  (fset 'neovm--zid-move-left
    (lambda (z)
      (if (null (nth 0 z)) nil
        (list (cdr (nth 0 z)) (car (nth 0 z)) (cons (nth 1 z) (nth 2 z))))))

  (fset 'neovm--zid-to-list
    (lambda (z)
      (append (reverse (nth 0 z)) (list (nth 1 z)) (nth 2 z))))

  (fset 'neovm--zid-insert-left
    (lambda (z val)
      "Insert VAL to the left of focus."
      (list (cons val (nth 0 z)) (nth 1 z) (nth 2 z))))

  (fset 'neovm--zid-insert-right
    (lambda (z val)
      "Insert VAL to the right of focus."
      (list (nth 0 z) (nth 1 z) (cons val (nth 2 z)))))

  (fset 'neovm--zid-replace
    (lambda (z val)
      "Replace focus element with VAL."
      (list (nth 0 z) val (nth 2 z))))

  (fset 'neovm--zid-delete
    (lambda (z)
      "Delete focus element, moving focus right if possible, else left."
      (cond
        ((nth 2 z)  ;; has right neighbor
         (list (nth 0 z) (car (nth 2 z)) (cdr (nth 2 z))))
        ((nth 0 z)  ;; has left neighbor
         (list (cdr (nth 0 z)) (car (nth 0 z)) nil))
        (t  ;; single element: empty zipper
         (list nil nil nil)))))

  (unwind-protect
      (let ((z (funcall 'neovm--zid-from-list '(1 2 3 4 5))))
        (list
          ;; Insert left: (X 1 2 3 4 5)
          (funcall 'neovm--zid-to-list
                   (funcall 'neovm--zid-insert-left z 0))
          ;; Insert right: (1 X 2 3 4 5)
          (funcall 'neovm--zid-to-list
                   (funcall 'neovm--zid-insert-right z 99))
          ;; Replace focus: (10 2 3 4 5)
          (funcall 'neovm--zid-to-list
                   (funcall 'neovm--zid-replace z 10))
          ;; Delete focus from start: (2 3 4 5)
          (funcall 'neovm--zid-to-list
                   (funcall 'neovm--zid-delete z))
          ;; Delete from middle: move right twice, delete '3'
          (let* ((z1 (funcall 'neovm--zid-move-right z))
                 (z2 (funcall 'neovm--zid-move-right z1))
                 (z3 (funcall 'neovm--zid-delete z2)))
            (list (funcall 'neovm--zid-to-list z3)
                  (nth 1 z3)))  ;; focus should be 4
          ;; Insert multiple: insert at different positions
          (let* ((z1 (funcall 'neovm--zid-move-right z))
                 (z2 (funcall 'neovm--zid-insert-left z1 'a))
                 (z3 (funcall 'neovm--zid-insert-right z2 'b)))
            (funcall 'neovm--zid-to-list z3))
          ;; Delete from end
          (let* ((z1 (funcall 'neovm--zid-move-right z))
                 (z2 (funcall 'neovm--zid-move-right z1))
                 (z3 (funcall 'neovm--zid-move-right z2))
                 (z4 (funcall 'neovm--zid-move-right z3))
                 ;; z4 is at '5', the last element
                 (z5 (funcall 'neovm--zid-delete z4)))
            (list (funcall 'neovm--zid-to-list z5)
                  (nth 1 z5)))  ;; focus should be 4
          ;; Delete single-element zipper
          (let* ((z1 (funcall 'neovm--zid-from-list '(only)))
                 (z2 (funcall 'neovm--zid-delete z1)))
            z2)))
    (fmakunbound 'neovm--zid-from-list)
    (fmakunbound 'neovm--zid-move-right)
    (fmakunbound 'neovm--zid-move-left)
    (fmakunbound 'neovm--zid-to-list)
    (fmakunbound 'neovm--zid-insert-left)
    (fmakunbound 'neovm--zid-insert-right)
    (fmakunbound 'neovm--zid-replace)
    (fmakunbound 'neovm--zid-delete)))"#;
    let expect = expect_test::expect![[
        r#""OK ((0 1 2 3 4 5) (1 99 2 3 4 5) (10 2 3 4 5) (2 3 4 5) ((1 2 4 5) 4) (1 a 2 b 3 4 5) ((1 2 3 4) 4) (nil nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// List zipper: modify focus with function application
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_zipper_modify_focus() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--zmf-from-list
    (lambda (lst)
      (if (null lst) (list nil nil nil)
        (list nil (car lst) (cdr lst)))))

  (fset 'neovm--zmf-move-right
    (lambda (z)
      (if (null (nth 2 z)) nil
        (list (cons (nth 1 z) (nth 0 z)) (car (nth 2 z)) (cdr (nth 2 z))))))

  (fset 'neovm--zmf-to-list
    (lambda (z) (append (reverse (nth 0 z)) (list (nth 1 z)) (nth 2 z))))

  (fset 'neovm--zmf-update
    (lambda (z fn)
      "Apply FN to focus element."
      (list (nth 0 z) (funcall fn (nth 1 z)) (nth 2 z))))

  (fset 'neovm--zmf-map-all
    (lambda (z fn)
      "Apply FN to every element, returning list."
      (let ((left (mapcar fn (reverse (nth 0 z))))
            (focus (funcall fn (nth 1 z)))
            (right (mapcar fn (nth 2 z))))
        (append left (list focus) right))))

  (fset 'neovm--zmf-find-right
    (lambda (z pred)
      "Move right until PRED is true for focus, or nil if not found."
      (let ((current z) (found nil))
        (while (and current (not found))
          (if (funcall pred (nth 1 current))
              (setq found current)
            (setq current (funcall 'neovm--zmf-move-right current))))
        found)))

  (unwind-protect
      (let ((z (funcall 'neovm--zmf-from-list '(1 2 3 4 5))))
        (list
          ;; Update focus: double it
          (funcall 'neovm--zmf-to-list
                   (funcall 'neovm--zmf-update z (lambda (x) (* x 2))))
          ;; Update in the middle
          (let* ((z1 (funcall 'neovm--zmf-move-right z))
                 (z2 (funcall 'neovm--zmf-move-right z1))
                 (z3 (funcall 'neovm--zmf-update z2 (lambda (x) (* x 10)))))
            (funcall 'neovm--zmf-to-list z3))
          ;; Map all elements
          (funcall 'neovm--zmf-map-all z (lambda (x) (+ x 100)))
          ;; Map from middle position
          (let ((z-mid (funcall 'neovm--zmf-move-right
                                 (funcall 'neovm--zmf-move-right z))))
            (funcall 'neovm--zmf-map-all z-mid (lambda (x) (* x x))))
          ;; Find: locate element > 3
          (let ((found (funcall 'neovm--zmf-find-right z
                                 (lambda (x) (> x 3)))))
            (when found (nth 1 found)))
          ;; Find: locate even element
          (let ((found (funcall 'neovm--zmf-find-right z
                                 (lambda (x) (= (% x 2) 0)))))
            (when found (nth 1 found)))
          ;; Find: not found
          (let ((found (funcall 'neovm--zmf-find-right z
                                 (lambda (x) (> x 100)))))
            found)
          ;; Chain updates: double first, then move right, triple second
          (let* ((z1 (funcall 'neovm--zmf-update z (lambda (x) (* x 2))))
                 (z2 (funcall 'neovm--zmf-move-right z1))
                 (z3 (funcall 'neovm--zmf-update z2 (lambda (x) (* x 3)))))
            (funcall 'neovm--zmf-to-list z3))))
    (fmakunbound 'neovm--zmf-from-list)
    (fmakunbound 'neovm--zmf-move-right)
    (fmakunbound 'neovm--zmf-to-list)
    (fmakunbound 'neovm--zmf-update)
    (fmakunbound 'neovm--zmf-map-all)
    (fmakunbound 'neovm--zmf-find-right)))"#;
    let expect = expect_test::expect![[
        r#""OK ((2 2 3 4 5) (1 2 30 4 5) (101 102 103 104 105) (1 4 9 16 25) 4 2 nil (2 6 3 4 5))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Tree zipper: navigate a tree structure
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_zipper_tree_navigation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Tree zipper for trees of form (value children...)
    // Context is a list of (parent-value left-siblings right-siblings parent-context)
    let form = r#"(progn
  ;; Tree node: (value child1 child2 ...)
  (fset 'neovm--tz-node-value (lambda (node) (car node)))
  (fset 'neovm--tz-node-children (lambda (node) (cdr node)))

  ;; Tree zipper: (focused-node . context)
  ;; Context: nil (root) or (parent-value left-siblings right-siblings . parent-context)
  (fset 'neovm--tz-create
    (lambda (tree) (cons tree nil)))

  (fset 'neovm--tz-focus
    (lambda (tz) (car tz)))

  (fset 'neovm--tz-context
    (lambda (tz) (cdr tz)))

  (fset 'neovm--tz-focus-value
    (lambda (tz) (funcall 'neovm--tz-node-value (car tz))))

  ;; Move to first child
  (fset 'neovm--tz-down
    (lambda (tz)
      (let* ((node (car tz))
             (ctx (cdr tz))
             (children (funcall 'neovm--tz-node-children node)))
        (if (null children) nil
          (let ((new-ctx (list (funcall 'neovm--tz-node-value node)
                               nil                      ;; left siblings
                               (cdr children)            ;; right siblings
                               ctx)))                    ;; parent context
            (cons (car children) new-ctx))))))

  ;; Move to right sibling
  (fset 'neovm--tz-right
    (lambda (tz)
      (let ((ctx (cdr tz)))
        (if (or (null ctx) (null (nth 2 ctx))) nil
          (let ((pval (nth 0 ctx))
                (left (nth 1 ctx))
                (right (nth 2 ctx))
                (pctx (nthcdr 3 ctx)))
            (cons (car right)
                  (list pval
                        (cons (car tz) left)
                        (cdr right)
                        (car pctx))))))))

  ;; Move to left sibling
  (fset 'neovm--tz-left
    (lambda (tz)
      (let ((ctx (cdr tz)))
        (if (or (null ctx) (null (nth 1 ctx))) nil
          (let ((pval (nth 0 ctx))
                (left (nth 1 ctx))
                (right (nth 2 ctx))
                (pctx (nthcdr 3 ctx)))
            (cons (car left)
                  (list pval
                        (cdr left)
                        (cons (car tz) right)
                        (car pctx))))))))

  ;; Move up to parent (reconstructing the node)
  (fset 'neovm--tz-up
    (lambda (tz)
      (let ((ctx (cdr tz)))
        (if (null ctx) nil
          (let ((pval (nth 0 ctx))
                (left (nth 1 ctx))
                (right (nth 2 ctx))
                (pctx (nthcdr 3 ctx)))
            ;; Reconstruct parent: value + (reverse left) + focus + right
            (let ((children (append (reverse left) (list (car tz)) right)))
              (cons (cons pval children) (car pctx))))))))

  ;; Reconstruct full tree from any position
  (fset 'neovm--tz-to-tree
    (lambda (tz)
      (let ((current tz))
        (while (cdr current)
          (setq current (funcall 'neovm--tz-up current)))
        (car current))))

  (unwind-protect
      (let* ((tree '(a (b (d) (e)) (c (f) (g))))
             (tz (funcall 'neovm--tz-create tree)))
        (list
          ;; Root value
          (funcall 'neovm--tz-focus-value tz)
          ;; Down to first child 'b'
          (let ((tz1 (funcall 'neovm--tz-down tz)))
            (funcall 'neovm--tz-focus-value tz1))
          ;; Down then right to 'c'
          (let* ((tz1 (funcall 'neovm--tz-down tz))
                 (tz2 (funcall 'neovm--tz-right tz1)))
            (funcall 'neovm--tz-focus-value tz2))
          ;; Down to 'b', down to 'd'
          (let* ((tz1 (funcall 'neovm--tz-down tz))
                 (tz2 (funcall 'neovm--tz-down tz1)))
            (funcall 'neovm--tz-focus-value tz2))
          ;; Down to 'b', down to 'd', right to 'e'
          (let* ((tz1 (funcall 'neovm--tz-down tz))
                 (tz2 (funcall 'neovm--tz-down tz1))
                 (tz3 (funcall 'neovm--tz-right tz2)))
            (funcall 'neovm--tz-focus-value tz3))
          ;; Navigate down and back up: should reconstruct tree
          (let* ((tz1 (funcall 'neovm--tz-down tz))
                 (tz2 (funcall 'neovm--tz-up tz1)))
            (equal (car tz2) tree))
          ;; Deep navigation and tree reconstruction
          (let* ((tz1 (funcall 'neovm--tz-down tz))
                 (tz2 (funcall 'neovm--tz-right tz1))
                 (tz3 (funcall 'neovm--tz-down tz2)))
            (list (funcall 'neovm--tz-focus-value tz3)
                  (equal (funcall 'neovm--tz-to-tree tz3) tree)))
          ;; Can't go up from root
          (funcall 'neovm--tz-up tz)
          ;; Can't go down from leaf
          (let* ((tz1 (funcall 'neovm--tz-down tz))
                 (tz2 (funcall 'neovm--tz-down tz1)))
            (funcall 'neovm--tz-down tz2))
          ;; Left from first sibling is nil
          (let ((tz1 (funcall 'neovm--tz-down tz)))
            (funcall 'neovm--tz-left tz1))))
    (fmakunbound 'neovm--tz-node-value)
    (fmakunbound 'neovm--tz-node-children)
    (fmakunbound 'neovm--tz-create)
    (fmakunbound 'neovm--tz-focus)
    (fmakunbound 'neovm--tz-context)
    (fmakunbound 'neovm--tz-focus-value)
    (fmakunbound 'neovm--tz-down)
    (fmakunbound 'neovm--tz-right)
    (fmakunbound 'neovm--tz-left)
    (fmakunbound 'neovm--tz-up)
    (fmakunbound 'neovm--tz-to-tree)))"#;
    let expect = expect_test::expect![[r#""OK (a b c d e t (f t) nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Buffer-like editing with a character-level zipper
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_zipper_buffer_editing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate a text buffer as a zipper of characters.
    // Left is reversed chars before cursor, right is chars after cursor.
    let form = r#"(progn
  (fset 'neovm--zb-from-string
    (lambda (str)
      "Create buffer zipper at start of STR."
      (cons nil (append str nil))))

  (fset 'neovm--zb-to-string
    (lambda (buf)
      (concat (reverse (car buf)) (cdr buf))))

  (fset 'neovm--zb-cursor-pos
    (lambda (buf) (length (car buf))))

  (fset 'neovm--zb-insert-char
    (lambda (buf ch)
      "Insert CH at cursor position."
      (cons (cons ch (car buf)) (cdr buf))))

  (fset 'neovm--zb-insert-string
    (lambda (buf str)
      "Insert STR at cursor position."
      (let ((chars (append str nil))
            (left (car buf)))
        (dolist (ch chars)
          (setq left (cons ch left)))
        (cons left (cdr buf)))))

  (fset 'neovm--zb-delete-backward
    (lambda (buf)
      "Delete char before cursor (backspace)."
      (if (null (car buf)) buf
        (cons (cdr (car buf)) (cdr buf)))))

  (fset 'neovm--zb-delete-forward
    (lambda (buf)
      "Delete char after cursor (delete key)."
      (if (null (cdr buf)) buf
        (cons (car buf) (cdr (cdr buf))))))

  (fset 'neovm--zb-move-left
    (lambda (buf)
      (if (null (car buf)) buf
        (cons (cdr (car buf)) (cons (car (car buf)) (cdr buf))))))

  (fset 'neovm--zb-move-right
    (lambda (buf)
      (if (null (cdr buf)) buf
        (cons (cons (car (cdr buf)) (car buf)) (cdr (cdr buf))))))

  (fset 'neovm--zb-move-start
    (lambda (buf)
      (cons nil (append (reverse (car buf)) (cdr buf)))))

  (fset 'neovm--zb-move-end
    (lambda (buf)
      (cons (append (reverse (cdr buf)) (car buf)) nil)))

  (unwind-protect
      (let ((buf (funcall 'neovm--zb-from-string "hello")))
        (list
          ;; Initial state
          (funcall 'neovm--zb-to-string buf)
          (funcall 'neovm--zb-cursor-pos buf)
          ;; Insert at beginning
          (let ((buf2 (funcall 'neovm--zb-insert-string buf ">> ")))
            (list (funcall 'neovm--zb-to-string buf2)
                  (funcall 'neovm--zb-cursor-pos buf2)))
          ;; Move to end and insert
          (let* ((buf2 (funcall 'neovm--zb-move-end buf))
                 (buf3 (funcall 'neovm--zb-insert-string buf2 " world")))
            (funcall 'neovm--zb-to-string buf3))
          ;; Move right 2, insert in middle
          (let* ((buf2 (funcall 'neovm--zb-move-right
                                 (funcall 'neovm--zb-move-right buf)))
                 (buf3 (funcall 'neovm--zb-insert-char buf2 ?X)))
            (list (funcall 'neovm--zb-to-string buf3)
                  (funcall 'neovm--zb-cursor-pos buf3)))
          ;; Delete backward from end
          (let* ((buf2 (funcall 'neovm--zb-move-end buf))
                 (buf3 (funcall 'neovm--zb-delete-backward buf2))
                 (buf4 (funcall 'neovm--zb-delete-backward buf3)))
            (funcall 'neovm--zb-to-string buf4))
          ;; Delete forward from start
          (let* ((buf2 (funcall 'neovm--zb-delete-forward buf))
                 (buf3 (funcall 'neovm--zb-delete-forward buf2)))
            (funcall 'neovm--zb-to-string buf3))
          ;; Complex editing sequence: type "abc" at position 2
          (let* ((b1 (funcall 'neovm--zb-move-right
                               (funcall 'neovm--zb-move-right buf)))
                 (b2 (funcall 'neovm--zb-insert-string b1 "abc"))
                 ;; Delete 1 char forward
                 (b3 (funcall 'neovm--zb-delete-forward b2))
                 ;; Move to start
                 (b4 (funcall 'neovm--zb-move-start b3)))
            (list (funcall 'neovm--zb-to-string b4)
                  (funcall 'neovm--zb-cursor-pos b4)))
          ;; Empty buffer operations
          (let* ((empty (funcall 'neovm--zb-from-string ""))
                 (e1 (funcall 'neovm--zb-insert-string empty "new"))
                 (e2 (funcall 'neovm--zb-delete-backward empty)))
            (list (funcall 'neovm--zb-to-string e1)
                  (funcall 'neovm--zb-to-string e2)))))
    (fmakunbound 'neovm--zb-from-string)
    (fmakunbound 'neovm--zb-to-string)
    (fmakunbound 'neovm--zb-cursor-pos)
    (fmakunbound 'neovm--zb-insert-char)
    (fmakunbound 'neovm--zb-insert-string)
    (fmakunbound 'neovm--zb-delete-backward)
    (fmakunbound 'neovm--zb-delete-forward)
    (fmakunbound 'neovm--zb-move-left)
    (fmakunbound 'neovm--zb-move-right)
    (fmakunbound 'neovm--zb-move-start)
    (fmakunbound 'neovm--zb-move-end)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"hello\" 0 (\">> hello\" 3) \"hello world\" (\"heXllo\" 3) \"hel\" \"llo\" (\"heabclo\" 0) (\"new\" \"\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Zipper-based undo: track edits and undo them
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_zipper_undo_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Combine zipper with an undo stack: each operation pushes old state
    let form = r#"(progn
  ;; Undo-capable list zipper: (zipper-state . undo-stack)
  (fset 'neovm--zu-create
    (lambda (lst)
      (let ((z (if (null lst) (list nil nil nil)
                 (list nil (car lst) (cdr lst)))))
        (cons z nil))))

  (fset 'neovm--zu-state (lambda (uz) (car uz)))
  (fset 'neovm--zu-stack (lambda (uz) (cdr uz)))

  (fset 'neovm--zu-to-list
    (lambda (z) (append (reverse (nth 0 z)) (list (nth 1 z)) (nth 2 z))))

  (fset 'neovm--zu-get-list
    (lambda (uz) (funcall 'neovm--zu-to-list (car uz))))

  (fset 'neovm--zu-focus
    (lambda (uz) (nth 1 (car uz))))

  (fset 'neovm--zu-do
    (lambda (uz new-state)
      "Apply new state, pushing old state to undo stack."
      (cons new-state (cons (car uz) (cdr uz)))))

  (fset 'neovm--zu-insert
    (lambda (uz val)
      (let ((z (car uz)))
        (funcall 'neovm--zu-do uz
                 (list (nth 0 z) val (cons (nth 1 z) (nth 2 z)))))))

  (fset 'neovm--zu-delete
    (lambda (uz)
      (let ((z (car uz)))
        (cond
          ((nth 2 z)
           (funcall 'neovm--zu-do uz
                    (list (nth 0 z) (car (nth 2 z)) (cdr (nth 2 z)))))
          ((nth 0 z)
           (funcall 'neovm--zu-do uz
                    (list (cdr (nth 0 z)) (car (nth 0 z)) nil)))
          (t uz)))))

  (fset 'neovm--zu-replace
    (lambda (uz val)
      (let ((z (car uz)))
        (funcall 'neovm--zu-do uz
                 (list (nth 0 z) val (nth 2 z))))))

  (fset 'neovm--zu-undo
    (lambda (uz)
      "Undo last operation, restoring previous state."
      (if (null (cdr uz)) uz
        (cons (cadr uz) (cddr uz)))))

  (fset 'neovm--zu-undo-count
    (lambda (uz) (length (cdr uz))))

  (unwind-protect
      (let ((uz (funcall 'neovm--zu-create '(a b c d e))))
        (list
          ;; Initial state
          (funcall 'neovm--zu-get-list uz)
          (funcall 'neovm--zu-undo-count uz)
          ;; Insert X at start
          (let ((uz1 (funcall 'neovm--zu-insert uz 'X)))
            (list (funcall 'neovm--zu-get-list uz1)
                  (funcall 'neovm--zu-undo-count uz1)))
          ;; Insert then undo
          (let* ((uz1 (funcall 'neovm--zu-insert uz 'X))
                 (uz2 (funcall 'neovm--zu-undo uz1)))
            (funcall 'neovm--zu-get-list uz2))
          ;; Multiple operations then undo all
          (let* ((uz1 (funcall 'neovm--zu-replace uz 'Z))
                 (uz2 (funcall 'neovm--zu-delete uz1))
                 (uz3 (funcall 'neovm--zu-insert uz2 'W)))
            (list
              (funcall 'neovm--zu-get-list uz3)
              (funcall 'neovm--zu-undo-count uz3)
              ;; Undo insert
              (funcall 'neovm--zu-get-list (funcall 'neovm--zu-undo uz3))
              ;; Undo insert + delete
              (funcall 'neovm--zu-get-list
                       (funcall 'neovm--zu-undo (funcall 'neovm--zu-undo uz3)))
              ;; Undo all three
              (funcall 'neovm--zu-get-list
                       (funcall 'neovm--zu-undo
                                (funcall 'neovm--zu-undo
                                         (funcall 'neovm--zu-undo uz3))))))
          ;; Undo on empty stack: no change
          (equal (funcall 'neovm--zu-get-list uz)
                 (funcall 'neovm--zu-get-list (funcall 'neovm--zu-undo uz)))))
    (fmakunbound 'neovm--zu-create)
    (fmakunbound 'neovm--zu-state)
    (fmakunbound 'neovm--zu-stack)
    (fmakunbound 'neovm--zu-to-list)
    (fmakunbound 'neovm--zu-get-list)
    (fmakunbound 'neovm--zu-focus)
    (fmakunbound 'neovm--zu-do)
    (fmakunbound 'neovm--zu-insert)
    (fmakunbound 'neovm--zu-delete)
    (fmakunbound 'neovm--zu-replace)
    (fmakunbound 'neovm--zu-undo)
    (fmakunbound 'neovm--zu-undo-count)))"#;
    let expect = expect_test::expect![[
        r#""OK ((a b c d e) 0 ((X a b c d e) 1) (a b c d e) ((W b c d e) 3 (b c d e) (Z b c d e) (a b c d e)) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
