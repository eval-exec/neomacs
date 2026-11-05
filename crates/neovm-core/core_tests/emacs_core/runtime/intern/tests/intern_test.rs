use super::*;

/// GNU stores a Lisp string object directly in every symbol and
/// `Fsymbol_name` returns that same object on every call.  Symbols synthesized
/// from Rust text must lazily acquire the same per-heap identity instead of
/// cloning their process-lifetime name atom for each read.
#[test]
fn atom_only_symbol_name_materializes_one_lisp_object_per_heap() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let symbol = crate::value::Value::from_sym_id(intern("atom-only-symbol-name-identity-probe"));

    let first = crate::emacs_core::builtins::misc_pure::builtin_symbol_name_1(&mut eval, symbol)
        .expect("symbol-name should materialize an atom-only name");
    let second = crate::emacs_core::builtins::misc_pure::builtin_symbol_name_1(&mut eval, symbol)
        .expect("symbol-name should reuse the materialized name");

    assert_eq!(
        first.bits(),
        second.bits(),
        "GNU Fsymbol_name returns the symbol's one stored name object"
    );

    neovm_eval::test_support::builtin_aset_args_bare(&[
        first,
        crate::value::Value::fixnum(0),
        crate::value::Value::fixnum('X' as i64),
    ])
    .expect("GNU permits mutation of the stored symbol-name string");
    let after_mutation =
        crate::emacs_core::builtins::misc_pure::builtin_symbol_name_1(&mut eval, symbol)
            .expect("symbol-name should retain the mutated object");
    assert_eq!(after_mutation.bits(), first.bits());
    assert_eq!(
        after_mutation
            .as_lisp_string()
            .expect("symbol-name remains a string")
            .as_bytes()[0],
        b'X'
    );
}

#[test]
fn symbol_diagnostics_use_the_lisp_visible_name_object() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let symbol_id = intern("diagnostic-live-name-object-probe");
    let symbol = crate::value::Value::from_sym_id(symbol_id);
    let name = crate::emacs_core::builtins::misc_pure::builtin_symbol_name_1(&mut eval, symbol)
        .expect("symbol-name should materialize the Lisp-visible name object");

    crate::emacs_core::builtins::collections::builtin_aset_args_in_state(
        &mut eval,
        &[
            name,
            crate::value::Value::fixnum(0),
            crate::value::Value::fixnum('X' as i64),
        ],
    )
    .expect("GNU permits mutation of the stored symbol-name string");

    assert_eq!(
        format_symbol_name_for_diagnostic(symbol_id),
        "Xiagnostic-live-name-object-probe"
    );
}
