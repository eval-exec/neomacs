//! Oracle parity tests for `macrop` via binary execution.
//!
//! Uses `eval_oracle_and_neovm` which spawns the actual
//! Neomacs release binary (matching how the GNU Emacs binary is used).
//! This gives true end-to-end parity and supports the full Lisp library
//! (defun, defmacro, etc.).

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_macrop_nil_for_defun_via_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (defun neovm--test-macrop-binary-defun () 42)
  (macrop 'neovm--test-macrop-binary-defun))"#,
        expect,
    );
    assert_ok_eq("nil", &oracle, &neovm);
}

#[test]
fn oracle_macrop_t_for_defmacro_via_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (defmacro neovm--test-macrop-binary-macro () 42)
  (macrop 'neovm--test-macrop-binary-macro))"#,
        expect,
    );
    assert_ok_eq("t", &oracle, &neovm);
}
