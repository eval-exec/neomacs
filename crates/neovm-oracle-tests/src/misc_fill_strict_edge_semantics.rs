//! Oracle parity for misc coverage fillers: `proper-list-p`,
//! `plistp`, `readablep`, `flatten-tree`, `copy-tree`, `gensym`.
//! GNU src/fns.c, src/alloc.c, src/lread.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_proper_list_p_true() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 3""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(proper-list-p '(a b c))"#, expect);
    assert_ok_eq("3", &o, &n);
}

#[test]
fn oracle_proper_list_p_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(proper-list-p nil)"#, expect);
    assert_ok_eq("0", &o, &n);
}

#[test]
fn oracle_proper_list_p_dotted() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(proper-list-p '(a b . c))"#, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_consp_on_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(consp '(a . b))"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_consp_on_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(consp nil)"#, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_listp_on_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(listp '(a b c))"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_listp_on_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(listp nil)"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_nlistp_on_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(nlistp nil)"#, expect);
    assert_ok_eq("nil", &o, &n);
}
