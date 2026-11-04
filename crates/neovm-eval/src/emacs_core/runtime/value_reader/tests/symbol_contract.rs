use super::*;
use neovm_value::heap_types::LispString;

#[test]
fn symbols_contract_delimiters_and_escape_eof() {
    crate::test_utils::init_test_tracing();
    let ob = neovm_value::symbol::Obarray::new();
    let (value, position) = read_one(
        "gdj-symbol#tail",
        0,
        crate::emacs_core::eval::VariableView::canonical(
            neovm_value::symbol::CanonicalAccess::TEST,
            &ob,
        ),
    )
    .unwrap()
    .unwrap();
    assert!(value.is_symbol_named("gdj-symbol", SymbolMode::Bare));
    assert_eq!(position, 10);
    for source in ["a\\", "\\", "#:a\\", "#_a\\"] {
        assert_eq!(
            read_one(
                source,
                0,
                crate::emacs_core::eval::VariableView::canonical(
                    neovm_value::symbol::CanonicalAccess::TEST,
                    &ob
                )
            )
            .unwrap_err()
            .kind,
            ReadErrorKind::EndOfFile
        );
    }
    let (empty, position) = read_one(
        "#_#_tail",
        0,
        crate::emacs_core::eval::VariableView::canonical(
            neovm_value::symbol::CanonicalAccess::TEST,
            &ob,
        ),
    )
    .unwrap()
    .unwrap();
    assert_eq!(empty.as_bare_symbol_id(), Some(intern("")));
    assert_eq!(position, 2);
    let pairs = Value::list(vec![Value::cons(
        Value::string(""),
        Value::string("gdj-unit-empty-prefix-multibyte-new"),
    )]);
    let shorthands = ReadSymbolShorthands::from_lisp_value(pairs).unwrap();
    for source in ["#_", "#_ ", "##"] {
        let source = LispString::from_unibyte(source.as_bytes().to_vec());
        let (symbol, _) = LispReadSource::new(&source)
            .read_one_with_shorthands(
                0,
                crate::emacs_core::eval::VariableView::canonical(
                    neovm_value::symbol::CanonicalAccess::TEST,
                    &ob,
                ),
                Some(&shorthands),
            )
            .unwrap()
            .unwrap();
        let name = neovm_value::intern::resolve_lisp_visible_symbol_name(
            symbol.as_bare_symbol_id().unwrap(),
        );
        assert_eq!(
            name.text().as_bytes(),
            b"gdj-unit-empty-prefix-multibyte-new"
        );
        assert!(name.text().is_multibyte());
    }
}

#[test]
fn symbols_contract_source_encoding_is_distinct_from_byte_identity() {
    crate::test_utils::init_test_tracing();
    let ob = neovm_value::symbol::Obarray::new();
    let source = LispString::from_unibyte(b"a\xa0b".to_vec());
    let (symbol, position) = LispReadSource::new(&source)
        .read_one(
            0,
            crate::emacs_core::eval::VariableView::canonical(
                neovm_value::symbol::CanonicalAccess::TEST,
                &ob,
            ),
        )
        .unwrap()
        .unwrap();
    assert_eq!(position, 3);
    let name =
        neovm_value::intern::resolve_lisp_visible_symbol_name(symbol.as_bare_symbol_id().unwrap());
    assert_eq!(name.text().as_bytes(), b"a\xa0b");
    assert!(!name.text().is_multibyte());
    let source = LispString::from_emacs_bytes(b"gdj-unit-multibyte-new-name".to_vec());
    let (symbol, _) = LispReadSource::new(&source)
        .read_one(
            0,
            crate::emacs_core::eval::VariableView::canonical(
                neovm_value::symbol::CanonicalAccess::TEST,
                &ob,
            ),
        )
        .unwrap()
        .unwrap();
    let name =
        neovm_value::intern::resolve_lisp_visible_symbol_name(symbol.as_bare_symbol_id().unwrap());
    assert!(name.text().is_multibyte());
}

#[test]
fn symbols_contract_shorthands_keep_source_order_and_skip_core_operators() {
    crate::test_utils::init_test_tracing();
    let ob = neovm_value::symbol::Obarray::new();
    let pairs = Value::list(vec![
        Value::cons(Value::string("s-"), Value::string("string-")),
        Value::cons(Value::string("s-t"), Value::string("zz")),
    ]);
    let shorthands = ReadSymbolShorthands::from_lisp_value(pairs).unwrap();
    let source = LispString::from_unibyte(b"s-trim".to_vec());
    let (symbol, _) = LispReadSource::new(&source)
        .read_one_with_shorthands(
            0,
            crate::emacs_core::eval::VariableView::canonical(
                neovm_value::symbol::CanonicalAccess::TEST,
                &ob,
            ),
            Some(&shorthands),
        )
        .unwrap()
        .unwrap();
    assert!(symbol.is_symbol_named("string-trim", SymbolMode::Bare));
    let pairs = Value::list(vec![Value::cons(
        Value::string("<"),
        Value::string("wrong-"),
    )]);
    let shorthands = ReadSymbolShorthands::from_lisp_value(pairs).unwrap();
    let source = LispString::from_unibyte(b"<=".to_vec());
    let (symbol, _) = LispReadSource::new(&source)
        .read_one_with_shorthands(
            0,
            crate::emacs_core::eval::VariableView::canonical(
                neovm_value::symbol::CanonicalAccess::TEST,
                &ob,
            ),
            Some(&shorthands),
        )
        .unwrap()
        .unwrap();
    assert!(symbol.is_symbol_named("<=", SymbolMode::Bare));
}
