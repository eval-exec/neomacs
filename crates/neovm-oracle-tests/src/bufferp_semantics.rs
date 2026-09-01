//! Oracle parity tests for `bufferp`.
//!
//! GNU implements `bufferp` in `src/buffer.c` — simple type predicate
//! for buffer objects.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_bufferp_live_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (get-buffer-create "*neovm-test-bp*")
  (bufferp (get-buffer "*neovm-test-bp*")))"#,
        expect,
    );
    assert_ok_eq("t", &oracle, &neovm);
}

#[test]
fn oracle_bufferp_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(bufferp nil)", expect);
    assert_ok_eq("nil", &oracle, &neovm);
}

#[test]
fn oracle_bufferp_non_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil t)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(list (bufferp 42) (bufferp "hello") (bufferp 'sym) (bufferp (current-buffer)))"#,
        expect,
    );
    assert_ok_eq("(nil nil nil t)", &oracle, &neovm);
}

#[test]
fn oracle_bufferp_killed_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (let ((b (get-buffer-create "*neovm-test-bp-killed*")))
    (kill-buffer b)
    (bufferp b)))"#,
        expect,
    );
    assert_ok_eq("t", &oracle, &neovm);
}
