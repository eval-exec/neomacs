use super::*;
use crate::emacs_core::eval::signal_entry::builtin_signal;
use crate::emacs_core::print::builtin_error_message_string;
use crate::error::{FlowKind, FlowResultExt};
use crate::test_utils::{runtime_startup_context, runtime_startup_eval_all};

fn init_runtime_errors(context: &mut crate::emacs_core::eval::Context) {
    // SAFETY: standard-error setup only initializes native condition/message
    // properties. It never calls Lisp, collects, mutates canonical dispatch,
    // replaces storage or exports a raw manager/interior reference.
    unsafe {
        context
            .obarray
            .with_legacy_value_write(init_standard_errors);
    }
}

fn bootstrap_context() -> crate::emacs_core::Context {
    runtime_startup_context()
}

fn bootstrap_eval_all(src: &str) -> Vec<String> {
    runtime_startup_eval_all(src)
}

#[test]
fn error_message_string_substitutes_custom_error_message_quotes_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(progn
              (define-error 'neomacs-test-quoted-error
                "Couldn't find `aria2c' executable, aborting"
                'error)
              (list
                (let ((text-quoting-style 'curve))
                  (error-message-string '(neomacs-test-quoted-error)))
                (let ((text-quoting-style 'straight))
                  (error-message-string '(neomacs-test-quoted-error)))
                (let ((text-quoting-style 'grave))
                  (error-message-string '(neomacs-test-quoted-error)))
                (let ((text-quoting-style 'curve))
                  (error-message-string
                    '(error "Couldn't find `aria2c' executable, aborting")))
                (let ((original (symbol-function 'substitute-command-keys)))
                  (unwind-protect
                      (progn
                        (fset 'substitute-command-keys
                              (lambda (_) (error "broken substitution")))
                        (error-message-string '(neomacs-test-quoted-error)))
                    (fset 'substitute-command-keys original)))
                (let ((original (symbol-function 'substitute-command-keys)))
                  (unwind-protect
                      (progn
                        (fset 'substitute-command-keys (lambda (_) t))
                        (error-message-string '(neomacs-test-quoted-error)))
                    (fset 'substitute-command-keys original)))
                (let ((original (symbol-function 'substitute-command-keys)))
                  (unwind-protect
                      (catch 'neomacs-test-safe-call
                        (fset 'substitute-command-keys
                              (lambda (_)
                                (throw 'neomacs-test-safe-call 'escaped)))
                        (error-message-string '(neomacs-test-quoted-error)))
                    (fset 'substitute-command-keys original)))))"#,
    );

    assert_eq!(
        results,
        vec![
            r#"OK ("Couldn’t find ‘aria2c’ executable, aborting" "Couldn't find 'aria2c' executable, aborting" "Couldn't find `aria2c' executable, aborting" "Couldn't find `aria2c' executable, aborting" "Couldn't find `aria2c' executable, aborting" "peculiar error" escaped)"#
        ]
    );
}

#[test]
fn module_error_symbols_match_gnu_emacs_module_c() {
    crate::test_utils::init_test_tracing();
    // GNU src/emacs-module.c `syms_of_module'.
    let results = bootstrap_eval_all(
        r#"(condition-case err
               (module-load "/nonexistent/neomacs-missing-module.so")
             (module-load-failed (car err)))
           (mapcar (lambda (sym)
                     (list sym
                           (get sym 'error-conditions)
                           (get sym 'error-message)))
                   '(module-out-of-memory
                     module-load-failed
                     module-open-failed
                     module-not-gpl-compatible
                     missing-module-init-function
                     module-init-failed
                     invalid-arity
                     memory-buffer-too-small))
           (error-message-string '(module-open-failed "/x.so" "no such file"))"#,
    );
    assert_eq!(
        results,
        vec![
            "OK module-open-failed".to_string(),
            concat!(
                "OK (",
                "(module-out-of-memory (module-out-of-memory error) \"Module out of memory\") ",
                "(module-load-failed (module-load-failed error) \"Module load failed\") ",
                "(module-open-failed (module-open-failed module-load-failed error) ",
                "\"Module could not be opened\") ",
                "(module-not-gpl-compatible (module-not-gpl-compatible module-load-failed error) ",
                "\"Module is not GPL compatible\") ",
                "(missing-module-init-function ",
                "(missing-module-init-function module-load-failed error) ",
                "\"Module does not export an initialization function\") ",
                "(module-init-failed (module-init-failed module-load-failed error) ",
                "\"Module initialization failed\") ",
                "(invalid-arity (invalid-arity error) \"Invalid function arity\") ",
                "(memory-buffer-too-small (memory-buffer-too-small error) ",
                "\"Memory buffer too small\"))"
            )
            .to_string(),
            "OK \"Module could not be opened: \\\"/x.so\\\", \\\"no such file\\\"\"".to_string(),
        ]
    );
}

#[test]
fn define_error_basic() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = bootstrap_context();
    let result = evaluator.eval_str(r#"(define-error 'my-error "My error")"#);
    assert!(result.is_ok());

    // Check plist.
    let conds = evaluator
        .obarray
        .get_property("my-error", "error-conditions")
        .unwrap();
    let items = iter_symbol_list(&conds, crate::tagged::SymbolMode::Bare);
    assert!(items.contains(&"my-error".to_string()));
    assert!(items.contains(&"error".to_string()));

    let msg = evaluator
        .obarray
        .get_property("my-error", "error-message")
        .unwrap();
    assert_eq!(msg.as_utf8_str(), Some("My error"));
}

#[test]
fn define_error_with_parent() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = bootstrap_context();
    let result = evaluator.eval_str(r#"(define-error 'my-file-error "My file error" 'file-error)"#);
    assert!(result.is_ok());

    assert!(signal_matches_hierarchical(
        &evaluator.obarray,
        "my-file-error",
        "file-error",
        crate::tagged::SymbolMode::Bare
    ));
    assert!(signal_matches_hierarchical(
        &evaluator.obarray,
        "my-file-error",
        "error",
        crate::tagged::SymbolMode::Bare
    ));
}

#[test]
fn define_error_with_parent_list() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = bootstrap_context();
    let result =
        evaluator.eval_str(r#"(define-error 'multi-error "Multi" '(file-error arith-error))"#);
    assert!(result.is_ok());

    assert!(signal_matches_hierarchical(
        &evaluator.obarray,
        "multi-error",
        "file-error",
        crate::tagged::SymbolMode::Bare
    ));
    assert!(signal_matches_hierarchical(
        &evaluator.obarray,
        "multi-error",
        "arith-error",
        crate::tagged::SymbolMode::Bare
    ));
    assert!(signal_matches_hierarchical(
        &evaluator.obarray,
        "multi-error",
        "error",
        crate::tagged::SymbolMode::Bare
    ));
}

#[test]
fn define_error_wrong_type_name() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(r#"(condition-case err (define-error 42 "Bad") (error err))"#);
    assert_eq!(results[0], "OK (wrong-type-argument symbolp 42)");
}

#[test]
fn define_error_accepts_non_string_message() {
    // GNU emacs 31.0.50 verified: define-error does NOT type-check
    // its MESSAGE argument; (define-error 'foo 42) returns 42.
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(r#"(define-error 'foo 42)"#);
    assert_eq!(results[0], "OK 42");
}

#[test]
fn define_error_too_many_args() {
    // GNU emacs 31.0.50 verified: define-error has arity (2 . 3),
    // so wrong-arity errors carry the (MIN . MAX) tuple.
    crate::test_utils::init_test_tracing();
    let results =
        bootstrap_eval_all(r#"(condition-case err (define-error 'x "X" 'error 99) (error err))"#);
    assert_eq!(results[0], "OK (wrong-number-of-arguments (2 . 3) 4)");
}

#[test]
fn builtin_signal_basic() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let args = vec![Value::symbol("void-variable"), Value::NIL];
    let result = builtin_signal(&mut eval, args);
    assert!(result.is_err());
    match result.kinded() {
        Err(FlowKind::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "void-variable");
            assert!(sig.data.is_empty());
        }
        _ => panic!("expected signal"),
    }
}

#[test]
fn builtin_signal_with_data() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let data_list = Value::list(vec![Value::symbol("x")]);
    let args = vec![Value::symbol("void-variable"), data_list];
    let result = builtin_signal(&mut eval, args);
    match result.kinded() {
        Err(FlowKind::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "void-variable");
            assert_eq!(sig.data.len(), 1);
        }
        _ => panic!("expected signal"),
    }
}

#[test]
fn builtin_signal_atom_preserves_raw_payload() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let result = builtin_signal(&mut eval, vec![Value::symbol("error"), Value::fixnum(1)]);
    match result.kinded() {
        Err(FlowKind::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(sig.data, vec![Value::fixnum(1)]);
            assert_eq!(sig.raw_data, Some(Value::fixnum(1)));
        }
        _ => panic!("expected signal"),
    }
}

#[test]
fn condition_case_preserves_raw_signal_binding_shape() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let value = eval
        .eval_str("(condition-case err (signal 'error 1) (error err))")
        .expect("condition-case should catch signal");
    assert_eq!(value, Value::cons(Value::symbol("error"), Value::fixnum(1)));
}

#[test]
fn condition_case_preserves_proper_signal_data_identity_like_gnu_fsignal() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let value = eval
        .eval_str(
            "(let ((data (list :keys (list \"unknown\"))))\
               (condition-case err\
                   (signal 'wrong-type-argument data)\
                 (error (eq (cdr err) data))))",
        )
        .expect("condition-case should catch signal");
    assert_eq!(value, Value::T);
}

#[test]
fn builtin_signal_wrong_arity() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let result = builtin_signal(&mut eval, vec![Value::symbol("error")]);
    assert!(result.is_err());
}

#[test]
fn builtin_signal_non_symbol() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let result = builtin_signal(&mut eval, vec![Value::fixnum(42), Value::NIL]);
    assert!(result.is_err());
}

#[test]
fn builtin_error_message_string_basic() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = crate::test_utils::new_context();
    init_runtime_errors(&mut evaluator);

    // (error-message-string '(void-variable x))
    let err_data = Value::list(vec![Value::symbol("void-variable"), Value::symbol("x")]);
    let result = builtin_error_message_string(&mut evaluator, vec![err_data]);
    assert!(result.is_ok());
    let msg = result.unwrap();
    assert_eq!(
        msg.as_utf8_str(),
        Some("Symbol\u{2019}s value as variable is void: x")
    );
}

#[test]
fn builtin_error_message_string_no_data() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = crate::test_utils::new_context();
    init_runtime_errors(&mut evaluator);

    // (error-message-string '(arith-error))
    let err_data = Value::list(vec![Value::symbol("arith-error")]);
    let result = builtin_error_message_string(&mut evaluator, vec![err_data]);
    assert!(result.is_ok());
    let msg = result.unwrap();
    assert_eq!(msg.as_utf8_str(), Some("Arithmetic error"));
}

#[test]
fn builtin_error_message_string_void_function_typography() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = crate::test_utils::new_context();
    init_runtime_errors(&mut evaluator);

    let err_data = Value::list(vec![Value::symbol("void-function"), Value::symbol("x")]);
    let result = builtin_error_message_string(&mut evaluator, vec![err_data]);
    assert!(result.is_ok());
    let msg = result.unwrap();
    assert_eq!(
        msg.as_utf8_str(),
        Some("Symbol\u{2019}s function definition is void: x")
    );
}

#[test]
fn builtin_error_message_string_unknown() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = crate::test_utils::new_context();

    // Unknown condition symbols are treated as peculiar errors.
    let err_data = Value::list(vec![Value::symbol("mystery-error")]);
    let result = builtin_error_message_string(&mut evaluator, vec![err_data]);
    assert!(result.is_ok());
    let msg = result.unwrap();
    assert_eq!(msg.as_utf8_str(), Some("peculiar error"));

    let err_data_payload = Value::list(vec![
        Value::symbol("mystery-error"),
        Value::fixnum(1),
        Value::fixnum(2),
        Value::fixnum(3),
    ]);
    let payload_result = builtin_error_message_string(&mut evaluator, vec![err_data_payload]);
    assert!(payload_result.is_ok());
    assert_eq!(
        payload_result.unwrap().as_utf8_str(),
        Some("peculiar error: 1, 2, 3")
    );
}

#[test]
fn builtin_error_message_string_no_payload_specials() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = crate::test_utils::new_context();
    init_runtime_errors(&mut evaluator);

    let error_no_payload = Value::list(vec![Value::symbol("error")]);
    let error_result = builtin_error_message_string(&mut evaluator, vec![error_no_payload]);
    assert!(error_result.is_ok());
    assert_eq!(error_result.unwrap().as_utf8_str(), Some("peculiar error"));

    let user_error_no_payload = Value::list(vec![Value::symbol("user-error")]);
    let user_result = builtin_error_message_string(&mut evaluator, vec![user_error_no_payload]);
    assert!(user_result.is_ok());
    assert_eq!(user_result.unwrap().as_utf8_str(), Some(""));
}

#[test]
fn builtin_error_message_string_quit_conditions_are_printable_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = crate::test_utils::new_context();
    init_runtime_errors(&mut evaluator);

    let quit_no_payload = Value::list(vec![Value::symbol("quit")]);
    let quit_result = builtin_error_message_string(&mut evaluator, vec![quit_no_payload]);
    assert!(quit_result.is_ok());
    assert_eq!(quit_result.unwrap().as_utf8_str(), Some("Quit"));

    let minibuffer_quit_no_payload = Value::list(vec![Value::symbol("minibuffer-quit")]);
    let minibuffer_quit_result =
        builtin_error_message_string(&mut evaluator, vec![minibuffer_quit_no_payload]);
    assert!(minibuffer_quit_result.is_ok());
    assert_eq!(minibuffer_quit_result.unwrap().as_utf8_str(), Some("Quit"));

    let quit_with_payload = Value::list(vec![Value::symbol("quit"), Value::fixnum(1)]);
    let quit_payload_result = builtin_error_message_string(&mut evaluator, vec![quit_with_payload]);
    assert!(quit_payload_result.is_ok());
    assert_eq!(quit_payload_result.unwrap().as_utf8_str(), Some("Quit: 1"));
}

#[test]
fn builtin_error_message_string_message_only_symbol_is_printable_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = crate::test_utils::new_context();
    evaluator
        .obarray
        .put_property(
            "neomacs-message-only",
            "error-message",
            Value::string("Message only"),
        )
        .expect("test plist should be writable");

    let err_data = Value::list(vec![
        Value::symbol("neomacs-message-only"),
        Value::fixnum(1),
    ]);
    let result = builtin_error_message_string(&mut evaluator, vec![err_data]);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().as_utf8_str(), Some("Message only: 1"));
}

#[test]
fn builtin_error_message_string_error_with_string_payload() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = crate::test_utils::new_context();
    init_runtime_errors(&mut evaluator);

    let err_data = Value::list(vec![Value::symbol("error"), Value::string("abc")]);
    let result = builtin_error_message_string(&mut evaluator, vec![err_data]);
    assert!(result.is_ok());
    let msg = result.unwrap();
    assert_eq!(msg.as_utf8_str(), Some("abc"));
}

#[test]
fn builtin_error_message_string_error_with_string_and_extra() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = crate::test_utils::new_context();
    init_runtime_errors(&mut evaluator);

    let err_data = Value::list(vec![
        Value::symbol("error"),
        Value::string("abc"),
        Value::fixnum(1),
    ]);
    let result = builtin_error_message_string(&mut evaluator, vec![err_data]);
    assert!(result.is_ok());
    let msg = result.unwrap();
    assert_eq!(msg.as_utf8_str(), Some("abc: 1"));
}

#[test]
fn builtin_error_message_string_ignores_dotted_error_data_tails() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = crate::test_utils::new_context();
    init_runtime_errors(&mut evaluator);
    // SAFETY: this fixture publishes only error properties using the same
    // noncalling native property path; neither canonical dispatch word changes.
    unsafe {
        evaluator.obarray.with_legacy_value_write(|obarray| {
            put_error_properties(
                obarray,
                "iter-end-of-sequence",
                "Iteration terminated",
                vec!["iter-end-of-sequence"],
            );
        });
    }

    let generator_eof = Value::cons(Value::symbol("iter-end-of-sequence"), Value::symbol(":eof"));
    let generator_result = builtin_error_message_string(&mut evaluator, vec![generator_eof]);
    assert!(generator_result.is_ok());
    assert_eq!(
        generator_result.unwrap().as_utf8_str(),
        Some("Iteration terminated")
    );

    let error_string_dotted = Value::cons(
        Value::symbol("error"),
        Value::cons(Value::string("a"), Value::symbol(":b")),
    );
    let error_string_result =
        builtin_error_message_string(&mut evaluator, vec![error_string_dotted]);
    assert!(error_string_result.is_ok());
    assert_eq!(error_string_result.unwrap().as_utf8_str(), Some("a"));

    let error_extra_dotted = Value::cons(
        Value::symbol("error"),
        Value::cons(
            Value::string("a"),
            Value::cons(Value::symbol(":b"), Value::symbol(":c")),
        ),
    );
    let error_extra_result = builtin_error_message_string(&mut evaluator, vec![error_extra_dotted]);
    assert!(error_extra_result.is_ok());
    assert_eq!(error_extra_result.unwrap().as_utf8_str(), Some("a: :b"));

    let unknown_dotted = Value::cons(
        Value::symbol("foo"),
        Value::cons(Value::symbol(":a"), Value::symbol(":b")),
    );
    let unknown_result = builtin_error_message_string(&mut evaluator, vec![unknown_dotted]);
    assert!(unknown_result.is_ok());
    assert_eq!(
        unknown_result.unwrap().as_utf8_str(),
        Some("peculiar error: :a")
    );
}

#[test]
fn builtin_error_message_string_user_error_variants() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = crate::test_utils::new_context();
    init_runtime_errors(&mut evaluator);

    let with_string = Value::list(vec![
        Value::symbol("user-error"),
        Value::string("u"),
        Value::fixnum(1),
    ]);
    let with_string_result = builtin_error_message_string(&mut evaluator, vec![with_string]);
    assert!(with_string_result.is_ok());
    assert_eq!(with_string_result.unwrap().as_utf8_str(), Some("u, 1"));

    let non_string = Value::list(vec![
        Value::symbol("user-error"),
        Value::symbol("integerp"),
        Value::string("x"),
    ]);
    let non_string_result = builtin_error_message_string(&mut evaluator, vec![non_string]);
    assert!(non_string_result.is_ok());
    assert_eq!(
        non_string_result.unwrap().as_utf8_str(),
        Some("integerp, x")
    );
}

#[test]
fn builtin_error_message_string_file_error_string_payload() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = crate::test_utils::new_context();
    init_runtime_errors(&mut evaluator);

    let err_data = Value::list(vec![
        Value::symbol("file-error"),
        Value::string("No such file"),
        Value::string("foo"),
    ]);
    let result = builtin_error_message_string(&mut evaluator, vec![err_data]);
    assert!(result.is_ok());
    let msg = result.unwrap();
    assert_eq!(msg.as_utf8_str(), Some("No such file: foo"));
}

#[test]
fn builtin_error_message_string_file_missing_string_payload() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = crate::test_utils::new_context();
    init_runtime_errors(&mut evaluator);

    let err_data = Value::list(vec![
        Value::symbol("file-missing"),
        Value::string("Opening input file"),
        Value::string("No such file or directory"),
        Value::string("/tmp/probe"),
    ]);
    let result = builtin_error_message_string(&mut evaluator, vec![err_data]);
    assert!(result.is_ok());
    let msg = result.unwrap();
    assert_eq!(
        msg.as_utf8_str(),
        Some("Opening input file: No such file or directory, /tmp/probe")
    );
}

#[test]
fn builtin_error_message_string_escapes_raw_unibyte_payload_in_multibyte_buffer_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = crate::test_utils::new_context();
    init_runtime_errors(&mut evaluator);

    let raw = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![0xFF]));
    let err_data = Value::list(vec![Value::symbol("file-error"), raw, Value::string("foo")]);
    let result = builtin_error_message_string(&mut evaluator, vec![err_data])
        .expect("error-message-string should succeed");
    let text = result
        .as_lisp_string()
        .expect("error-message-string should return a LispString");
    assert!(text.is_multibyte());
    assert_eq!(text.as_bytes(), br#"\377: foo"#);
}

#[test]
fn builtin_error_message_string_peculiar_error_paths() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = crate::test_utils::new_context();
    init_runtime_errors(&mut evaluator);

    let error_single = Value::list(vec![Value::symbol("error"), Value::fixnum(1)]);
    let error_single_result = builtin_error_message_string(&mut evaluator, vec![error_single]);
    assert!(error_single_result.is_ok());
    assert_eq!(
        error_single_result.unwrap().as_utf8_str(),
        Some("peculiar error")
    );

    let error_double = Value::list(vec![
        Value::symbol("error"),
        Value::fixnum(1),
        Value::fixnum(2),
    ]);
    let error_double_result = builtin_error_message_string(&mut evaluator, vec![error_double]);
    assert!(error_double_result.is_ok());
    assert_eq!(
        error_double_result.unwrap().as_utf8_str(),
        Some("peculiar error: 2")
    );

    let error_triple = Value::list(vec![
        Value::symbol("error"),
        Value::fixnum(1),
        Value::fixnum(2),
        Value::fixnum(3),
    ]);
    let error_triple_result = builtin_error_message_string(&mut evaluator, vec![error_triple]);
    assert!(error_triple_result.is_ok());
    assert_eq!(
        error_triple_result.unwrap().as_utf8_str(),
        Some("peculiar error: 2, 3")
    );

    let file_single = Value::list(vec![Value::symbol("file-error"), Value::fixnum(1)]);
    let file_single_result = builtin_error_message_string(&mut evaluator, vec![file_single]);
    assert!(file_single_result.is_ok());
    assert_eq!(
        file_single_result.unwrap().as_utf8_str(),
        Some("peculiar error")
    );

    let file_double = Value::list(vec![
        Value::symbol("file-error"),
        Value::fixnum(1),
        Value::fixnum(2),
    ]);
    let file_double_result = builtin_error_message_string(&mut evaluator, vec![file_double]);
    assert!(file_double_result.is_ok());
    assert_eq!(
        file_double_result.unwrap().as_utf8_str(),
        Some("peculiar error: 2")
    );

    let file_triple = Value::list(vec![
        Value::symbol("file-error"),
        Value::fixnum(1),
        Value::fixnum(2),
        Value::fixnum(3),
    ]);
    let file_triple_result = builtin_error_message_string(&mut evaluator, vec![file_triple]);
    assert!(file_triple_result.is_ok());
    assert_eq!(
        file_triple_result.unwrap().as_utf8_str(),
        Some("peculiar error: 2, 3")
    );

    let file_missing_triple = Value::list(vec![
        Value::symbol("file-missing"),
        Value::fixnum(1),
        Value::fixnum(2),
        Value::fixnum(3),
    ]);
    let file_missing_triple_result =
        builtin_error_message_string(&mut evaluator, vec![file_missing_triple]);
    assert!(file_missing_triple_result.is_ok());
    assert_eq!(
        file_missing_triple_result.unwrap().as_utf8_str(),
        Some("peculiar error: 2, 3")
    );

    let file_locked_strings = Value::list(vec![
        Value::symbol("file-locked"),
        Value::string("Locking file"),
        Value::string("Permission denied"),
        Value::string("/tmp/probe"),
    ]);
    let file_locked_strings_result =
        builtin_error_message_string(&mut evaluator, vec![file_locked_strings]);
    assert!(file_locked_strings_result.is_ok());
    assert_eq!(
        file_locked_strings_result.unwrap().as_utf8_str(),
        Some("peculiar error: \"Locking file\", \"Permission denied\", \"/tmp/probe\"")
    );
}

#[test]
fn builtin_error_message_string_end_of_file_does_not_quote_string_payload() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = crate::test_utils::new_context();
    init_runtime_errors(&mut evaluator);

    let err_data = Value::list(vec![
        Value::symbol("end-of-file"),
        Value::string("EOF while reading"),
    ]);
    let result = builtin_error_message_string(&mut evaluator, vec![err_data]);
    assert!(result.is_ok());
    assert_eq!(
        result.unwrap().as_utf8_str(),
        Some("End of file during parsing: EOF while reading")
    );
}

#[test]
fn error_message_string_end_of_file_prints_live_buffer_name_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(let ((source (generate-new-buffer " *end-of-file-source*")))
              (unwind-protect
                  (error-message-string (list 'end-of-file source))
                (kill-buffer source)))"#,
    );

    assert_eq!(
        results,
        vec![r#"OK "End of file during parsing:  *end-of-file-source*""#]
    );
}

#[test]
fn builtin_error_message_string_args_out_of_range_uses_base_message() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = crate::test_utils::new_context();
    init_runtime_errors(&mut evaluator);

    let err_data = Value::list(vec![
        Value::symbol("args-out-of-range"),
        Value::string("abc"),
        Value::fixnum(9),
    ]);
    let result = builtin_error_message_string(&mut evaluator, vec![err_data]);
    assert!(result.is_ok());
    assert_eq!(
        result.unwrap().as_utf8_str(),
        Some("Args out of range: \"abc\", 9")
    );
}

#[test]
fn builtin_error_message_string_formats_buffer_handles_with_names() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = crate::test_utils::new_context();
    init_runtime_errors(&mut evaluator);

    let live_id = evaluator.buffers.create_buffer("*ems-live*");
    let live_err = Value::list(vec![
        Value::symbol("args-out-of-range"),
        Value::make_buffer(live_id),
        Value::fixnum(0),
    ]);
    let live_result = builtin_error_message_string(&mut evaluator, vec![live_err]);
    assert!(live_result.is_ok());
    assert_eq!(
        live_result.unwrap().as_utf8_str(),
        Some("Args out of range: #<buffer *ems-live*>, 0")
    );

    let dead_id = evaluator.buffers.create_buffer("*ems-dead*");
    assert!(evaluator.buffers.kill_buffer(dead_id));
    let dead_err = Value::list(vec![
        Value::symbol("args-out-of-range"),
        Value::make_buffer(dead_id),
        Value::fixnum(0),
    ]);
    let dead_result = builtin_error_message_string(&mut evaluator, vec![dead_err]);
    assert!(dead_result.is_ok());
    assert_eq!(
        dead_result.unwrap().as_utf8_str(),
        Some("Args out of range: #<killed buffer>, 0")
    );
}

#[test]
fn builtin_error_message_string_formats_mutex_and_condvar_handles() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = crate::test_utils::new_context();
    init_runtime_errors(&mut evaluator);

    let mutex =
        super::super::threads::builtin_make_mutex(&mut evaluator, vec![Value::string("ems-mutex")])
            .expect("make-mutex should succeed");

    let mutex_err = Value::list(vec![
        Value::symbol("args-out-of-range"),
        mutex,
        Value::fixnum(0),
    ]);
    let mutex_result = builtin_error_message_string(&mut evaluator, vec![mutex_err]);
    assert!(mutex_result.is_ok());
    let mutex_text = mutex_result
        .unwrap()
        .as_utf8_str()
        .expect("error-message-string must return a string")
        .to_string();
    assert!(mutex_text.starts_with("Args out of range: #<mutex"));
    assert!(mutex_text.ends_with(", 0"));

    let condvar = super::super::threads::builtin_make_condition_variable(
        &mut evaluator,
        vec![mutex, Value::string("ems-condvar")],
    )
    .expect("make-condition-variable should succeed");

    let condvar_err = Value::list(vec![
        Value::symbol("args-out-of-range"),
        condvar,
        Value::fixnum(0),
    ]);
    let condvar_result = builtin_error_message_string(&mut evaluator, vec![condvar_err]);
    assert!(condvar_result.is_ok());
    let condvar_text = condvar_result
        .unwrap()
        .as_utf8_str()
        .expect("error-message-string must return a string")
        .to_string();
    assert!(condvar_text.starts_with("Args out of range: #<condvar"));
    assert!(condvar_text.ends_with(", 0"));
}

#[test]
fn builtin_error_message_string_formats_thread_handles() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = crate::test_utils::new_context();
    init_runtime_errors(&mut evaluator);

    let thread = super::super::threads::builtin_current_thread(&mut evaluator, vec![])
        .expect("current-thread should succeed");
    let thread_err = Value::list(vec![
        Value::symbol("args-out-of-range"),
        thread,
        Value::fixnum(0),
    ]);
    let thread_result = builtin_error_message_string(&mut evaluator, vec![thread_err]);
    assert!(thread_result.is_ok());
    let thread_text = thread_result
        .unwrap()
        .as_utf8_str()
        .expect("error-message-string must return a string")
        .to_string();
    assert!(thread_text.starts_with("Args out of range: #<thread"));
    assert!(thread_text.ends_with(", 0"));
}

#[test]
fn builtin_error_message_string_formats_terminal_handles() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = crate::test_utils::new_context();
    init_runtime_errors(&mut evaluator);

    let terminals = super::super::terminal::pure::builtin_terminal_list(vec![])
        .expect("terminal-list should succeed");
    let terminal = crate::value::list_to_vec(&terminals)
        .and_then(|values| values.into_iter().next())
        .expect("terminal-list should return one terminal handle");

    let terminal_err = Value::list(vec![
        Value::symbol("args-out-of-range"),
        terminal,
        Value::fixnum(0),
    ]);
    let terminal_result = builtin_error_message_string(&mut evaluator, vec![terminal_err]);
    assert!(terminal_result.is_ok());
    let terminal_text = terminal_result
        .unwrap()
        .as_utf8_str()
        .expect("error-message-string must return a string")
        .to_string();
    assert!(terminal_text.starts_with("Args out of range: #<terminal"));
    assert!(terminal_text.ends_with(", 0"));
}

#[test]
fn builtin_error_message_string_formats_frame_and_window_handles() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = crate::test_utils::new_context();
    init_runtime_errors(&mut evaluator);

    let frame = super::super::frame::builtin_selected_frame(&mut evaluator, vec![])
        .expect("selected-frame should succeed");
    let frame_err = Value::list(vec![
        Value::symbol("args-out-of-range"),
        frame,
        Value::fixnum(0),
    ]);
    let frame_result = builtin_error_message_string(&mut evaluator, vec![frame_err]);
    assert!(frame_result.is_ok());
    let frame_text = frame_result
        .unwrap()
        .as_utf8_str()
        .expect("error-message-string must return a string")
        .to_string();
    assert!(frame_text.starts_with("Args out of range: #<frame"));
    assert!(frame_text.ends_with(", 0"));

    let window = super::super::window_cmds::builtin_selected_window(&mut evaluator, &vec![])
        .expect("selected-window should succeed");
    let window_err = Value::list(vec![
        Value::symbol("args-out-of-range"),
        window,
        Value::fixnum(0),
    ]);
    let window_result = builtin_error_message_string(&mut evaluator, vec![window_err]);
    assert!(window_result.is_ok());
    let window_text = window_result
        .unwrap()
        .as_utf8_str()
        .expect("error-message-string must return a string")
        .to_string();
    assert!(window_text.starts_with("Args out of range: #<window"));
    assert!(window_text.ends_with(", 0"));
}

#[test]
fn builtin_error_message_string_not_cons() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = crate::test_utils::new_context();

    // Non-list input signals wrong-type-argument (listp VALUE).
    let result = builtin_error_message_string(&mut evaluator, vec![Value::fixnum(42)]);
    assert!(result.is_err());
    match result.kinded() {
        Err(FlowKind::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("listp"), Value::fixnum(42)]);
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
}

#[test]
fn builtin_error_message_string_symbol_input_is_wrong_type() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = crate::test_utils::new_context();

    let result = builtin_error_message_string(&mut evaluator, vec![Value::symbol("foo")]);
    assert!(result.is_err());
    match result.kinded() {
        Err(FlowKind::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("listp"), Value::symbol("foo")]);
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }

    let result_true = builtin_error_message_string(&mut evaluator, vec![Value::T]);
    assert!(result_true.is_err());
    match result_true.kinded() {
        Err(FlowKind::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("listp"), Value::T]);
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
}

#[test]
fn builtin_error_message_string_wrong_arity() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = crate::test_utils::new_context();
    let result = builtin_error_message_string(&mut evaluator, vec![]);
    assert!(result.is_err());
}

/// Evaluate SRC in a bare context that has `scan-error` defined, returning the
/// printed value or signal.
///
/// `define-error` is an elisp macro (subr.el), unavailable on a bare
/// `Context`, so we install the `error-conditions`/`error-message` plist
/// directly with `put` — exactly what `define-error` does internally.
fn resignal_eval(src: &str) -> String {
    let mut eval = crate::test_utils::new_context();
    eval.eval_str(
        r#"(progn
             (put 'scan-error 'error-conditions '(scan-error error))
             (put 'scan-error 'error-message "Scan error"))"#,
    )
    .expect("install scan-error conditions");
    crate::emacs_core::format_eval_result(&eval.eval_str(src))
}

#[test]
fn signal_cons_error_object_resignals_whole_object() {
    // GNU verified: (condition-case e (signal (list 'scan-error "msg" 2 4))
    //                 (error e)) => (scan-error "msg" 2 4)
    crate::test_utils::init_test_tracing();
    let result =
        resignal_eval(r#"(condition-case e (signal (list 'scan-error "msg" 2 4)) (error e))"#);
    assert_eq!(result, r#"OK (scan-error "msg" 2 4)"#);
}

#[test]
fn signal_cons_error_object_caught_by_specific_condition() {
    // The re-raised object must match the *real* error symbol's conditions,
    // so a `scan-error` handler catches it (this is the `up-list` idiom).
    crate::test_utils::init_test_tracing();
    let result = resignal_eval(
        r#"(condition-case e (signal (list 'scan-error "msg" 2 4)) (scan-error (car e)))"#,
    );
    assert_eq!(result, "OK scan-error");
}

#[test]
fn signal_cons_error_object_preserves_improper_cdr() {
    // GNU verified: (signal (cons 'scan-error 5)) => (scan-error . 5)
    crate::test_utils::init_test_tracing();
    let result = resignal_eval(r#"(condition-case e (signal (cons 'scan-error 5)) (error e))"#);
    assert_eq!(result, "OK (scan-error . 5)");
}

#[test]
fn signal_cons_error_object_non_symbol_car_is_wrong_type() {
    // GNU verified: (signal (list 5 "msg")) => wrong-type-argument symbolp 5
    crate::test_utils::init_test_tracing();
    let result = resignal_eval(r#"(condition-case e (signal (list 5 "msg")) (error e))"#);
    assert_eq!(result, "OK (wrong-type-argument symbolp 5)");
}

#[test]
fn signal_cons_error_object_unknown_symbol_is_invalid() {
    // GNU verified: car symbol without error-conditions =>
    //   (error "Invalid error symbol" totally-unknown-sym)
    crate::test_utils::init_test_tracing();
    let result =
        resignal_eval(r#"(condition-case e (signal (list 'totally-unknown-sym "msg")) (error e))"#);
    assert_eq!(
        result,
        r#"OK (error "Invalid error symbol" totally-unknown-sym)"#
    );
}

#[test]
fn signal_treesit_query_error_is_valid_and_catchable() {
    crate::test_utils::init_test_tracing();
    let result = resignal_eval(
        r#"(condition-case e
              (signal 'treesit-query-error (list "bad query"))
            (treesit-query-error (list 'caught (car e)))
            (error (list 'wrong (car e) (cadr e))))"#,
    );
    assert_eq!(result, "OK (caught treesit-query-error)");
}

#[test]
fn signal_plain_symbol_still_works() {
    // Regression guard: the normal (signal 'error "x") / (signal 'wrong-type
    // -argument ...) paths must stay byte-identical.
    crate::test_utils::init_test_tracing();
    let result = resignal_eval(
        r#"(condition-case e (signal 'wrong-type-argument (list 'symbolp 5)) (error e))"#,
    );
    assert_eq!(result, "OK (wrong-type-argument symbolp 5)");
}

#[test]
fn signal_cons_with_explicit_data_keeps_old_behavior() {
    // When DATA is non-nil, GNU does Fcons(error_symbol, data); the car of the
    // resulting error is a cons => Fget runs CHECK_SYMBOL => symbolp error.
    crate::test_utils::init_test_tracing();
    let result =
        resignal_eval(r#"(condition-case e (signal (list 'scan-error "msg") '(x y)) (error e))"#);
    assert_eq!(
        result,
        r#"OK (wrong-type-argument symbolp (scan-error "msg"))"#
    );
}

fn bootstrap_eval_one(src: &str) -> String {
    bootstrap_eval_all(src)
        .into_iter()
        .next()
        .expect("at least one form")
}

/// GNU's `Ferror_message_string' (print.c:1046) hands back the payload string
/// itself only for an error of exactly the shape (error STRING).  Every other
/// shape is rendered by printing into a buffer, so the answer carries no text
/// properties even when the payload did -- a `user-error' built by
/// `format-message' from propertized buffer text comes back plain.
#[test]
fn error_message_string_keeps_properties_only_for_a_lone_error_string() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one(
        r#"(let ((s (propertize "SYM" 'p 1)))
             (mapcar (lambda (data)
                       (let ((m (error-message-string data)))
                         (list m (text-properties-at 0 m))))
                     (list (list 'error s)
                           (list 'user-error s)
                           (list 'error s "extra")
                           (list 'file-error s "more"))))"#,
    );
    assert_eq!(
        result,
        r#"OK ((#("SYM" 0 3 (p 1)) (p 1)) ("SYM" nil) ("SYM: \"extra\"" nil) ("SYM: more" nil))"#
    );
}
