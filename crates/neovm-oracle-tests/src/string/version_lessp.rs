//! Oracle parity tests for `string-version-lessp`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_prop_string_version_lessp_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    // identical
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(string-version-lessp "v1.0" "v1.0")"#,
        expect,
    );
    assert_ok_eq("nil", &o, &n);

    let expect = expect_test::expect![[r#""OK t""#]];
    // numeric ordering within strings
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(string-version-lessp "file2" "file10")"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(string-version-lessp "file10" "file2")"#,
        expect,
    );
    assert_ok_eq("nil", &o, &n);

    let expect = expect_test::expect![[r#""OK t""#]];
    // pure numeric
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-version-lessp "9" "10")"#, expect);
    assert_ok_eq("t", &o, &n);

    let expect = expect_test::expect![[r#""OK t""#]];
    // version-style
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(string-version-lessp "1.9.3" "1.10.1")"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(string-version-lessp "1.10.1" "1.9.3")"#,
        expect,
    );
    assert_ok_eq("nil", &o, &n);

    let expect = expect_test::expect![[r#""OK t""#]];
    // prefix relation
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(string-version-lessp "pkg" "pkg1")"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(string-version-lessp "pkg1" "pkg")"#,
        expect,
    );
    assert_ok_eq("nil", &o, &n);

    let expect = expect_test::expect![[r#""OK nil""#]];
    // empty strings
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-version-lessp "" "")"#, expect);
    assert_ok_eq("nil", &o, &n);

    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-version-lessp "" "a")"#, expect);
    assert_ok_eq("t", &o, &n);

    let expect = expect_test::expect![[r#""OK nil""#]];
    // leading zeros
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-version-lessp "007" "7")"#, expect);
    assert_ok_eq("nil", &o, &n);

    let expect = expect_test::expect![[r#""OK t""#]];
    // mixed alpha-numeric with dots
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(string-version-lessp "v2.0" "v10.0")"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_prop_string_version_lessp_symbol_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    // GNU Emacs accepts symbols — neovm should too
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect("(string-version-lessp 'v2 'v10)", expect);
    assert_ok_eq("t", &o, &n);
}
