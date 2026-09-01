//! Oracle parity tests for `substring`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{
    assert_err_kind, assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm, run_neovm_eval,
};

#[test]
fn oracle_prop_substring_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"hello\"""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(substring "hello world" 0 5)"#, expect);
    assert_ok_eq(r#""hello""#, &o, &n);

    let expect = expect_test::expect![[r#""OK \"world\"""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(substring "hello world" 6)"#, expect);
    assert_ok_eq(r#""world""#, &o, &n);
}

#[test]
fn oracle_prop_substring_full() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"hello\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(substring "hello" 0)"#, expect);
    assert_ok_eq(r#""hello""#, &o, &n);
}

#[test]
fn oracle_prop_substring_empty_result() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(substring "hello" 3 3)"#, expect);
    assert_ok_eq(r#""""#, &o, &n);
}

#[test]
fn oracle_prop_substring_negative_index() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"world\"""#]];
    // Negative indices count from end
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(substring "hello world" -5)"#, expect);
    assert_ok_eq(r#""world""#, &o, &n);

    let expect = expect_test::expect![[r#""OK \"ll\"""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect(r#"(substring "hello" -3 -1)"#, expect);
    assert_ok_eq(r#""ll""#, &o, &n);
}

#[test]
fn oracle_prop_substring_single_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"e\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(substring "hello" 1 2)"#, expect);
    assert_ok_eq(r#""e""#, &o, &n);
}

#[test]
fn oracle_prop_substring_out_of_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(substring "hello" 0 100)"####;
    let (oracle, neovm) = eval_oracle_and_neovm(form);
    assert_err_kind(&oracle, &neovm, "args-out-of-range");
}

#[test]
fn oracle_prop_substring_with_concat() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(concat (substring "hello" 0 2) (substring "world" 3))"####;
    let expect = expect_test::expect![[r#""OK \"held\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq(r#""held""#, &o, &n);
}

#[test]
fn oracle_prop_substring_on_empty_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(substring "" 0)"#, expect);
    assert_ok_eq(r#""""#, &o, &n);
}

#[test]
fn oracle_substring_rejects_non_vector_arraylikes_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/fns.c:Fsubstring uses CHECK_VECTOR_OR_STRING: strings and
    // ordinary vectors are accepted, but records, byte-code objects,
    // bool-vectors, and char-tables are rejected with `arrayp`.
    // Fsubstring_no_properties starts with CHECK_STRING.
    let form = r#"
(list
 (substring [1 2 3] 0 2)
 (condition-case err
     (substring (record 'neovm--substring-record 1 2) 0 1)
   (error (list (car err) (cdr err))))
 (condition-case err
     (substring #[257 "\300\207" [42] 1] 0 1)
   (error (list (car err) (cdr err))))
 (condition-case err
     (substring (make-bool-vector 3 t) 0 1)
   (error (list (car err) (cdr err))))
 (condition-case err
     (substring (make-char-table 'generic 65) 0 1)
   (error (list (car err) (cdr err))))
 (condition-case err
     (substring-no-properties [1 2 3] 0 1)
   (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK ([1 2] (wrong-type-argument (arrayp #s(neovm--substring-record 1 2))) (wrong-type-argument (arrayp #[257 \"��\" [42] 1])) (wrong-type-argument (arrayp #&3\"\u{7}\")) (wrong-type-argument (arrayp #^[65 nil generic 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65])) (wrong-type-argument (stringp [1 2 3])))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
