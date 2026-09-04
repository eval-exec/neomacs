use super::*;
use crate::emacs_core::intern::intern;

// -----------------------------------------------------------------------
// CaseTable tests
// -----------------------------------------------------------------------

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

// -----------------------------------------------------------------------
// CaseTableManager tests
// -----------------------------------------------------------------------

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
fn manager_upcase_string() {
    crate::test_utils::init_test_tracing();
    let mgr = CaseTableManager::new();
    assert_eq!(mgr.upcase_string("hello"), "HELLO");
    assert_eq!(mgr.upcase_string("Hello World"), "HELLO WORLD");
    assert_eq!(mgr.upcase_string("ABC"), "ABC");
    assert_eq!(mgr.upcase_string(""), "");
    assert_eq!(mgr.upcase_string("a1b2c3"), "A1B2C3");
}

#[test]
fn manager_downcase_string() {
    crate::test_utils::init_test_tracing();
    let mgr = CaseTableManager::new();
    assert_eq!(mgr.downcase_string("HELLO"), "hello");
    assert_eq!(mgr.downcase_string("Hello World"), "hello world");
    assert_eq!(mgr.downcase_string("abc"), "abc");
    assert_eq!(mgr.downcase_string(""), "");
    assert_eq!(mgr.downcase_string("A1B2C3"), "a1b2c3");
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

// -----------------------------------------------------------------------
// Builtin tests
// -----------------------------------------------------------------------

#[test]
fn builtin_case_table_p_on_non_table() {
    crate::test_utils::init_test_tracing();
    assert!(builtin_case_table_p(vec![Value::NIL]).unwrap().is_nil());
    assert!(
        builtin_case_table_p(vec![Value::fixnum(42)])
            .unwrap()
            .is_nil()
    );
    assert!(
        builtin_case_table_p(vec![Value::string("hello")])
            .unwrap()
            .is_nil()
    );
}

#[test]
fn builtin_case_table_p_on_char_table() {
    crate::test_utils::init_test_tracing();
    // A proper char-table with case-table subtype.
    let ct = make_case_table_value();
    assert!(builtin_case_table_p(vec![ct]).unwrap().is_t());
}

#[test]
fn builtin_case_table_p_wrong_arg_count() {
    crate::test_utils::init_test_tracing();
    assert!(builtin_case_table_p(vec![]).is_err());
    assert!(builtin_case_table_p(vec![Value::NIL, Value::NIL]).is_err());
}

#[test]
fn builtin_current_case_table_returns_case_table() {
    crate::test_utils::init_test_tracing();
    let mut ctx = super::super::eval::Context::new();
    let result = builtin_current_case_table(&mut ctx, vec![]).unwrap();
    assert!(is_case_table(&result));
}

#[test]
fn builtin_current_case_table_wrong_args() {
    crate::test_utils::init_test_tracing();
    let mut ctx = super::super::eval::Context::new();
    assert!(builtin_current_case_table(&mut ctx, vec![Value::NIL]).is_err());
}

#[test]
fn builtin_standard_case_table_returns_case_table() {
    crate::test_utils::init_test_tracing();
    let mut ctx = super::super::eval::Context::new();
    let result = builtin_standard_case_table(&mut ctx, vec![]).unwrap();
    assert!(is_case_table(&result));
}

#[test]
fn evaluator_activation_restores_standard_case_table_identity() {
    crate::test_utils::init_test_tracing();
    let mut ctx = super::super::eval::Context::new();
    let standard = builtin_standard_case_table(&mut ctx, vec![]).unwrap();
    let current = builtin_current_case_table(&mut ctx, vec![]).unwrap();
    assert_eq!(current.bits(), standard.bits());

    reset_casetab_thread_locals();
    STANDARD_CASE_TABLE_OBJECT.with(|slot| assert!(slot.borrow().is_none()));

    ctx.setup_thread_locals();
    STANDARD_CASE_TABLE_OBJECT.with(|slot| {
        assert_eq!(slot.borrow().unwrap().bits(), standard.bits());
    });
    assert!(buffer_case_canon_table(ctx.buffers.current_buffer().unwrap()).is_none());
}

#[test]
fn builtin_standard_case_table_wrong_args() {
    crate::test_utils::init_test_tracing();
    let mut ctx = super::super::eval::Context::new();
    assert!(builtin_standard_case_table(&mut ctx, vec![Value::NIL]).is_err());
}

#[test]
fn builtin_set_case_table_returns_arg() {
    crate::test_utils::init_test_tracing();
    let mut ctx = super::super::eval::Context::new();
    let table = make_case_table_value();
    let result = builtin_set_case_table(&mut ctx, vec![table]).unwrap();
    assert_eq!(result, table);
}

#[test]
fn builtin_set_case_table_rejects_non_table() {
    crate::test_utils::init_test_tracing();
    let mut ctx = super::super::eval::Context::new();
    assert!(builtin_set_case_table(&mut ctx, vec![Value::fixnum(1)]).is_err());
}

#[test]
fn builtin_set_case_table_wrong_args() {
    crate::test_utils::init_test_tracing();
    let mut ctx = super::super::eval::Context::new();
    assert!(builtin_set_case_table(&mut ctx, vec![]).is_err());
    assert!(builtin_set_case_table(&mut ctx, vec![Value::NIL, Value::NIL]).is_err());
}

#[test]
fn builtin_set_standard_case_table_returns_arg() {
    crate::test_utils::init_test_tracing();
    let mut ctx = super::super::eval::Context::new();
    let table = make_case_table_value();
    let result = builtin_set_standard_case_table(&mut ctx, vec![table]).unwrap();
    assert_eq!(result, table);
}

#[test]
fn builtin_set_standard_case_table_rejects_non_table() {
    crate::test_utils::init_test_tracing();
    let mut ctx = super::super::eval::Context::new();
    assert!(builtin_set_standard_case_table(&mut ctx, vec![Value::fixnum(1)]).is_err());
}

#[test]
fn builtin_set_standard_case_table_wrong_args() {
    crate::test_utils::init_test_tracing();
    let mut ctx = super::super::eval::Context::new();
    assert!(builtin_set_standard_case_table(&mut ctx, vec![]).is_err());
}

#[test]
fn evaluator_case_table_roundtrip_and_isolation() {
    crate::test_utils::init_test_tracing();
    let mut eval = super::super::eval::Context::new();
    let standard = builtin_standard_case_table(&mut eval, vec![]).unwrap();
    let current = builtin_current_case_table(&mut eval, vec![]).unwrap();
    assert_eq!(standard, current);

    let current_id = eval.buffers.current_buffer().expect("current buffer").id;
    let other_id = eval.buffers.create_buffer("*case-other*");

    let custom = make_case_table_value();
    builtin_set_case_table(&mut eval, vec![custom]).unwrap();
    let after_set = builtin_current_case_table(&mut eval, vec![]).unwrap();
    assert_eq!(after_set, custom);

    eval.buffers.set_current(other_id);
    let other_current = builtin_current_case_table(&mut eval, vec![]).unwrap();
    assert_eq!(other_current, standard);

    eval.buffers.set_current(current_id);
    let restored = builtin_current_case_table(&mut eval, vec![]).unwrap();
    assert_eq!(restored, custom);
}

#[test]
fn builtin_downcase_char_uppercase() {
    crate::test_utils::init_test_tracing();
    // (downcase ?A) -> 97 (i.e., ?a)
    let result = builtin_downcase_char(vec![Value::char('A')]).unwrap();
    assert!(result.is_fixnum());
}

#[test]
fn builtin_downcase_char_lowercase_unchanged() {
    crate::test_utils::init_test_tracing();
    // (downcase ?a) -> 97
    let result = builtin_downcase_char(vec![Value::char('a')]).unwrap();
    assert!(result.is_fixnum());
}

#[test]
fn builtin_downcase_char_from_int() {
    crate::test_utils::init_test_tracing();
    // (downcase 65) -> 97 (65 = ?A, 97 = ?a)
    let result = builtin_downcase_char(vec![Value::fixnum(65)]).unwrap();
    assert!(result.is_fixnum());
}

#[test]
fn builtin_downcase_char_wrong_type() {
    crate::test_utils::init_test_tracing();
    assert!(builtin_downcase_char(vec![Value::string("A")]).is_err());
    assert!(builtin_downcase_char(vec![Value::NIL]).is_err());
}

#[test]
fn builtin_downcase_char_wrong_arg_count() {
    crate::test_utils::init_test_tracing();
    assert!(builtin_downcase_char(vec![]).is_err());
    assert!(builtin_downcase_char(vec![Value::char('A'), Value::char('B')]).is_err());
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
fn string_roundtrip() {
    crate::test_utils::init_test_tracing();
    let mgr = CaseTableManager::new();
    let original = "Hello World";
    let upper = mgr.upcase_string(original);
    let lower = mgr.downcase_string(&upper);
    assert_eq!(lower, "hello world");
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
    let v = Value::vector(vec![Value::symbol(intern(CT_CHAR_TABLE_TAG)), Value::NIL]);
    assert!(!is_case_table(&v));
}

#[test]
fn is_case_table_wrong_subtype() {
    crate::test_utils::init_test_tracing();
    // A char-table with a different subtype is NOT a case table.
    let v = build_char_table("syntax-table", &[], Value::NIL, &[]);
    assert!(!is_case_table(&v));
}

#[test]
fn is_case_table_rejects_missing_extra_slots() {
    crate::test_utils::init_test_tracing();
    let v = build_char_table("case-table", &[], Value::NIL, &[]);
    assert!(!is_case_table(&v));
}

#[test]
fn is_case_table_rejects_invalid_extra_slots() {
    crate::test_utils::init_test_tracing();
    let invalid_upcase = build_char_table(
        "case-table",
        &[Value::fixnum(1), Value::NIL, Value::NIL],
        Value::NIL,
        &[],
    );
    assert!(!is_case_table(&invalid_upcase));

    let eqv_without_canon = build_char_table(
        "case-table",
        &[Value::NIL, Value::NIL, make_case_table_value()],
        Value::NIL,
        &[],
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
    let ct = make_standard_case_table_value();
    if ct.is_vector() {
        let vec = ct.as_vector_data().unwrap().clone();
        // extra count should be 3
        assert!(vec[CT_EXTRA_COUNT].is_fixnum());
        // extra slots 0,1,2 should be char-tables (subsidiary tables)
        use super::super::chartable::is_char_table;
        assert!(is_char_table(&vec[CT_EXTRA_START])); // upcase
        assert!(is_char_table(&vec[CT_EXTRA_START + 1])); // canonicalize
        assert!(is_char_table(&vec[CT_EXTRA_START + 2])); // equivalences
    } else {
        panic!("expected vector");
    }
}

// -----------------------------------------------------------------------
// Buffer-local case table (`set-case-table`) is honored by the case ops.
//
// GNU `casefiddle.c` / `editfns.c` resolve every cased character through the
// per-buffer up/down/canon case tables (`buffer.h` `downcase`/`upcase`). These
// install a custom table with one extra pair (mirroring `set-case-syntax-pair`)
// and assert the documented GNU oracle behavior, plus that the default path
// stays byte-identical.
// -----------------------------------------------------------------------

/// Install a custom case table with a single (UC, LC) pair in the current
/// buffer, returning the prepared evaluator. Mirrors `(set-case-table
/// (set-case-syntax-pair UC LC (copy-case-table (standard-case-table))))`.
#[cfg(test)]
fn ctx_with_pair(uc: i64, lc: i64) -> super::super::eval::Context {
    let mut ev = super::super::eval::Context::new();
    let table = make_case_table_with_pair(uc, lc);
    builtin_set_case_table(&mut ev, vec![table]).expect("set-case-table");
    ev
}

#[test]
fn custom_case_table_overrides_upcase_downcase_char() {
    crate::test_utils::init_test_tracing();
    // (set-case-syntax-pair ?z ?A): ?z is the uppercase, ?A the lowercase.
    // GNU oracle: (upcase ?z) => 122, (downcase ?A) => 65.
    let mut ev = ctx_with_pair('z' as i64, 'A' as i64);
    let up = crate::emacs_core::builtins::builtin_upcase_in_state(&mut ev, vec![Value::char('z')])
        .unwrap();
    assert_eq!(up.as_int(), Some(122));
    let down =
        crate::emacs_core::builtins::builtin_downcase_in_state(&mut ev, vec![Value::char('A')])
            .unwrap();
    assert_eq!(down.as_int(), Some(65));
}

#[test]
fn custom_case_table_overrides_upcase_region() {
    crate::test_utils::init_test_tracing();
    // Upcasing "Az" with the z/A pair: ?A upcases to ?z, ?z stays ?z => "zz".
    let mut ev = ctx_with_pair('z' as i64, 'A' as i64);
    let buffer_id = ev.buffers.current_buffer_id().expect("current buffer");
    ev.buffers.insert_into_buffer(buffer_id, "Az");
    super::super::casefiddle::builtin_upcase_region(
        &mut ev,
        vec![Value::fixnum(1), Value::fixnum(3)],
    )
    .expect("upcase-region");
    let buffer = ev.buffers.get(buffer_id).expect("buffer");
    assert_eq!(buffer.buffer_string(), "zz");
}

#[test]
fn custom_case_table_overrides_upcase_downcase_string() {
    crate::test_utils::init_test_tracing();
    let mut ev = ctx_with_pair('z' as i64, 'A' as i64);
    // upcase "Az" => "zz" (A->z, z->z).
    let up =
        crate::emacs_core::builtins::builtin_upcase_in_state(&mut ev, vec![Value::string("Az")])
            .unwrap();
    assert_eq!(up.as_utf8_str(), Some("zz"));
    // downcase "Az" => "AA" (A->A, z->A).
    let down =
        crate::emacs_core::builtins::builtin_downcase_in_state(&mut ev, vec![Value::string("Az")])
            .unwrap();
    assert_eq!(down.as_utf8_str(), Some("AA"));
}

#[test]
fn custom_case_table_makes_char_equal_case_fold() {
    crate::test_utils::init_test_tracing();
    // (set-case-syntax-pair ?x ?Y): ?x/?Y become a case pair, so with
    // case-fold-search t, (char-equal ?x ?Y) => t. GNU oracle: t.
    let mut ev = ctx_with_pair('x' as i64, 'Y' as i64);
    ev.set_variable("case-fold-search", Value::T);
    let r = crate::emacs_core::builtins::builtin_char_equal(
        &mut ev,
        vec![Value::char('x'), Value::char('Y')],
    )
    .unwrap();
    assert!(r.is_t(), "expected t, got {r:?}");

    // case-fold-search nil: exact match only.
    ev.set_variable("case-fold-search", Value::NIL);
    let r = crate::emacs_core::builtins::builtin_char_equal(
        &mut ev,
        vec![Value::char('x'), Value::char('Y')],
    )
    .unwrap();
    assert!(r.is_nil(), "expected nil, got {r:?}");
}

#[test]
fn standard_case_table_path_is_byte_identical() {
    crate::test_utils::init_test_tracing();
    // Without a custom table, the default Unicode path must be unchanged.
    let mut ev = super::super::eval::Context::new();
    let up = crate::emacs_core::builtins::builtin_upcase_in_state(&mut ev, vec![Value::char('z')])
        .unwrap();
    assert_eq!(up.as_int(), Some('Z' as i64));
    let down =
        crate::emacs_core::builtins::builtin_downcase_in_state(&mut ev, vec![Value::char('A')])
            .unwrap();
    assert_eq!(down.as_int(), Some('a' as i64));
    // Non-ASCII Unicode casing still works (é <-> É).
    let up =
        crate::emacs_core::builtins::builtin_upcase_in_state(&mut ev, vec![Value::string("héllo")])
            .unwrap();
    assert_eq!(up.as_utf8_str(), Some("HÉLLO"));
    let down = crate::emacs_core::builtins::builtin_downcase_in_state(
        &mut ev,
        vec![Value::string("HÉLLO")],
    )
    .unwrap();
    assert_eq!(down.as_utf8_str(), Some("héllo"));
    // char-equal still case-folds with the standard table.
    ev.set_variable("case-fold-search", Value::T);
    let r = crate::emacs_core::builtins::builtin_char_equal(
        &mut ev,
        vec![Value::char('a'), Value::char('A')],
    )
    .unwrap();
    assert!(r.is_t());
}
