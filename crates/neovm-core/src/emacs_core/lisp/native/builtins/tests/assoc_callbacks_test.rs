//! Resolver engagement and guard tests. GNU-refreshed Lisp behavior lives in
//! neovm-oracle-tests/src/assoc/tests/callback_seams.rs under both knob states.
use crate::emacs_core::eval::Context;
use crate::emacs_core::eval::assoc_predicate::PureAssocPredicate;
use crate::emacs_core::value::Value;

#[test]
fn assoc_callbacks_identify_pure_implementations_and_aliases() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    for (name, expected) in [
        ("eq", PureAssocPredicate::Eq),
        ("eql", PureAssocPredicate::Eql),
        ("equal", PureAssocPredicate::Equal),
        (
            "equal-including-properties",
            PureAssocPredicate::EqualIncludingProperties,
        ),
        ("string-equal", PureAssocPredicate::StringEqual),
    ] {
        eval.eval_str(&format!("(defalias 'assoc-callback-alias '{name})"))
            .expect("install builtin alias");
        eval.eval_str("(defalias 'assoc-callback-outer 'assoc-callback-alias)")
            .expect("install outer alias");
        let callable = eval.obarray().symbol_function(name).expect("builtin cell");
        for designator in [
            Value::symbol(name),
            Value::symbol("assoc-callback-outer"),
            callable,
        ] {
            let captured = eval
                .resolve_assoc_predicate(designator)
                .expect("builtin implementation must engage");
            assert_eq!(captured.pure, Some(expected), "{name}");
            assert_eq!(captured.callable, callable, "{name}");
        }
    }
}

#[test]
fn assoc_callbacks_cache_lambda_targets_and_observe_redefinition() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.eval_str("(defalias 'assoc-callback-lambda (lambda (entry key) (cons entry key)))")
        .expect("install lambda target");
    let designator = Value::symbol("assoc-callback-lambda");
    let captured = eval
        .resolve_assoc_predicate(designator)
        .expect("lambda is resolved");
    assert_eq!(captured.pure, None);
    assert_eq!(
        captured.callable,
        eval.obarray()
            .symbol_function("assoc-callback-lambda")
            .expect("lambda cell")
    );
    let roots = eval.save_specpdl_roots();
    eval.push_specpdl_root(captured.callable);
    let left = Value::symbol("entry");
    let right = Value::symbol("lookup");
    let expected = eval.apply2(designator, left, right).expect("general call");
    eval.push_specpdl_root(expected);
    let actual = eval
        .apply2_assoc_predicate(designator, &captured, left, right)
        .expect("cached call");
    assert!(crate::emacs_core::value::equal_value(&actual, &expected, 0));
    eval.eval_str("(fset 'assoc-callback-lambda (lambda (_entry _key) 'replacement))")
        .expect("replace lambda target");
    let expected = eval
        .apply2(designator, left, right)
        .expect("redefined general call");
    let actual = eval
        .apply2_assoc_predicate(designator, &captured, left, right)
        .expect("stale target fallback");
    assert_eq!(actual, expected);
    eval.restore_specpdl_roots(roots);
}

#[test]
fn assoc_callbacks_pure_guard_sees_alias_changes_and_preserves_subr_objects() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.eval_str("(defalias 'assoc-callback-eq 'eq)")
        .expect("install alias");
    let designator = Value::symbol("assoc-callback-eq");
    let captured = eval
        .resolve_assoc_predicate(designator)
        .expect("pure alias");
    let direct = eval
        .resolve_assoc_predicate(captured.callable)
        .expect("direct subr");
    let roots = eval.save_specpdl_roots();
    eval.push_specpdl_root(captured.callable);
    eval.eval_str("(fset 'assoc-callback-eq (lambda (_entry _key) 'changed))")
        .expect("change alias");
    let key = Value::fixnum(1);
    let expected = eval
        .apply2(designator, key, key)
        .expect("general alias call");
    let actual = eval
        .apply2_assoc_predicate(designator, &captured, key, key)
        .expect("stale pure fallback");
    assert_eq!(actual, expected);
    let expected = eval
        .apply2(direct.callable, key, key)
        .expect("general direct subr call");
    let actual = eval
        .apply2_assoc_predicate(direct.callable, &direct, key, key)
        .expect("direct object fallback");
    assert_eq!(actual, expected);
    eval.restore_specpdl_roots(roots);
}

#[test]
fn assoc_callbacks_decline_autoloads_invalid_values_and_compiler_overrides() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.eval_str("(autoload 'assoc-callback-autoload \"assoc-callback-missing-file\")")
        .expect("install autoload");
    for designator in [
        Value::symbol("assoc-callback-autoload"),
        Value::symbol("assoc-callback-void"),
        Value::fixnum(42),
    ] {
        assert!(eval.resolve_assoc_predicate(designator).is_none());
    }
    eval.eval_str("(setq internal--compiler-function-overrides '((eq . equal)))")
        .expect("enable compiler overrides");
    assert!(eval.resolve_assoc_predicate(Value::symbol("eq")).is_none());
}

#[test]
fn assoc_callbacks_cached_errors_restore_the_call_frame() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let designator = Value::symbol("string-equal");
    let captured = eval
        .resolve_assoc_predicate(designator)
        .expect("string comparison");
    let roots = eval.save_specpdl_roots();
    eval.push_specpdl_root(captured.callable);
    let left = Value::fixnum(17);
    let right = Value::string("key");
    eval.push_specpdl_root(right);
    let depth = eval.depth;
    let specpdl = eval.specpdl.len();
    let expected = crate::emacs_core::format_eval_result(
        &eval
            .apply2(designator, left, right)
            .map_err(crate::emacs_core::error::map_flow),
    );
    let actual = crate::emacs_core::format_eval_result(
        &eval
            .apply2_assoc_predicate(designator, &captured, left, right)
            .map_err(crate::emacs_core::error::map_flow),
    );
    assert_eq!(actual, expected);
    assert_eq!((eval.depth, eval.specpdl.len()), (depth, specpdl));
    eval.restore_specpdl_roots(roots);
}

#[test]
fn assoc_callbacks_resolve_and_call_named_bytecode() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::bytecode::opcode::Op;
    use crate::emacs_core::intern::intern;
    use crate::emacs_core::value::LambdaParams;

    let mut eval = Context::new();
    let mut bytecode = ByteCodeFunction::new(LambdaParams {
        required: vec![intern("entry"), intern("lookup")],
        optional: vec![],
        rest: None,
    });
    bytecode.lexical = true;
    bytecode.ops = vec![Op::StackRef(1), Op::StackRef(1), Op::List(2), Op::Return];
    bytecode.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(8);
    let callable = Value::make_bytecode(bytecode);
    eval.obarray
        .set_symbol_function("assoc-callback-bytecode", callable);
    let designator = Value::symbol("assoc-callback-bytecode");
    let captured = eval
        .resolve_assoc_predicate(designator)
        .expect("bytecode resolution");
    assert_eq!(captured.callable, callable);
    assert_eq!(captured.pure, None);
    let roots = eval.save_specpdl_roots();
    eval.push_specpdl_root(callable);
    for gc_stress in [false, true] {
        eval.gc_stress = gc_stress;
        let expected = eval
            .apply2(designator, Value::fixnum(1), Value::fixnum(2))
            .expect("general bytecode call");
        eval.push_specpdl_root(expected);
        let actual = eval
            .apply2_assoc_predicate(designator, &captured, Value::fixnum(1), Value::fixnum(2))
            .expect("resolved bytecode call");
        assert!(crate::emacs_core::value::equal_value(&actual, &expected, 0));
    }
    eval.restore_specpdl_roots(roots);
}
