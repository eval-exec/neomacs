use super::*;
use crate::emacs_core::builtins::symbols::{builtin_set, builtin_symbol_value};
use crate::emacs_core::intern::intern_uninterned;

fn test_obarray() -> crate::emacs_core::symbol::Obarray {
    crate::emacs_core::symbol::Obarray::new()
}

fn eval_all(source: &str) -> Vec<String> {
    let mut ctx = Context::new();
    let forms = crate::emacs_core::value_reader::read_all(source, &test_obarray()).expect("parse");
    let roots = ctx.save_specpdl_roots();
    for form in &forms {
        ctx.push_specpdl_root(*form);
    }
    let results = forms
        .iter()
        .map(|form| crate::emacs_core::format_eval_result(&ctx.eval_form(*form)))
        .collect();
    ctx.restore_specpdl_roots(roots);
    results
}

fn eval_one(source: &str) -> String {
    let mut ctx = Context::new();
    crate::emacs_core::format_eval_result(&ctx.eval_str(source))
}

#[test]
fn default_value_returns_global() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(r#"(defvar my-var 42) (default-value 'my-var)"#);
    assert_eq!(results[1], "OK 42");
}

#[test]
fn default_value_void_signals_error() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(r#"(default-value 'nonexistent-var)"#);
    assert!(results[0].starts_with("ERR"));
}

#[test]
fn keyword_defaults_and_symbol_values_self_evaluate() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(list (default-value :foo) (default-toplevel-value :foo) (symbol-value :foo))"#,
    );
    assert_eq!(results[0], "OK (:foo :foo :foo)");
}

#[test]
fn uninterned_keyword_defaults_do_not_self_evaluate() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(let ((s (make-symbol ":vm-k")))
             (list (condition-case e (eval s nil) (error (car e)))
                   (condition-case e (symbol-value s) (error (car e)))
                   (condition-case e (default-value s) (error (car e)))))"#,
    );
    assert_eq!(results[0], "OK (void-variable void-variable void-variable)");
}

#[test]
fn uninterned_value_cells_ignore_buffer_local_namesakes() {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new();
    let canonical = intern("depth-alist");
    let uninterned = intern_uninterned("depth-alist");
    ctx.buffers
        .current_buffer_mut()
        .expect("current buffer")
        .set_buffer_local("depth-alist", Value::fixnum(7));

    builtin_set(&mut ctx, vec![Value::symbol(uninterned), Value::NIL])
        .expect("set should bind uninterned symbol");

    assert_eq!(
        ctx.obarray().symbol_value_id(uninterned).copied(),
        Some(Value::NIL)
    );
    assert_eq!(ctx.obarray().symbol_value_id(canonical).copied(), None);
    assert_eq!(
        ctx.buffers
            .current_buffer()
            .expect("current buffer")
            .get_buffer_local("depth-alist"),
        Some(Value::fixnum(7))
    );

    let value = default_value(&mut ctx, vec![Value::symbol(uninterned)])
        .expect("default-value should read uninterned symbol");
    assert_eq!(value, Value::NIL);
    let symbol_value = builtin_symbol_value(&mut ctx, vec![Value::symbol(uninterned)])
        .expect("symbol-value should read uninterned symbol");
    assert_eq!(symbol_value, Value::NIL);
}

#[test]
fn set_default_sets_global() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(r#"(set-default 'my-var 99) (default-value 'my-var)"#);
    assert_eq!(results[1], "OK 99");
}

#[test]
fn set_default_preserves_current_buffer_local_binding() {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new();
    let current = ctx.buffers.current_buffer_id().expect("current buffer");
    ctx.set_buffer_local_binding_by_id(current, intern("vm-set-default-local"), Value::fixnum(7))
        .expect("buffer-local binding");

    set_default(
        &mut ctx,
        vec![Value::symbol("vm-set-default-local"), Value::fixnum(99)],
    )
    .expect("set-default");

    assert_eq!(
        ctx.buffers
            .current_buffer()
            .expect("current buffer")
            .buffer_local_value("vm-set-default-local"),
        Some(Value::fixnum(7))
    );
    assert_eq!(
        default_value(&mut ctx, vec![Value::symbol("vm-set-default-local")])
            .expect("default-value"),
        Value::fixnum(99)
    );
    assert_eq!(
        builtin_symbol_value(&mut ctx, vec![Value::symbol("vm-set-default-local")])
            .expect("symbol-value"),
        Value::fixnum(7)
    );
}

#[test]
fn set_default_and_default_value_follow_alias_resolution() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(defvaralias 'vm-set-default-alias 'vm-set-default-base)
           (set-default 'vm-set-default-alias 5)
           (list (default-value 'vm-set-default-base)
                 (default-value 'vm-set-default-alias))"#,
    );
    assert_eq!(results[2], "OK (5 5)");
}

#[test]
fn default_value_alias_void_uses_original_symbol_in_error_payload() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(defvaralias 'vm-default-alias-unbound 'vm-default-base-unbound)
           (condition-case err
               (default-value 'vm-default-alias-unbound)
             (error err))"#,
    );
    assert_eq!(results[1], "OK (void-variable vm-default-alias-unbound)");
}

#[test]
fn set_default_rejects_constant_symbols() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(list
             (condition-case err (set-default nil 1) (error err))
             (condition-case err (set-default t 1) (error err))
             (condition-case err (set-default :foo 1) (error err)))"#,
    );
    assert_eq!(
        results[0],
        "OK ((setting-constant nil) (setting-constant t) (setting-constant :foo))"
    );
}

#[test]
fn set_default_triggers_variable_watchers() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(fset 'vm-set-default-watch-rec
                 (lambda (symbol newval operation where)
                   (setq vm-set-default-watch-last
                         (list symbol newval operation where))))
           (add-variable-watcher 'vm-set-default-watch-target 'vm-set-default-watch-rec)
           (set-default 'vm-set-default-watch-target 42)
           vm-set-default-watch-last"#,
    );
    assert_eq!(results[3], "OK (vm-set-default-watch-target 42 set nil)");
}

#[test]
fn set_default_alias_triggers_variable_watchers_twice() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(setq vm-set-default-alias-watch-events nil)
           (fset 'vm-set-default-alias-watch-rec
                 (lambda (symbol newval operation where)
                   (setq vm-set-default-alias-watch-events
                         (cons (list symbol newval operation where)
                               vm-set-default-alias-watch-events))))
           (defvaralias 'vm-set-default-alias-watch 'vm-set-default-alias-base)
           (add-variable-watcher 'vm-set-default-alias-base 'vm-set-default-alias-watch-rec)
           (set-default 'vm-set-default-alias-watch 9)
           (length vm-set-default-alias-watch-events)"#,
    );
    assert_eq!(results[5], "OK 2");
}

#[test]
fn set_default_buffer_alias_notifies_variable_watcher_once() {
    crate::test_utils::init_test_tracing();
    let results = eval_all(
        r#"(setq vm-set-default-buffer-alias-events nil)
           (fset 'vm-set-default-buffer-alias-rec
                 (lambda (symbol newval operation where)
                   (setq vm-set-default-buffer-alias-events
                         (cons (list symbol newval operation where)
                               vm-set-default-buffer-alias-events))))
           (defvaralias 'vm-set-default-buffer-alias 'truncate-lines)
           (add-variable-watcher 'truncate-lines 'vm-set-default-buffer-alias-rec)
           (set-default 'vm-set-default-buffer-alias t)
           (length vm-set-default-buffer-alias-events)"#,
    );
    assert_eq!(results[5], "OK 1");
}

#[test]
fn set_default_returns_requested_value_before_forwarder_canonicalization() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one("(list (set-default 'inverse-video 9) (default-value 'inverse-video))"),
        "OK (9 t)"
    );
}

#[test]
fn set_default_notifies_watcher_before_forwarder_rejects_value() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_one(
            r#"(progn
                 (setq vm-set-default-event nil)
                 (fset 'vm-set-default-watcher
                       (lambda (symbol value operation where)
                         (setq vm-set-default-event
                               (list symbol value operation where))))
                 (add-variable-watcher 'undo-limit 'vm-set-default-watcher)
                 (list
                  (condition-case error
                      (set-default 'undo-limit "x")
                    (error error))
                  vm-set-default-event))"#,
        ),
        "OK ((wrong-type-argument integerp \"x\") (undo-limit \"x\" set nil))"
    );
}
