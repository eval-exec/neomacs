use super::*;

#[test]
fn windows_symbol_registration_matches_gnu_bootstrap_shape() {
    let mut obarray = Obarray::new();
    register_bootstrap_symbols(&mut obarray);

    assert_eq!(
        obarray.symbol_value("w32-quote-process-args").copied(),
        Some(Value::T)
    );
    assert_eq!(
        obarray
            .symbol_value("w32-get-true-file-attributes")
            .copied(),
        Some(Value::symbol("local"))
    );
    assert_eq!(
        obarray.symbol_value("x-toolkit-scroll-bars").copied(),
        Some(Value::T)
    );
    assert_eq!(
        obarray.symbol_value("tree-sitter--library-abi").copied(),
        Some(Value::fixnum(tree_sitter::LANGUAGE_VERSION as i64))
    );
    assert!(obarray.is_special("w32-quote-process-args"));
    assert!(obarray.is_special("tree-sitter--library-abi"));
    assert!(obarray.intern_soft("w32-charset-shiftjis").is_some());
}

#[test]
fn undefined_color_has_gnu_error_properties() {
    let mut obarray = Obarray::new();
    register_bootstrap_symbols(&mut obarray);

    assert_eq!(
        obarray.get_property("undefined-color", "error-message"),
        Some(Value::string("Undefined color"))
    );
    assert_eq!(
        obarray.get_property("undefined-color", "error-conditions"),
        Some(Value::list(vec![
            Value::symbol("undefined-color"),
            Value::symbol("error"),
        ]))
    );
}

#[cfg(not(windows))]
#[test]
fn non_windows_context_does_not_leak_w32_surface() {
    let eval = super::super::eval::Context::new();
    let features = eval
        .obarray()
        .symbol_value("features")
        .copied()
        .expect("features should be bound");
    let feature_list =
        super::super::value::list_to_vec(&features).expect("features should be a list");

    assert!(
        eval.obarray()
            .intern_soft("w32-quote-process-args")
            .is_none()
    );
    assert!(!feature_list.contains(&Value::symbol("w32")));
}
