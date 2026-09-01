//! Oracle parity tests for `reverse` and `nreverse`.
//!
//! GNU implements both primitives in `src/fns.c`.  `reverse` returns a
//! reversed copy for supported sequences, while `nreverse` destructively
//! reverses lists, vectors, and bool-vectors but returns a reversed copy for
//! strings.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{ORACLE_PROP_CASES, assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

#[test]
fn oracle_prop_reverse_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (5 4 3 2 1)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(reverse '(1 2 3 4 5))", expect);
    assert_ok_eq("(5 4 3 2 1)", &o, &n);
}

#[test]
fn oracle_prop_reverse_empty() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(reverse nil)", expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_prop_reverse_single() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (42)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(reverse '(42))", expect);
    assert_ok_eq("(42)", &o, &n);
}

#[test]
fn oracle_prop_reverse_does_not_mutate_original() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((lst '(1 2 3)))
                  (reverse lst)
                  lst)";
    let expect = expect_test::expect![[r#""OK (1 2 3)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(1 2 3)", &o, &n);
}

#[test]
fn oracle_prop_reverse_nested_lists() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((5 6) (3 4) (1 2))""#]];
    // reverse should only reverse top-level, not recurse
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect("(reverse '((1 2) (3 4) (5 6)))", expect);
    assert_ok_eq("((5 6) (3 4) (1 2))", &o, &n);
}

#[test]
fn oracle_prop_reverse_mixed_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(reverse (list 1 "two" 'three 4.0 '(five)))"####;
    let expect = expect_test::expect![[r#""OK ((five) 4.0 three \"two\" 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_reverse_double_reversal_identity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        "(equal '(1 2 3 4 5) (reverse (reverse '(1 2 3 4 5))))",
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_prop_reverse_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"olleh\"""#]];
    // reverse also works on strings
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(reverse "hello")"#, expect);
    assert_ok_eq(r#""olleh""#, &o, &n);
}

#[test]
fn oracle_prop_reverse_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK [4 3 2 1]""#]];
    // reverse works on vectors too
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(reverse [1 2 3 4])", expect);
    assert_ok_eq("[4 3 2 1]", &o, &n);
}

#[test]
fn oracle_reverse_copies_supported_sequences_without_mutating_originals() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((list (list 'a (list 'b) 'c))
       (vec (vector 'a (list 'b) 'c))
       (str "ab\u00e9")
       (boolv (bool-vector t nil t nil))
       (rlist (reverse list))
       (rvec (reverse vec))
       (rstr (reverse str))
       (rboolv (reverse boolv)))
  (list
   list rlist (eq list rlist) (eq (cadr list) (cadr rlist))
   vec rvec (eq vec rvec) (eq (aref vec 1) (aref rvec 1))
   str rstr (eq str rstr)
   boolv rboolv (eq boolv rboolv)))
"#;

    let expect = expect_test::expect![[
        r#""OK ((a (b) c) (c (b) a) nil t [a (b) c] [c (b) a] nil t \"abé\" \"éba\" nil #&4\"\u{5}\" #&4\"\\n\" nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_nreverse_mutates_lists_vectors_and_bool_vectors_but_copies_strings() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((list (list 'a 'b 'c))
       (first list)
       (second (cdr list))
       (vec (vector 'a 'b 'c))
       (boolv (bool-vector t nil t nil))
       (str "ab\u00e9")
       (rlist (nreverse list))
       (rvec (nreverse vec))
       (rboolv (nreverse boolv))
       (rstr (nreverse str)))
  (list
   rlist list
   (eq first (last rlist))
   (eq second (cdr (last rlist)))
   vec rvec (eq vec rvec)
   boolv rboolv (eq boolv rboolv)
   str rstr (eq str rstr)))
"#;

    let expect = expect_test::expect![[
        r#""OK ((c b a) (a) t nil [c b a] [c b a] t #&4\"\\n\" #&4\"\\n\" t \"abé\" \"éba\" nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_reverse_and_nreverse_improper_list_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (condition-case err
     (reverse (cons 'a 'tail))
   (error err))
 (condition-case err
     (nreverse (cons 'a 'tail))
   (error err)))
"#;

    let expect = expect_test::expect![[
        r#""OK ((wrong-type-argument listp tail) (wrong-type-argument listp (a)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_nreverse_self_circular_list_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((x (list 'a)))
  (setcdr x x)
  (condition-case err
      (nreverse x)
    (error (let ((arg (cadr err)))
             (list (car err)
                   (eq arg x)
                   (listp arg)
                   (car arg))))))
"#;

    let expect = expect_test::expect![[r#""OK (circular-list t t a)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_reverse_preserves_length(
        a in -100i64..100i64,
        b in -100i64..100i64,
        c in -100i64..100i64,
        d in -100i64..100i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(length (reverse '({} {} {} {})))",
            a, b, c, d
        );
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        prop_assert_eq!(neovm.as_str(), oracle.as_str());
        prop_assert_eq!(neovm.as_str(), "OK 4");
    }
}
