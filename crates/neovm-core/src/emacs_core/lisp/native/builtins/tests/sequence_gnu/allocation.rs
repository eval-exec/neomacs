use super::super::*;
use super::assert_gnu;

#[test]
fn constructor_limits() {
    assert_gnu(
        "constructor_limits",
        r#"
(list
(let ((memory-signal-data '(error "allocation exhausted"))) (list (condition-case e (make-vector most-positive-fixnum 0) (error e)) (condition-case e (make-string most-positive-fixnum ?a) (error e)) (condition-case e (make-string most-positive-fixnum ?é) (error e)) (condition-case e (make-bool-vector most-positive-fixnum t) (error e)) (make-vector 0 7) (make-string 3 ?é) (length (make-bool-vector 65 t))))
(with-temp-buffer
  (make-local-variable 'memory-signal-data)
  (setq memory-signal-data '(error "buffer-local exhausted"))
  (list (condition-case e (make-vector most-positive-fixnum 0) (error e))
        (condition-case e (make-string most-positive-fixnum ?a) (error e))
        (condition-case e (make-bool-vector most-positive-fixnum t) (error e))))
(let* ((memory-signal-data '(error . gdl-oom-tail))
       (gdl-oom-hook-count 0)
       (gdl-oom-debugger-count 0)
       (debug-on-error t)
       (debug-on-signal t)
       (debugger (lambda (&rest _) (setq gdl-oom-debugger-count (1+ gdl-oom-debugger-count))))
       (signal-hook-function (lambda (&rest _) (setq gdl-oom-hook-count (1+ gdl-oom-hook-count)))))
   (list
    (condition-case e (make-vector most-positive-fixnum 0)
      (error (list e (eq e memory-signal-data))))
    (condition-case e (make-string most-positive-fixnum ?a)
      (error (list e (eq e memory-signal-data))))
    (condition-case e (make-bool-vector most-positive-fixnum t)
      (error (list e (eq e memory-signal-data))))
    gdl-oom-hook-count gdl-oom-debugger-count
    (condition-case e (signal 'error '(gdl-control)) (error e))
    gdl-oom-hook-count gdl-oom-debugger-count))
(let* ((memory-signal-data '(gdl-oom-undefined-condition . gdl-tail))
       (gdl-oom-hooks 0)
       (gdl-oom-debuggers 0)
       (internal-when-entered-debugger -1)
       (debug-on-error t)
       (debug-on-signal t)
       (debugger (lambda (&rest _) (setq gdl-oom-debuggers (1+ gdl-oom-debuggers))))
       (signal-hook-function (lambda (&rest _) (setq gdl-oom-hooks (1+ gdl-oom-hooks)))))
  (list (condition-case e (make-vector most-positive-fixnum nil)
          (error (list e (eq e memory-signal-data))))
        gdl-oom-hooks gdl-oom-debuggers))
(let ((out nil))
  (dolist (datum '(nil t "oom" (17 . gdl-tail)))
    (let* ((memory-signal-data datum)
           (gdl-malformed-hook-count 0)
           (signal-hook-function
            (lambda (&rest _) (setq gdl-malformed-hook-count (1+ gdl-malformed-hook-count)))))
      (push (list datum (condition-case e (make-vector most-positive-fixnum nil) (error e))
                  gdl-malformed-hook-count) out)))
  (nreverse out))
)
"#,
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
