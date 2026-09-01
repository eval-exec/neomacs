//! Oracle parity tests for GNU `subr.el` tree/list helper semantics.
//!
//! GNU `flatten-tree` performs an iterative cons-tree traversal that drops nil
//! leaves, keeps dotted tails, and is also exposed as `flatten-list`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_flatten_tree_basic_nil_and_dotted_leaves() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (flatten-tree '(1 (2 . 3) nil (4 5 (6)) 7))
 (flatten-tree '(nil (a nil (b . c)) ((nil)) d))
 (flatten-tree '((a . b) . c))
 (flatten-tree nil)
 (flatten-tree 42)
 (flatten-tree '(nil . tail)))
"#;

    let expect =
        expect_test::expect![[r#""OK ((1 2 3 4 5 6 7) (a b c d) (a b c) nil (42) (tail))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_flatten_list_alias_and_ordering() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((tree '((alpha beta) ((gamma . delta) nil) epsilon)))
  (list
   (eq (symbol-function 'flatten-list) (symbol-function 'flatten-tree))
   (flatten-tree tree)
   (flatten-list tree)
   (equal (flatten-tree tree) (flatten-list tree))))
"#;

    let expect = expect_test::expect![[
        r#""OK (nil (alpha beta gamma delta epsilon) (alpha beta gamma delta epsilon) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_ensure_list_wraps_atoms_and_preserves_lists() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((proper '(a b))
      (dotted '(a . b))
      (empty nil)
      (vector [a b])
      (string "abc"))
  (list
   (eq (ensure-list proper) proper)
   (eq (ensure-list dotted) dotted)
   (eq (ensure-list empty) empty)
   (ensure-list vector)
   (car (ensure-list string))
   (eq (car (ensure-list string)) string)
   (ensure-list 17)))
"#;

    let expect = expect_test::expect![[r#""OK (t t t ([a b]) \"abc\" t (17))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_flatten_tree_after_mutating_dotted_structure() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((tail (list 'd 'e))
       (tree (list (cons 'a 'b) (list 'c tail))))
  (setcdr tail 'f)
  (list tree (flatten-tree tree)))
"#;

    let expect = expect_test::expect![[r#""OK (((a . b) (c (d . f))) (a b c d f))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_flatten_tree_ensure_list_alias_arity_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (eq (symbol-function 'flatten-list) (symbol-function 'flatten-tree))
 (eq (ensure-list nil) nil)
 (let ((x (list nil)))
   (list (eq (ensure-list x) x)
         (eq (car (ensure-list x)) nil)
         (flatten-list x)))
 (flatten-tree '(nil (nil . a) (b nil . c) ((d)) . e))
 (condition-case err
     (flatten-tree)
   (error (list (car err) (cdr err))))
 (condition-case err
     (flatten-list 'a 'b)
   (error (list (car err) (cdr err))))
 (condition-case err
     (ensure-list)
   (error (list (car err) (cdr err))))
 (condition-case err
     (ensure-list 'a 'b)
   (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK (nil t (t t nil) (a b c d e) (wrong-number-of-arguments ((1 . 1) 0)) (wrong-number-of-arguments ((1 . 1) 2)) (wrong-number-of-arguments ((1 . 1) 0)) (wrong-number-of-arguments ((1 . 1) 2)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
