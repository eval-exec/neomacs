//! Root ownership differs from dynamic binding ownership during overrides.
use super::*;

#[test]
fn override_cursor_root_release_keeps_print_binding_until_outer_unwind() {
    crate::test_utils::init_test_tracing();
    let mut context = super::super::eval::Context::new();
    let original = context.obarray.symbol_value_copied("print-length");
    let original_depth = context.specpdl.len();
    let mut scope = super::super::print::PrintEvaluationGuard::new(&mut context);
    {
        let mut cursor = PrintOverrideCursorGuard::new(
            scope.context(),
            Value::list(vec![Value::symbol("owned")]),
        );
        cursor
            .context()
            .try_specbind(intern("print-length"), Value::fixnum(7))
            .expect("test variable binding");
        cursor.set_cursor(Value::list(vec![Value::symbol("next")]));
    }
    assert_eq!(scope.context().specpdl.len(), original_depth + 1);
    assert_eq!(
        scope.context().obarray.symbol_value_copied("print-length"),
        Some(Value::fixnum(7))
    );
    scope.finish_value(Ok(Value::NIL)).expect("outer unwind");
    assert_eq!(context.specpdl.len(), original_depth);
    assert_eq!(
        context.obarray.symbol_value_copied("print-length"),
        original
    );
}
