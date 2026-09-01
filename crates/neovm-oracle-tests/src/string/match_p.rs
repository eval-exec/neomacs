//! Oracle parity tests for `string-match-p` (non-modifying match).

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

#[test]
fn oracle_prop_string_match_p_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 0""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-match-p "foo" "foobar")"#, expect);
    assert_ok_eq("0", &o, &n);

    let expect = expect_test::expect![[r#""OK 3""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-match-p "bar" "foobar")"#, expect);
    assert_ok_eq("3", &o, &n);
}

#[test]
fn oracle_prop_string_match_p_no_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-match-p "xyz" "foobar")"#, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_prop_string_match_p_with_start() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 2""#]];
    // Start searching from position 3
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-match-p "o" "foobar" 2)"#, expect);
    assert_ok_eq("2", &o, &n);

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(string-match-p "o" "foobar" 3)"#, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_prop_string_match_p_regex() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 0""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(string-match-p "^[0-9]+$" "12345")"#,
        expect,
    );
    assert_ok_eq("0", &o, &n);

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(string-match-p "^[0-9]+$" "123abc")"#,
        expect,
    );
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_prop_string_match_p_does_not_modify_match_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // string-match-p should NOT modify match data
    let form = r####"(progn
                    (string-match "\\(foo\\)" "foobar")
                    (let ((before (match-beginning 1)))
                      (string-match-p "bar" "xyzbar")
                      (let ((after (match-beginning 1)))
                        (list before after (= before after)))))"####;
    let expect = expect_test::expect![[r#""OK (0 0 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_string_match_p_character_classes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(string-match-p "[[:alpha:]]+" "hello")"#,
        expect,
    );
    let expect = expect_test::expect![[r#""OK 3""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(string-match-p "[[:digit:]]+" "abc123")"#,
        expect,
    );
    let expect = expect_test::expect![[r#""OK 5""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(string-match-p "[[:space:]]" "hello world")"#,
        expect,
    );
}

#[test]
fn oracle_prop_string_match_p_in_conditional() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Common pattern: use string-match-p as a predicate
    let form = r####"(mapcar (lambda (s)
                            (if (string-match-p "^test-" s) 'test 'other))
                          '("test-foo" "hello" "test-bar" "world"))"####;
    let expect = expect_test::expect![[r#""OK (test other test other)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
