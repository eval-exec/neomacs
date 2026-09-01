//! Oracle parity tests for string primitives.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{ORACLE_PROP_CASES, assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_prop_stringp_wrong_arity_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments stringp 0)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(stringp)", expect);
    assert_err_kind(&oracle, &neovm, "wrong-number-of-arguments");
}

#[test]
fn oracle_prop_concat_wrong_type_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument sequencep 1)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(r#"(concat "a" 1)"#, expect);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}

#[test]
fn oracle_prop_substring_out_of_range_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range \"abc\" 10 nil)""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect(r#"(substring "abc" 10)"#, expect);
    assert_err_kind(&oracle, &neovm, "args-out-of-range");
}

#[test]
fn oracle_prop_string_wrong_type_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument characterp \"a\")""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(r#"(string "a")"#, expect);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}

#[test]
fn oracle_string_rejects_character_above_gnu_max_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU Emacs src/character.c:Fstring validates every argument with
    // CHECK_CHARACTER before building the string.  The character range ends at
    // #x3fffff, so #x400000 must signal `characterp`.
    let form = r#"
(list
 (length (string #x3fffff))
 (condition-case err
     (string #x400000)
   (error (list (car err) (cdr err)))))
"#;
    let expect = expect_test::expect![[r#""OK (1 (wrong-type-argument (characterp 4194304)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_stringp_operator(
        s in proptest::string::string_regex(r"[A-Za-z0-9 _-]{0,24}").expect("regex should compile"),
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!("(stringp {:?})", s);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq("t", &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_concat_operator(
        a in proptest::string::string_regex(r"[A-Za-z0-9 _-]{0,16}").expect("regex should compile"),
        b in proptest::string::string_regex(r"[A-Za-z0-9 _-]{0,16}").expect("regex should compile"),
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!("(concat {:?} {:?})", a, b);
        let expected = format!("{:?}", format!("{a}{b}"));
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_substring_operator(
        len in 0usize..24usize,
        start in 0usize..24usize,
        end in 0usize..24usize,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));
        prop_assume!(start <= end && end <= len);

        let source = "a".repeat(len);
        let form = format!("(substring {:?} {} {})", source, start, end);
        let expected = format!("{:?}", &source[start..end]);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_length_string_operator(
        s in proptest::string::string_regex(r"[A-Za-z0-9 _-]{0,24}").expect("regex should compile"),
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let expected_len = s.len();
        let form = format!("(length {:?})", s);
        let expected = expected_len.to_string();
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_string_operator(
        chars in prop::collection::vec(97u8..123u8, 0usize..24usize),
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let args = chars
            .iter()
            .map(|c| (*c as i64).to_string())
            .collect::<Vec<_>>()
            .join(" ");
        let form = if args.is_empty() {
            "(string)".to_string()
        } else {
            format!("(string {args})")
        };

        let expected_string: String = chars.iter().map(|c| char::from(*c)).collect();
        let expected = format!("{expected_string:?}");
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }
}
