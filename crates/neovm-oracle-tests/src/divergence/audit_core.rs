//! Core reimplemented-function divergence probes.
//!
//! Functions commonly divergent when reimplemented from scratch: sort stability,
//! copy-tree depth (vector copying), mapcar/mapc over vectors, nreverse on
//! vectors, read of circular/shared structures, let-bound special var dynamics,
//! default-value/setq-default, indirect-variable aliasing, makunbound effects.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_aco_sort_stability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((1 . :a) (1 . :b) (1 . :d) (2 . :c))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(sort (copy-sequence '((1 . :a) (1 . :b) (2 . :c) (1 . :d)))
      (lambda (x y) (< (car x) (car y))))
"##,
        expect,
    );
}

#[test]
fn div_aco_sort_stability_strings() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"a3\" \"a1\" \"a2\" \"b1\" \"b2\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(sort (copy-sequence '("a3" "b1" "a1" "b2" "a2"))
      (lambda (x y) (string< (substring x 0 1) (substring y 0 1))))
"##,
        expect,
    );
}

#[test]
fn div_aco_copy_tree_deep_vectors() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK [1 [2 3]]""#]];
    // copy-tree with vecp=t must deep-copy vectors.
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((v (vector 1 (vector 2 3)))
       (c (copy-tree v t)))
  (aset (aref c 1) 0 99)
  v)
"##,
        expect,
    );
}

#[test]
fn div_aco_copy_tree_shallow_vectors() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK [1 [99 3]]""#]];
    // copy-tree nil vecp shares vectors.
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((v (vector 1 (vector 2 3)))
       (c (copy-tree v)))
  (aset (aref c 1) 0 99)
  v)
"##,
        expect,
    );
}

#[test]
fn div_aco_mapcar_over_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 2 3)""#]];
    crate::common::assert_oracle_parity_expect(r##"(mapcar #'identity [1 2 3])"##, expect);
}

#[test]
fn div_aco_mapc_return_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 2 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((acc nil))
  (mapc (lambda (x) (push x acc)) '(1 2 3))
  acc)
"##,
        expect,
    );
}

#[test]
fn div_aco_nreverse_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK [4 3 2 1]""#]];
    crate::common::assert_oracle_parity_expect(r##"(nreverse (copy-sequence [1 2 3 4]))"##, expect);
}

#[test]
fn div_aco_read_circular_label() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (a . #0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(car (read-from-string "#1=(a . #1#)"))"##,
        expect,
    );
}

#[test]
fn div_aco_read_shared_label() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((x (car (read-from-string "(a #1=(b) c #1#)"))))
  (eq (nth 1 x) (nth 3 x)))
"##,
        expect,
    );
}

#[test]
fn div_aco_read_struct_syntax() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK #s(foo 1 2 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(car (read-from-string (prin1-to-string #s(foo 1 2 3))))
"##,
        expect,
    );
}

#[test]
fn div_aco_default_value_setq_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK defaulted""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((neo-test-var 'original))
  (setq-default neo-test-var 'defaulted)
  (prog1 (default-value 'neo-test-var)
    (setq-default neo-test-var nil)))
"##,
        expect,
    );
}

#[test]
fn div_aco_indirect_variable_alias() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (neo-alias-target 42 99 99)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(defvar neo-alias-target 42)
(defvaralias 'neo-alias 'neo-alias-target)
(list (indirect-variable 'neo-alias)
      neo-alias
      (setq neo-alias 99)
      neo-alias-target)
"##,
        expect,
    );
}

#[test]
fn div_aco_makunbound_and_boundp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil neo-tmp nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((neo-tmp 'x))
  (list (boundp 'neo-tmp)
        (makunbound 'neo-tmp)
        (boundp 'neo-tmp)))
"##,
        expect,
    );
}

#[test]
fn div_aco_let_special_var_dynamic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (modified outer)""#]];
    // A dynamically-let-bound var: set on the symbol sees the let value.
    crate::common::assert_oracle_parity_expect(
        r##"
(defvar neo-dyn 'outer)
(list (let ((neo-dyn 'inner)) (set 'neo-dyn 'modified) neo-dyn)
      neo-dyn)
"##,
        expect,
    );
}
