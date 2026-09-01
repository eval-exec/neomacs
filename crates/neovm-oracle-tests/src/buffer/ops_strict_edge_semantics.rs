//! Oracle parity for buffer ops: current-buffer, set-buffer,
//! get-buffer-create, buffer-name, buffer-size, buffer-live-p.
//! GNU src/buffer.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_current_buffer_is_live() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(buffer-live-p (current-buffer))"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_set_buffer_returns_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(bufferp (set-buffer (get-buffer-create "*sb*")))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_get_buffer_create_creates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (get-buffer-create "*gbc*") (buffer-live-p (get-buffer "*gbc*")))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_buffer_name_returns_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(stringp (buffer-name))"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_buffer_size_returns_integer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 5""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*bsz*")) (erase-buffer) (insert "hello") (buffer-size))"#,
        expect,
    );
    assert_ok_eq("5", &o, &n);
}

#[test]
fn oracle_buffer_live_p_dead() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (let ((b (get-buffer-create "*blp*"))) (kill-buffer b) (buffer-live-p b)))"#,
        expect,
    );
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_set_buffer_wrong_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp 42)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(set-buffer 42)"#, expect);
    assert_err_kind(&o, &n, "wrong-type-argument");
}

#[test]
fn oracle_buffer_name_buffer_arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(stringp (buffer-name (current-buffer)))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}
