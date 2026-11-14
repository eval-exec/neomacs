//! Classifier unit tests rehomed with their private metadata helpers.
//! The reader and command invocation tests remain in core.

use super::*;

/// GNU `indirect_function` (`src/eval.c:Fcommandp`) reads `SYMBOL_FUNC` once
/// per symbol alias hop. A function-cell lookup is one logical operation in
/// Neomacs as well; splitting its unbound and value states into independent
/// `Obarray::slot` probes multiplies the dominant M-x completion loop.
#[test]
fn commandp_reads_each_function_cell_once() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let command = intern("forward-char");

    ev.obarray().reset_symbol_slot_read_count();
    assert_eq!(
        classify_command_designator_in_state(
            &ev.obarray(),
            &Value::from_sym_id(command),
            false,
            neovm_value::tagged::SymbolMode::Bare
        ),
        CommandpClassification::Interactive
    );
    assert_eq!(
        ev.obarray().symbol_slot_read_count(),
        1,
        "one symbol alias hop must take one typed function-cell snapshot"
    );

    let alias = intern("neo-commandp-function-cell-snapshot-alias");
    ev.obarray_view_mut()
        .set_symbol_function_id(alias, Value::from_sym_id(command));
    ev.obarray().reset_symbol_slot_read_count();
    assert_eq!(
        classify_command_designator_in_state(
            &ev.obarray(),
            &Value::from_sym_id(alias),
            false,
            neovm_value::tagged::SymbolMode::Bare
        ),
        CommandpClassification::Interactive
    );
    assert_eq!(
        ev.obarray().symbol_slot_read_count(),
        2,
        "a two-symbol alias chain must take exactly two typed snapshots"
    );
}

/// GNU `Fcommandp` reads `XSUBR (fun)->intspec.string` directly from the
/// primitive object.  Re-entering Neomacs's global subr registry for every
/// primitive candidate turns the M-x obarray scan into thousands of TLS
/// `RefCell` borrows.
#[test]
fn commandp_reads_interactivity_from_the_subr_object() {
    crate::test_utils::init_test_tracing();
    let ev = Context::new(crate::test_utils::unit_runtime());
    let function = ev
        .obarray()
        .symbol_function_id(intern("forward-char"))
        .expect("forward-char should have a primitive function cell");
    assert_eq!(function.veclike_type(), Some(VecLikeType::Subr));

    crate::test_support::authority::reset_global_subr_lookup_count();
    assert_eq!(
        classify_command_object_in_state(&function, false, neovm_value::tagged::SymbolMode::Bare),
        CommandpClassification::Interactive
    );
    assert_eq!(
        crate::emacs_core::eval::global_subr_lookup_count(),
        0,
        "primitive interactivity must be an intrinsic SubrObj field"
    );
}

#[test]
fn commandp_distinguishes_gnu_docstring_references_from_oclosure_tags() {
    crate::test_utils::init_test_tracing();
    let classify_doc = |doc| {
        let closure = Value::make_lambda_with_slots(vec![
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            doc,
        ]);
        classify_command_object_in_state(&closure, false, neovm_value::tagged::SymbolMode::Bare)
    };

    for valid_doc in [
        Value::fixnum(7),
        Value::string("inline documentation"),
        Value::cons(Value::string("etc/DOC"), Value::fixnum(42)),
    ] {
        assert_eq!(
            classify_doc(valid_doc),
            CommandpClassification::CheckInteractiveFormProperty(
                InteractiveFormFallback::PropertyOnly
            )
        );
    }
    let oclosure_tag = Value::symbol("neo-test-oclosure-tag");
    assert!(matches!(
        classify_doc(oclosure_tag),
        CommandpClassification::CheckInteractiveFormProperty(
            InteractiveFormFallback::GenericDispatch(closure)
        ) if closure.closure_doc_value() == Some(oclosure_tag)
    ));
}
