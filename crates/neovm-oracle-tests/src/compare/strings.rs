//! Oracle parity tests for `compare-strings`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_prop_compare_strings_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    // identical strings
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(compare-strings "foobar" nil nil "foobar" nil nil)"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);

    let expect = expect_test::expect![[r#""OK -1""#]];
    // first < second
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(compare-strings "abc" nil nil "xyz" nil nil)"#,
        expect,
    );
    assert_ok_eq("-1", &o, &n);

    let expect = expect_test::expect![[r#""OK 1""#]];
    // first > second
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(compare-strings "xyz" nil nil "abc" nil nil)"#,
        expect,
    );
    assert_ok_eq("1", &o, &n);

    let expect = expect_test::expect![[r#""OK t""#]];
    // case-insensitive
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(compare-strings "HELLO" nil nil "hello" nil nil t)"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);

    let expect = expect_test::expect![[r#""OK t""#]];
    // subrange comparison
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(compare-strings "xxabcyy" 2 5 "zzabcww" 2 5)"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);

    let expect = expect_test::expect![[r#""OK -3""#]];
    // prefix shorter
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(compare-strings "ab" nil nil "abcd" nil nil)"#,
        expect,
    );
    assert_ok_eq("-3", &o, &n);

    let expect = expect_test::expect![[r#""OK t""#]];
    // empty strings
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(compare-strings "" nil nil "" nil nil)"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}
