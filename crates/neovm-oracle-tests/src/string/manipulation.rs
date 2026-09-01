//! Oracle parity tests for string manipulation primitives.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{ORACLE_PROP_CASES, assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

#[test]
fn oracle_prop_string_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 5""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-width "hello")"#, expect);
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-width "")"#, expect);
    let expect = expect_test::expect![[r#""OK 3""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-width "abc")"#, expect);
}

#[test]
fn oracle_prop_string_prefix_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-prefix-p "hel" "hello")"#, expect);
    assert_ok_eq("t", &o, &n);

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-prefix-p "xyz" "hello")"#, expect);
    assert_ok_eq("nil", &o, &n);

    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-prefix-p "" "hello")"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_prop_string_suffix_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-suffix-p "llo" "hello")"#, expect);
    assert_ok_eq("t", &o, &n);

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-suffix-p "xyz" "hello")"#, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_prop_string_trim() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"hello\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-trim "  hello  ")"#, expect);
    let expect = expect_test::expect![[r#""OK \"hello\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-trim "\t\nhello\n\t")"#, expect);
    let expect = expect_test::expect![[r#""OK \"hello\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-trim "hello")"#, expect);
    let expect = expect_test::expect![[r#""OK \"\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-trim "")"#, expect);
}

#[test]
fn oracle_prop_string_trim_left() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"hello  \"""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-trim-left "  hello  ")"#, expect);
}

#[test]
fn oracle_prop_string_trim_right() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"  hello\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-trim-right "  hello  ")"#, expect);
}

#[test]
fn oracle_prop_string_join() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"a-b-c\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-join '("a" "b" "c") "-")"#, expect);
    let expect = expect_test::expect![[r#""OK \"abc\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-join '("a" "b" "c") "")"#, expect);
    let expect = expect_test::expect![[r#""OK \"only\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-join '("only") ",")"#, expect);
    let expect = expect_test::expect![[r#""OK \"\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-join nil ",")"#, expect);
}

#[test]
fn oracle_prop_split_string_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"a\" \"b\" \"c\")""#]];
    crate::common::assert_oracle_parity_expect(r#"(split-string "a-b-c" "-")"#, expect);
    let expect = expect_test::expect![[r#""OK (\"hello\" \"world\")""#]];
    crate::common::assert_oracle_parity_expect(r#"(split-string "hello world" " ")"#, expect);
    let expect = expect_test::expect![[r#""OK (\"no-split\")""#]];
    crate::common::assert_oracle_parity_expect(r#"(split-string "no-split" "X")"#, expect);
}

#[test]
fn oracle_prop_string_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"hello emacs\"""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(string-replace "world" "emacs" "hello world")"#,
        expect,
    );
    let expect = expect_test::expect![[r#""OK \"no match\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-replace "x" "y" "no match")"#, expect);
    let expect = expect_test::expect![[r#""OK \"bbbnbbnbb\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-replace "a" "bb" "banana")"#, expect);
}

#[test]
fn oracle_prop_string_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 6""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-search "world" "hello world")"#, expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-search "xyz" "hello world")"#, expect);
    let expect = expect_test::expect![[r#""OK 3""#]];
    crate::common::assert_oracle_parity_expect(r#"(string-search "l" "hello" 3)"#, expect);
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_string_length_concat(
        a_len in 0usize..20usize,
        b_len in 0usize..20usize,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            r#"(length (concat (make-string {} ?a) (make-string {} ?b)))"#,
            a_len, b_len
        );
        let expected = format!("OK {}", a_len + b_len);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        prop_assert_eq!(neovm.as_str(), expected.as_str());
        prop_assert_eq!(oracle.as_str(), expected.as_str());
    }
}
