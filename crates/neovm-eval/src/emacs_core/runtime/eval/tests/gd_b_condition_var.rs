//! GNU condition-case VAR bindings ignore both global and local special declarations.
//! Threading: every test owns its evaluator and lexical environment on its mutator.

use super::{Context, format_eval_result, runtime_startup_context};

fn eval_condition_var(source: &str, lexical: bool) -> String {
    crate::test_utils::init_test_tracing();
    let mut context = Context::new();
    context.set_lexical_binding(lexical);
    let specpdl_base = context.specpdl.len();
    let result = format_eval_result(&context.eval_str(source));
    assert_eq!(context.specpdl.len(), specpdl_base);
    result
}

#[test]
fn special_error_var_is_lexical_and_closure_survives_handler() {
    assert_eq!(
        eval_condition_var(
            r#"(progn
  (defvar gd-b5-special 'global)
  (let ((reader
         (condition-case gd-b5-special (signal 'error '(bad))
           (error
            (setq gd-b5-special (list 'caught (car gd-b5-special)))
            (lambda () (list gd-b5-special (symbol-value 'gd-b5-special)))))))
    (list (funcall reader) gd-b5-special)))"#,
            true,
        ),
        "OK (((caught error) global) global)"
    );
}

#[test]
fn special_success_var_is_lexical_and_restores_outer_dynamic_binding() {
    assert_eq!(
        eval_condition_var(
            r#"(progn
  (defvar gd-b5-special 'global)
  (let ((gd-b5-special 'outer))
    (list
     (condition-case gd-b5-special 42
       (:success
        (list gd-b5-special (symbol-value 'gd-b5-special)
              (funcall (lambda () gd-b5-special)))))
     gd-b5-special)))"#,
            true,
        ),
        "OK ((42 outer 42) outer)"
    );
}

#[test]
fn locally_declared_special_var_is_lexical_in_error_and_success_handlers() {
    assert_eq!(
        eval_condition_var(
            r#"(progn
  (set 'gd-b5-local 'global)
  (let ((gd-b5-local 'outer))
    (defvar gd-b5-local)
    (list
     (condition-case gd-b5-local (signal 'error '(bad))
       (error (list gd-b5-local (symbol-value 'gd-b5-local))))
     (condition-case gd-b5-local 42
       (:success (list gd-b5-local (symbol-value 'gd-b5-local))))
     gd-b5-local)))"#,
            true,
        ),
        "OK (((error bad) global) (42 global) outer)"
    );
}

#[test]
fn dynamically_scoped_condition_var_updates_dynamic_cell_only_during_handler() {
    assert_eq!(
        eval_condition_var(
            r#"(progn
  (defvar gd-b5-special 'global)
  (list
   (condition-case gd-b5-special (signal 'error '(bad))
     (error (list gd-b5-special (symbol-value 'gd-b5-special))))
   (condition-case gd-b5-special 42
     (:success (list gd-b5-special (symbol-value 'gd-b5-special))))
   gd-b5-special))"#,
            false,
        ),
        "OK (((error bad) (error bad)) (42 42) global)"
    );
}

#[test]
fn special_condition_var_lexical_binding_is_restored_after_handler_throw() {
    assert_eq!(
        eval_condition_var(
            r#"(progn
  (defvar gd-b5-special 'global)
  (let ((gd-b5-special 'outer))
    (list
     (catch 'gd-b5-exit
       (condition-case gd-b5-special (signal 'error '(bad))
         (error
          (throw 'gd-b5-exit
                 (list gd-b5-special (symbol-value 'gd-b5-special))))))
     (catch 'gd-b5-exit
       (condition-case gd-b5-special 42
         (:success
          (throw 'gd-b5-exit
                 (list gd-b5-special (symbol-value 'gd-b5-special))))))
     gd-b5-special)))"#,
            true,
        ),
        "OK (((error bad) outer) (42 outer) outer)"
    );
}

#[test]
fn compiled_special_condition_var_closures_survive_both_handler_kinds() {
    crate::test_utils::init_test_tracing();
    let mut context = runtime_startup_context();
    let specpdl_base = context.specpdl.len();
    let result = context.eval_str(
        r#"(progn
  (defvar gd-b5-special 'global)
  (defalias 'gd-b5-maker
    (eval '(byte-compile
            (lambda (fail)
              (condition-case gd-b5-special
                  (if fail (signal 'error '(bad)) 42)
                (error
                 (setq gd-b5-special (list 'caught (car gd-b5-special)))
                 (lambda ()
                   (list gd-b5-special (symbol-value 'gd-b5-special))))
                (:success
                 (lambda ()
                   (list gd-b5-special (symbol-value 'gd-b5-special)))))))
          t))
  (let ((error-reader (gd-b5-maker t))
        (success-reader (gd-b5-maker nil)))
    (garbage-collect)
    (list (funcall error-reader) (funcall success-reader) gd-b5-special)))"#,
    );
    assert_eq!(
        format_eval_result(&result),
        "OK (((caught error) global) (42 global) global)"
    );
    assert_eq!(context.specpdl.len(), specpdl_base);
}
