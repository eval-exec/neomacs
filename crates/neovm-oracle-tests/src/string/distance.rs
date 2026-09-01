//! Oracle parity tests for `string-distance`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_prop_string_distance_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 0""#]];
    // identical
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(string-distance "kitten" "kitten")"#,
        expect,
    );
    assert_ok_eq("0", &o, &n);

    let expect = expect_test::expect![[r#""OK 1""#]];
    // single substitution
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-distance "cat" "bat")"#, expect);
    assert_ok_eq("1", &o, &n);

    let expect = expect_test::expect![[r#""OK 3""#]];
    // classic levenshtein example
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(string-distance "kitten" "sitting")"#,
        expect,
    );
    assert_ok_eq("3", &o, &n);

    let expect = expect_test::expect![[r#""OK 1""#]];
    // insertion
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-distance "abc" "abcd")"#, expect);
    assert_ok_eq("1", &o, &n);

    let expect = expect_test::expect![[r#""OK 1""#]];
    // deletion
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-distance "abcd" "abc")"#, expect);
    assert_ok_eq("1", &o, &n);

    let expect = expect_test::expect![[r#""OK 4""#]];
    // empty vs non-empty
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-distance "" "test")"#, expect);
    assert_ok_eq("4", &o, &n);

    let expect = expect_test::expect![[r#""OK 0""#]];
    // both empty
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(string-distance "" "")"#, expect);
    assert_ok_eq("0", &o, &n);

    let expect = expect_test::expect![[r#""OK 3""#]];
    // completely different
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-distance "abc" "xyz")"#, expect);
    assert_ok_eq("3", &o, &n);

    let expect = expect_test::expect![[r#""OK 1""#]];
    // byte length mode
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-distance "abc" "axc" t)"#, expect);
    assert_ok_eq("1", &o, &n);
}
