//! Resolver engagement tests. Lisp behavior is pinned by GNU-refreshed forms in
//! `neovm-oracle-tests/src/sort/captured_predicate.rs` under the VM and default JIT.
//!
//! P1 F1 re-home of eval's builtins/tests/sort_capture_test.rs. Test names and
//! assertions are unchanged; only the imports moved to the signed test-support
//! facade (the private predicate/proof types stay in eval), the resolver is
//! driven through the product's `resolve_sort_predicate_for_test` route, and
//! `Context::new(unit_runtime())` became core's `new_context()` facade.
use neovm_eval::emacs_core::builtins::higher_order_test_support::{
    CapturedSortPredicate, resolve_sort_predicate_for_test,
};
use neovm_value::value::Value;

#[test]
fn sort_capture_resolves_builtin_alias_objects() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    eval.eval_str("(defalias 'sort-capture-alias 'string-lessp)")
        .expect("install alias");
    let builtin = eval
        .obarray()
        .symbol_function("string-lessp")
        .expect("builtin cell");
    let captured = resolve_sort_predicate_for_test(&mut eval, Value::symbol("sort-capture-alias"));
    let CapturedSortPredicate::StringLessp { subr } = captured else {
        panic!("the resolved builtin implementation must engage string ordering");
    };
    assert_eq!(subr, builtin);
}

#[test]
fn sort_capture_redefined_or_advised_string_predicate_is_generic() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    eval.eval_str(
        "(progn
           (defalias 'sort-capture-custom (lambda (a b) (string-lessp b a)))
           (advice-add 'sort-capture-custom :around
                       (lambda (original a b) (funcall original a b))))",
    )
    .expect("install redefinition and advice");
    let callable = eval
        .obarray()
        .symbol_function("sort-capture-custom")
        .expect("advised callable");
    let captured = resolve_sort_predicate_for_test(&mut eval, Value::symbol("sort-capture-custom"));
    let CapturedSortPredicate::Generic { function } = captured else {
        panic!("an advised callable must retain ordinary funcall");
    };
    assert_eq!(function, callable);
    eval.eval_str("(fset 'sort-capture-custom (lambda (_a _b) nil))")
        .expect("replace cell");
    assert_ne!(
        eval.obarray().symbol_function("sort-capture-custom"),
        Some(function)
    );
}

#[test]
fn sort_capture_keeps_void_and_autoload_symbols() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    eval.eval_str("(autoload 'sort-capture-autoload \"sort-capture-missing-file\")")
        .expect("install autoload");
    for name in ["sort-capture-void", "sort-capture-autoload"] {
        let symbol = Value::symbol(name);
        let CapturedSortPredicate::Generic { function } =
            resolve_sort_predicate_for_test(&mut eval, symbol)
        else {
            panic!("void/autoload must preserve symbol dispatch");
        };
        assert_eq!(function, symbol);
    }
}

#[test]
fn sort_capture_direct_numeric_subr_is_its_own_designator() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let builtin = eval.obarray().symbol_function(">").expect("builtin cell");
    let CapturedSortPredicate::Subr { designator, subr } =
        resolve_sort_predicate_for_test(&mut eval, builtin)
    else {
        panic!("a direct builtin object must engage subr dispatch");
    };
    assert_eq!(designator, builtin);
    assert_eq!(subr, builtin);
}

#[test]
fn sort_capture_direct_numeric_lessp_takes_the_numeric_path() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let builtin = eval.obarray().symbol_function("<").expect("builtin cell");
    let CapturedSortPredicate::NumericLessp { subr } =
        resolve_sort_predicate_for_test(&mut eval, builtin)
    else {
        panic!("a direct `<` builtin object must engage the numeric comparison path");
    };
    assert_eq!(subr, builtin);
}
