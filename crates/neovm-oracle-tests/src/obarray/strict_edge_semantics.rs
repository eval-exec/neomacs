//! Oracle parity tests for obarray strict edge cases.
//!
//! GNU src/lread.c: `intern`, `intern-soft`, `unintern`, `obarrayp`,
//! `mapatoms` operate on obarrays.  Symbol interning and obarray
//! manipulation has subtle semantics.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_intern_creates_symbol() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(symbolp (intern "neovm--test-intern-abc"))"#,
        expect,
    );
    assert_ok_eq("t", &oracle, &neovm);
}

#[test]
fn oracle_intern_same_name_returns_same_symbol() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(eq (intern "neovm--test-same")
             (intern "neovm--test-same"))"#,
        expect,
    );
    assert_ok_eq("t", &oracle, &neovm);
}

#[test]
fn oracle_intern_soft_returns_nil_for_unknown() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(intern-soft "neovm--test-never-interned-xyz")"#,
        expect,
    );
    assert_ok_eq("nil", &oracle, &neovm);
}

#[test]
fn oracle_intern_soft_returns_symbol_when_present() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (intern "neovm--test-is-there")
  (symbolp (intern-soft "neovm--test-is-there")))"#,
        expect,
    );
    assert_ok_eq("t", &oracle, &neovm);
}

#[test]
fn oracle_make_symbol_creates_uninterned() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (let ((s (make-symbol "neovm--test-uninterned")))
    (list (symbolp s)
          (eq s (intern-soft "neovm--test-uninterned")))))"#,
        expect,
    );
    assert_ok_eq("(t nil)", &oracle, &neovm);
}

#[test]
fn oracle_intern_empty_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(symbolp (intern ""))"#, expect);
    assert_ok_eq("t", &oracle, &neovm);
}

#[test]
fn oracle_obarrayp_for_standard_obarray() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(obarrayp obarray)", expect);
    assert_ok_eq("t", &oracle, &neovm);
}

#[test]
fn oracle_intern_wrong_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp 42)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(intern 42)", expect);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}
