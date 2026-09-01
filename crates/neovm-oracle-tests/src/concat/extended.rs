//! Oracle parity tests for `concat` extended patterns.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{ORACLE_PROP_CASES, assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

#[test]
fn oracle_prop_concat_strings() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"hello world\"""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(concat "hello" " " "world")"#, expect);
    assert_ok_eq(r#""hello world""#, &o, &n);
}

#[test]
fn oracle_prop_concat_empty() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(concat)", expect);
    assert_ok_eq(r#""""#, &o, &n);
}

#[test]
fn oracle_prop_concat_single() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"only\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(concat "only")"#, expect);
    assert_ok_eq(r#""only""#, &o, &n);
}

#[test]
fn oracle_prop_concat_many() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"abcdef\"""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(concat "a" "b" "c" "d" "e" "f")"#, expect);
    assert_ok_eq(r#""abcdef""#, &o, &n);
}

#[test]
fn oracle_prop_concat_rejects_bool_vector_as_sequence_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU fns.c:concat_to_string accepts strings, vectors, nil, and conses,
    // but not bool-vectors.  Bool-vectors are accepted by append/vconcat, so
    // this is an observable concat-specific sequence predicate boundary.
    let form = r#"(let ((bv (make-bool-vector 3 nil)))
                    (aset bv 1 t)
                    (concat bv))"#;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument sequencep #&3\"\u{2}\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_concat_with_empty_strings() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"hi!\"""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(concat "" "hi" "" "!" "")"#, expect);
    assert_ok_eq(r#""hi!""#, &o, &n);
}

#[test]
fn oracle_prop_concat_with_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(concat (format "%d" 42) "-" (format "%s" "hello"))"####;
    let expect = expect_test::expect![[r#""OK \"42-hello\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq(r#""42-hello""#, &o, &n);
}

#[test]
fn oracle_prop_concat_with_number_to_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(concat "[" (number-to-string 42) "]")"####;
    let expect = expect_test::expect![[r#""OK \"[42]\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq(r#""[42]""#, &o, &n);
}

#[test]
fn oracle_prop_concat_in_loop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(let ((result ""))
                    (dotimes (i 5)
                      (setq result (concat result (number-to-string i))))
                    result)"####;
    let expect = expect_test::expect![[r#""OK \"01234\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq(r#""01234""#, &o, &n);
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_concat_lengths_add(
        a_len in 0usize..10usize,
        b_len in 0usize..10usize,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(length (concat (make-string {} ?a) (make-string {} ?b)))",
            a_len, b_len
        );
        let expected = format!("OK {}", a_len + b_len);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        prop_assert_eq!(neovm.as_str(), expected.as_str());
        prop_assert_eq!(oracle.as_str(), expected.as_str());
    }
}
