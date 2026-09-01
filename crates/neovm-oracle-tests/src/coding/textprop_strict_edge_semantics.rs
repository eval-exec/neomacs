//! Oracle parity tests for coding-system + text-property operations.
//!
//! GNU src/coding.c, src/textprop.c: `check-coding-system`,
//! `coding-system-p`, `get-byte`, `text-properties-at`, `propertize`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_coding_system_p_on_utf8() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(coding-system-p 'utf-8)"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_coding_system_p_on_unknown() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(coding-system-p 'no-such-coding-system-xyz)"#,
        expect,
    );
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_check_coding_system_utf8() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK utf-8""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(check-coding-system 'utf-8)"#, expect);
    assert_ok_eq("utf-8", &o, &n);
}

#[test]
fn oracle_get_byte_position() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*gb*")) (erase-buffer) (insert "hello") (integerp (get-byte 3)))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_text_properties_at_none() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*tpa*")) (erase-buffer) (insert "hello") (text-properties-at 2))"#,
        expect,
    );
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_text_properties_at_with_prop() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (switch-to-buffer (get-buffer-create "*tpap*")) (erase-buffer) (insert "hello") (put-text-property 1 3 'face 'bold) (eq 'bold (get-text-property 2 'face)))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_propertize_creates_string_with_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (let ((s (propertize "hello" 'face 'bold))) (eq 'bold (get-text-property 0 'face s))))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_check_coding_system_wrong_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument symbolp 42)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(check-coding-system 42)"#, expect);
    assert_err_kind(&o, &n, "wrong-type-argument");
}
