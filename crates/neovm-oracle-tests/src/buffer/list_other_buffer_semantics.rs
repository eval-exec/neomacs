//! Oracle parity tests for `buffer-list` and `other-buffer`.
//!
//! GNU implements `buffer-list` in `src/buffer.c` via `Fbuffer_list`,
//! which maps `mapcar` over `Vbuffer_alist`.  `other-buffer` is implemented
//! via `Fother_buffer` which scans frame buffer lists, then global buffers,
//! falling back to `*scratch*`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// buffer-list
// ---------------------------------------------------------------------------

#[test]
fn oracle_buffer_list_returns_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect("(listp (buffer-list))", expect);
    assert_ok_eq("t", &oracle, &neovm);
}

#[test]
fn oracle_buffer_list_includes_created_buffers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (get-buffer-create "*neovm-test-bl-alpha*")
  (get-buffer-create "*neovm-test-bl-beta*")
  (and (member (get-buffer "*neovm-test-bl-alpha*") (buffer-list))
       (member (get-buffer "*neovm-test-bl-beta*") (buffer-list))
       t))"#,
        expect,
    );
    assert_ok_eq("t", &oracle, &neovm);
}

#[test]
fn oracle_buffer_list_no_arg_returns_all_live() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (get-buffer-create "*neovm-test-bl-all*")
  (set-buffer (get-buffer "*neovm-test-bl-all*"))
  (and (listp (buffer-list))
       (member (current-buffer) (buffer-list))
       t))"#,
        expect,
    );
    assert_ok_eq("t", &oracle, &neovm);
}

#[test]
fn oracle_buffer_list_excludes_killed_buffers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (let ((b (get-buffer-create "*neovm-test-bl-killed*")))
    (kill-buffer b)
    (not (member b (buffer-list)))))"#,
        expect,
    );
    assert_ok_eq("t", &oracle, &neovm);
}

#[test]
fn oracle_buffer_list_too_many_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments buffer-list 2)""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect("(buffer-list nil nil)", expect);
    assert_err_kind(&oracle, &neovm, "wrong-number-of-arguments");
}

// ---------------------------------------------------------------------------
// other-buffer
// ---------------------------------------------------------------------------

#[test]
fn oracle_other_buffer_returns_buffer_object() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect("(bufferp (other-buffer))", expect);
    assert_ok_eq("t", &oracle, &neovm);
}

#[test]
fn oracle_other_buffer_skips_named_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (get-buffer-create "*neovm-test-ob-skip*")
  (set-buffer (get-buffer "*neovm-test-ob-skip*"))
  (not (eq (current-buffer) (other-buffer (current-buffer)))))"#,
        expect,
    );
    assert_ok_eq("t", &oracle, &neovm);
}

#[test]
fn oracle_other_buffer_hidden_buffers_ignored() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (get-buffer-create " *neovm-test-ob-hidden*")
  (not (eq (other-buffer) (get-buffer " *neovm-test-ob-hidden*"))))"#,
        expect,
    );
    assert_ok_eq("t", &oracle, &neovm);
}

#[test]
fn oracle_other_buffer_nil_arg_same_as_no_arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(eq (other-buffer) (other-buffer nil))"#,
        expect,
    );
    assert_ok_eq("t", &oracle, &neovm);
}

#[test]
fn oracle_other_buffer_killed_buffer_arg_ignored() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (let ((b (get-buffer-create "*neovm-test-ob-killed*")))
    (kill-buffer b)
    (bufferp (other-buffer b))))"#,
        expect,
    );
    assert_ok_eq("t", &oracle, &neovm);
}

#[test]
fn oracle_other_buffer_too_many_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments other-buffer 4)""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect("(other-buffer nil nil nil nil)", expect);
    assert_err_kind(&oracle, &neovm, "wrong-number-of-arguments");
}
