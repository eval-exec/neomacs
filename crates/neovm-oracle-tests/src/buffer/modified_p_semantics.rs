//! Oracle parity tests for `buffer-modified-p`.
//!
//! GNU implements `buffer-modified-p` in `src/buffer.c` via `Fbuffer_modified_p`,
//! which compares `BUF_SAVE_MODIFF` vs `BUF_MODIFF` and checks `BUF_AUTOSAVE_MODIFF`
//! for the `autosaved` return value.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_buffer_modified_p_fresh_buffer_returns_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (get-buffer-create "*neovm-test-bmp*")
  (set-buffer (get-buffer "*neovm-test-bmp*"))
  (buffer-modified-p))"#,
        expect,
    );
    assert_ok_eq("nil", &oracle, &neovm);
}

#[test]
fn oracle_buffer_modified_p_after_insert_returns_t() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (get-buffer-create "*neovm-test-bmp-insert*")
  (set-buffer (get-buffer "*neovm-test-bmp-insert*"))
  (erase-buffer)
  (insert "hello")
  (buffer-modified-p))"#,
        expect,
    );
    assert_ok_eq("t", &oracle, &neovm);
}

#[test]
fn oracle_buffer_modified_p_nil_arg_means_current() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (get-buffer-create "*neovm-test-bmp-current*")
  (set-buffer (get-buffer "*neovm-test-bmp-current*"))
  (list
   (buffer-modified-p)
   (buffer-modified-p nil)))"#,
        expect,
    );
    assert_ok_eq("(nil nil)", &oracle, &neovm);
}

#[test]
fn oracle_buffer_modified_p_killed_buffer_returns_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (let ((b (get-buffer-create "*neovm-test-bmp-killed*")))
    (kill-buffer b)
    (list (buffer-modified-p b) (buffer-live-p b))))"#,
        expect,
    );
    assert_ok_eq("(nil nil)", &oracle, &neovm);
}

#[test]
fn oracle_buffer_modified_p_wrong_type_arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument bufferp 99)""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect("(buffer-modified-p 99)", expect);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}

#[test]
fn oracle_buffer_modified_p_too_many_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments buffer-modified-p 2)""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect("(buffer-modified-p nil nil)", expect);
    assert_err_kind(&oracle, &neovm, "wrong-number-of-arguments");
}

#[test]
fn oracle_buffer_modified_p_set_and_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil t)""#]];
    // Test set-buffer-modified-p and restore-buffer-modified-p independently.
    // GNU: set-buffer-modified-p delegates to restore-buffer-modified-p;
    // both compare MODIFF to SAVE_MODIFF.
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (get-buffer-create "*neovm-test-bmp-set*")
  (set-buffer (get-buffer "*neovm-test-bmp-set*"))
  (erase-buffer)
  (insert "x")
  (prog1
      (list (buffer-modified-p)
            (progn (set-buffer-modified-p nil) (buffer-modified-p))
            (progn (set-buffer-modified-p t) (buffer-modified-p)))
    (set-buffer-modified-p nil)))"#,
        expect,
    );
    assert_ok_eq("(t nil t)", &oracle, &neovm);
}
