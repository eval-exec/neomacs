//! Oracle parity tests for `buffer-file-name`.
//!
//! GNU implements `buffer-file-name` in `src/buffer.c` via `Fbuffer_file_name`,
//! which calls `BVAR(decode_buffer(buffer), filename)`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_buffer_file_name_current_buffer_nil_for_non_file() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (get-buffer-create "*neovm-test-nonfile*")
  (set-buffer (get-buffer "*neovm-test-nonfile*"))
  (buffer-file-name))"#,
        expect,
    );
    assert_ok_eq("nil", &oracle, &neovm);
}

#[test]
fn oracle_buffer_file_name_nil_arg_means_current() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (get-buffer-create "*neovm-test-bfn*")
  (set-buffer (get-buffer "*neovm-test-bfn*"))
  (list
   (buffer-file-name)
   (buffer-file-name nil)))"#,
        expect,
    );
    assert_ok_eq("(nil nil)", &oracle, &neovm);
}

#[test]
fn oracle_buffer_file_name_killed_buffer_returns_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (let ((b (get-buffer-create "*neovm-test-killed*")))
    (kill-buffer b)
    (list (buffer-file-name b) (buffer-live-p b))))"#,
        expect,
    );
    assert_ok_eq("(nil nil)", &oracle, &neovm);
}

#[test]
fn oracle_buffer_file_name_wrong_type_arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument bufferp 42)""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect("(buffer-file-name 42)", expect);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument bufferp some-symbol)""#]];
    let (oracle2, neovm2) =
        crate::common::eval_oracle_and_neovm_expect("(buffer-file-name 'some-symbol)", expect);
    assert_err_kind(&oracle2, &neovm2, "wrong-type-argument");
}

#[test]
fn oracle_buffer_file_name_too_many_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments buffer-file-name 2)""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect("(buffer-file-name nil nil)", expect);
    assert_err_kind(&oracle, &neovm, "wrong-number-of-arguments");
}
