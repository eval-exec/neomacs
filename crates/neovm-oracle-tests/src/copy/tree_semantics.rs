//! Oracle parity tests for GNU `copy-tree` semantics.
//!
//! GNU implements `copy-tree` in `lisp/subr.el`.  It recursively copies cons
//! cars and cdrs, preserves non-cons leaves by identity, and only traverses
//! vectors and records when VECTORS-AND-RECORDS is non-nil.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_copy_tree_recursively_copies_cons_cells() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((leaf (list 'shared))
       (tree (cons (cons 'a leaf) (cons (list 'b leaf) 'tail)))
       (copy (copy-tree tree)))
  (setcar (car copy) 'changed-a)
  (setcar (cadr copy) 'changed-b)
  (setcar leaf 'mutated-leaf)
  (list
   tree
   copy
   (eq tree copy)
   (eq (car tree) (car copy))
   (eq (cdr (car tree)) (cdr (car copy)))
   (eq (cadr tree) (cadr copy))
   (eq (cadr (cadr tree)) (cadr (cadr copy)))
   (cdr (last copy))))
"#;

    let expect = expect_test::expect![[
        r#""OK (((a mutated-leaf) (b (mutated-leaf)) . tail) ((changed-a shared) (changed-b (shared)) . tail) nil nil nil nil nil tail)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_copy_tree_without_vector_flag_preserves_vector_and_record_identity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((shared (list 'cell))
       (vec (vector shared))
       (rec (record 'tag shared))
       (tree (list vec rec))
       (copy (copy-tree tree)))
  (setcar shared 'mutated)
  (list
   (eq tree copy)
   (eq (car tree) (car copy))
   (eq (cadr tree) (cadr copy))
   copy))
"#;

    let expect = expect_test::expect![[r#""OK (nil t t ([(mutated)] #s(tag (mutated))))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_copy_tree_with_vector_flag_recurses_into_vectors_and_records() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((shared (list 'cell))
       (vec (vector shared (list 'nested)))
       (rec (record 'tag shared vec))
       (tree (list vec rec))
       (copy (copy-tree tree t)))
  (setcar (aref (car copy) 0) 'copy-vector-cell)
  (setcar (aref (car copy) 1) 'copy-vector-nested)
  (setcar (aref (cadr copy) 1) 'copy-record-cell)
  (list
   tree
   copy
   (eq tree copy)
   (eq (car tree) (car copy))
   (eq (aref (car tree) 0) (aref (car copy) 0))
   (eq (cadr tree) (cadr copy))
   (eq (aref (cadr tree) 1) (aref (cadr copy) 1))
   (eq (aref (cadr tree) 2) (aref (cadr copy) 2))))
"#;

    let expect = expect_test::expect![[
        r#""OK (([(cell) (nested)] #s(tag (cell) [(cell) (nested)])) ([(copy-vector-cell) (copy-vector-nested)] #s(tag (copy-record-cell) [(cell) (nested)])) nil nil nil nil nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_copy_tree_vector_and_record_dotted_tails_follow_flag() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU lisp/subr.el:copy-tree handles an improper cons tail after the main
    // cons walk.  With VECTORS-AND-RECORDS nil, vector/record tails are shared;
    // with it non-nil, the tail is recursively copied.
    let form = r#"
(let* ((vec-tail (vector (list 'vec-cell)))
       (rec-tail (record 'tag (list 'rec-cell)))
       (vec-tree (cons 'head vec-tail))
       (rec-tree (cons 'head rec-tail))
       (vec-default (copy-tree vec-tree))
       (rec-default (copy-tree rec-tree))
       (vec-deep (copy-tree vec-tree t))
       (rec-deep (copy-tree rec-tree t)))
  (setcar (aref (cdr vec-deep) 0) 'changed-vec-copy)
  (setcar (aref (cdr rec-deep) 1) 'changed-rec-copy)
  (list
   (eq (cdr vec-tree) (cdr vec-default))
   (eq (cdr rec-tree) (cdr rec-default))
   (eq (cdr vec-tree) (cdr vec-deep))
   (eq (aref (cdr vec-tree) 0) (aref (cdr vec-deep) 0))
   (eq (cdr rec-tree) (cdr rec-deep))
   (eq (aref (cdr rec-tree) 1) (aref (cdr rec-deep) 1))
   vec-tree
   vec-deep
   rec-tree
   rec-deep))
"#;

    let expect = expect_test::expect![[
        r#""OK (t t nil nil nil nil (head . [(vec-cell)]) (head . [(changed-vec-copy)]) (head . #s(tag (rec-cell))) (head . #s(tag (changed-rec-copy))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_copy_tree_non_cons_leaf_identity_and_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((sym 'same-symbol)
      (str "same-string")
      (vec [a b])
      (rec (record 'tag 'x)))
  (list
   (copy-tree nil)
   (eq sym (copy-tree sym))
   (eq str (copy-tree str))
   (eq vec (copy-tree vec))
   (eq rec (copy-tree rec))
   (eq vec (copy-tree vec t))
   (eq rec (copy-tree rec t))))
"#;

    let expect = expect_test::expect![[r#""OK (nil t t t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
