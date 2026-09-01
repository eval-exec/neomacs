//! Strict combo oracle probes, batch 39: self-contained data-structure
//! libraries via assert_oracle_parity_with_load — avl-tree.el (insert/
//! flatten/member/map), generator.el (iter yield/next via CPS), and
//! radix-tree.el (radix-tree-insert/lookup).
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity_with_load;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_h6_avl_tree_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(let ((t (avl-tree-create #'<)))
  (avl-tree-enter t 3)
  (avl-tree-enter t 1)
  (avl-tree-enter t 2)
  (avl-tree-enter t 1)
  (list (avl-tree-member t 2)
        (avl-tree-member t 9)
        (avl-tree-flatten t)
        (avl-tree-height t)))
"##,
        &["emacs-lisp/avl-tree.el"],
        expect,
    );
}

#[test]
fn div_h6_generator_iter_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function letiter)""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(let ((iter (letiter (next) (yield 1) (yield 2) (yield 3))))
  (list (iter-next iter)
        (iter-next iter)
        (iter-next iter)
        (iter-next iter)))
"##,
        &["emacs-lisp/generator.el"],
        expect,
    );
}

#[test]
fn div_h6_generator_iter_do_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function iter-gen)""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(let ((iter (iter-gen (x '(1 2 3 4)) (yield x))))
  (let (out)
    (iter-do (v iter)
      (push v out))
    (nreverse out)))
"##,
        &["emacs-lisp/generator.el"],
        expect,
    );
}

#[test]
fn div_h6_radix_tree_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(let ((t (radix-tree-insert radix-tree-empty "cat" 'cat)))
  (setq t (radix-tree-insert t "car" 'car))
  (setq t (radix-tree-insert t "dog" 'dog))
  (list (radix-tree-lookup t "cat")
        (radix-tree-lookup t "car")
        (radix-tree-lookup t "dog")
        (radix-tree-lookup t "ca")
        (length (radix-tree-keys t))))
"##,
        &["emacs-lisp/radix-tree.el"],
        expect,
    );
}

#[test]
fn div_h6_avl_tree_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(let ((t (avl-tree-create #'<)))
  (dolist (n '(5 3 7 1 4 6 8)) (avl-tree-enter t n))
  (avl-tree-delete t 5)
  (list (avl-tree-flatten t)
        (avl-tree-member t 5)
        (avl-tree-member t 4)))
"##,
        &["emacs-lisp/avl-tree.el"],
        expect,
    );
}
