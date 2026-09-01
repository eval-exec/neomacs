//! Oracle parity tests for `funcall`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_prop_funcall_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 49""#]];
    // simple lambda
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect("(funcall (lambda (x) (* x x)) 7)", expect);
    assert_ok_eq("49", &o, &n);

    let expect = expect_test::expect![[r#""OK 42""#]];
    // no-arg lambda
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(funcall (lambda () 42))", expect);
    assert_ok_eq("42", &o, &n);

    let expect = expect_test::expect![[r#""OK 60""#]];
    // multiple args
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        "(funcall (lambda (a b c) (+ a b c)) 10 20 30)",
        expect,
    );
    assert_ok_eq("60", &o, &n);

    let expect = expect_test::expect![[r#""OK 5""#]];
    // optional args
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        "(funcall (lambda (x &optional y) (if y (+ x y) x)) 5)",
        expect,
    );
    assert_ok_eq("5", &o, &n);

    let expect = expect_test::expect![[r#""OK 8""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        "(funcall (lambda (x &optional y) (if y (+ x y) x)) 5 3)",
        expect,
    );
    assert_ok_eq("8", &o, &n);

    let expect = expect_test::expect![[r#""OK 4""#]];
    // rest args
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        "(funcall (lambda (&rest xs) (length xs)) 1 2 3 4)",
        expect,
    );
    assert_ok_eq("4", &o, &n);

    let expect = expect_test::expect![[r#""OK 300""#]];
    // funcall with named function
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(funcall '+ 100 200)", expect);
    assert_ok_eq("300", &o, &n);

    let expect = expect_test::expect![[r#""OK first""#]];
    // funcall with sharp-quote
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect("(funcall #'car '(first second))", expect);
    assert_ok_eq("first", &o, &n);

    let expect = expect_test::expect![[r#""OK 15""#]];
    // closure capturing lexical binding
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        "(let ((offset 10)) (funcall (lambda (n) (+ n offset)) 5))",
        expect,
    );
    assert_ok_eq("15", &o, &n);
}
