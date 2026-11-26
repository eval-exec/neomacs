use super::*;

use neovm_value::tagged::SymbolMode;

#[test]
fn is_case_table_on_short_vector() {
    crate::test_utils::init_test_tracing();
    // A vector too short to be a char-table.
    let v = Value::vector(vec![Value::symbol("--char-table--"), Value::NIL]);
    assert!(!is_case_table_with_symbols(&v, SymbolMode::Bare));
}

#[test]
fn is_case_table_wrong_subtype() {
    crate::test_utils::init_test_tracing();
    // A char-table with a different subtype is NOT a case table.
    let v = char_table_with_extras("syntax-table", &[]);
    assert!(!is_case_table_with_symbols(&v, SymbolMode::Bare));
}

#[test]
fn is_case_table_rejects_missing_extra_slots() {
    crate::test_utils::init_test_tracing();
    let v = char_table_with_extras("case-table", &[]);
    assert!(!is_case_table_with_symbols(&v, SymbolMode::Bare));
}

#[test]
fn is_case_table_rejects_invalid_extra_slots() {
    crate::test_utils::init_test_tracing();
    let invalid_upcase =
        char_table_with_extras("case-table", &[Value::fixnum(1), Value::NIL, Value::NIL]);
    assert!(!is_case_table_with_symbols(
        &invalid_upcase,
        SymbolMode::Bare
    ));

    let eqv_without_canon = char_table_with_extras(
        "case-table",
        &[Value::NIL, Value::NIL, make_empty_case_table()],
    );
    assert!(!is_case_table_with_symbols(
        &eqv_without_canon,
        SymbolMode::Bare
    ));
}

#[test]
fn standard_case_table_is_char_table() {
    crate::test_utils::init_test_tracing();
    use super::super::chartable::is_char_table;
    let ct = make_standard_case_table_value();
    assert!(is_char_table(&ct));
    assert!(is_case_table_with_symbols(&ct, SymbolMode::Bare));
}

#[test]
fn standard_case_table_has_extra_slots() {
    crate::test_utils::init_test_tracing();
    // GNU `init_casetab_once': four real `case-table' char-tables, each with
    // the purpose's three extra slots; canon's third slot is eqv.
    use super::super::chartable::is_char_table;
    let ct = make_standard_case_table_value();
    assert!(
        ct.is_char_table(),
        "the standard table is a real char-table"
    );
    assert!(is_case_table_with_symbols(&ct, SymbolMode::Bare));
    let up = case_table_extra(ct, 0);
    let canon = case_table_extra(ct, 1);
    let eqv = case_table_extra(ct, 2);
    for sub in [up, canon, eqv] {
        assert!(sub.is_char_table() && is_char_table(&sub));
        assert_eq!(sub.as_char_table_obj().unwrap().extras.len(), 3);
    }
    assert_eq!(case_table_extra(canon, 2).bits(), eqv.bits());
    let at = |table: Value, ch: i64| super::super::chartable::ct_ref(&table, ch);
    assert_eq!(at(ct, 'A' as i64), Value::fixnum('a' as i64));
    assert_eq!(at(up, 'a' as i64), Value::fixnum('A' as i64));
    assert_eq!(at(canon, 'Q' as i64), Value::fixnum('q' as i64));
    assert_eq!(at(eqv, 'q' as i64), Value::fixnum('Q' as i64));
    assert_eq!(at(eqv, 'Q' as i64), Value::fixnum('q' as i64));
    assert_eq!(at(up, '1' as i64), Value::fixnum('1' as i64));
}

/// A real char-table with purpose `purpose` and these extra slots.
fn char_table_with_extras(purpose: &str, extras: &[Value]) -> Value {
    let table = Value::make_char_table(
        Value::from_sym_id(neovm_value::intern::intern(&purpose)),
        Value::NIL,
        extras.len(),
    );
    let _ = table.with_char_table_mut(|obj| {
        obj.copy_extras(extras);
    });
    table
}
