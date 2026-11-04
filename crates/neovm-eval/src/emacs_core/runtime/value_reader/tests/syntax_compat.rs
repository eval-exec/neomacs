use super::*;

fn read_result(input: &str) -> Result<Option<(Value, usize)>, ReadError> {
    let ob = neovm_value::symbol::Obarray::new();
    read_one(
        input,
        0,
        crate::emacs_core::eval::VariableView::canonical(
            neovm_value::symbol::CanonicalAccess::TEST,
            &ob,
        ),
    )
}

#[test]
fn rea09_property_triples_replace_and_validate_like_set_text_properties() {
    let value = read1(r#"#("abc" 0 1 (a b) 0 2 (c d))"#);
    let runs = get_string_text_properties_for_value(value).unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!((runs[0].start, runs[0].end), (0, 2));
    assert!(
        runs[0]
            .plist
            .cons_car()
            .is_symbol_named("c", SymbolMode::Bare)
    );
    let value = read1(r#"#("abc" 2 1 face)"#);
    let runs = get_string_text_properties_for_value(value).unwrap();
    assert_eq!((runs[0].start, runs[0].end), (1, 2));
    assert!(
        runs[0]
            .plist
            .cons_car()
            .is_symbol_named("face", SymbolMode::Bare)
    );
    let error = read_result(r#"#("abc" 0 1 (a))"#).unwrap_err();
    assert_eq!(
        error.signal_symbol.map(neovm_value::intern::resolve_sym),
        Some("error")
    );
    assert_eq!(error.message, "Odd length text property list");
    let error = read_result(r#"#("abc" a 1 (a))"#).unwrap_err();
    assert_eq!(error.message, "Odd length text property list");
    let error = read_result(r#"#("abc" a 1 (a b))"#).unwrap_err();
    assert_eq!(
        error.signal_symbol.map(neovm_value::intern::resolve_sym),
        Some("wrong-type-argument")
    );
    assert!(error.signal_data[0].is_symbol_named("integer-or-marker-p", SymbolMode::Bare));
    assert_eq!(read_result(r#"#("a" . nil)"#).unwrap_err().message, ".");
}

#[test]
fn rea12_read_labels_reject_fixnum_and_machine_integer_overflow() {
    for input in [
        "#2305843009213693952=(a)",
        "#2305843009213693952#",
        "#999999999999999999999=a",
    ] {
        let error = read_result(input).unwrap_err();
        assert_eq!(error.kind, ReadErrorKind::InvalidReadSyntax);
    }
    assert!(read_result("#2305843009213693951=(a)").is_ok());
    assert_eq!(
        read_result("#999999999999999999999=a").unwrap_err().message,
        "#9999999999999999999"
    );
}

#[test]
fn rea13_radix_underscore_terminates_integer() {
    for (source, offset) in [("#x1_0", 3), ("#b1_0", 3), ("#o1_7", 3), ("#24r1_0", 5)] {
        let (value, end) = read_result(source).unwrap().unwrap();
        assert_eq!(value.as_fixnum(), Some(1));
        assert_eq!(end, offset);
    }
    assert_eq!(
        read_result("#x_1").unwrap_err().message,
        "integer, radix 16"
    );
}

#[test]
fn rea32_dot_tokens_require_list_context() {
    for input in [
        ".?",
        ".#x",
        "[a .?x]",
        "[a .#x]",
        ".",
        ". ",
        r#"#("a" . nil)"#,
    ] {
        assert_eq!(read_result(input).unwrap_err().message, ".", "{input}");
    }
    assert!(read1("(a .?x)").is_cons());
    assert!(read1("(a .#x1)").is_cons());
}

#[test]
fn rea33_sub_chartable_validation_order_and_condition() {
    for (input, message) in [
        ("#^^[1 0]", "Invalid size in sub-char-table"),
        ("#^^[0 0]", "Invalid depth in sub-char-table"),
        ("#^^[1]", "Invalid size of sub-char-table"),
        ("#^^[3 -1]", "Invalid size in sub-char-table"),
    ] {
        let error = read_result(input).unwrap_err();
        assert_eq!(error.kind, ReadErrorKind::Error);
        assert_eq!(error.message, message);
    }
    let input = format!("#^^[3 -1 {}]", "nil ".repeat(128));
    assert_eq!(
        read_result(&input).unwrap_err().message,
        "Invalid minimum character in sub-char-table"
    );
}

#[test]
fn rea34_reader_error_data_retains_offending_input() {
    for (input, message) in [
        ("#24x", "#24x"),
        ("#1 =(a)", "#1 "),
        ("#^x", "#^x"),
        ("#^^x", "#^^x"),
        ("(a]", "]"),
        ("(a . b c)", "expected )"),
        (r#"#("abc" 0 1)"#, "Invalid string property list"),
    ] {
        assert_eq!(read_result(input).unwrap_err().message, message, "{input}");
    }
    for (input, position) in [("#^x", 2), ("#^^x", 3), ("(a . b c)", 8)] {
        assert_eq!(
            read_result(input).unwrap_err().position,
            position,
            "{input}"
        );
    }
    let error = read_result("#s(hash-table data (a 1 . 2))").unwrap_err();
    assert_eq!(error.signal_data[1].as_fixnum(), Some(2));
}
