use super::super::*;
use super::assert_gnu;

#[test]
fn ntake_dotted() {
    assert_gnu(
        "ntake_dotted",
        r#"(list (condition-case e (ntake 2 (cons 1 2)) (error e)) (condition-case e (ntake 4 (cons 1 (cons 2 (cons 3 4)))) (error e)) (condition-case e (take 2 (cons 1 2)) (error e)) (ntake 1 (cons 1 2)) (condition-case e (ntake 2 5) (error e)))"#,
        include_str!("ntake_dotted.expect"),
    );
}

#[test]
fn ntake_error_retains_original_dotted_list() {
    crate::test_utils::init_test_tracing();
    let head = Value::cons(Value::fixnum(1), Value::fixnum(2));
    let error = builtin_ntake(vec![Value::fixnum(2), head]).expect_err("dotted list");
    let FlowKind::Signal(signal) = error.into_kind() else {
        panic!("ntake must signal");
    };
    assert_eq!(signal.data, vec![Value::symbol("listp"), head]);
    assert_eq!(head.cons_cdr(), Value::fixnum(2));
}
