use super::*;

fn positioned_purpose(context: &mut Context) -> Value {
    context
        .tagged_heap
        .alloc_symbol_with_pos(Value::symbol("char-code-property-table"), Value::fixnum(0))
}

#[test]
fn positioned_char_table_purpose_classification_follows_mode_and_preserves_value() {
    let mut context = Context::new();
    let purpose = positioned_purpose(&mut context);
    let table = make_char_table_with_extra_slots(purpose, Value::NIL, 5);
    for (mode, expected) in [(SymbolMode::Bare, false), (SymbolMode::Positioned, true)] {
        assert_eq!(is_char_code_property_table(&table, mode), expected);
        assert_eq!(
            char_table_has_subtype_named_with_symbols(&table, "char-code-property-table", mode),
            expected,
        );
    }
    assert_eq!(
        builtin_char_table_subtype(vec![table]).unwrap().bits(),
        purpose.bits()
    );
}

#[test]
fn positioned_char_table_purpose_controls_compressed_lookup() {
    let mut context = Context::new();
    let table = make_char_table_with_extra_slots(positioned_purpose(&mut context), Value::NIL, 5);
    // GNU's run-length compressed leaf: index 2 for three characters, then 1.
    // Keep it outside the ASCII cache so lookup must classify the stored purpose.
    let compressed = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
        2, 2, 0xC2, 0x83, 1,
    ]));
    let mut depth2_contents = vec![Value::NIL; 32];
    depth2_contents[2] = compressed;
    let depth2 = Value::make_sub_char_table(2, 0, depth2_contents);
    let mut depth1_contents = vec![Value::NIL; 16];
    depth1_contents[0] = depth2;
    let depth1 = Value::make_sub_char_table(1, 0, depth1_contents);
    set_char_table_contents(table, 0, depth1);
    assert_eq!(
        ct_lookup_with_symbols(&table, 256, SymbolMode::Bare)
            .unwrap()
            .bits(),
        compressed.bits()
    );
    // GNU chartab.c:1097 replaces the compressed leaf in place when an
    // honoring lookup decompresses it. Keep an independent compressed copy
    // for the bare translation guard instead of expecting a mutated table
    // to retain its original string leaf.
    let bare_table = copy_char_table_with_symbols(table, SymbolMode::Bare).unwrap();
    assert_eq!(
        ct_lookup_with_symbols(&table, 256, SymbolMode::Positioned).unwrap(),
        Value::fixnum(2)
    );
    assert_eq!(
        ct_lookup_with_symbols(&table, 256, SymbolMode::Bare).unwrap(),
        Value::fixnum(2),
    );
    let bare_translation =
        crate::emacs_core::regex_emacs::CaseTranslation::from_char_table_with_symbols(
            bare_table,
            SymbolMode::Bare,
        );
    let positioned_translation =
        crate::emacs_core::regex_emacs::CaseTranslation::from_char_table_with_symbols(
            table,
            SymbolMode::Positioned,
        );
    // Repeat to exercise each translator's policy-bound wide-character memo.
    for _ in 0..2 {
        assert_eq!(bare_translation.translate(256), 256);
        assert_eq!(positioned_translation.translate(256), 2);
    }
    assert_signal_invalid_property_table(builtin_get_unicode_property_internal_with_symbols(
        vec![table, Value::fixnum(256)],
        SymbolMode::Bare,
    ));
    assert_eq!(
        builtin_get_unicode_property_internal_with_symbols(
            vec![table, Value::fixnum(256)],
            SymbolMode::Positioned
        )
        .unwrap(),
        Value::fixnum(2),
    );
}

fn assert_signal_invalid_property_table(result: EvalResult) {
    assert!(result.is_err());
}

#[test]
fn positioned_unicode_property_alist_and_encoder_follow_eq_policy() {
    let mut context = Context::new();
    let key = context
        .tagged_heap
        .alloc_symbol_with_pos(Value::symbol("general-category"), Value::fixnum(0));
    let cell = Value::cons(key, Value::fixnum(7));
    let list = Value::list(vec![cell]);
    let bare_key = Value::symbol("general-category");
    assert!(
        assq_cell_eq(bare_key, list, SymbolMode::Bare)
            .unwrap()
            .is_nil()
    );
    assert_eq!(
        assq_cell_eq(bare_key, list, SymbolMode::Positioned)
            .unwrap()
            .bits(),
        cell.bits()
    );
    let table =
        make_char_table_with_extra_slots(Value::symbol("char-code-property-table"), Value::NIL, 5);
    set_char_table_extra_slot(&table, 4, Value::vector(vec![key]));
    assert!(encode_uniprop_value_run_length(table, bare_key, SymbolMode::Bare).is_err());
    assert_eq!(
        encode_uniprop_value_run_length(table, bare_key, SymbolMode::Positioned).unwrap(),
        Value::fixnum(0)
    );
}
