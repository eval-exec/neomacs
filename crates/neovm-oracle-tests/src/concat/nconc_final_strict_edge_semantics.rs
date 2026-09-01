//! Oracle parity for concat mixed, vconcat mixed, nconc, mapcan edges.
//! GNU src/fns.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_concat_integers_signal_sequence_type_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument sequencep 72)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(concat 72 73)"#, expect);
    assert_eq!(o, "ERR (wrong-type-argument sequencep 72)");
    assert_eq!(n, o);
}

#[test]
fn oracle_concat_one_arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"hello\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(concat "hello")"#, expect);
    assert_ok_eq("\"hello\"", &o, &n);
}

#[test]
fn oracle_vconcat_lists_to_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK [1 2 3 4]""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(vconcat '(1 2) '(3 4))"#, expect);
    assert_ok_eq("[1 2 3 4]", &o, &n);
}

#[test]
fn oracle_vconcat_vectors() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK [1 2 3 4 5 6]""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(vconcat [1 2] [3 4] [5 6])"#, expect);
    assert_ok_eq("[1 2 3 4 5 6]", &o, &n);
}

#[test]
fn oracle_nconc_last_non_list_dotted() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 2 . 3)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(nconc '(1 2) 3)"#, expect);
    assert_ok_eq("(1 2 . 3)", &o, &n);
}

#[test]
fn oracle_nconc_nil_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 2)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(nconc nil '(1 2) nil)"#, expect);
    assert_ok_eq("(1 2)", &o, &n);
}

#[test]
fn oracle_mapcan_simple() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 2 3)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(mapcan 'list '(1 2 3))"#, expect);
    assert_ok_eq("(1 2 3)", &o, &n);
}
