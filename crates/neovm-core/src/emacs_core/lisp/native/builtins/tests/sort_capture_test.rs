//! Resolver engagement tests. Lisp behavior is pinned by GNU-refreshed forms in
//! `neovm-oracle-tests/src/sort/captured_predicate.rs` under the VM and default JIT.
use super::higher_order::{SortPredicate, capture_sort_predicate};
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;

#[test]
fn sort_capture_resolves_builtin_alias_objects() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.eval_str("(defalias 'sort-capture-alias 'string-lessp)")
        .expect("install alias");
    let builtin = eval
        .obarray()
        .symbol_function("string-lessp")
        .expect("builtin cell");
    let captured = capture_sort_predicate(&mut eval, Value::symbol("sort-capture-alias"));
    let Some(SortPredicate::StringLessp { subr, .. }) = captured else {
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
    let captured = capture_sort_predicate(&mut eval, Value::symbol("sort-capture-custom"));
    let Some(SortPredicate::Generic(function)) = captured else {
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
    let mut eval = Context::new();
    eval.eval_str("(autoload 'sort-capture-autoload \"sort-capture-missing-file\")")
        .expect("install autoload");
    for name in ["sort-capture-void", "sort-capture-autoload"] {
        let symbol = Value::symbol(name);
        let Some(SortPredicate::Generic(function)) = capture_sort_predicate(&mut eval, symbol)
        else {
            panic!("void/autoload must preserve symbol dispatch");
        };
        assert_eq!(function, symbol);
    }
}

#[test]
fn sort_capture_direct_numeric_subr_is_its_own_designator() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let builtin = eval.obarray().symbol_function("<").expect("builtin cell");
    let Some(SortPredicate::Subr {
        designator, subr, ..
    }) = capture_sort_predicate(&mut eval, builtin)
    else {
        panic!("a direct builtin object must engage subr dispatch");
    };
    assert_eq!(designator, builtin);
    assert_eq!(subr, builtin);
}
