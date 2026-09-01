//! Oracle parity tests for `buffer-last-name`.
//!
//! GNU implements `buffer-last-name` in `src/buffer.c` — returns the name
//! a buffer had before the last rename (or the current name if never renamed).

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_buffer_last_name_fresh_buffer_returns_its_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"*neovm-test-bln-fresh*\"""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (get-buffer-create "*neovm-test-bln-fresh*")
  (buffer-last-name (get-buffer "*neovm-test-bln-fresh*")))"#,
        expect,
    );
    assert_ok_eq("\"*neovm-test-bln-fresh*\"", &oracle, &neovm);
}

#[test]
fn oracle_buffer_last_name_after_rename_returns_old_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"*neovm-test-bln-orig*\"""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (get-buffer-create "*neovm-test-bln-orig*")
  (let ((b (get-buffer "*neovm-test-bln-orig*")))
    (rename-buffer "*neovm-test-bln-new*")
    (buffer-last-name b)))"#,
        expect,
    );
    assert_ok_eq("\"*neovm-test-bln-orig*\"", &oracle, &neovm);
}

#[test]
fn oracle_buffer_last_name_nil_arg_means_current() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (get-buffer-create "*neovm-test-bln-cur*")
  (set-buffer (get-buffer "*neovm-test-bln-cur*"))
  (eq (buffer-last-name) (buffer-last-name nil)))"#,
        expect,
    );
    assert_ok_eq("t", &oracle, &neovm);
}

#[test]
fn oracle_buffer_last_name_wrong_type_arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument bufferp 42)""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect("(buffer-last-name 42)", expect);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}
