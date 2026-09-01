//! Strict combo oracle probes, batch 251: avl-tree. avl-tree-create with a
//! compare function, enter/members/flatten, height, delete, and map.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_avl_tree_enter_member_flatten_height() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'avl-tree)
(let ((tree (avl-tree-create (lambda (a b) (cond ((< a b) -1) ((> a b) 1) (t 0))))))
  (avl-tree-enter tree 5)
  (avl-tree-enter tree 3)
  (avl-tree-enter tree 7)
  (avl-tree-enter tree 1)
  (avl-tree-enter tree 9)
  (list (avl-tree-member tree 5)
        (avl-tree-member tree 4)
        (avl-tree-flatten tree)
        (avl-tree-empty tree)
        (avl-tree-height tree)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function avl-tree-height)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_avl_tree_delete_root_min_max() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'avl-tree)
(let ((tree (avl-tree-create (lambda (a b) (- a b)))))
  (dolist (n '(8 4 12 2 6 10 14)) (avl-tree-enter tree n))
  (list (avl-tree-first tree)
        (avl-tree-last tree)
        (avl-tree-delete tree 6)
        (avl-tree-member tree 6)
        (avl-tree-flatten tree)
        (avl-tree-delete-all tree 8)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function avl-tree-delete-all)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_avl_tree_map_copy_size() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'avl-tree)
(let ((tree (avl-tree-create (lambda (a b) (- a b)))))
  (dolist (n '(5 3 7 4 6)) (avl-tree-enter tree n))
  (let ((mapped nil))
    (avl-tree-map (lambda (n) (push (* n 10) mapped) n) tree)
    (list (sort mapped #'<)
          (avl-tree-size tree)
          (avl-tree-flatten tree))))
"##;
    let expect = expect_test::expect![[r#""OK ((30 40 50 60 70) 5 (6 4 7 3 5))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
