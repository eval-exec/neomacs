//! Oracle parity tests for `type-of`.

use expect_test::expect;

use crate::common::{assert_oracle_parity_expect, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_prop_type_of_integer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect![[r#""OK integer""#]];
    assert_oracle_parity_expect("(type-of 42)", expect);
    let expect = expect![[r#""OK integer""#]];
    assert_oracle_parity_expect("(type-of 0)", expect);
    let expect = expect![[r#""OK integer""#]];
    assert_oracle_parity_expect("(type-of -1)", expect);
}

#[test]
fn oracle_prop_type_of_float() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect![[r#""OK float""#]];
    assert_oracle_parity_expect("(type-of 3.14)", expect);
    let expect = expect![[r#""OK float""#]];
    assert_oracle_parity_expect("(type-of 0.0)", expect);
    let expect = expect![[r#""OK float""#]];
    assert_oracle_parity_expect("(type-of -1.5)", expect);
}

#[test]
fn oracle_prop_type_of_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect![[r#""OK string""#]];
    assert_oracle_parity_expect(r#"(type-of "hello")"#, expect);
    let expect = expect![[r#""OK string""#]];
    assert_oracle_parity_expect(r#"(type-of "")"#, expect);
}

#[test]
fn oracle_prop_type_of_symbol() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect![[r#""OK symbol""#]];
    assert_oracle_parity_expect("(type-of 'foo)", expect);
    let expect = expect![[r#""OK symbol""#]];
    assert_oracle_parity_expect("(type-of t)", expect);
    let expect = expect![[r#""OK symbol""#]];
    assert_oracle_parity_expect("(type-of nil)", expect);
}

#[test]
fn oracle_prop_type_of_cons() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect![[r#""OK cons""#]];
    assert_oracle_parity_expect("(type-of '(1 2 3))", expect);
    let expect = expect![[r#""OK cons""#]];
    assert_oracle_parity_expect("(type-of (cons 'a 'b))", expect);
}

#[test]
fn oracle_prop_type_of_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect![[r#""OK vector""#]];
    assert_oracle_parity_expect("(type-of [1 2 3])", expect);
    let expect = expect![[r#""OK vector""#]];
    assert_oracle_parity_expect("(type-of [])", expect);
}

#[test]
fn oracle_prop_type_of_hash_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect![[r#""OK hash-table""#]];
    assert_oracle_parity_expect("(type-of (make-hash-table))", expect);
}

#[test]
fn oracle_prop_type_of_char_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect![[r#""OK char-table""#]];
    assert_oracle_parity_expect("(type-of (make-char-table 'foo))", expect);
}

#[test]
fn oracle_prop_type_of_in_conditional() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use type-of for dispatching
    let form = "(let ((val '(1 2 3)))
                  (cond
                    ((eq (type-of val) 'integer) 'int)
                    ((eq (type-of val) 'cons) 'list)
                    ((eq (type-of val) 'string) 'str)
                    (t 'other)))";
    let expect = expect![[r#""OK list""#]];
    assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_type_of_mapped_over_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(mapcar 'type-of (list 1 "s" 'sym '(a) [v] 3.0))"####;
    let expect = expect![[r#""OK (integer string symbol cons vector float)""#]];
    assert_oracle_parity_expect(form, expect);
}
