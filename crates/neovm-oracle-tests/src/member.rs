//! Oracle parity tests for `member`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{
    ORACLE_PROP_CASES, assert_err_kind, assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm,
};

#[test]
fn oracle_prop_member_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (2 3)""#]];
    let (oracle_found, neovm_found) =
        crate::common::eval_oracle_and_neovm_expect("(member 2 '(1 2 3))", expect);
    assert_ok_eq("(2 3)", &oracle_found, &neovm_found);

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle_missing, neovm_missing) =
        crate::common::eval_oracle_and_neovm_expect("(member 9 '(1 2 3))", expect);
    assert_ok_eq("nil", &oracle_missing, &neovm_missing);
}

#[test]
fn oracle_prop_member_wrong_type_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp 2)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(member 1 2)", expect);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}

#[test]
fn oracle_member_ignore_case_ignores_non_strings_and_returns_tail() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU lisp/subr.el:member-ignore-case is a Lisp loop over LIST.  It skips
    // non-string elements, returns the original list tail at the first
    // string-equal-ignore-case match, and only signals if cdr traversal reaches
    // a malformed tail before any match.
    let form = r#"
(list
 (member-ignore-case "foo" '(1 "bar" foo "FoO" "later"))
 (member-ignore-case "foo" '(1 foo "FoO" . bad-tail))
 (condition-case err
     (member-ignore-case "missing" '(1 "bar" foo . bad-tail))
   (error (list (car err) (cdr err))))
 (condition-case err
     (member-ignore-case 'foo '("foo"))
   (error (list (car err) (cdr err)))))
"#;
    let expect = expect_test::expect![[
        r#""OK ((\"FoO\" \"later\") (\"FoO\" . bad-tail) (wrong-type-argument (listp bad-tail)) (wrong-type-argument (stringp foo)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_member_ignore_case_strict_edge_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU lisp/subr.el:member-ignore-case tests (stringp (car list)) before
    // calling string-equal-ignore-case.  Bad ELT values are therefore delayed
    // until a string element is encountered, and text properties do not affect
    // the string comparison result.
    let form = r#"
(let ((capture (lambda (form)
                 (condition-case err
                     (eval form)
                   (error (cons (car err) (cdr err)))))))
  (list
   (let ((s (copy-sequence "FoO")))
     (add-text-properties 0 3 '(face bold) s)
     (member-ignore-case "foo" (list 1 s "later")))
   (member-ignore-case "" '(nil "" "later"))
   (member-ignore-case 'bad '(1 nil bad))
   (funcall capture '(member-ignore-case 'bad '(1 nil "bad")))
   (funcall capture '(member-ignore-case "missing" '(1 nil . bad-tail)))
   (funcall capture '(member-ignore-case "x" 42))
   (funcall capture '(member-ignore-case))
   (funcall capture '(member-ignore-case "x" nil 'extra))))
"#;
    let expect = expect_test::expect![[
        r#""OK ((#(\"FoO\" 0 3 (face bold)) \"later\") (\"\" \"later\") nil (wrong-type-argument stringp bad) (wrong-type-argument listp bad-tail) (wrong-type-argument listp 42) (wrong-number-of-arguments (2 . 2) 0) (wrong-number-of-arguments (2 . 2) 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_member_returns_tail(
        a in -100_000i64..100_000i64,
        b in -100_000i64..100_000i64,
        c in -100_000i64..100_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));
        prop_assume!(a != b);

        let form = format!("(member {} (list {} {} {}))", a, b, a, c);
        let expected = format!("({} {})", a, c);
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }
}
