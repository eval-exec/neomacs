use super::super::*;
use super::assert_gnu;

#[test]
fn constructor_limits() {
    assert_gnu(
        "constructor_limits",
        r#"(let ((memory-signal-data '(error "allocation exhausted"))) (list (condition-case e (make-vector most-positive-fixnum 0) (error e)) (condition-case e (make-string most-positive-fixnum ?a) (error e)) (condition-case e (make-string most-positive-fixnum ?é) (error e)) (condition-case e (make-bool-vector most-positive-fixnum t) (error e)) (make-vector 0 7) (make-string 3 ?é) (length (make-bool-vector 65 t))))"#,
        include_str!("constructor_limits.expect"),
    );
}

#[test]
fn constructors_report_failed_storage_as_lisp_conditions() {
    crate::test_utils::init_test_tracing();
    let huge = Value::fixnum(Value::MOST_POSITIVE_FIXNUM);
    for result in [
        builtin_make_vector(vec![huge, Value::NIL]),
        builtin_make_string(vec![huge, Value::fixnum(97)]),
        crate::emacs_core::boolvec::builtin_make_bool_vector(vec![huge, Value::T]),
    ] {
        let FlowKind::Signal(signal) = result.expect_err("huge allocation").into_kind() else {
            panic!("allocation must signal");
        };
        assert_eq!(signal.symbol_name(), "error");
    }
    let error = builtin_make_string(vec![huge, Value::fixnum(233)])
        .expect_err("multibyte byte extent overflows GNU string bound");
    let FlowKind::Signal(signal) = error.into_kind() else {
        panic!("string overflow must signal");
    };
    assert_eq!(signal.symbol_name(), "error");
    assert_eq!(
        signal.data,
        vec![Value::string("Maximum string size exceeded")]
    );
}
