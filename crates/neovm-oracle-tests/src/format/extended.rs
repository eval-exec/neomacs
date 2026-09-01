//! Oracle parity tests for `format` with thorough parameter coverage.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{ORACLE_PROP_CASES, assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

#[test]
fn oracle_prop_format_percent_d_integers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"42\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%d" 42)"#, expect);
    let expect = expect_test::expect![[r#""OK \"-42\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%d" -42)"#, expect);
    let expect = expect_test::expect![[r#""OK \"0\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%d" 0)"#, expect);
}

#[test]
fn oracle_prop_format_percent_s_various_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"hello\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%s" "hello")"#, expect);
    let expect = expect_test::expect![[r#""OK \"42\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%s" 42)"#, expect);
    let expect = expect_test::expect![[r#""OK \"symbol\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%s" 'symbol)"#, expect);
    let expect = expect_test::expect![[r#""OK \"nil\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%s" nil)"#, expect);
    let expect = expect_test::expect![[r#""OK \"t\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%s" t)"#, expect);
    let expect = expect_test::expect![[r#""OK \"(1 2 3)\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%s" '(1 2 3))"#, expect);
}

#[test]
fn oracle_prop_format_percent_S_prin1_style() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"\\\"hello\\\"\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%S" "hello")"#, expect);
    let expect = expect_test::expect![[r#""OK \"42\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%S" 42)"#, expect);
    let expect = expect_test::expect![[r#""OK \"(1 \\\"two\\\" three)\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%S" '(1 "two" three))"#, expect);
}

#[test]
fn oracle_prop_format_padding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"        42\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%10d" 42)"#, expect);
    let expect = expect_test::expect![[r#""OK \"42        |\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%-10d|" 42)"#, expect);
    let expect = expect_test::expect![[r#""OK \"0000000042\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%010d" 42)"#, expect);
    let expect = expect_test::expect![[r#""OK \"        hi\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%10s" "hi")"#, expect);
    let expect = expect_test::expect![[r#""OK \"hi        |\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%-10s|" "hi")"#, expect);
}

#[test]
fn oracle_prop_format_float() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"3.140000\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%f" 3.14)"#, expect);
    let expect = expect_test::expect![[r#""OK \"3.14\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%.2f" 3.14159)"#, expect);
    let expect = expect_test::expect![[r#""OK \"1.234568e+04\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%e" 12345.6789)"#, expect);
    let expect = expect_test::expect![[r#""OK \"1e-05\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%g" 0.00001)"#, expect);
    let expect = expect_test::expect![[r#""OK \"12345\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%g" 12345.0)"#, expect);
}

#[test]
fn oracle_prop_format_hex_octal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"ff\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%x" 255)"#, expect);
    let expect = expect_test::expect![[r#""OK \"FF\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%X" 255)"#, expect);
    let expect = expect_test::expect![[r#""OK \"377\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%o" 255)"#, expect);
    let expect = expect_test::expect![[r#""OK \"0xff\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%#x" 255)"#, expect);
    let expect = expect_test::expect![[r#""OK \"0377\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%#o" 255)"#, expect);
}

#[test]
fn oracle_prop_format_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"A\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%c" 65)"#, expect);
    let expect = expect_test::expect![[r#""OK \"A\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%c" ?A)"#, expect);
    let expect = expect_test::expect![[r#""OK \"z\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%c" ?z)"#, expect);
}

#[test]
fn oracle_prop_format_multiple_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"Name: Alice, Age: 30, Score: 95.5\"""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(format "Name: %s, Age: %d, Score: %.1f" "Alice" 30 95.5)"#,
        expect,
    );
    let expect = expect_test::expect![[r#""OK \"key1=val1&key2=val2\"""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(format "%s=%s&%s=%s" "key1" "val1" "key2" "val2")"#,
        expect,
    );
}

#[test]
fn oracle_prop_format_percent_literal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"100%\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "100%%")"#, expect);
    let expect = expect_test::expect![[r#""OK \"50%\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%d%%" 50)"#, expect);
}

#[test]
fn oracle_prop_format_no_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"plain text\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(format "plain text")"#, expect);
    assert_ok_eq(r#""plain text""#, &o, &n);
}

#[test]
fn oracle_prop_format_complex_template() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(format "[%04d] %-15s %+8.2f (%s)"
                          7 "transaction" -42.5 "pending")"####;
    let expect = expect_test::expect![[r#""OK \"[0007] transaction       -42.50 (pending)\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_format_d_proptest(
        n in -10000i64..10000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(r#"(format "%d" {})"#, n);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        prop_assert_eq!(neovm.as_str(), oracle.as_str());
    }

    #[test]
    fn oracle_prop_format_x_proptest(
        n in 0u32..65536u32,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(r#"(format "%x" {})"#, n);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        prop_assert_eq!(neovm.as_str(), oracle.as_str());
    }
}
