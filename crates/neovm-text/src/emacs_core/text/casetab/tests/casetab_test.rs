use super::*;

#[test]
fn standard_ascii_upcase() {
    crate::test_utils::init_test_tracing();
    let table = CaseTable::standard_ascii();
    assert_eq!(table.upcase.get(&'a'), Some(&'A'));
    assert_eq!(table.upcase.get(&'z'), Some(&'Z'));
    assert_eq!(table.upcase.get(&'m'), Some(&'M'));
    // Uppercase letters should have no upcase mapping.
    assert_eq!(table.upcase.get(&'A'), None);
}

#[test]
fn standard_ascii_downcase() {
    crate::test_utils::init_test_tracing();
    let table = CaseTable::standard_ascii();
    assert_eq!(table.downcase.get(&'A'), Some(&'a'));
    assert_eq!(table.downcase.get(&'Z'), Some(&'z'));
    assert_eq!(table.downcase.get(&'M'), Some(&'m'));
    // Lowercase letters should have no downcase mapping.
    assert_eq!(table.downcase.get(&'a'), None);
}

#[test]
fn standard_ascii_canonicalize() {
    crate::test_utils::init_test_tracing();
    let table = CaseTable::standard_ascii();
    // Both upper and lower should canonicalize to lowercase.
    assert_eq!(table.canonicalize.get(&'A'), Some(&'a'));
    assert_eq!(table.canonicalize.get(&'a'), Some(&'a'));
    assert_eq!(table.canonicalize.get(&'Z'), Some(&'z'));
    assert_eq!(table.canonicalize.get(&'z'), Some(&'z'));
}

#[test]
fn standard_ascii_equivalences() {
    crate::test_utils::init_test_tracing();
    let table = CaseTable::standard_ascii();
    // Equivalences form a cycle: A -> a -> A.
    assert_eq!(table.equivalences.get(&'A'), Some(&'a'));
    assert_eq!(table.equivalences.get(&'a'), Some(&'A'));
}

#[test]
fn empty_table_has_no_mappings() {
    crate::test_utils::init_test_tracing();
    let table = CaseTable::empty();
    assert!(table.upcase.is_empty());
    assert!(table.downcase.is_empty());
    assert!(table.canonicalize.is_empty());
    assert!(table.equivalences.is_empty());
}

#[test]
fn manager_upcase_char() {
    crate::test_utils::init_test_tracing();
    let mgr = CaseTableManager::new();
    assert_eq!(mgr.upcase_char('a'), 'A');
    assert_eq!(mgr.upcase_char('z'), 'Z');
    assert_eq!(mgr.upcase_char('A'), 'A'); // already uppercase, no mapping
    assert_eq!(mgr.upcase_char('0'), '0'); // non-letter unchanged
    assert_eq!(mgr.upcase_char(' '), ' ');
}

#[test]
fn manager_downcase_char() {
    crate::test_utils::init_test_tracing();
    let mgr = CaseTableManager::new();
    assert_eq!(mgr.downcase_char('A'), 'a');
    assert_eq!(mgr.downcase_char('Z'), 'z');
    assert_eq!(mgr.downcase_char('a'), 'a'); // already lowercase, no mapping
    assert_eq!(mgr.downcase_char('5'), '5'); // non-letter unchanged
}

#[test]
fn manager_default() {
    crate::test_utils::init_test_tracing();
    let mgr = CaseTableManager::default();
    assert_eq!(mgr.upcase_char('a'), 'A');
    assert_eq!(mgr.downcase_char('A'), 'a');
}

#[test]
fn manager_set_current() {
    crate::test_utils::init_test_tracing();
    let mut mgr = CaseTableManager::new();
    let mut custom = CaseTable::empty();
    // Map 'x' to 'Y' for upcase.
    custom.upcase.insert('x', 'Y');
    mgr.set_current(custom);
    assert_eq!(mgr.upcase_char('x'), 'Y');
    // 'a' no longer has an upcase mapping in the custom table.
    assert_eq!(mgr.upcase_char('a'), 'a');
}

#[test]
fn manager_set_standard() {
    crate::test_utils::init_test_tracing();
    let mut mgr = CaseTableManager::new();
    let custom = CaseTable::empty();
    mgr.set_standard(custom);
    assert!(mgr.standard_table().upcase.is_empty());
}

#[test]
fn upcase_all_letters() {
    crate::test_utils::init_test_tracing();
    let mgr = CaseTableManager::new();
    for lower in b'a'..=b'z' {
        let lc = lower as char;
        let uc = (lower - b'a' + b'A') as char;
        assert_eq!(mgr.upcase_char(lc), uc);
    }
}

#[test]
fn downcase_all_letters() {
    crate::test_utils::init_test_tracing();
    let mgr = CaseTableManager::new();
    for upper in b'A'..=b'Z' {
        let uc = upper as char;
        let lc = (upper - b'A' + b'a') as char;
        assert_eq!(mgr.downcase_char(uc), lc);
    }
}

#[test]
fn roundtrip_upcase_downcase() {
    crate::test_utils::init_test_tracing();
    let mgr = CaseTableManager::new();
    for lower in b'a'..=b'z' {
        let lc = lower as char;
        let uc = mgr.upcase_char(lc);
        let back = mgr.downcase_char(uc);
        assert_eq!(back, lc);
    }
}

#[test]
fn non_ascii_chars_unchanged() {
    crate::test_utils::init_test_tracing();
    let mgr = CaseTableManager::new();
    // Non-ASCII characters should pass through unchanged with the ASCII table.
    assert_eq!(mgr.upcase_char('\u{00e9}'), '\u{00e9}'); // e-acute
    assert_eq!(mgr.downcase_char('\u{00c9}'), '\u{00c9}'); // E-acute
    assert_eq!(mgr.upcase_string("\u{00e9}"), "\u{00e9}");
}

#[test]
fn is_case_table_on_short_vector() {
    crate::test_utils::init_test_tracing();
    // A vector too short to be a char-table.
    let v = Value::vector(vec![Value::symbol("--char-table--"), Value::NIL]);
    assert!(!is_case_table(&v));
}

#[test]
fn is_case_table_wrong_subtype() {
    crate::test_utils::init_test_tracing();
    // A char-table with a different subtype is NOT a case table.
    let v = char_table_with_extras("syntax-table", &[]);
    assert!(!is_case_table(&v));
}

#[test]
fn is_case_table_rejects_missing_extra_slots() {
    crate::test_utils::init_test_tracing();
    let v = char_table_with_extras("case-table", &[]);
    assert!(!is_case_table(&v));
}

#[test]
fn is_case_table_rejects_invalid_extra_slots() {
    crate::test_utils::init_test_tracing();
    let invalid_upcase =
        char_table_with_extras("case-table", &[Value::fixnum(1), Value::NIL, Value::NIL]);
    assert!(!is_case_table(&invalid_upcase));

    let eqv_without_canon = char_table_with_extras(
        "case-table",
        &[Value::NIL, Value::NIL, make_empty_case_table()],
    );
    assert!(!is_case_table(&eqv_without_canon));
}

#[test]
fn standard_case_table_is_char_table() {
    crate::test_utils::init_test_tracing();
    use super::super::chartable::is_char_table;
    let ct = make_standard_case_table_value();
    assert!(is_char_table(&ct));
    assert!(is_case_table(&ct));
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
    assert!(is_case_table(&ct));
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
    let table = Value::make_char_table(Value::symbol(purpose), Value::NIL, extras.len());
    let _ = table.with_char_table_mut(|obj| {
        obj.copy_extras(extras);
    });
    table
}
