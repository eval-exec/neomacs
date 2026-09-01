//! Oracle parity tests for `copy-alist`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

#[test]
fn oracle_prop_copy_alist_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((a . 1) (b . 2) (c . 3))""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        "(copy-alist '((a . 1) (b . 2) (c . 3)))",
        expect,
    );
    assert_ok_eq("((a . 1) (b . 2) (c . 3))", &o, &n);

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(copy-alist nil)", expect);
    assert_ok_eq("nil", &o, &n);

    let expect = expect_test::expect![[r#""OK ((x . 10))""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(copy-alist '((x . 10)))", expect);
    assert_ok_eq("((x . 10))", &o, &n);

    let expect = expect_test::expect![[r#""OK 1""#]];
    // verify it's a distinct copy (setcdr on copy doesn't affect original)
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        "(let* ((orig '((k . 1))) (cp (copy-alist orig))) (setcdr (car cp) 99) (cdar orig))",
        expect,
    );
    assert_ok_eq("1", &o, &n);
}

#[test]
fn oracle_copy_alist_improper_tail_error_payload() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/fns.c:Fcopy_alist first CHECK_LISTs the alist, so dotted alists
    // signal with the offending tail.  Non-cons alist elements are otherwise
    // legal and copied as shared elements.
    let form = r#"
(list
 (copy-alist '(loose (a . 1) atom (b . 2)))
 (condition-case err
     (copy-alist '(loose (a . 1) . tail))
   (error (list (car err) (cdr err))))
 (condition-case err
     (copy-alist 42)
   (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((loose (a . 1) atom (b . 2)) (wrong-type-argument (listp tail)) (wrong-type-argument (listp 42)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
