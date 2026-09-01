use super::*;
use crate::emacs_core::builtins::builtin_documentation_stringp;
use crate::emacs_core::{Context, EvalError};
use crate::test_utils::{
    load_minimal_gnu_help_runtime, runtime_startup_context, runtime_startup_eval_all,
};

fn bootstrap_eval_all(src: &str) -> Vec<String> {
    runtime_startup_eval_all(src)
}

/// A `Context` at the moment `lisp/loadup.el:448` calls `Snarf-documentation`:
/// every C `DEFVAR` declared, and the `etc/DOC` stand-in installed onto the
/// ones that are bound.
///
/// A **bare** `Context` is the moment before that, and GNU's answer there is
/// nil for every built-in variable -- `Fsnarf_documentation` is what puts a
/// `variable-documentation` on a `DEFVAR_*` name at all (`src/doc.c:613`), and
/// in temacs it has not run.  So a test about a built-in's documentation has
/// to snarf first.  Before ledger 182 these tests did not, because the port
/// consulted its DOC stand-in lazily on every query, which is a fallback and
/// therefore answered in a state where GNU answers nothing.
fn snarfed_context() -> Context {
    let mut eval = Context::new();
    super::snarf_variable_documentation(&mut eval.obarray);
    eval
}

#[test]
fn raw_documentation_property_does_not_require_substitute_command_keys() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.obarray_mut()
        .put_property(
            "vm-doc-prop",
            "variable-documentation",
            Value::string("Press \\[save-buffer] to save."),
        )
        .unwrap();

    let result = builtin_documentation_property(
        &mut eval,
        vec![
            Value::symbol("vm-doc-prop"),
            Value::symbol("variable-documentation"),
        ],
    )
    .expect("raw documentation-property should succeed");
    assert_eq!(result.as_utf8_str(), Some("Press \\[save-buffer] to save."));
}

#[test]
fn runtime_documentation_property_uses_gnu_substitute_command_keys() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    load_minimal_gnu_help_runtime(&mut eval);
    eval.obarray_mut()
        .put_property(
            "vm-doc-prop",
            "variable-documentation",
            Value::string("Press \\[save-buffer] to save."),
        )
        .unwrap();

    let result = builtin_documentation_property(
        &mut eval,
        vec![
            Value::symbol("vm-doc-prop"),
            Value::symbol("variable-documentation"),
        ],
    )
    .expect("runtime documentation-property should succeed");
    let text = result
        .as_utf8_str()
        .expect("runtime doc should stay string");
    assert!(text.contains("save-buffer"));
    assert!(!text.contains("\\["));
}

#[test]
fn documentation_preserves_uninterned_autoload_symbol_identity() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    load_minimal_gnu_help_runtime(&mut eval);
    let sym = eval
        .eval_str(r#"(make-symbol "neo-auto")"#)
        .expect("make uninterned symbol");

    crate::emacs_core::autoload::builtin_autoload(
        &mut eval,
        vec![
            sym,
            Value::string("nofile"),
            Value::string("doc"),
            Value::T,
            Value::NIL,
        ],
    )
    .expect("autoload uninterned symbol");

    let function = resolve_documentation_function_value(eval.obarray(), sym, false)
        .expect("documentation resolver should find uninterned symbol function");
    assert!(crate::emacs_core::autoload::is_autoload_value(&function));
}

#[test]
fn documentation_accepts_symbol_with_pos_when_enabled_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = eval
        .eval_str(
            r#"(let ((symbols-with-pos-enabled t))
                 (substring (documentation (position-symbol 'file-exists-p 406) t) 0 20))"#,
        )
        .expect("documentation should resolve positioned function symbols when enabled");
    assert_eq!(result.as_utf8_str(), Some("Return t if file FIL"));
}

#[test]
fn documentation_rejects_symbol_with_pos_when_disabled_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let err = eval
        .eval_str(
            r#"(let ((symbols-with-pos-enabled nil))
                 (documentation (position-symbol 'file-exists-p 406) t))"#,
        )
        .expect_err("documentation should not unwrap positioned symbols when disabled");
    match err {
        EvalError::Signal { symbol, data, .. } => {
            assert_eq!(
                crate::emacs_core::intern::resolve_sym(symbol),
                "invalid-function"
            );
            assert_eq!(data.len(), 1);
            assert_eq!(
                crate::emacs_core::print::print_value(&data[0]),
                "#<symbol file-exists-p at 406>"
            );
        }
        other => panic!("expected invalid-function signal, got {other:?}"),
    }
}

// =======================================================================
// documentation-property (stub)
// =======================================================================

#[test]
fn documentation_property_returns_nil() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = builtin_documentation_property(
        &mut eval,
        vec![
            Value::symbol("foo"),
            Value::symbol("variable-documentation"),
        ],
    );
    assert!(result.is_ok());
    assert!(result.unwrap().is_nil());
}

#[test]
fn documentation_property_with_raw() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = builtin_documentation_property(
        &mut eval,
        vec![
            Value::symbol("foo"),
            Value::symbol("variable-documentation"),
            Value::T,
        ],
    );
    assert!(result.is_ok());
    assert!(result.unwrap().is_nil());
}

#[test]
fn documentation_property_wrong_type() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = builtin_documentation_property(
        &mut eval,
        vec![Value::fixnum(42), Value::symbol("variable-documentation")],
    );
    assert!(result.is_err());
}

#[test]
fn documentation_property_wrong_arity() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = builtin_documentation_property(&mut eval, vec![Value::symbol("foo")]);
    assert!(result.is_err());
}

// =======================================================================
// Snarf-documentation runtime/error semantics
// =======================================================================

#[test]
fn snarf_documentation_returns_nil() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();
    let result = builtin_snarf_documentation(&mut evaluator, vec![Value::string("DOC")]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_nil());
}

#[test]
fn snarf_documentation_wrong_type() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();
    let result = builtin_snarf_documentation(&mut evaluator, vec![Value::fixnum(42)]);
    assert!(result.is_err());
}

#[test]
fn snarf_documentation_empty_path_errors() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();
    let result = builtin_snarf_documentation(&mut evaluator, vec![Value::string("")]);
    match result {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "error"),
        other => panic!("expected error signal, got {other:?}"),
    }
}

#[test]
fn snarf_documentation_parent_dir_path_errors() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();
    let result = builtin_snarf_documentation(&mut evaluator, vec![Value::string("../")]);
    match result {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "error"),
        other => panic!("expected error signal, got {other:?}"),
    }
}

#[test]
fn snarf_documentation_single_dot_path_errors() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();
    let result = builtin_snarf_documentation(&mut evaluator, vec![Value::string(".")]);
    match result {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "error"),
        other => panic!("expected error signal, got {other:?}"),
    }
}

#[test]
fn snarf_documentation_root_path_errors() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();
    let result = builtin_snarf_documentation(&mut evaluator, vec![Value::string("/")]);
    match result {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "error"),
        other => panic!("expected error signal, got {other:?}"),
    }
}

#[test]
fn snarf_documentation_doc_dir_path_file_error() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();
    let result = builtin_snarf_documentation(&mut evaluator, vec![Value::string("DOC/")]);
    match result {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "file-error"),
        other => panic!("expected file-error signal, got {other:?}"),
    }
}

#[test]
fn snarf_documentation_doc_subpath_file_error() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();
    let result = builtin_snarf_documentation(&mut evaluator, vec![Value::string("DOC/a")]);
    match result {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "file-error"),
        other => panic!("expected file-error signal, got {other:?}"),
    }
}

#[test]
fn snarf_documentation_missing_path_errors() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();
    let result =
        builtin_snarf_documentation(&mut evaluator, vec![Value::string("NO_SUCH_DOC_FILE")]);
    match result {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "file-missing"),
        other => panic!("expected file-missing signal, got {other:?}"),
    }
}

#[test]
fn snarf_documentation_missing_dir_path_errors() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();
    let result =
        builtin_snarf_documentation(&mut evaluator, vec![Value::string("NO_SUCH_DOC_DIR/")]);
    match result {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "file-missing"),
        other => panic!("expected file-missing signal, got {other:?}"),
    }
}

#[test]
fn snarf_documentation_wrong_arity() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();
    let result = builtin_snarf_documentation(&mut evaluator, vec![]);
    assert!(result.is_err());
}

// =======================================================================
// help-function-arglist
// =======================================================================

#[test]
fn help_function_arglist_is_real_lisp_function_after_bootstrap() {
    crate::test_utils::init_test_tracing();
    let eval = runtime_startup_context();
    let function = eval
        .obarray
        .symbol_function("help-function-arglist")
        .expect("missing help-function-arglist bootstrapped function cell");
    assert!(!crate::emacs_core::autoload::is_autoload_value(&function));
}

#[test]
fn help_function_arglist_loads_from_gnu_help_el() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(list (help-function-arglist 'car)
                 (help-function-arglist 'car t)
                 (help-function-arglist 'describe-function)
                 (subrp (symbol-function 'help-function-arglist)))"#,
    );
    assert_eq!(
        results[0],
        r#"OK ((arg1) (list) "[Arg list not available until function definition is loaded.]" nil)"#
    );
}

#[test]
fn help_function_arglist_loaded_supports_lambda_forms() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(list (help-function-arglist '(lambda (x y) x))
                 (help-function-arglist '(lambda x x))
                 (help-function-arglist '(macro lambda)))"#,
    );
    assert_eq!(results[0], r#"OK ((x y) x nil)"#);
}

#[test]
fn help_function_arglist_loaded_wrong_arity_matches_gnu() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(condition-case err
               (help-function-arglist)
             (error (list 'err (car err))))"#,
    );
    assert_eq!(results[0], r#"OK (err wrong-number-of-arguments)"#);
}

// =======================================================================
// documentation (eval-dependent)
// =======================================================================

#[test]
fn documentation_lambda_with_docstring() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();

    // Set up a lambda with a docstring in the function cell.
    let lambda = Value::make_lambda(LambdaData {
        params: LambdaParams::simple(vec![intern("x")]),
        body: vec![].into(),
        env: None,
        docstring: Some(crate::heap_types::LispString::from_utf8("Add one to X.")),
        doc_form: None,
        interactive: None,
    });
    evaluator.obarray.set_symbol_function("my-fn", lambda);

    let result = builtin_documentation(&mut evaluator, vec![Value::symbol("my-fn")]);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().as_utf8_str(), Some("Add one to X."));
}

#[test]
fn documentation_lambda_no_docstring() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();

    let lambda = Value::make_lambda(LambdaData {
        params: LambdaParams::simple(vec![]),
        body: vec![].into(),
        env: None,
        docstring: None,
        doc_form: None,
        interactive: None,
    });
    evaluator.obarray.set_symbol_function("no-doc", lambda);

    let result = builtin_documentation(&mut evaluator, vec![Value::symbol("no-doc")]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_nil());
}

#[test]
fn documentation_substitutes_command_keys_unless_raw() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();
    // `(documentation 'foo)` calls `substitute-command-keys` on the
    // raw doc when RAW is nil. That function lives in `lisp/help.el`,
    // not in C/Rust, so the test must load enough of the GNU runtime
    // for help.el's `defun substitute-command-keys` to be reachable.
    // Mirrors GNU loadup.el ordering, which loads help.el before any
    // documentation query.
    crate::test_utils::load_minimal_gnu_help_runtime(&mut evaluator);
    let lambda = Value::make_lambda(LambdaData {
        params: LambdaParams::simple(vec![]),
        body: vec![Value::symbol("t")],
        env: None,
        docstring: Some(crate::heap_types::LispString::from_utf8(
            "Press \\[save-buffer] to save.",
        )),
        doc_form: None,
        interactive: None,
    });
    evaluator.obarray.set_symbol_function("doc-raw-fn", lambda);

    let display = builtin_documentation(&mut evaluator, vec![Value::symbol("doc-raw-fn")]).unwrap();
    let raw =
        builtin_documentation(&mut evaluator, vec![Value::symbol("doc-raw-fn"), Value::T]).unwrap();

    let display = display.as_utf8_str().expect("display documentation string");
    let raw = raw.as_utf8_str().expect("raw documentation string");
    assert!(display.contains("save-buffer"));
    assert!(!display.contains("\\["));
    assert!(raw.contains("\\[save-buffer]"));
}

#[test]
fn documentation_lambda_preserves_raw_unibyte_docstring() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();

    let raw_doc = crate::heap_types::LispString::from_unibyte(vec![0xFF, b'X']);
    let lambda = Value::make_lambda(LambdaData {
        params: LambdaParams::simple(vec![]),
        body: vec![].into(),
        env: None,
        docstring: Some(raw_doc.clone()),
        doc_form: None,
        interactive: None,
    });
    evaluator.obarray.set_symbol_function("raw-doc-fn", lambda);

    let result = builtin_documentation(&mut evaluator, vec![Value::symbol("raw-doc-fn"), Value::T])
        .expect("documentation result");
    let got = result.as_lisp_string().expect("raw docstring result");
    assert_eq!(got, &raw_doc);
}

#[test]
fn documentation_unbound_function() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();
    let result = builtin_documentation(&mut evaluator, vec![Value::symbol("nonexistent")]);
    assert!(result.is_err());
}

#[test]
fn documentation_subr() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();
    evaluator
        .obarray
        .set_symbol_function("plus", Value::subr(intern("+")));

    let result = builtin_documentation(&mut evaluator, vec![Value::symbol("plus")]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_string());
}

#[test]
fn documentation_car_subr_uses_oracle_text_shape() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();
    evaluator
        .obarray
        .set_symbol_function("car", Value::subr(intern("car")));

    let result = builtin_documentation(&mut evaluator, vec![Value::symbol("car")]).unwrap();
    let text = result
        .as_utf8_str()
        .expect("documentation for car should return a string");
    assert!(text.starts_with("Return the car of LIST.  If LIST is nil, return nil."));
    assert_ne!(text, "Built-in function.");
}

#[test]
fn documentation_if_special_form_uses_oracle_text_shape() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();
    evaluator
        .obarray
        .set_symbol_function("if", Value::subr(intern("if")));

    let result = builtin_documentation(&mut evaluator, vec![Value::symbol("if")]).unwrap();
    let text = result
        .as_utf8_str()
        .expect("documentation for if should return a string");
    assert!(text.starts_with("If COND yields non-nil, do THEN, else do ELSE..."));
    assert_ne!(text, "Built-in function.");
}

#[test]
fn documentation_core_subr_stubs_use_oracle_first_line_shapes() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();
    // The shim now stores raw grave-quoted text matching GNU's actual
    // DEFUN doc: comments. With bare `Context::new()' there's no
    // substitute-command-keys yet, so the prefix is checked against
    // the raw form. Once help.el loads,
    // `substitute-command-keys' will rewrite the quotes per
    // `text-quoting-style' (see the post-shim integration test below).
    let probes = [
        (
            "cons",
            "Create a new cons, give it CAR and CDR as components, and return it.",
        ),
        (
            "list",
            "Return a newly created list with specified arguments as elements.",
        ),
        ("eq", "Return t if the two args are the same Lisp object."),
        (
            "equal",
            "Return t if two Lisp objects have similar structure and contents.",
        ),
        (
            "length",
            "Return the length of vector, list or string SEQUENCE.",
        ),
        (
            "append",
            "Concatenate all the arguments and make the result a list.",
        ),
        (
            "mapcar",
            "Apply FUNCTION to each element of SEQUENCE, and make a list of the results.",
        ),
        (
            "assoc",
            "Return non-nil if KEY is equal to the car of an element of ALIST.",
        ),
        (
            "member",
            "Return non-nil if ELT is an element of LIST.  Comparison done with `equal'.",
        ),
        ("symbol-name", "Return SYMBOL's name, a string."),
    ];

    for (name, expected_prefix) in probes {
        evaluator
            .obarray
            .set_symbol_function(name, Value::subr(intern(name)));
        let result = builtin_documentation(&mut evaluator, vec![Value::symbol(name)]).unwrap();
        let text = result
            .as_utf8_str()
            .expect("core subr documentation should return a string");
        assert!(
            text.starts_with(expected_prefix),
            "unexpected documentation text for {name}: {text:?}"
        );
        assert_ne!(text, "Built-in function.");
    }
}

#[test]
fn documentation_symbol_alias_to_builtin_returns_docstring() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();
    evaluator
        .obarray
        .set_symbol_function("alias-builtin", Value::symbol("car"));

    let result =
        builtin_documentation(&mut evaluator, vec![Value::symbol("alias-builtin")]).unwrap();
    let text = result
        .as_utf8_str()
        .expect("documentation alias to car should return a string");
    assert!(text.starts_with("Return the car of LIST.  If LIST is nil, return nil."));
    assert_ne!(text, "Built-in function.");
}

#[test]
fn documentation_prefers_function_documentation_property() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();
    evaluator
        .obarray
        .set_symbol_function("doc-prop", Value::fixnum(7));
    evaluator
        .obarray
        .put_property(
            "doc-prop",
            "function-documentation",
            Value::string("propdoc"),
        )
        .unwrap();

    let result = builtin_documentation(&mut evaluator, vec![Value::symbol("doc-prop")]);
    assert_eq!(result.unwrap().as_utf8_str(), Some("propdoc"));
}

#[test]
fn documentation_prefers_uninterned_symbol_function_documentation_property() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();

    let result = evaluator
        .eval_str(
            r#"(let ((s (make-symbol "doc-prop")))
                 (fset s (lambda () "lambda-doc" 1))
                 (put s 'function-documentation "propdoc")
                 (documentation s t))"#,
        )
        .unwrap();

    assert_eq!(result.as_utf8_str(), Some("propdoc"));
}

#[test]
fn documentation_integer_function_documentation_property_returns_nil() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();
    evaluator
        .obarray
        .set_symbol_function("doc-prop", Value::fixnum(7));
    evaluator
        .obarray
        .put_property("doc-prop", "function-documentation", Value::fixnum(9))
        .unwrap();

    let result = builtin_documentation(&mut evaluator, vec![Value::symbol("doc-prop")]);
    assert!(result.unwrap().is_nil());
}

#[test]
fn documentation_list_function_documentation_property_is_evaluated() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();
    evaluator
        .obarray
        .set_symbol_function("doc-prop", Value::fixnum(7));
    evaluator
        .obarray
        .put_property(
            "doc-prop",
            "function-documentation",
            Value::list(vec![Value::symbol("identity"), Value::string("doc")]),
        )
        .unwrap();

    let result = builtin_documentation(&mut evaluator, vec![Value::symbol("doc-prop")]);
    assert_eq!(result.unwrap().as_utf8_str(), Some("doc"));
}

#[test]
fn documentation_symbol_function_documentation_property_is_evaluated() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();
    evaluator
        .obarray
        .set_symbol_function("doc-prop", Value::fixnum(7));
    evaluator
        .obarray
        .put_property("doc-prop", "function-documentation", Value::symbol("t"))
        .unwrap();

    let result = builtin_documentation(&mut evaluator, vec![Value::symbol("doc-prop")]);
    assert!(result.unwrap().is_truthy());
}

#[test]
fn documentation_vector_function_documentation_property_is_evaluated() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();
    evaluator
        .obarray
        .set_symbol_function("doc-prop", Value::fixnum(7));
    evaluator
        .obarray
        .put_property(
            "doc-prop",
            "function-documentation",
            Value::vector(vec![Value::fixnum(1), Value::fixnum(2)]),
        )
        .unwrap();

    let result = builtin_documentation(&mut evaluator, vec![Value::symbol("doc-prop")]);
    assert!(result.unwrap().is_vector());
}

#[test]
fn documentation_unbound_symbol_function_documentation_property_errors() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();
    evaluator
        .obarray
        .set_symbol_function("doc-prop", Value::fixnum(7));
    evaluator
        .obarray
        .put_property(
            "doc-prop",
            "function-documentation",
            Value::symbol("doc-prop-unbound"),
        )
        .unwrap();

    let result = builtin_documentation(&mut evaluator, vec![Value::symbol("doc-prop")]);
    match result {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "void-variable"),
        other => panic!("expected void-variable signal, got {other:?}"),
    }
}

#[test]
fn documentation_invalid_form_function_documentation_property_errors() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();
    evaluator
        .obarray
        .set_symbol_function("doc-prop", Value::fixnum(7));
    evaluator
        .obarray
        .put_property(
            "doc-prop",
            "function-documentation",
            Value::list(vec![Value::fixnum(1), Value::fixnum(2)]),
        )
        .unwrap();

    let result = builtin_documentation(&mut evaluator, vec![Value::symbol("doc-prop")]);
    match result {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "invalid-function"),
        other => panic!("expected invalid-function signal, got {other:?}"),
    }
}

#[test]
fn documentation_quoted_lambda_docstring() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();
    let quoted = Value::list(vec![
        Value::symbol("lambda"),
        Value::list(vec![Value::symbol("x")]),
        Value::string("d"),
        Value::symbol("x"),
    ]);

    let result = builtin_documentation(&mut evaluator, vec![quoted]).unwrap();
    assert_eq!(result.as_utf8_str(), Some("d"));
}

#[test]
fn documentation_quoted_lambda_without_docstring_returns_nil() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();
    let quoted = Value::list(vec![
        Value::symbol("lambda"),
        Value::list(vec![Value::symbol("x")]),
        Value::symbol("x"),
    ]);

    let result = builtin_documentation(&mut evaluator, vec![quoted]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn documentation_vector_designator_returns_keyboard_macro_doc() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();
    let result =
        builtin_documentation(&mut evaluator, vec![Value::vector(vec![Value::fixnum(1)])]).unwrap();
    assert_eq!(result.as_utf8_str(), Some("Keyboard macro."));
}

#[test]
fn documentation_string_designator_returns_keyboard_macro_doc() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();
    let result = builtin_documentation(&mut evaluator, vec![Value::string("abc")]).unwrap();
    assert_eq!(result.as_utf8_str(), Some("Keyboard macro."));
}

#[test]
fn documentation_quoted_macro_payload_matches_oracle_shape() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();
    let quoted = Value::list(vec![
        Value::symbol("macro"),
        Value::list(vec![Value::symbol("x")]),
        Value::string("md"),
        Value::symbol("x"),
    ]);

    let result = builtin_documentation(&mut evaluator, vec![quoted]);
    match result {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "invalid-function");
            assert_eq!(
                sig.data.first(),
                Some(&Value::list(vec![
                    Value::list(vec![Value::symbol("x")]),
                    Value::string("md"),
                    Value::symbol("x"),
                ]))
            );
        }
        other => panic!("expected invalid-function signal, got {other:?}"),
    }
}

#[test]
fn documentation_empty_quoted_macro_errors_void_function_nil() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();
    let quoted = Value::list(vec![Value::symbol("macro")]);

    let result = builtin_documentation(&mut evaluator, vec![quoted]);
    match result {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "void-function");
            assert!(sig.data.first().is_some_and(|v| v.is_nil()));
        }
        other => panic!("expected void-function signal, got {other:?}"),
    }
}

#[test]
fn documentation_non_symbol_non_function_errors_invalid_function() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();
    let result = builtin_documentation(
        &mut evaluator,
        vec![Value::list(vec![Value::fixnum(1), Value::fixnum(2)])],
    );
    assert!(result.is_err());
}

#[test]
fn documentation_wrong_arity() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();
    let result = builtin_documentation(&mut evaluator, vec![]);
    assert!(result.is_err());
}

#[test]
fn startup_doc_quote_style_display_handles_backtick_pairs() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        startup_doc_quote_style_display("`C source code`."),
        "‘C source code’."
    );
    assert_eq!(
        startup_doc_quote_style_display("`default-directory'"),
        "‘default-directory’"
    );
    assert_eq!(
        startup_doc_quote_style_display("Keymap for subcommands of \\`C-x 4'."),
        "Keymap for subcommands of C-x 4."
    );
}

#[test]
fn documentation_property_eval_returns_string_property() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();
    evaluator
        .obarray
        .put_property("doc-sym", "variable-documentation", Value::string("doc"))
        .unwrap();

    let result = builtin_documentation_property(
        &mut evaluator,
        vec![
            Value::symbol("doc-sym"),
            Value::symbol("variable-documentation"),
        ],
    )
    .unwrap();
    assert_eq!(result.as_utf8_str(), Some("doc"));
}

#[test]
fn documentation_property_eval_substitutes_command_keys_unless_raw() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();
    // See `documentation_substitutes_command_keys_unless_raw' for why
    // help.el must be loaded before exercising the substitute path.
    crate::test_utils::load_minimal_gnu_help_runtime(&mut evaluator);
    evaluator
        .obarray
        .put_property(
            "doc-sym",
            "variable-documentation",
            Value::string("Press \\[save-buffer] to save."),
        )
        .unwrap();

    let display = builtin_documentation_property(
        &mut evaluator,
        vec![
            Value::symbol("doc-sym"),
            Value::symbol("variable-documentation"),
        ],
    )
    .unwrap();
    let raw = builtin_documentation_property(
        &mut evaluator,
        vec![
            Value::symbol("doc-sym"),
            Value::symbol("variable-documentation"),
            Value::T,
        ],
    )
    .unwrap();

    let display = display
        .as_utf8_str()
        .expect("display documentation-property string");
    let raw = raw
        .as_utf8_str()
        .expect("raw documentation-property string");
    assert!(display.contains("save-buffer"));
    assert!(!display.contains("\\["));
    assert!(raw.contains("\\[save-buffer]"));
}

#[test]
fn documentation_property_eval_integer_property_returns_nil() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();
    evaluator
        .obarray
        .put_property("doc-sym", "variable-documentation", Value::fixnum(7))
        .unwrap();

    let result = builtin_documentation_property(
        &mut evaluator,
        vec![
            Value::symbol("doc-sym"),
            Value::symbol("variable-documentation"),
        ],
    )
    .unwrap();
    assert!(result.is_nil());
}

#[test]
fn documentation_stringp_accepts_compiled_file_refs() {
    crate::test_utils::init_test_tracing();
    let doc_ref = Value::cons(Value::string("/tmp/docref.elc"), Value::fixnum(17));
    let result = builtin_documentation_stringp(vec![doc_ref]).unwrap();
    assert!(result.is_truthy());
}

#[test]
fn documentation_property_eval_reads_compiled_doc_ref() {
    crate::test_utils::init_test_tracing();
    let unique = format!(
        "neovm-docref-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time should be after epoch")
            .as_nanos()
    );
    let path = std::env::temp_dir().join(format!("{unique}.elc"));
    std::fs::write(&path, b"#@11 compiled doc\x1f").expect("write doc fixture");

    let mut evaluator = super::super::eval::Context::new();
    evaluator
        .obarray
        .put_property(
            "doc-sym",
            "variable-documentation",
            Value::cons(
                Value::string(path.to_string_lossy().into_owned()),
                Value::fixnum(5),
            ),
        )
        .unwrap();

    let result = builtin_documentation_property(
        &mut evaluator,
        vec![
            Value::symbol("doc-sym"),
            Value::symbol("variable-documentation"),
        ],
    )
    .unwrap();

    assert_eq!(result.as_utf8_str(), Some("compiled doc"));

    let _ = std::fs::remove_file(path);
}

#[test]
fn documentation_property_eval_load_path_integer_property_returns_string() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = snarfed_context();
    let result = builtin_documentation_property(
        &mut evaluator,
        vec![
            Value::symbol("load-path"),
            Value::symbol("variable-documentation"),
        ],
    )
    .unwrap();
    assert!(
        result
            .as_utf8_str()
            .is_some_and(|s| s.contains("List of directories to search for files to load"))
    );
}

#[test]
fn documentation_property_eval_fill_column_uses_full_gnu_per_buffer_doc() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = snarfed_context();
    let result = builtin_documentation_property(
        &mut evaluator,
        vec![
            Value::symbol("fill-column"),
            Value::symbol("variable-documentation"),
            Value::T,
        ],
    )
    .unwrap();
    let doc = result
        .as_utf8_str()
        .expect("fill-column documentation-property should return a string");

    assert!(doc.contains("automatic line-wrapping"));
    assert!(doc.contains("fill-region"));
    assert!(doc.contains("current-fill-column"));
    assert!(doc.contains("\\[set-fill-column]"));
    assert!(!doc.contains("\\\\[set-fill-column]"));
}

#[test]
fn documentation_property_eval_load_path_raw_t_preserves_ascii_quotes() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = snarfed_context();
    let display = builtin_documentation_property(
        &mut evaluator,
        vec![
            Value::symbol("load-path"),
            Value::symbol("variable-documentation"),
            Value::NIL,
        ],
    )
    .unwrap();
    let raw = builtin_documentation_property(
        &mut evaluator,
        vec![
            Value::symbol("load-path"),
            Value::symbol("variable-documentation"),
            Value::T,
        ],
    )
    .unwrap();
    let display = display
        .as_utf8_str()
        .expect("display documentation-property should return a string");
    let raw = raw
        .as_utf8_str()
        .expect("raw documentation-property should return a string");

    assert_ne!(display, raw);
    assert!(display.contains("‘default-directory’"));
    assert!(raw.contains("`default-directory'"));
}

/// `ctl-x-4-map`'s documentation, on the surface that has one.
///
/// This test used to run against a bare `Context` and assert that RAW and
/// DISPLAY were *equal*.  Both halves were artefacts of a bootstrap seed that
/// ledger 178 removed, and the seed's text is why:
///
/// ```ignore
/// ("ctl-x-4-map", "Keymap for subcommands of C-x 4."),
/// ```
///
/// GNU's text is `"Keymap for subcommands of \\`C-x 4'."`
/// (`lisp/subr.el:1732-1733`), so the row had hard-coded the *rendered* form
/// -- the output of `substitute-command-keys`, not the docstring -- which made
/// RAW and DISPLAY agree for a reason GNU does not share.  And nothing else
/// could have answered in a bare `Context`: `ctl-x-4-map` is a Lisp `defvar`,
/// not a `DEFVAR_*`, so it is absent from `var_docs::gnu_table`, and GNU's own
/// answer before `subr.el` runs is nil.
///
/// Entry 176 recorded exactly this: a bare `Context` is not a GNU-comparable
/// surface for a Lisp `defvar`.  So the test moves to the one that is -- the
/// runtime-startup image, where the `defvar` has run -- and pins what both
/// editors answer there.  Measured `-Q --batch`, byte-identical in GNU
/// 31.0.90 and in this port: DISPLAY is `"Keymap for subcommands of C-x 4."`
/// with a `help-key-binding` face on the key, RAW keeps the `\\`...'` markup,
/// and the two therefore **differ**.
#[test]
fn documentation_property_eval_ctl_x_4_map_raw_keeps_the_markup_display_renders() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        "(documentation-property 'ctl-x-4-map 'variable-documentation)
         (documentation-property 'ctl-x-4-map 'variable-documentation t)",
    );

    assert_eq!(
        results[0],
        "OK #(\"Keymap for subcommands of C-x 4.\" 26 31 \
         (font-lock-face help-key-binding face help-key-binding))"
    );
    assert_eq!(results[1], "OK \"Keymap for subcommands of \\\\`C-x 4'.\"");
    assert_ne!(results[0], results[1]);
}

/// And the same name in a bare `Context`, which is the fact ledger 178 changed.
///
/// Nothing has defined `ctl-x-4-map` yet, so there is no documentation to
/// give: GNU's `Fsnarf_documentation` would not have recorded one for an
/// unbound name (`src/doc.c:606-613`, where the `Fput` is the whole branch),
/// and its Lisp `defvar` has not run.  Before entry 178 the bootstrap seed
/// answered here.
#[test]
fn documentation_property_eval_ctl_x_4_map_is_nil_before_subr_el_defines_it() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();
    assert!(
        !evaluator.obarray().boundp_id(intern("ctl-x-4-map")),
        "a bare Context has not loaded lisp/subr.el, so the name is unbound"
    );
    let doc = builtin_documentation_property(
        &mut evaluator,
        vec![
            Value::symbol("ctl-x-4-map"),
            Value::symbol("variable-documentation"),
            Value::NIL,
        ],
    )
    .unwrap();
    assert!(doc.is_nil(), "expected nil, got {doc:?}");
}

#[test]
fn documentation_property_eval_case_fold_search_integer_property_returns_string() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = snarfed_context();
    let result = builtin_documentation_property(
        &mut evaluator,
        vec![
            Value::symbol("case-fold-search"),
            Value::symbol("variable-documentation"),
        ],
    )
    .unwrap();
    assert!(
        result
            .as_utf8_str()
            .is_some_and(|s| s.contains("searches and matches should ignore case"))
    );
}

#[test]
fn documentation_property_eval_unread_command_events_integer_property_returns_string() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = snarfed_context();
    let result = builtin_documentation_property(
        &mut evaluator,
        vec![
            Value::symbol("unread-command-events"),
            Value::symbol("variable-documentation"),
        ],
    )
    .unwrap();
    assert!(
        result
            .as_utf8_str()
            .is_some_and(|s| s.contains("events to be read as the command input"))
    );
}

#[test]
fn documentation_property_eval_auto_hscroll_mode_integer_property_returns_string() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = snarfed_context();
    let result = builtin_documentation_property(
        &mut evaluator,
        vec![
            Value::symbol("auto-hscroll-mode"),
            Value::symbol("variable-documentation"),
        ],
    )
    .unwrap();
    assert!(
        result
            .as_utf8_str()
            .is_some_and(|s| s.contains("automatic horizontal scrolling of windows"))
    );
}

#[test]
fn documentation_property_eval_auto_composition_mode_integer_property_returns_string() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = snarfed_context();
    let result = builtin_documentation_property(
        &mut evaluator,
        vec![
            Value::symbol("auto-composition-mode"),
            Value::symbol("variable-documentation"),
        ],
    )
    .unwrap();
    assert!(
        result
            .as_utf8_str()
            .is_some_and(|s| s.contains("Auto-Composition mode is enabled"))
    );
}

#[test]
fn documentation_property_eval_coding_system_alist_integer_property_returns_string() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = snarfed_context();
    let result = builtin_documentation_property(
        &mut evaluator,
        vec![
            Value::symbol("coding-system-alist"),
            Value::symbol("variable-documentation"),
        ],
    )
    .unwrap();
    assert!(
        result
            .as_utf8_str()
            .is_some_and(|s| s.contains("Alist of coding system names"))
    );
}

#[test]
fn documentation_property_eval_debug_on_message_integer_property_returns_string() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = snarfed_context();
    let result = builtin_documentation_property(
        &mut evaluator,
        vec![
            Value::symbol("debug-on-message"),
            Value::symbol("variable-documentation"),
        ],
    )
    .unwrap();
    assert!(
        result
            .as_utf8_str()
            .is_some_and(|s| s.contains("debug if a message matching this regexp is displayed"))
    );
}

#[test]
fn documentation_property_eval_display_hourglass_integer_property_returns_string() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = snarfed_context();
    let result = builtin_documentation_property(
        &mut evaluator,
        vec![
            Value::symbol("display-hourglass"),
            Value::symbol("variable-documentation"),
        ],
    )
    .unwrap();
    assert!(
        result
            .as_utf8_str()
            .is_some_and(|s| s.contains("show an hourglass pointer"))
    );
}

#[test]
fn documentation_property_eval_exec_directory_integer_property_returns_string() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = snarfed_context();
    let result = builtin_documentation_property(
        &mut evaluator,
        vec![
            Value::symbol("exec-directory"),
            Value::symbol("variable-documentation"),
        ],
    )
    .unwrap();
    assert!(
        result
            .as_utf8_str()
            .is_some_and(|s| s.contains("Directory for executables for Emacs to invoke"))
    );
}

#[test]
fn documentation_property_eval_frame_title_format_integer_property_returns_string() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = snarfed_context();
    let result = builtin_documentation_property(
        &mut evaluator,
        vec![
            Value::symbol("frame-title-format"),
            Value::symbol("variable-documentation"),
        ],
    )
    .unwrap();
    assert!(
        result
            .as_utf8_str()
            .is_some_and(|s| s.contains("Template for displaying the title bar of visible frames"))
    );
}

#[test]
fn documentation_property_eval_header_line_format_integer_property_returns_string() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = snarfed_context();
    let result = builtin_documentation_property(
        &mut evaluator,
        vec![
            Value::symbol("header-line-format"),
            Value::symbol("variable-documentation"),
        ],
    )
    .unwrap();
    assert!(
        result
            .as_utf8_str()
            .is_some_and(|s| s.contains("controls the header line"))
    );
}

#[test]
fn documentation_property_eval_input_method_function_integer_property_returns_string() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = snarfed_context();
    let result = builtin_documentation_property(
        &mut evaluator,
        vec![
            Value::symbol("input-method-function"),
            Value::symbol("variable-documentation"),
        ],
    )
    .unwrap();
    assert!(
        result
            .as_utf8_str()
            .is_some_and(|s| s.contains("implements the current input method"))
    );
}

#[test]
fn documentation_property_eval_load_suffixes_integer_property_returns_string() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = snarfed_context();
    let result = builtin_documentation_property(
        &mut evaluator,
        vec![
            Value::symbol("load-suffixes"),
            Value::symbol("variable-documentation"),
        ],
    )
    .unwrap();
    assert!(
        result
            .as_utf8_str()
            .is_some_and(|s| s.contains("suffixes for Emacs Lisp files and dynamic modules"))
    );
}

/// `native-comp-eln-load-path` is declared inside `#ifdef HAVE_NATIVE_COMP`,
/// so a build without native compilation does not have the variable and GNU
/// records no documentation for it.
///
/// GNU's `DEFVAR_LISP ("native-comp-eln-load-path", ...)` is at
/// `src/comp.c:5742`, between the `#ifdef HAVE_NATIVE_COMP` that opens
/// `syms_of_comp` (`src/comp.c:5568`) and the `#endif` that closes it
/// (`src/comp.c:5826`); only `defsubr (&Snative_comp_available_p)`
/// (`src/comp.c:5828`) sits outside.  Every Lisp reference is a valueless
/// `(defvar native-comp-eln-load-path)` -- `lisp/startup.el:520`,
/// `lisp/subr.el:3333`, `lisp/emacs-lisp/comp.el:43`,
/// `lisp/emacs-lisp/comp-common.el:34` -- which makes the name special without
/// binding it, and the one `setq` (`lisp/startup.el:538`) is reached only
/// behind `(featurep 'native-compile)`, itself provided inside the same
/// `#ifdef` (`src/comp.c:5825`).
///
/// This port is such a build: `native-comp-available-p` answers nil
/// unconditionally, and nothing here ever binds the name.  So is the GNU
/// binary the oracle runs against, measured directly -- `(boundp
/// 'native-comp-eln-load-path)`, `(native-comp-available-p)` and
/// `(featurep 'native-compile)` are all nil there, and
/// `(documentation-property 'native-comp-eln-load-path
/// 'variable-documentation)` is nil.
///
/// Three assertions, because "nil" alone would also pass if the doc record had
/// simply been lost:
///
/// 1. the name is UNBOUND here, which is the reason;
/// 2. the generated table still CARRIES the record, because GNU's `etc/DOC`
///    carries it too -- `make-docfile` is a text scanner that ignores the
///    preprocessor and `comp.o` is unconditionally in `base_obj`
///    (`src/Makefile.in:459`, `470`, `667`);
/// 3. `documentation-property` nevertheless answers nil, because
///    `Fsnarf_documentation` gates the `Fput` on `Fboundp`
///    (`src/doc.c:603-615`) -- see the comment at `src/doc.c:586-595`, which
///    says in as many words that this is how docs for names another
///    configuration declares are kept out.
///
/// Until ledger 173 added that gate this test asserted the doc string, i.e. it
/// pinned a divergence: the port answered where GNU answers nil.
#[test]
fn documentation_property_eval_native_comp_eln_load_path_is_nil_when_unbound() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();

    let id = crate::emacs_core::intern::intern("native-comp-eln-load-path");
    assert!(
        !evaluator.obarray().boundp_id(id),
        "this build must not bind a variable GNU declares only under HAVE_NATIVE_COMP"
    );
    assert!(
        crate::emacs_core::var_docs::gnu_table::GNU_VAR_DOCS
            .iter()
            .any(|(name, doc)| *name == "native-comp-eln-load-path"
                && doc.contains("native-compiled *.eln files")),
        "the DOC record must stay in the table; GNU's etc/DOC has it too"
    );

    let result = builtin_documentation_property(
        &mut evaluator,
        vec![
            Value::symbol("native-comp-eln-load-path"),
            Value::symbol("variable-documentation"),
        ],
    )
    .unwrap();
    assert!(
        result.is_nil(),
        "GNU records no documentation for an unbound name, so documentation-property answers nil; got {result:?}"
    );
}

#[test]
fn documentation_property_eval_process_environment_integer_property_returns_string() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = snarfed_context();
    let result = builtin_documentation_property(
        &mut evaluator,
        vec![
            Value::symbol("process-environment"),
            Value::symbol("variable-documentation"),
        ],
    )
    .unwrap();
    assert!(
        result
            .as_utf8_str()
            .is_some_and(|s| s.contains("environment variables for subprocesses"))
    );
}

#[test]
fn documentation_property_eval_scroll_margin_integer_property_returns_string() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = snarfed_context();
    let result = builtin_documentation_property(
        &mut evaluator,
        vec![
            Value::symbol("scroll-margin"),
            Value::symbol("variable-documentation"),
        ],
    )
    .unwrap();
    assert!(
        result
            .as_utf8_str()
            .is_some_and(|s| s.contains("margin at the top and bottom"))
    );
}

#[test]
fn documentation_property_eval_truncate_partial_width_windows_integer_property_returns_string() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = snarfed_context();
    let result = builtin_documentation_property(
        &mut evaluator,
        vec![
            Value::symbol("truncate-partial-width-windows"),
            Value::symbol("variable-documentation"),
        ],
    )
    .unwrap();
    assert!(
        result
            .as_utf8_str()
            .is_some_and(|s| s.contains("windows narrower than the frame"))
    );
}

#[test]
fn documentation_property_eval_yes_or_no_prompt_integer_property_returns_string() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = snarfed_context();
    let result = builtin_documentation_property(
        &mut evaluator,
        vec![
            Value::symbol("yes-or-no-prompt"),
            Value::symbol("variable-documentation"),
        ],
    )
    .unwrap();
    assert!(
        result
            .as_utf8_str()
            .is_some_and(|s| s.contains("append when"))
    );
}

#[test]
fn documentation_property_eval_debug_on_error_integer_property_returns_string() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = snarfed_context();
    let result = builtin_documentation_property(
        &mut evaluator,
        vec![
            Value::symbol("debug-on-error"),
            Value::symbol("variable-documentation"),
        ],
    )
    .unwrap();
    assert!(
        result
            .as_utf8_str()
            .is_some_and(|s| s.contains("Non-nil means enter debugger if an error is signaled"))
    );
}

#[test]
fn documentation_property_eval_list_property_is_evaluated() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();
    evaluator
        .obarray
        .put_property(
            "doc-sym",
            "variable-documentation",
            Value::list(vec![Value::symbol("identity"), Value::string("doc")]),
        )
        .unwrap();

    let result = builtin_documentation_property(
        &mut evaluator,
        vec![
            Value::symbol("doc-sym"),
            Value::symbol("variable-documentation"),
        ],
    )
    .unwrap();
    assert_eq!(result.as_utf8_str(), Some("doc"));
}

#[test]
fn documentation_property_eval_symbol_property_is_evaluated() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();
    evaluator
        .obarray
        .put_property("doc-sym", "variable-documentation", Value::symbol("t"))
        .unwrap();

    let result = builtin_documentation_property(
        &mut evaluator,
        vec![
            Value::symbol("doc-sym"),
            Value::symbol("variable-documentation"),
        ],
    )
    .unwrap();
    assert!(result.is_truthy());
}

#[test]
fn documentation_property_eval_vector_property_is_evaluated() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();
    evaluator
        .obarray
        .put_property(
            "doc-sym",
            "variable-documentation",
            Value::vector(vec![Value::fixnum(1), Value::fixnum(2)]),
        )
        .unwrap();

    let result = builtin_documentation_property(
        &mut evaluator,
        vec![
            Value::symbol("doc-sym"),
            Value::symbol("variable-documentation"),
        ],
    )
    .unwrap();
    assert!(result.is_vector());
}

#[test]
fn documentation_property_eval_unbound_symbol_property_errors() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();
    evaluator
        .obarray
        .put_property(
            "doc-sym",
            "variable-documentation",
            Value::symbol("doc-sym-unbound"),
        )
        .unwrap();

    let result = builtin_documentation_property(
        &mut evaluator,
        vec![
            Value::symbol("doc-sym"),
            Value::symbol("variable-documentation"),
        ],
    );
    match result {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "void-variable"),
        other => panic!("expected void-variable signal, got {other:?}"),
    }
}

#[test]
fn documentation_property_eval_invalid_form_property_errors() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();
    evaluator
        .obarray
        .put_property(
            "doc-sym",
            "variable-documentation",
            Value::list(vec![Value::fixnum(1), Value::fixnum(2)]),
        )
        .unwrap();

    let result = builtin_documentation_property(
        &mut evaluator,
        vec![
            Value::symbol("doc-sym"),
            Value::symbol("variable-documentation"),
        ],
    );
    match result {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "invalid-function"),
        other => panic!("expected invalid-function signal, got {other:?}"),
    }
}

#[test]
fn documentation_property_eval_non_symbol_prop_returns_nil() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();
    evaluator
        .obarray
        .put_property("doc-sym", "x", Value::string("v"))
        .unwrap();

    let result = builtin_documentation_property(
        &mut evaluator,
        vec![Value::symbol("doc-sym"), Value::fixnum(1)],
    )
    .unwrap();
    assert!(result.is_nil());
}

#[test]
fn documentation_property_eval_accepts_non_symbol_prop_when_present() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();

    let result = evaluator
        .eval_str(
            r#"(let ((p (cons 'k nil)))
                 (put 'doc-sym p "v")
                 (documentation-property 'doc-sym p t))"#,
        )
        .unwrap();

    assert_eq!(result.as_utf8_str(), Some("v"));
}

#[test]
fn documentation_property_eval_non_symbol_target_errors() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();
    let result = builtin_documentation_property(
        &mut evaluator,
        vec![Value::fixnum(1), Value::symbol("variable-documentation")],
    );
    assert!(result.is_err());
}

// =======================================================================
// Ledger 182: `Fsnarf_documentation` is a LAST WRITER, not a fallback
// =======================================================================

/// The whole entry in one assertion.
///
/// `indent-tabs-mode` is a `DEFVAR_PER_BUFFER` in `src/buffer.c` **and** a
/// `(define-minor-mode indent-tabs-mode ...)` in `lisp/simple.el:7639`, and
/// both editors have both.  The order decides which text survives:
///
/// - `lisp/loadup.el:251` -- `(load "simple")`, whose `define-minor-mode`
///   expands to a `defvar` with a docstring, so `src/eval.c:911` `Fput`s
///   "Non-nil if Indent-Tabs mode is enabled." onto the plist;
/// - `lisp/loadup.el:448` (GNU's `:476`) -- `(Snarf-documentation "DOC")`,
///   197 lines later, whose `Fput` (`src/doc.c:613`) puts `buffer.c`'s record
///   **over the top of it**.
///
/// Measured in GNU 31.0.90 `-Q --batch`:
/// `(get 'indent-tabs-mode 'variable-documentation)` is `641753`.  A fixnum,
/// not the string `simple.el` put there -- which is the whole proof that the
/// snarf overwrites rather than defers.  Over all 894 names of the DOC
/// stand-in, GNU has 762 bound and **762 integers**, with no string among
/// them.
#[test]
fn snarf_documentation_is_the_last_writer_over_a_preloaded_lisp_defvar() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        "(integerp (get 'indent-tabs-mode 'variable-documentation))
         (documentation-property 'indent-tabs-mode 'variable-documentation t)",
    );
    assert_eq!(results[0], "OK t");
    assert_eq!(
        results[1],
        "OK \"Indentation can insert tabs if this is non-nil.\""
    );
}

/// One `mapatoms` over the whole obarray, joined to the DOC stand-in on the
/// Rust side, rather than N per-name pins -- ledger 173's law, which ledger
/// 178 applied to the same table: *a predicate over rows that exist cannot
/// see a row that was never written*, so a per-name pin reports green the
/// moment the table it is about goes empty.
///
/// The empty state is closed here by asserting the POSITIVE count as well: an
/// emptied `GNU_VAR_DOCS`, a snarf that never runs, or a `loadup.el` that
/// stopped calling it all drive `installed` to zero and fail the first
/// assertion, before the diagonal has a chance to report zero for the wrong
/// reason.
#[test]
fn every_variable_the_doc_image_documents_answers_out_of_the_doc_image() {
    crate::test_utils::init_test_tracing();
    let eval = crate::test_utils::runtime_startup_context();
    let obarray = eval.obarray();
    let prop = crate::emacs_core::intern::intern("variable-documentation");

    let mut bound = 0_usize;
    let mut installed = 0_usize;
    let mut answered_from_lisp: Vec<&str> = Vec::new();
    for (name, _) in super::super::var_docs::gnu_table::GNU_VAR_DOCS {
        let Some(id) = crate::emacs_core::intern::lookup_interned(name) else {
            continue;
        };
        if !obarray.is_global_member(id) || !obarray.boundp_id(id) {
            continue;
        }
        bound += 1;
        let entry = crate::emacs_core::plist::plist_get(
            obarray.symbol_plist_id(id),
            &Value::from_sym_id(prop),
        );
        match entry {
            Some(value) if value.is_fixnum() => installed += 1,
            _ => answered_from_lisp.push(name),
        }
    }

    assert!(
        installed > 700,
        "the DOC stand-in installed {installed} records; an empty table, a \
         `Snarf-documentation' that no longer runs, or a `loadup.el' that \
         stopped calling it would all report a clean diagonal below"
    );
    assert_eq!(
        answered_from_lisp,
        Vec::<&str>::new(),
        "these names are documented by the DOC stand-in and bound, so GNU's \
         snarf overwrote whatever Lisp put on their plist (src/doc.c:613)"
    );
    assert_eq!(bound, installed);
}

/// And the same image from the obarray's side: what the snarf may not leave
/// behind.
///
/// `Fsnarf_documentation` writes a **fixnum** and only for a name `Fboundp`
/// accepts (`src/doc.c:606-613`), so an unbound symbol carrying a fixnum
/// `variable-documentation` is the state its gate exists to prevent, and a
/// fixnum `0` is `src/doc.c:433-434`'s reserved "there is no doc" -- which
/// `make-docfile` cannot emit and the DOC image cannot either, since every
/// record's text starts after a `^_V<name>\n` header.
///
/// **The entry-agnostic form of this diagonal does NOT belong to this
/// surface**, and finding that out is worth the test's existence.  Ledger 178
/// asserted "no unbound symbol carries a `variable-documentation`" of a bare
/// `Context`, where it is true because nothing has written one at all.  Asked
/// of the image `loadup` leaves behind, it is false in both editors by design:
/// fifteen names here carry a Lisp docstring while unbound --
/// `user-mail-address` (`lisp/startup.el:401-407`), `abbrev-file-name`
/// (`lisp/abbrev.el:45`), `compile-command`, `package-user-dir` and eleven
/// more -- because they are `defcustom`s with `:initialize
/// #'custom-initialize-delay`, which `lisp/custom.el:142-161` marks special
/// and deliberately leaves unbound until `startup.el` re-evaluates them.  That
/// is the very class GNU's snarf carves out with `!NILP (Fmemq (sym,
/// delayed_init))`.  The whole-image version of the diagonal is pinned across
/// editors instead, in
/// `crates/neovm-oracle-tests/src/snarf_documentation_last_writer.rs`, where both
/// answer 0 because `startup.el` has run by then.
#[test]
fn the_snarf_leaves_no_documentation_on_a_variable_the_image_does_not_bind() {
    crate::test_utils::init_test_tracing();
    // Names, not counts: a count says a diagonal moved and a name says which
    // row moved it, and this test's whole job is to be read by whoever breaks
    // it.  The third element is the positive control -- an emptied DOC image
    // or a `loadup.el` that stopped calling `Snarf-documentation` makes the
    // first two nil for the wrong reason.
    let results = bootstrap_eval_all(
        "(list
           (let (names)
             (mapatoms (lambda (s)
               (if (integerp (get s 'variable-documentation))
                   (if (boundp s) nil (setq names (cons s names))))))
             (sort names #'string<))
           (let (names)
             (mapatoms (lambda (s)
               (if (eq (get s 'variable-documentation) 0)
                   (setq names (cons s names)))))
             (sort names #'string<))
           (let ((n 0))
             (mapatoms (lambda (s)
               (if (integerp (get s 'variable-documentation)) (setq n (1+ n)))))
             (> n 700)))",
    );
    assert_eq!(results[0], "OK (nil nil t)");
}

/// `oblookup` does not intern (`src/doc.c:596-600`): a DOC record whose name
/// this build has no symbol for is skipped by `if (SYMBOLP (sym))`.
///
/// The table is generated from ALL of GNU's `src/*.c`, so it names variables
/// no build declares -- 132 of the 894 are unbound in GNU's own image.  A
/// snarf that used `intern` would put every one of them in the obarray, which
/// is a state GNU has zero of and which ledger 178's `mapatoms` diagonal is
/// the check for.
#[test]
fn snarfing_the_doc_image_interns_nothing() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();
    let before = evaluator.obarray().len();
    let installed = super::snarf_variable_documentation(&mut evaluator.obarray);
    assert!(installed > 700, "installed {installed} records");
    assert_eq!(evaluator.obarray().len(), before);
    assert!(
        crate::emacs_core::intern::lookup_interned("w32-quit-key")
            .is_none_or(|id| !evaluator.obarray().is_global_member(id)),
        "`w32-quit-key' has a DOC record and no declaration on this platform"
    );
}

/// The snarf overwrites: run it over a plist that already carries a Lisp
/// docstring for the same name, and the docstring is gone.
///
/// This is `Fput`, not a write-if-absent, and it is the difference between
/// GNU's design and the one this port had.
#[test]
fn snarf_documentation_overwrites_a_docstring_already_on_the_plist() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();
    evaluator
        .obarray
        .put_property(
            "indent-tabs-mode",
            "variable-documentation",
            Value::string("Non-nil if Indent-Tabs mode is enabled."),
        )
        .unwrap();

    super::snarf_variable_documentation(&mut evaluator.obarray);

    let result = builtin_documentation_property(
        &mut evaluator,
        vec![
            Value::symbol("indent-tabs-mode"),
            Value::symbol("variable-documentation"),
            Value::T,
        ],
    )
    .unwrap();
    assert_eq!(
        result.as_utf8_str(),
        Some("Indentation can insert tabs if this is non-nil.")
    );
}

/// `get_doc_string`'s validity check (`src/doc.c:254-260`), which is why the
/// DOC stand-in is a byte image and not a row index.
///
/// GNU answers nil for a fixnum that does not point just past a `^_V<name>\n`
/// header -- measured in GNU 31.0.90 `-Q --batch`,
/// `(put 'x 'variable-documentation 7)` then `documentation-property` is nil,
/// because offset 7 lands inside the first record's own header.  A row index
/// has no invalid values and could not reproduce that.
#[test]
fn a_position_that_does_not_point_at_a_doc_record_answers_nil() {
    crate::test_utils::init_test_tracing();
    let image = super::super::var_docs::doc_image();
    assert!(image.text_at(0).is_none());
    assert!(image.text_at(7).is_none());
    assert!(image.text_at(-1).is_none());
    assert!(image.text_at(i64::MAX).is_none());
}

/// Every record in the DOC stand-in round-trips: the position the snarf would
/// store resolves back to that row's exact text.
#[test]
fn every_doc_image_record_round_trips_through_its_position() {
    crate::test_utils::init_test_tracing();
    let mut evaluator = super::super::eval::Context::new();
    let obarray = &mut evaluator.obarray;
    let mut checked = 0_usize;
    for (name, doc) in super::super::var_docs::gnu_table::GNU_VAR_DOCS {
        let Some(id) = crate::emacs_core::intern::lookup_interned(name) else {
            continue;
        };
        let Some(found) = super::super::var_docs::SnarfedVariable::if_bound_in(obarray, id, name)
            .and_then(super::super::var_docs::lookup)
        else {
            continue;
        };
        let position = found.position();
        assert_eq!(
            super::super::var_docs::doc_image().text_at(position),
            Some(*doc),
            "record for {name} at position {position}"
        );
        checked += 1;
    }
    assert!(checked > 700, "checked {checked} records");
}

// ---------------------------------------------------------------------------
// `documentation-dynamic-reload`: the retry (`src/doc.c:311-317`, `:365-375`,
// `:441-447`), which ledger 182 §10 recorded as declared here and not
// implemented.
// ---------------------------------------------------------------------------

/// The `(FILE . POS)` arm, on a file this test writes so the fixture cannot
/// drift.
///
/// `#@14 ` is five bytes, so position 5 is the first byte of the record and
/// `\037` ends it -- `make-docfile`'s and the byte compiler's dynamic-docstring
/// layout, and what `src/doc.c:240-263` validates.
///
/// Moving the position off the record is what **recompiling** a preloaded
/// `.elc` does to every reference an image already holds into it -- the offset
/// is a literal `(#$ . N)` in the compiled file, so it only moves when the
/// compiler writes a new one.  This port's dumped image carries **1835** such
/// references.  (Prefixing an existing `.elc` with bytes does NOT model that
/// state: the recorded `N` does not move either, and GNU answers nil there too.
/// Measured both ways.)
///
/// The reload-off row is the control.  Without it a green here would also be
/// green on a port that answers the docstring by ignoring the position
/// entirely.
#[test]
fn a_stale_reference_into_a_compiled_file_is_reread_and_retried() {
    crate::test_utils::init_test_tracing();
    // Under the repo's own `tmp/`, not `/tmp`: this project's temp output goes
    // in the tree (and `tmp/` is ignored), so a fixture cannot land on a
    // volume that has no room for it.
    let dir = std::path::Path::new(env!("CARGO_WORKSPACE_DIR")).join("tmp/l194-doc-reread-unit");
    std::fs::create_dir_all(&dir).expect("fixture dir");
    let path = dir.join("victim.el");
    let escaped = path.display().to_string();
    std::fs::write(
        &path,
        format!("#@14 doc for 194.\u{1f}\n(put 'l194-victim 'variable-documentation (cons \"{escaped}\" 5))\n"),
    )
    .expect("fixture");

    let results = bootstrap_eval_all(&format!(
        r#"(set 'documentation-dynamic-reload nil)
           (load "{escaped}" nil t t)
           (documentation-property 'l194-victim 'variable-documentation t)
           (put 'l194-victim 'variable-documentation (cons "{escaped}" 9))
           (documentation-property 'l194-victim 'variable-documentation t)
           (get 'l194-victim 'variable-documentation)
           (set 'documentation-dynamic-reload t)
           (documentation-property 'l194-victim 'variable-documentation t)
           (get 'l194-victim 'variable-documentation)"#
    ));
    let _ = std::fs::remove_file(&path);

    assert_eq!(
        results[2], "OK \"doc for 194.\"",
        "the fresh reference reads"
    );
    assert_eq!(
        results[4], "OK nil",
        "reload off: the stale reference is nil"
    );
    assert_eq!(
        results[5],
        format!("OK (\"{escaped}\" . 9)"),
        "reload off rewrites nothing"
    );
    assert_eq!(
        results[7], "OK \"doc for 194.\"",
        "reload on: reread and retry"
    );
    assert_eq!(
        results[8],
        format!("OK (\"{escaped}\" . 5)"),
        "and the reread reinstalled the reference"
    );
}

/// `try_reload = false` is assigned before the `goto`, so the reread happens
/// **once** even when it does not repair anything.
///
/// The count is the assertion that matters: nil alone is also what a port with
/// no retry answers, and a port that looped would never reach the assertion at
/// all.
#[test]
fn a_reread_that_does_not_repair_the_reference_happens_exactly_once() {
    crate::test_utils::init_test_tracing();
    // Under the repo's own `tmp/`, not `/tmp`: this project's temp output goes
    // in the tree (and `tmp/` is ignored), so a fixture cannot land on a
    // volume that has no room for it.
    let dir = std::path::Path::new(env!("CARGO_WORKSPACE_DIR")).join("tmp/l194-doc-reread-unit");
    std::fs::create_dir_all(&dir).expect("fixture dir");
    let path = dir.join("norepair.el");
    let escaped = path.display().to_string();
    std::fs::write(
        &path,
        "(setq l194-load-count (1+ (or (and (boundp 'l194-load-count) l194-load-count) 0)))\n",
    )
    .expect("fixture");

    let results = bootstrap_eval_all(&format!(
        r#"(set 'documentation-dynamic-reload t)
           (set 'l194-load-count 0)
           (put 'l194-nr 'variable-documentation (cons "{escaped}" 5))
           (documentation-property 'l194-nr 'variable-documentation t)
           l194-load-count"#
    ));
    let _ = std::fs::remove_file(&path);

    assert_eq!(results[3], "OK nil", "the retry still cannot resolve it");
    assert_eq!(results[4], "OK 1", "and the file was reread exactly once");
}

/// The bare-fixnum arm: GNU's `reread_doc_file (Fcar_safe (doc))` with a nil
/// car re-runs `Fsnarf_documentation`, which `Fput`s the correct position back
/// over the corrupted one -- so the plist is REPAIRED and the retry answers.
///
/// Measured in GNU 31.0.90 `-Q --batch`, with the reload off as the control:
///
/// ```text
/// (put 'case-fold-search 'variable-documentation 7)
///   reload off -> nil, plist stays 7
///   reload on  -> "Non-nil if searches and matches should ignore case.",
///                 plist is 556387 again
/// ```
#[test]
fn a_doc_position_that_is_not_a_record_is_repaired_by_the_reread() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(setq l194-orig (get 'case-fold-search 'variable-documentation))
           (integerp l194-orig)
           (set 'documentation-dynamic-reload nil)
           (put 'case-fold-search 'variable-documentation 7)
           (documentation-property 'case-fold-search 'variable-documentation t)
           (get 'case-fold-search 'variable-documentation)
           (set 'documentation-dynamic-reload t)
           (documentation-property 'case-fold-search 'variable-documentation t)
           (equal (get 'case-fold-search 'variable-documentation) l194-orig)"#,
    );
    assert_eq!(results[1], "OK t", "the entry is a snarfed integer");
    assert_eq!(results[4], "OK nil", "reload off: nil");
    assert_eq!(results[5], "OK 7", "reload off repairs nothing");
    assert_eq!(
        results[7], "OK \"Non-nil if searches and matches should ignore case.\"",
        "reload on: the re-snarf repaired the entry and the retry read it"
    );
    assert_eq!(results[8], "OK t", "and the entry is the original position");
}

/// `if (BASE_EQ (tem, make_fixnum (0))) tem = Qnil;` runs BEFORE the `FIXNUMP`
/// test (`src/doc.c:433-437`), so GNU's reserved "there is no doc" fixnum never
/// reaches `get_doc_string` and never triggers a reread.
///
/// The name is deliberately one the re-snarf WOULD repair: on `case-fold-search`
/// a wrongly-taken reread turns the reserved zero into a docstring, and the
/// plist column is the only thing that can see it -- the docstring column reads
/// nil either way.
#[test]
fn the_reserved_zero_is_not_a_stale_reference_and_is_never_reread() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_all(
        r#"(set 'documentation-dynamic-reload t)
           (put 'case-fold-search 'variable-documentation 0)
           (documentation-property 'case-fold-search 'variable-documentation t)
           (get 'case-fold-search 'variable-documentation)"#,
    );
    assert_eq!(results[2], "OK nil");
    assert_eq!(results[3], "OK 0", "the zero was not rewritten by a reread");
}
