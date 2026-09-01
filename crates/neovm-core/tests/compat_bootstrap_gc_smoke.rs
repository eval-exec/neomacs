use neovm_core::emacs_core::{Context, LambdaData, LambdaParams, Value};

#[test]
fn compat_macro_cache_keeps_opaque_values_alive_across_gc() {
    let mut eval = Context::new();

    let macro_body = vec![Value::list(vec![
        Value::symbol("function"),
        Value::list(vec![
            Value::symbol("lambda"),
            Value::NIL,
            Value::fixnum(123),
        ]),
    ])];
    let opaque_macro = Value::make_macro(LambdaData {
        params: LambdaParams::simple(vec![]),
        body: macro_body,
        env: None,
        docstring: None,
        doc_form: None,
        interactive: None,
    });
    eval.obarray_mut()
        .set_symbol_function("opaque-macro", opaque_macro);

    let first = eval
        .eval_str("(opaque-macro)")
        .expect("first macro expansion");
    assert!(
        first.is_lambda(),
        "macro expansion should yield a runtime closure, got {first:?}"
    );

    eval.gc_collect();

    // After GC, the macro cache is cleared (gc_collect() calls
    // macro_expansion_cache.clear()), so the macro is re-expanded,
    // producing a new Lambda.  The key invariant is that the macro
    // expansion still succeeds and yields a valid Lambda after GC.
    let second = eval
        .eval_str("(opaque-macro)")
        .expect("second macro expansion after GC");
    assert!(
        second.is_lambda(),
        "macro expansion should still yield a runtime closure after GC, got {second:?}"
    );
}
