use super::*;
use crate::emacs_core::error::Flow;
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::{intern, intern_uninterned};
use crate::emacs_core::value::ValueKind;

// -----------------------------------------------------------------------
// Char-table tests
// -----------------------------------------------------------------------

fn assert_signal_symbol_and_predicate(result: EvalResult, symbol: &str, predicate: &str) {
    match result {
        Err(Flow::Signal(signal)) => {
            assert_eq!(signal.symbol_name(), symbol);
            assert_eq!(
                signal.data.first().and_then(|v| v.as_symbol_name()),
                Some(predicate)
            );
        }
        other => panic!("expected signal {symbol} {predicate}, got {other:?}"),
    }
}

#[test]
fn make_char_table_basic() {
    crate::test_utils::init_test_tracing();
    let ct = make_char_table_value(Value::symbol("syntax-table"), Value::NIL);
    assert!(is_char_table(&ct));
    assert!(!is_bool_vector(&ct));
}

#[test]
fn ct_ref_reads_char_entry_with_default_and_range_guard() {
    // `ct_ref` is the public display-table reader (GNU `char_table_ref` /
    // `CHAR_TABLE_REF` behind `DISP_CHAR_VECTOR`): own entry, else default,
    // else nil; out-of-range chars never signal.
    crate::test_utils::init_test_tracing();
    let ct = Value::make_char_table(Value::symbol("display-table"), Value::NIL, 6);
    let glyphs = Value::vector(vec![Value::fixnum('<' as i64), Value::fixnum('>' as i64)]);
    ct_set_single(&ct, 'x' as i64, glyphs);

    // Own entry returned as-is.
    let entry = ct_ref(&ct, 'x' as i64);
    assert_eq!(entry.as_vector_data().map(|v| v.len()), Some(2));
    // Unmapped char -> nil (no default).
    assert!(ct_ref(&ct, 'y' as i64).is_nil());
    // Out-of-range / negative chars -> nil, never a signal.
    assert!(ct_ref(&ct, -1).is_nil());
    assert!(ct_ref(&ct, 0x40_0000).is_nil());
    // Not a char-table -> nil.
    assert!(ct_ref(&Value::fixnum(1), 'x' as i64).is_nil());
}

#[test]
fn make_char_table_with_default() {
    crate::test_utils::init_test_tracing();
    let ct = make_char_table_value(Value::symbol("syntax-table"), Value::fixnum(42));
    assert!(is_char_table(&ct));
    // Default lookup should return the default.
    let def = builtin_char_table_range(vec![ct, Value::NIL], None).unwrap();
    assert!(def.is_fixnum());
    assert_eq!(
        builtin_char_table_range(vec![ct, Value::fixnum('A' as i64)], None).unwrap(),
        Value::fixnum(42)
    );
}

#[test]
fn make_char_table_initializes_gnu_standard_and_extra_slots() {
    crate::test_utils::init_test_tracing();
    let ct = make_char_table_with_extra_slots(Value::symbol("neo-purpose"), Value::fixnum(7), 2);

    assert_eq!(
        builtin_char_table_extra_slot(vec![ct, Value::fixnum(0)]).unwrap(),
        Value::fixnum(7)
    );
    assert_eq!(
        builtin_char_table_extra_slot(vec![ct, Value::fixnum(1)]).unwrap(),
        Value::fixnum(7)
    );

    let slots = char_table_external_slots(&ct).unwrap();
    assert_eq!(slots[0], Value::fixnum(7));
    assert_eq!(slots[3], Value::fixnum(7));
    assert!(slots[4..68].iter().all(|slot| *slot == Value::fixnum(7)));
    assert_eq!(slots[68], Value::fixnum(7));
    assert_eq!(slots[69], Value::fixnum(7));
}

#[test]
fn builtin_make_char_table_checks_symbol_and_extra_slot_property_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    assert_signal_symbol_and_predicate(
        builtin_make_char_table(&mut eval, vec![Value::fixnum(1)]),
        "wrong-type-argument",
        "symbolp",
    );

    let purpose = Value::symbol(intern_uninterned("neo-extra"));
    eval.obarray
        .put_property_id(
            purpose.as_symbol_id().unwrap(),
            intern("char-table-extra-slots"),
            Value::make_float(1.0),
        )
        .unwrap();
    assert_signal_symbol_and_predicate(
        builtin_make_char_table(&mut eval, vec![purpose, Value::NIL]),
        "wrong-type-argument",
        "wholenump",
    );

    eval.obarray
        .put_property_id(
            purpose.as_symbol_id().unwrap(),
            intern("char-table-extra-slots"),
            Value::fixnum(1),
        )
        .unwrap();
    let ct = builtin_make_char_table(&mut eval, vec![purpose, Value::symbol("init")]).unwrap();
    assert_eq!(
        builtin_char_table_extra_slot(vec![ct, Value::fixnum(0)]).unwrap(),
        Value::symbol("init")
    );
}

#[test]
fn char_table_p_predicate() {
    crate::test_utils::init_test_tracing();
    let ct = make_char_table_value(Value::symbol("test"), Value::NIL);
    assert!(builtin_char_table_p(vec![ct]).unwrap().is_t());
    assert!(
        builtin_char_table_p(vec![Value::fixnum(5)])
            .unwrap()
            .is_nil()
    );
    assert!(builtin_char_table_p(vec![Value::NIL]).unwrap().is_nil());
}

#[test]
fn set_and_get_single_char() {
    crate::test_utils::init_test_tracing();
    let ct = make_char_table_value(Value::symbol("test"), Value::NIL);
    builtin_set_char_table_range(vec![ct, Value::fixnum(65), Value::symbol("letter-a")], None)
        .unwrap();
    let val = builtin_char_table_range(vec![ct, Value::fixnum(65)], None).unwrap();
    assert!(val.is_symbol_named("letter-a"));
}

#[test]
fn lookup_falls_back_to_default() {
    crate::test_utils::init_test_tracing();
    let ct = make_char_table_value(Value::symbol("test"), Value::symbol("default-val"));
    // No entry for char 90.
    let val = builtin_char_table_range(vec![ct, Value::fixnum(90)], None).unwrap();
    assert!(val.is_symbol_named("default-val"));
}

#[test]
fn set_range_cons() {
    crate::test_utils::init_test_tracing();
    let ct = make_char_table_value(Value::symbol("test"), Value::NIL);
    // Set chars 65..=67 (A, B, C)
    let range = Value::cons(Value::fixnum(65), Value::fixnum(67));
    builtin_set_char_table_range(vec![ct, range, Value::symbol("abc")], None).unwrap();
    for ch in 65..=67 {
        let val = builtin_char_table_range(vec![ct, Value::fixnum(ch)], None).unwrap();
        assert!(val.is_symbol_named("abc"));
    }
    // Char 68 should be nil (default).
    let val = builtin_char_table_range(vec![ct, Value::fixnum(68)], None).unwrap();
    assert!(val.is_nil());
}

#[test]
fn optimize_char_table_compacts_local_runs_without_changing_lookup() {
    crate::test_utils::init_test_tracing();
    let ct = make_char_table_value(Value::symbol("test"), Value::NIL);
    builtin_set_char_table_range(
        vec![
            ct,
            Value::cons(Value::fixnum('A' as i64), Value::fixnum('C' as i64)),
            Value::symbol("letter"),
        ],
        None,
    )
    .unwrap();
    builtin_set_char_table_range(vec![ct, Value::fixnum('B' as i64), Value::NIL], None).unwrap();
    builtin_set_char_table_range(
        vec![
            ct,
            Value::cons(Value::fixnum('D' as i64), Value::fixnum('F' as i64)),
            Value::symbol("letter"),
        ],
        None,
    )
    .unwrap();
    builtin_set_char_table_range(
        vec![
            ct,
            Value::cons(Value::fixnum('G' as i64), Value::fixnum('I' as i64)),
            Value::symbol("letter"),
        ],
        None,
    )
    .unwrap();

    optimize_char_table(&ct, OptimizeCharTableTest::Equal).unwrap();

    assert!(char_table_external_slots(&ct).is_some());
    for ch in ['A', 'C', 'D', 'E', 'F', 'G', 'H', 'I'] {
        let value = builtin_char_table_range(vec![ct, Value::fixnum(ch as i64)], None).unwrap();
        assert!(value.is_symbol_named("letter"));
    }
    assert!(
        builtin_char_table_range(vec![ct, Value::fixnum('B' as i64)], None)
            .unwrap()
            .is_nil()
    );
    assert!(
        builtin_char_table_range(vec![ct, Value::fixnum('J' as i64)], None)
            .unwrap()
            .is_nil()
    );
}

#[test]
fn optimize_char_table_preserves_later_override_precedence() {
    crate::test_utils::init_test_tracing();
    let ct = make_char_table_value(Value::symbol("test"), Value::symbol("default"));
    builtin_set_char_table_range(
        vec![
            ct,
            Value::cons(Value::fixnum(10), Value::fixnum(20)),
            Value::symbol("base"),
        ],
        None,
    )
    .unwrap();
    optimize_char_table(&ct, OptimizeCharTableTest::Equal).unwrap();

    builtin_set_char_table_range(
        vec![
            ct,
            Value::cons(Value::fixnum(15), Value::fixnum(17)),
            Value::symbol("later"),
        ],
        None,
    )
    .unwrap();
    builtin_set_char_table_range(vec![ct, Value::fixnum(16), Value::NIL], None).unwrap();

    assert!(
        builtin_char_table_range(vec![ct, Value::fixnum(14)], None)
            .unwrap()
            .is_symbol_named("base")
    );
    assert!(
        builtin_char_table_range(vec![ct, Value::fixnum(15)], None)
            .unwrap()
            .is_symbol_named("later")
    );
    assert!(
        builtin_char_table_range(vec![ct, Value::fixnum(16)], None)
            .unwrap()
            .is_symbol_named("default")
    );
    assert!(
        builtin_char_table_range(vec![ct, Value::fixnum(18)], None)
            .unwrap()
            .is_symbol_named("base")
    );
}

#[test]
fn translation_table_extra_slot_optimizes_constructed_entries() {
    crate::test_utils::init_test_tracing();
    let ct = make_char_table_with_extra_slots(Value::symbol("translation-table"), Value::NIL, 2);
    for ch in 0..96 {
        builtin_set_char_table_range(
            vec![ct, Value::fixnum(0x1000 + ch), Value::fixnum(ch)],
            None,
        )
        .unwrap();
    }

    builtin_set_char_table_extra_slot(vec![ct, Value::fixnum(1), Value::fixnum(1)]).unwrap();

    assert!(char_table_external_slots(&ct).is_some());
    for ch in [0, 11, 57, 95] {
        assert_eq!(
            builtin_char_table_range(vec![ct, Value::fixnum(0x1000 + ch)], None)
                .unwrap()
                .as_fixnum(),
            Some(ch)
        );
    }
}

#[test]
fn optimize_char_table_custom_test_is_noop_until_callbacks_are_supported() {
    crate::test_utils::init_test_tracing();
    let ct = make_char_table_value(Value::symbol("test"), Value::NIL);
    let first = Value::string("same");
    let second = Value::string("same");
    builtin_set_char_table_range(vec![ct, Value::fixnum('A' as i64), first], None).unwrap();
    builtin_set_char_table_range(vec![ct, Value::fixnum('B' as i64), second], None).unwrap();

    crate::emacs_core::builtins::symbols::builtin_optimize_char_table(vec![
        ct,
        Value::symbol("custom-test"),
    ])
    .unwrap();

    assert!(char_table_external_slots(&ct).is_some());
    let first_lookup = builtin_char_table_range(vec![ct, Value::fixnum('A' as i64)], None).unwrap();
    let second_lookup =
        builtin_char_table_range(vec![ct, Value::fixnum('B' as i64)], None).unwrap();
    assert!(crate::emacs_core::value::eq_value(&first_lookup, &first));
    assert!(crate::emacs_core::value::eq_value(&second_lookup, &second));
}

#[test]
fn set_default_via_range_nil() {
    crate::test_utils::init_test_tracing();
    let ct = make_char_table_value(Value::symbol("test"), Value::NIL);
    builtin_set_char_table_range(vec![ct, Value::NIL, Value::fixnum(999)], None).unwrap();
    let def = builtin_char_table_range(vec![ct, Value::NIL], None).unwrap();
    assert!(def.is_fixnum());
}

#[test]
fn set_range_t_sets_default_value() {
    crate::test_utils::init_test_tracing();
    // In GNU Emacs, (set-char-table-range ct t value) sets all character
    // entries, but leaves the default slot untouched.
    let ct = make_char_table_value(Value::symbol("test"), Value::fixnum(0));
    builtin_set_char_table_range(vec![ct, Value::T, Value::fixnum(5)], None).unwrap();

    let a = builtin_char_table_range(vec![ct, Value::fixnum('a' as i64)], None).unwrap();
    let b = builtin_char_table_range(vec![ct, Value::fixnum('b' as i64)], None).unwrap();
    let def = builtin_char_table_range(vec![ct, Value::NIL], None).unwrap();
    assert!(a.is_fixnum());
    assert!(b.is_fixnum());
    assert!(def.is_fixnum());
}

#[test]
fn set_range_t_allows_single_char_override() {
    crate::test_utils::init_test_tracing();
    // (set-char-table-range ct t 5) sets all characters to 5 without touching
    // the default slot. Later single-char overrides take precedence.
    let ct = make_char_table_value(Value::symbol("test"), Value::fixnum(0));
    builtin_set_char_table_range(vec![ct, Value::T, Value::fixnum(5)], None).unwrap();
    builtin_set_char_table_range(vec![ct, Value::fixnum('a' as i64), Value::fixnum(9)], None)
        .unwrap();

    let a = builtin_char_table_range(vec![ct, Value::fixnum('a' as i64)], None).unwrap();
    let b = builtin_char_table_range(vec![ct, Value::fixnum('b' as i64)], None).unwrap();
    let def = builtin_char_table_range(vec![ct, Value::NIL], None).unwrap();
    assert!(a.is_fixnum());
    assert!(b.is_fixnum());
    assert!(def.is_fixnum());
}

#[test]
fn later_t_write_overrides_prior_specific_entries() {
    crate::test_utils::init_test_tracing();
    let ct = make_char_table_value(Value::symbol("test"), Value::NIL);
    builtin_set_char_table_range(vec![ct, Value::fixnum('a' as i64), Value::fixnum(9)], None)
        .unwrap();
    builtin_set_char_table_range(
        vec![
            ct,
            Value::cons(Value::fixnum('0' as i64), Value::fixnum('9' as i64)),
            Value::fixnum(7),
        ],
        None,
    )
    .unwrap();
    builtin_set_char_table_range(vec![ct, Value::T, Value::fixnum(5)], None).unwrap();

    let a = builtin_char_table_range(vec![ct, Value::fixnum('a' as i64)], None).unwrap();
    let five = builtin_char_table_range(vec![ct, Value::fixnum('5' as i64)], None).unwrap();
    let def = builtin_char_table_range(vec![ct, Value::NIL], None).unwrap();
    assert!(a.is_fixnum());
    assert!(five.is_fixnum());
    assert!(def.is_nil());
}

#[test]
fn parent_chain_lookup() {
    crate::test_utils::init_test_tracing();
    let parent = make_char_table_value(Value::symbol("test"), Value::NIL);
    builtin_set_char_table_range(
        vec![parent, Value::fixnum(65), Value::symbol("from-parent")],
        None,
    )
    .unwrap();
    let child = make_char_table_value(Value::symbol("test"), Value::NIL);
    builtin_set_char_table_parent(vec![child, parent]).unwrap();

    // Lookup in child falls through to parent.
    let val = builtin_char_table_range(vec![child, Value::fixnum(65)], None).unwrap();
    assert!(val.is_symbol_named("from-parent"));

    // Child override takes priority.
    builtin_set_char_table_range(
        vec![child, Value::fixnum(65), Value::symbol("child-val")],
        None,
    )
    .unwrap();
    let val = builtin_char_table_range(vec![child, Value::fixnum(65)], None).unwrap();
    assert!(val.is_symbol_named("child-val"));
}

#[test]
fn ascii_cache_follows_latest_write_and_default_fallback() {
    crate::test_utils::init_test_tracing();
    let ct = make_char_table_value(Value::symbol("test"), Value::symbol("default"));

    builtin_set_char_table_range(
        vec![ct, Value::fixnum('A' as i64), Value::symbol("first")],
        None,
    )
    .unwrap();
    builtin_set_char_table_range(
        vec![ct, Value::fixnum('A' as i64), Value::symbol("second")],
        None,
    )
    .unwrap();
    let val = builtin_char_table_range(vec![ct, Value::fixnum('A' as i64)], None).unwrap();
    assert!(val.is_symbol_named("second"));

    builtin_set_char_table_range(vec![ct, Value::fixnum('A' as i64), Value::NIL], None).unwrap();
    let val = builtin_char_table_range(vec![ct, Value::fixnum('A' as i64)], None).unwrap();
    assert!(val.is_symbol_named("default"));
}

#[test]
fn ascii_cache_falls_through_to_parent_when_local_value_is_nil() {
    crate::test_utils::init_test_tracing();
    let parent = make_char_table_value(Value::symbol("test"), Value::NIL);
    builtin_set_char_table_range(
        vec![parent, Value::fixnum('A' as i64), Value::symbol("parent")],
        None,
    )
    .unwrap();

    let child = make_char_table_value(Value::symbol("test"), Value::NIL);
    builtin_set_char_table_parent(vec![child, parent]).unwrap();
    builtin_set_char_table_range(
        vec![child, Value::fixnum('A' as i64), Value::symbol("child")],
        None,
    )
    .unwrap();
    builtin_set_char_table_range(vec![child, Value::fixnum('A' as i64), Value::NIL], None).unwrap();

    let val = builtin_char_table_range(vec![child, Value::fixnum('A' as i64)], None).unwrap();
    assert!(val.is_symbol_named("parent"));
}

#[test]
fn char_table_parent_get_set() {
    crate::test_utils::init_test_tracing();
    let ct = make_char_table_value(Value::symbol("test"), Value::NIL);
    // Initially nil.
    let p = builtin_char_table_parent(vec![ct]).unwrap();
    assert!(p.is_nil());

    let parent = make_char_table_value(Value::symbol("parent"), Value::NIL);
    builtin_set_char_table_parent(vec![ct, parent]).unwrap();
    let p = builtin_char_table_parent(vec![ct]).unwrap();
    assert!(is_char_table(&p));
}

#[test]
fn set_char_table_parent_nil() {
    crate::test_utils::init_test_tracing();
    let ct = make_char_table_value(Value::symbol("test"), Value::NIL);
    let parent = make_char_table_value(Value::symbol("parent"), Value::NIL);
    builtin_set_char_table_parent(vec![ct, parent]).unwrap();
    builtin_set_char_table_parent(vec![ct, Value::NIL]).unwrap();
    let p = builtin_char_table_parent(vec![ct]).unwrap();
    assert!(p.is_nil());
}

#[test]
fn set_char_table_parent_wrong_type() {
    crate::test_utils::init_test_tracing();
    let ct = make_char_table_value(Value::symbol("test"), Value::NIL);
    let result = builtin_set_char_table_parent(vec![ct, Value::fixnum(5)]);
    assert!(result.is_err());
}

#[test]
fn char_table_extra_slot_basic() {
    crate::test_utils::init_test_tracing();
    let ct = make_char_table_value(Value::symbol("test"), Value::NIL);
    // Initially 0 extra slots -- should error.
    let result = builtin_char_table_extra_slot(vec![ct, Value::fixnum(0)]);
    assert!(result.is_err());

    // Setting an out-of-range slot also errors in Emacs.
    let set_result =
        builtin_set_char_table_extra_slot(vec![ct, Value::fixnum(0), Value::symbol("extra0")]);
    assert!(set_result.is_err());
}

#[test]
fn char_table_extra_slot_index_type_error_uses_fixnump() {
    crate::test_utils::init_test_tracing();
    let ct = make_char_table_with_extra_slots(Value::symbol("test"), Value::NIL, 1);

    assert_signal_symbol_and_predicate(
        builtin_char_table_extra_slot(vec![ct, Value::make_float(0.0)]),
        "wrong-type-argument",
        "fixnump",
    );
    assert_signal_symbol_and_predicate(
        builtin_set_char_table_extra_slot(vec![ct, Value::make_float(0.0), Value::symbol("x")]),
        "wrong-type-argument",
        "fixnump",
    );
}

#[test]
fn char_table_extra_slot_preserves_data() {
    crate::test_utils::init_test_tracing();
    let ct = make_char_table_value(Value::symbol("test"), Value::NIL);
    // Set a char entry first.
    builtin_set_char_table_range(vec![ct, Value::fixnum(65), Value::symbol("a-val")], None)
        .unwrap();
    // Attempting to set an out-of-range extra slot should fail.
    assert!(
        builtin_set_char_table_extra_slot(vec![ct, Value::fixnum(0), Value::symbol("e0")]).is_err()
    );
    // The char entry should still be intact.
    let val = builtin_char_table_range(vec![ct, Value::fixnum(65)], None).unwrap();
    assert!(val.is_symbol_named("a-val"));
    // Extra slot remains out-of-range.
    assert!(builtin_char_table_extra_slot(vec![ct, Value::fixnum(0)]).is_err());
}

#[test]
fn char_table_subtype() {
    crate::test_utils::init_test_tracing();
    let ct = make_char_table_value(Value::symbol("syntax-table"), Value::NIL);
    let st = builtin_char_table_subtype(vec![ct]).unwrap();
    assert!(st.is_symbol_named("syntax-table"));
}

#[test]
fn char_table_overwrite_entry() {
    crate::test_utils::init_test_tracing();
    let ct = make_char_table_value(Value::symbol("test"), Value::NIL);
    builtin_set_char_table_range(vec![ct, Value::fixnum(65), Value::fixnum(1)], None).unwrap();
    builtin_set_char_table_range(vec![ct, Value::fixnum(65), Value::fixnum(2)], None).unwrap();
    let val = builtin_char_table_range(vec![ct, Value::fixnum(65)], None).unwrap();
    assert!(val.is_fixnum());
}

#[test]
fn later_range_overrides_earlier_single_entry() {
    crate::test_utils::init_test_tracing();
    let ct = make_char_table_value(Value::symbol("test"), Value::NIL);
    builtin_set_char_table_range(
        vec![ct, Value::fixnum('M' as i64), Value::symbol("single")],
        None,
    )
    .unwrap();
    builtin_set_char_table_range(
        vec![
            ct,
            Value::cons(Value::fixnum('A' as i64), Value::fixnum('Z' as i64)),
            Value::symbol("range"),
        ],
        None,
    )
    .unwrap();

    let val = builtin_char_table_range(vec![ct, Value::fixnum('M' as i64)], None).unwrap();
    assert!(val.is_symbol_named("range"));
}

#[test]
fn explicit_nil_entry_inherits_from_parent() {
    crate::test_utils::init_test_tracing();
    let parent = make_char_table_value(Value::symbol("test"), Value::NIL);
    builtin_set_char_table_range(
        vec![parent, Value::fixnum('a' as i64), Value::symbol("parent-a")],
        None,
    )
    .unwrap();

    let child = make_char_table_value(Value::symbol("test"), Value::NIL);
    builtin_set_char_table_parent(vec![child, parent]).unwrap();
    builtin_set_char_table_range(vec![child, Value::fixnum('a' as i64), Value::NIL], None).unwrap();

    let val = builtin_char_table_range(vec![child, Value::fixnum('a' as i64)], None).unwrap();
    assert!(val.is_symbol_named("parent-a"));
}

#[test]
fn set_char_table_parent_rejects_cycles() {
    crate::test_utils::init_test_tracing();
    let parent = make_char_table_value(Value::symbol("test"), Value::NIL);
    let child = make_char_table_value(Value::symbol("test"), Value::NIL);
    builtin_set_char_table_parent(vec![child, parent]).unwrap();

    let result = builtin_set_char_table_parent(vec![parent, child]);
    assert!(result.is_err());
}

#[test]
fn map_char_table_coalesces_ranges_after_single_override() {
    crate::test_utils::init_test_tracing();
    let ct = make_char_table_value(Value::symbol("test"), Value::NIL);
    builtin_set_char_table_range(
        vec![
            ct,
            Value::cons(Value::fixnum('A' as i64), Value::fixnum('Z' as i64)),
            Value::symbol("upper"),
        ],
        None,
    )
    .unwrap();
    builtin_set_char_table_range(
        vec![ct, Value::fixnum('M' as i64), Value::symbol("middle")],
        None,
    )
    .unwrap();

    let entries = ct_resolved_entries(&ct);
    assert_eq!(entries.len(), 3);
    assert_eq!(
        entries,
        vec![
            (
                Value::cons(Value::fixnum('A' as i64), Value::fixnum('L' as i64)),
                Value::symbol("upper"),
            ),
            (Value::fixnum('M' as i64), Value::symbol("middle")),
            (
                Value::cons(Value::fixnum('N' as i64), Value::fixnum('Z' as i64)),
                Value::symbol("upper"),
            ),
        ]
    );
}

#[test]
fn map_char_table_coalesces_adjacent_single_overrides() {
    crate::test_utils::init_test_tracing();
    let ct = make_char_table_value(Value::symbol("test"), Value::NIL);
    builtin_set_char_table_range(
        vec![ct, Value::fixnum('A' as i64), Value::symbol("upper")],
        None,
    )
    .unwrap();
    builtin_set_char_table_range(
        vec![ct, Value::fixnum('B' as i64), Value::symbol("upper")],
        None,
    )
    .unwrap();
    builtin_set_char_table_range(
        vec![ct, Value::fixnum('Z' as i64), Value::symbol("last")],
        None,
    )
    .unwrap();

    let entries = ct_resolved_entries(&ct);
    assert_eq!(
        entries,
        vec![
            (
                Value::cons(Value::fixnum('A' as i64), Value::fixnum('B' as i64)),
                Value::symbol("upper"),
            ),
            (Value::fixnum('Z' as i64), Value::symbol("last")),
        ]
    );
}

#[test]
fn map_char_table_latest_nil_entry_falls_back_to_parent_run() {
    crate::test_utils::init_test_tracing();
    let parent = make_char_table_value(Value::symbol("test"), Value::NIL);
    builtin_set_char_table_range(
        vec![
            parent,
            Value::cons(Value::fixnum('A' as i64), Value::fixnum('Z' as i64)),
            Value::symbol("parent"),
        ],
        None,
    )
    .unwrap();

    let child = make_char_table_value(Value::symbol("test"), Value::NIL);
    builtin_set_char_table_parent(vec![child, parent]).unwrap();
    builtin_set_char_table_range(
        vec![
            child,
            Value::cons(Value::fixnum('A' as i64), Value::fixnum('Z' as i64)),
            Value::symbol("child"),
        ],
        None,
    )
    .unwrap();
    builtin_set_char_table_range(vec![child, Value::fixnum('M' as i64), Value::NIL], None).unwrap();

    let entries = ct_resolved_entries(&child);
    assert_eq!(
        entries,
        vec![
            (
                Value::cons(Value::fixnum('A' as i64), Value::fixnum('L' as i64)),
                Value::symbol("child"),
            ),
            (Value::fixnum('M' as i64), Value::symbol("parent")),
            (
                Value::cons(Value::fixnum('N' as i64), Value::fixnum('Z' as i64)),
                Value::symbol("child"),
            ),
        ]
    );
}

#[test]
fn effective_runs_parent_fallback_handles_multiple_nil_child_spans() {
    crate::test_utils::init_test_tracing();
    let parent = make_char_table_value(Value::symbol("test"), Value::NIL);
    builtin_set_char_table_range(
        vec![
            parent,
            Value::cons(Value::fixnum('A' as i64), Value::fixnum('C' as i64)),
            Value::symbol("parent-left"),
        ],
        None,
    )
    .unwrap();
    builtin_set_char_table_range(
        vec![
            parent,
            Value::cons(Value::fixnum('G' as i64), Value::fixnum('I' as i64)),
            Value::symbol("parent-right"),
        ],
        None,
    )
    .unwrap();

    let child = make_char_table_value(Value::symbol("test"), Value::NIL);
    builtin_set_char_table_parent(vec![child, parent]).unwrap();
    builtin_set_char_table_range(
        vec![
            child,
            Value::cons(Value::fixnum('A' as i64), Value::fixnum('I' as i64)),
            Value::symbol("child"),
        ],
        None,
    )
    .unwrap();
    builtin_set_char_table_range(
        vec![
            child,
            Value::cons(Value::fixnum('B' as i64), Value::fixnum('C' as i64)),
            Value::NIL,
        ],
        None,
    )
    .unwrap();
    builtin_set_char_table_range(
        vec![
            child,
            Value::cons(Value::fixnum('G' as i64), Value::fixnum('H' as i64)),
            Value::NIL,
        ],
        None,
    )
    .unwrap();

    let entries = ct_resolved_entries(&child);
    assert_eq!(
        entries,
        vec![
            (Value::fixnum('A' as i64), Value::symbol("child")),
            (
                Value::cons(Value::fixnum('B' as i64), Value::fixnum('C' as i64)),
                Value::symbol("parent-left"),
            ),
            (
                Value::cons(Value::fixnum('D' as i64), Value::fixnum('F' as i64)),
                Value::symbol("child"),
            ),
            (
                Value::cons(Value::fixnum('G' as i64), Value::fixnum('H' as i64)),
                Value::symbol("parent-right"),
            ),
            (Value::fixnum('I' as i64), Value::symbol("child")),
        ]
    );
}

#[test]
fn atomic_runs_in_range_preserve_child_shadowing_and_parent_fallback() {
    crate::test_utils::init_test_tracing();
    let parent = make_char_table_value(Value::symbol("test"), Value::NIL);
    builtin_set_char_table_range(
        vec![
            parent,
            Value::cons(Value::fixnum('A' as i64), Value::fixnum('F' as i64)),
            Value::symbol("parent"),
        ],
        None,
    )
    .unwrap();

    let child = make_char_table_value(Value::symbol("test"), Value::NIL);
    builtin_set_char_table_parent(vec![child, parent]).unwrap();
    builtin_set_char_table_range(
        vec![
            child,
            Value::cons(Value::fixnum('A' as i64), Value::fixnum('F' as i64)),
            Value::symbol("child"),
        ],
        None,
    )
    .unwrap();
    builtin_set_char_table_range(
        vec![
            child,
            Value::cons(Value::fixnum('C' as i64), Value::fixnum('D' as i64)),
            Value::NIL,
        ],
        None,
    )
    .unwrap();

    let runs =
        char_table_atomic_runs_in_range(&child, 'A' as i64, 'F' as i64).expect("atomic runs");
    assert_eq!(
        runs,
        vec![
            (Value::symbol("child"), 'A' as i64, 'B' as i64),
            (Value::symbol("parent"), 'C' as i64, 'D' as i64),
            (Value::symbol("child"), 'E' as i64, 'F' as i64),
        ]
    );
}

#[test]
fn map_char_table_shared_range_survives_callback_gc() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let ct = make_char_table_value(Value::symbol("test"), Value::NIL);
    builtin_set_char_table_range(
        vec![
            ct,
            Value::cons(Value::fixnum('A' as i64), Value::fixnum('Z' as i64)),
            Value::symbol("upper"),
        ],
        None,
    )
    .unwrap();
    builtin_set_char_table_range(
        vec![ct, Value::fixnum('M' as i64), Value::symbol("middle")],
        None,
    )
    .unwrap();

    let roots = eval.save_specpdl_roots();
    eval.push_specpdl_root(ct);
    let mut seen = 0;
    let result = for_each_char_table_mapping(&ct, |_key, _value| {
        seen += 1;
        eval.gc_collect_exact();
        Ok(())
    });
    eval.restore_specpdl_roots(roots);

    result.unwrap();
    assert_eq!(seen, 3);

    let first = Value::cons(Value::fixnum(1), Value::NIL);
    let second = Value::cons(Value::fixnum(2), first);
    assert!(second.is_cons());
}

#[test]
fn map_char_table_decodes_unicode_property_run_length_values() {
    crate::test_utils::init_test_tracing();
    let table =
        make_char_table_with_extra_slots(Value::symbol("char-code-property-table"), Value::NIL, 5);
    builtin_set_char_table_extra_slot(vec![
        table,
        Value::fixnum(0),
        Value::symbol("general-category"),
    ])
    .unwrap();
    builtin_set_char_table_extra_slot(vec![table, Value::fixnum(1), Value::fixnum(0)]).unwrap();
    builtin_set_char_table_extra_slot(vec![
        table,
        Value::fixnum(4),
        Value::vector(vec![Value::NIL, Value::symbol("Lu"), Value::symbol("Ll")]),
    ])
    .unwrap();
    builtin_set_char_table_range(
        vec![
            table,
            Value::cons(Value::fixnum('A' as i64), Value::fixnum('B' as i64)),
            Value::fixnum(1),
        ],
        None,
    )
    .unwrap();
    builtin_set_char_table_range(
        vec![table, Value::fixnum('c' as i64), Value::fixnum(2)],
        None,
    )
    .unwrap();

    let mut values = Vec::new();
    for_each_char_table_mapping(&table, |_key, value| {
        values.push(value);
        Ok(())
    })
    .unwrap();

    assert_eq!(values, vec![Value::symbol("Lu"), Value::symbol("Ll")]);
}

#[test]
fn char_table_range_uncompresses_unicode_property_character_blocks() {
    crate::test_utils::init_test_tracing();
    let table =
        make_char_table_with_extra_slots(Value::symbol("char-code-property-table"), Value::NIL, 5);
    builtin_set_char_table_extra_slot(vec![table, Value::fixnum(0), Value::symbol("uppercase")])
        .unwrap();
    builtin_set_char_table_extra_slot(vec![table, Value::fixnum(2), Value::fixnum(0)]).unwrap();
    builtin_set_char_table_range(
        vec![
            table,
            Value::cons(Value::fixnum(128), Value::fixnum(255)),
            Value::string("\u{1}\u{2}AB"),
        ],
        None,
    )
    .unwrap();

    assert_eq!(
        builtin_char_table_range(vec![table, Value::fixnum(129)], None).unwrap(),
        Value::NIL
    );
    assert_eq!(
        builtin_char_table_range(vec![table, Value::fixnum(130)], None).unwrap(),
        Value::fixnum('A' as i64)
    );
    assert_eq!(
        builtin_char_table_range(vec![table, Value::fixnum(131)], None).unwrap(),
        Value::fixnum('B' as i64)
    );
}

#[test]
fn get_unicode_property_internal_uncompresses_run_length_blocks() {
    crate::test_utils::init_test_tracing();
    let table =
        make_char_table_with_extra_slots(Value::symbol("char-code-property-table"), Value::NIL, 5);
    builtin_set_char_table_extra_slot(vec![
        table,
        Value::fixnum(0),
        Value::symbol("general-category"),
    ])
    .unwrap();
    builtin_set_char_table_extra_slot(vec![table, Value::fixnum(1), Value::fixnum(0)]).unwrap();
    builtin_set_char_table_extra_slot(vec![
        table,
        Value::fixnum(4),
        Value::vector(vec![Value::NIL, Value::symbol("Lu"), Value::symbol("Ll")]),
    ])
    .unwrap();
    builtin_set_char_table_range(
        vec![
            table,
            Value::cons(Value::fixnum(256), Value::fixnum(383)),
            Value::string("\u{2}\u{1}\u{83}\u{2}"),
        ],
        None,
    )
    .unwrap();

    assert_eq!(
        builtin_get_unicode_property_internal(vec![table, Value::fixnum(256)]).unwrap(),
        Value::symbol("Lu")
    );
    assert_eq!(
        builtin_get_unicode_property_internal(vec![table, Value::fixnum(258)]).unwrap(),
        Value::symbol("Lu")
    );
    assert_eq!(
        builtin_get_unicode_property_internal(vec![table, Value::fixnum(259)]).unwrap(),
        Value::symbol("Ll")
    );
    assert_eq!(
        builtin_get_unicode_property_internal(vec![table, Value::fixnum(260)]).unwrap(),
        Value::NIL
    );
}

#[test]
fn unicode_property_run_length_decoder_treats_raw_byte_counts_as_bytes() {
    crate::test_utils::init_test_tracing();
    let compressed = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
        2, 2, 0xC0, 0x83, 1,
    ]));
    let mut depth2_slots = vec![Value::fixnum(2), Value::fixnum(0), compressed];
    depth2_slots.extend(std::iter::repeat_n(Value::NIL, 31));
    let depth2 = make_sub_char_table_from_external_slots(&depth2_slots).unwrap();
    let mut depth1_slots = vec![Value::fixnum(1), Value::fixnum(0), depth2];
    depth1_slots.extend(std::iter::repeat_n(Value::NIL, 15));
    let depth1 = make_sub_char_table_from_external_slots(&depth1_slots).unwrap();
    let mut slots = vec![
        Value::NIL,
        Value::NIL,
        Value::symbol("char-code-property-table"),
        Value::fixnum(26),
        depth1,
    ];
    slots.extend(std::iter::repeat_n(Value::NIL, 63));
    slots.extend([
        Value::symbol("general-category"),
        Value::fixnum(0),
        Value::NIL,
        Value::NIL,
        Value::vector(vec![
            Value::NIL,
            Value::symbol("Lu"),
            Value::symbol("Ll"),
            Value::symbol("Lt"),
            Value::symbol("Lm"),
            Value::symbol("Lo"),
            Value::symbol("Mn"),
            Value::symbol("Mc"),
            Value::symbol("Me"),
            Value::symbol("Nd"),
            Value::symbol("Nl"),
            Value::symbol("No"),
            Value::symbol("Pc"),
            Value::symbol("Pd"),
            Value::symbol("Ps"),
            Value::symbol("Pe"),
            Value::symbol("Pi"),
            Value::symbol("Pf"),
            Value::symbol("Po"),
            Value::symbol("Sm"),
            Value::symbol("Sc"),
            Value::symbol("Sk"),
            Value::symbol("So"),
            Value::symbol("Zs"),
            Value::symbol("Zl"),
            Value::symbol("Zp"),
            Value::symbol("Cc"),
            Value::symbol("Cf"),
            Value::symbol("Cs"),
            Value::symbol("Co"),
            Value::symbol("Cn"),
        ]),
    ]);
    let table = make_char_table_from_external_slots(&slots).unwrap();

    assert_eq!(
        builtin_get_unicode_property_internal(vec![table, Value::fixnum(0)]).unwrap(),
        Value::symbol("Ll")
    );
    assert_eq!(
        builtin_get_unicode_property_internal(vec![table, Value::fixnum(3)]).unwrap(),
        Value::symbol("Lu")
    );
}

#[test]
fn unicode_property_ascii_general_category_prefix_decodes_uppercase_letters() {
    crate::test_utils::init_test_tracing();
    let compressed = Value::heap_string(crate::heap_types::LispString::from_unibyte(vec![
        2, 26, 0xC0, 0xA0, 23, 18, 0xC0, 0x83, 20, 18, 0xC0, 0x83, 14, 15, 18, 19, 18, 13, 18, 18,
        9, 0xC0, 0x8A, 18, 18, 19, 0xC0, 0x83, 18, 18, 1, 0xC0, 0x9A,
    ]));
    let mut depth2_slots = vec![Value::fixnum(2), Value::fixnum(0), compressed];
    depth2_slots.extend(std::iter::repeat_n(Value::NIL, 31));
    let depth2 = make_sub_char_table_from_external_slots(&depth2_slots).unwrap();
    let mut depth1_slots = vec![Value::fixnum(1), Value::fixnum(0), depth2];
    depth1_slots.extend(std::iter::repeat_n(Value::NIL, 15));
    let depth1 = make_sub_char_table_from_external_slots(&depth1_slots).unwrap();
    let mut slots = vec![
        Value::NIL,
        Value::NIL,
        Value::symbol("char-code-property-table"),
        Value::fixnum(26),
        depth1,
    ];
    slots.extend(std::iter::repeat_n(Value::NIL, 63));
    slots.extend([
        Value::symbol("general-category"),
        Value::fixnum(0),
        Value::NIL,
        Value::NIL,
        Value::vector(vec![
            Value::NIL,
            Value::symbol("Lu"),
            Value::symbol("Ll"),
            Value::symbol("Lt"),
            Value::symbol("Lm"),
            Value::symbol("Lo"),
            Value::symbol("Mn"),
            Value::symbol("Mc"),
            Value::symbol("Me"),
            Value::symbol("Nd"),
            Value::symbol("Nl"),
            Value::symbol("No"),
            Value::symbol("Pc"),
            Value::symbol("Pd"),
            Value::symbol("Ps"),
            Value::symbol("Pe"),
            Value::symbol("Pi"),
            Value::symbol("Pf"),
            Value::symbol("Po"),
            Value::symbol("Sm"),
            Value::symbol("Sc"),
            Value::symbol("Sk"),
            Value::symbol("So"),
            Value::symbol("Zs"),
            Value::symbol("Zl"),
            Value::symbol("Zp"),
            Value::symbol("Cc"),
            Value::symbol("Cf"),
            Value::symbol("Cs"),
            Value::symbol("Co"),
            Value::symbol("Cn"),
        ]),
    ]);
    let table = make_char_table_from_external_slots(&slots).unwrap();

    assert_eq!(
        builtin_get_unicode_property_internal(vec![table, Value::fixnum(64)]).unwrap(),
        Value::symbol("Po")
    );
    assert_eq!(
        builtin_get_unicode_property_internal(vec![table, Value::fixnum(65)]).unwrap(),
        Value::symbol("Lu")
    );
}

#[test]
fn unicode_property_literal_loaded_from_el_source_preserves_raw_compression_bytes() {
    crate::test_utils::init_test_tracing();
    let compressed = [
        2, 26, 0xC0, 0xA0, 23, 18, 0xC0, 0x83, 20, 18, 0xC0, 0x83, 14, 15, 18, 19, 18, 13, 18, 18,
        9, 0xC0, 0x8A, 18, 18, 19, 0xC0, 0x83, 18, 18, 1, 0xC0, 0x9A,
    ];
    let mut source = br#"#^[30 nil char-code-property-table 26 #^^[1 0 #^^[2 0 ""#.to_vec();
    source.extend_from_slice(&compressed);
    source.extend_from_slice(br#"" "#);
    source.extend(
        std::iter::repeat_n(b"nil ".as_slice(), 31)
            .flatten()
            .copied(),
    );
    source.extend_from_slice(br#"] "#);
    source.extend(
        std::iter::repeat_n(b"nil ".as_slice(), 15)
            .flatten()
            .copied(),
    );
    source.extend_from_slice(br#"] "#);
    source.extend(
        std::iter::repeat_n(b"nil ".as_slice(), 63)
            .flatten()
            .copied(),
    );
    source.extend_from_slice(
        b"general-category 0 nil nil [nil Lu Ll Lt Lm Lo Mn Mc Me Nd Nl No Pc Pd Ps Pe Pi Pf Po Sm Sc Sk So Zs Zl Zp Cc Cf Cs Co Cn]]",
    );

    let decoded = crate::emacs_core::load::decode_emacs_utf8(&source);
    let forms = crate::emacs_core::value_reader::read_all_with_source_multibyte(
        &decoded,
        true,
        &crate::emacs_core::symbol::Obarray::new(),
    )
    .expect("reader should preserve raw byte strings in .el source");
    assert_eq!(forms.len(), 1);
    let table = forms[0];

    assert_eq!(
        builtin_get_unicode_property_internal(vec![table, Value::fixnum(64)]).unwrap(),
        Value::symbol("Po")
    );
    assert_eq!(
        builtin_get_unicode_property_internal(vec![table, Value::fixnum(65)]).unwrap(),
        Value::symbol("Lu")
    );
}

fn find_char_table(value: Value) -> Option<Value> {
    if crate::emacs_core::chartable::is_char_table(&value) {
        return Some(value);
    }
    if let Some(items) = value.as_vector_data() {
        for item in items {
            if let Some(table) = find_char_table(*item) {
                return Some(table);
            }
        }
        return None;
    }
    if value.is_cons() {
        let mut current = value;
        while current.is_cons() {
            if let Some(table) = find_char_table(current.cons_car()) {
                return Some(table);
            }
            current = current.cons_cdr();
        }
        if !current.is_nil() {
            return find_char_table(current);
        }
    }
    None
}

fn run_key_from_map_key(key: Value) -> (i64, i64) {
    match key.kind() {
        ValueKind::Fixnum(ch) => (ch, ch),
        ValueKind::Cons => (
            key.cons_car().as_fixnum().unwrap_or(-1),
            key.cons_cdr().as_fixnum().unwrap_or(-1),
        ),
        _ => (-1, -1),
    }
}

#[test]
fn unicode_property_table_read_from_generated_uni_category_decodes_ascii() {
    crate::test_utils::init_test_tracing();
    let path = std::path::Path::new(env!("CARGO_WORKSPACE_DIR"))
        .join("lisp/international/uni-category.el");
    let bytes = std::fs::read(&path).expect("read generated Unicode category table");
    let decoded = crate::emacs_core::load::decode_emacs_utf8(&bytes);
    let forms = crate::emacs_core::value_reader::read_all_with_source_multibyte(
        &decoded,
        true,
        &crate::emacs_core::symbol::Obarray::new(),
    )
    .expect("reader should parse generated Unicode category table");
    let table = forms
        .into_iter()
        .find_map(find_char_table)
        .expect("generated file should contain a char-table literal");

    assert_eq!(
        builtin_get_unicode_property_internal(vec![table, Value::fixnum(64)]).unwrap(),
        Value::symbol("Po")
    );
    assert_eq!(
        builtin_get_unicode_property_internal(vec![table, Value::fixnum(65)]).unwrap(),
        Value::symbol("Lu")
    );
}

#[test]
fn unicode_property_table_read_from_generated_uni_bidi_maps_decoded_symbols() {
    crate::test_utils::init_test_tracing();
    let path =
        std::path::Path::new(env!("CARGO_WORKSPACE_DIR")).join("lisp/international/uni-bidi.el");
    let bytes = std::fs::read(&path).expect("read generated Unicode bidi table");
    let decoded = crate::emacs_core::load::decode_emacs_utf8(&bytes);
    let forms = crate::emacs_core::value_reader::read_all_with_source_multibyte(
        &decoded,
        true,
        &crate::emacs_core::symbol::Obarray::new(),
    )
    .expect("reader should parse generated Unicode bidi table");
    let table = forms
        .into_iter()
        .find_map(find_char_table)
        .expect("generated file should contain a char-table literal");

    assert_eq!(
        builtin_get_unicode_property_internal(vec![table, Value::fixnum(65)]).unwrap(),
        Value::symbol("L")
    );
    let mut has_l_for_a = false;
    let mut first_mappings = Vec::new();
    for_each_char_table_mapping(&table, |key, value| {
        if first_mappings.len() < 16 {
            first_mappings.push((run_key_from_map_key(key), value));
        }
        if match key.kind() {
            ValueKind::Fixnum(ch) => ch == 65 && value == Value::symbol("L"),
            ValueKind::Cons => {
                key.cons_car().as_fixnum().is_some_and(|min| min <= 65)
                    && key.cons_cdr().as_fixnum().is_some_and(|max| max >= 65)
                    && value == Value::symbol("L")
            }
            _ => false,
        } {
            has_l_for_a = true;
        }
        Ok(())
    })
    .unwrap();
    assert!(
        has_l_for_a,
        "map-char-table entries should expose decoded Unicode property values; first mappings: {:?}",
        first_mappings
    );
}

#[test]
fn format_percent_s_prints_unicode_property_table_as_gnu_char_table() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let table =
        make_char_table_with_extra_slots(Value::symbol("char-code-property-table"), Value::NIL, 5);
    builtin_set_char_table_extra_slot(vec![
        table,
        Value::fixnum(0),
        Value::symbol("general-category"),
    ])
    .unwrap();
    builtin_set_char_table_extra_slot(vec![table, Value::fixnum(1), Value::fixnum(0)]).unwrap();
    builtin_set_char_table_range(
        vec![
            table,
            Value::cons(Value::fixnum(256), Value::fixnum(383)),
            Value::string("\u{2}\u{1}\u{83}\u{2}"),
        ],
        None,
    )
    .unwrap();

    let formatted =
        crate::emacs_core::builtins::builtin_format(&mut eval, vec![Value::string("%S"), table])
            .unwrap();
    let rendered = formatted.as_runtime_string_owned().unwrap();
    assert!(rendered.starts_with("#^[nil nil char-code-property-table"));
    assert!(rendered.contains("#^^[1 0"));
    assert!(rendered.contains("#^^[2 0"));
    assert!(!rendered.contains("--char-table--"));
}

#[test]
fn char_table_p_on_plain_vector() {
    crate::test_utils::init_test_tracing();
    // A plain vector should not be detected as a char-table.
    let v = Value::vector(vec![Value::fixnum(1), Value::fixnum(2)]);
    assert!(!is_char_table(&v));
}

#[test]
fn char_table_wrong_type_signals() {
    crate::test_utils::init_test_tracing();
    let result = builtin_char_table_range(vec![Value::fixnum(5), Value::fixnum(65)], None);
    assert!(result.is_err());
    let result =
        builtin_set_char_table_range(vec![Value::NIL, Value::fixnum(65), Value::fixnum(1)], None);
    assert!(result.is_err());
    let result = builtin_char_table_parent(vec![Value::string("not-a-table")]);
    assert!(result.is_err());
}

#[test]
fn char_table_wrong_arg_count() {
    crate::test_utils::init_test_tracing();
    // builtin_make_char_table arity is validated by the Context dispatch
    // layer; make_char_table_value doesn't validate, so skip that assertion.
    assert!(builtin_char_table_p(vec![]).is_err());
    assert!(builtin_char_table_range(vec![Value::NIL], None).is_err());
    assert!(builtin_set_char_table_range(vec![Value::NIL, Value::NIL], None).is_err());
}

#[test]
fn char_table_char_key() {
    crate::test_utils::init_test_tracing();
    let ct = make_char_table_value(Value::symbol("test"), Value::NIL);
    // Use Value::Char for setting.
    builtin_set_char_table_range(vec![ct, Value::char('Z'), Value::symbol("zee")], None).unwrap();
    // Look up with Int.
    let val = builtin_char_table_range(vec![ct, Value::fixnum('Z' as i64)], None).unwrap();
    assert!(val.is_symbol_named("zee"));
}

#[test]
fn parent_default_fallback() {
    crate::test_utils::init_test_tracing();
    // Parent has default but no explicit entry.
    let parent = make_char_table_value(Value::symbol("test"), Value::symbol("parent-default"));
    let child = make_char_table_value(Value::symbol("test"), Value::NIL);
    builtin_set_char_table_parent(vec![child, parent]).unwrap();

    // Child has no entry, parent has no entry, parent default is used.
    let val = builtin_char_table_range(vec![child, Value::fixnum(100)], None).unwrap();
    assert!(val.is_symbol_named("parent-default"));
}

#[test]
fn non_nil_child_default_overrides_parent_lookup() {
    crate::test_utils::init_test_tracing();
    let parent = make_char_table_value(Value::symbol("test"), Value::fixnum(8));
    let child = make_char_table_value(Value::symbol("test"), Value::fixnum(0));
    builtin_set_char_table_parent(vec![child, parent]).unwrap();

    let val = builtin_char_table_range(vec![child, Value::fixnum('a' as i64)], None).unwrap();
    assert!(val.is_fixnum());
}

// -----------------------------------------------------------------------
// Bool-vector tests
// -----------------------------------------------------------------------

#[test]
fn make_bool_vector_basic() {
    crate::test_utils::init_test_tracing();
    let bv = builtin_make_bool_vector(vec![Value::fixnum(5), Value::NIL]).unwrap();
    assert!(is_bool_vector(&bv));
    assert!(!is_char_table(&bv));
}

#[test]
fn bool_vector_constructor_from_rest_args() {
    crate::test_utils::init_test_tracing();
    let bv = builtin_bool_vector(vec![
        Value::T,
        Value::NIL,
        Value::fixnum(0),
        Value::symbol("x"),
    ])
    .unwrap();
    assert!(is_bool_vector(&bv));
    assert_bv_bits(&bv, &[true, false, true, true]);

    let empty = builtin_bool_vector(vec![]).unwrap();
    assert!(is_bool_vector(&empty));
    assert_bv_bits(&empty, &[]);
}

#[test]
fn make_bool_vector_all_true() {
    crate::test_utils::init_test_tracing();
    let bv = builtin_make_bool_vector(vec![Value::fixnum(4), Value::T]).unwrap();
    let count = builtin_bool_vector_count_population(vec![bv]).unwrap();
    assert!(count.is_fixnum());
}

#[test]
fn make_bool_vector_all_false() {
    crate::test_utils::init_test_tracing();
    let bv = builtin_make_bool_vector(vec![Value::fixnum(4), Value::NIL]).unwrap();
    let count = builtin_bool_vector_count_population(vec![bv]).unwrap();
    assert!(count.is_fixnum());
}

#[test]
fn bool_vector_p_predicate() {
    crate::test_utils::init_test_tracing();
    let bv = builtin_make_bool_vector(vec![Value::fixnum(3), Value::NIL]).unwrap();
    assert!(builtin_bool_vector_p(vec![bv]).unwrap().is_t());
    assert!(
        builtin_bool_vector_p(vec![Value::fixnum(0)])
            .unwrap()
            .is_nil()
    );
}

#[test]
fn bool_vector_intersection() {
    crate::test_utils::init_test_tracing();
    // a = [1, 1, 0, 0], b = [1, 0, 1, 0] -> AND = [1, 0, 0, 0]
    let a = make_bv(&[true, true, false, false]);
    let b = make_bv(&[true, false, true, false]);
    let result = builtin_bool_vector_intersection(vec![a, b]).unwrap();
    assert_bv_bits(&result, &[true, false, false, false]);
}

#[test]
fn bool_vector_union() {
    crate::test_utils::init_test_tracing();
    let a = make_bv(&[true, true, false, false]);
    let b = make_bv(&[true, false, true, false]);
    let result = builtin_bool_vector_union(vec![a, b]).unwrap();
    assert_bv_bits(&result, &[true, true, true, false]);
}

#[test]
fn bool_vector_exclusive_or() {
    crate::test_utils::init_test_tracing();
    let a = make_bv(&[true, true, false, false]);
    let b = make_bv(&[true, false, true, false]);
    let result = builtin_bool_vector_exclusive_or(vec![a, b]).unwrap();
    assert_bv_bits(&result, &[false, true, true, false]);
}

#[test]
fn bool_vector_not() {
    crate::test_utils::init_test_tracing();
    let a = make_bv(&[true, false, true, false]);
    let result = builtin_bool_vector_not(vec![a]).unwrap();
    assert_bv_bits(&result, &[false, true, false, true]);
}

#[test]
fn bool_vector_not_into_dest() {
    crate::test_utils::init_test_tracing();
    let a = make_bv(&[false, false, true]);
    let dest = make_bv(&[false, false, false]);
    let result = builtin_bool_vector_not(vec![a, dest]).unwrap();
    assert_eq!(result, dest);
    assert_bv_bits(&dest, &[true, true, false]);
}

#[test]
fn bool_vector_set_difference() {
    crate::test_utils::init_test_tracing();
    let a = make_bv(&[true, true, false, true]);
    let b = make_bv(&[false, true, true, false]);
    let result = builtin_bool_vector_set_difference(vec![a, b]).unwrap();
    assert_bv_bits(&result, &[true, false, false, true]);
}

#[test]
fn bool_vector_count_consecutive() {
    crate::test_utils::init_test_tracing();
    let bv = make_bv(&[true, true, false, false, true, true]);
    let count_true_start =
        builtin_bool_vector_count_consecutive(vec![bv, Value::T, Value::fixnum(0)]).unwrap();
    let count_false_middle =
        builtin_bool_vector_count_consecutive(vec![bv, Value::NIL, Value::fixnum(2)]).unwrap();
    let count_true_mismatch =
        builtin_bool_vector_count_consecutive(vec![bv, Value::T, Value::fixnum(2)]).unwrap();
    assert!(count_true_start.is_fixnum());
    assert!(count_false_middle.is_fixnum());
    assert!(count_true_mismatch.is_fixnum());
}

#[test]
fn bool_vector_subsetp_true() {
    crate::test_utils::init_test_tracing();
    let a = make_bv(&[true, false, false]);
    let b = make_bv(&[true, true, false]);
    let result = builtin_bool_vector_subsetp(vec![a, b]).unwrap();
    assert!(result.is_t());
}

#[test]
fn bool_vector_subsetp_false() {
    crate::test_utils::init_test_tracing();
    let a = make_bv(&[true, false, true]);
    let b = make_bv(&[true, true, false]);
    let result = builtin_bool_vector_subsetp(vec![a, b]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn bool_vector_count_population_mixed() {
    crate::test_utils::init_test_tracing();
    let bv = make_bv(&[true, false, true, true, false]);
    let count = builtin_bool_vector_count_population(vec![bv]).unwrap();
    assert!(count.is_fixnum());
}

#[test]
fn bool_vector_empty() {
    crate::test_utils::init_test_tracing();
    let bv = builtin_make_bool_vector(vec![Value::fixnum(0), Value::NIL]).unwrap();
    assert!(is_bool_vector(&bv));
    let count = builtin_bool_vector_count_population(vec![bv]).unwrap();
    assert!(count.is_fixnum());
}

#[test]
fn bool_vector_negative_length() {
    crate::test_utils::init_test_tracing();
    let result = builtin_make_bool_vector(vec![Value::fixnum(-1), Value::NIL]);
    match result.expect_err("negative make-bool-vector length should signal") {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("wholenump"), Value::fixnum(-1)]
            );
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
}

#[test]
fn bool_vector_wrong_type_signals() {
    crate::test_utils::init_test_tracing();
    let result = builtin_bool_vector_count_population(vec![Value::fixnum(0)]);
    assert!(result.is_err());
}

#[test]
fn bool_vector_mismatched_length() {
    crate::test_utils::init_test_tracing();
    let a = make_bv(&[true, false]);
    let b = make_bv(&[true]);
    let result = builtin_bool_vector_intersection(vec![a, b]);
    assert!(result.is_err());
}

#[test]
fn bool_vector_intersection_into_dest() {
    crate::test_utils::init_test_tracing();
    let a = make_bv(&[true, true, false]);
    let b = make_bv(&[false, true, true]);
    let dest = make_bv(&[false, false, false]);
    let result = builtin_bool_vector_intersection(vec![a, b, dest]).unwrap();
    // Result should be the same object as dest.
    assert_bv_bits(&result, &[false, true, false]);
    // Dest should have been mutated.
    assert_bv_bits(&dest, &[false, true, false]);
}

#[test]
fn bool_vector_union_into_dest() {
    crate::test_utils::init_test_tracing();
    let a = make_bv(&[true, false, false]);
    let b = make_bv(&[false, true, false]);
    let dest = make_bv(&[false, false, false]);
    let result = builtin_bool_vector_union(vec![a, b, dest]).unwrap();
    assert_eq!(result, dest);
    assert_bv_bits(&dest, &[true, true, false]);
}

// GNU's `NILP (dest)`: an explicit nil destination is identical to an omitted
// one — both allocate a fresh bool-vector rather than signalling
// wrong-type-argument. `(bool-vector-union a b nil)` must not error.
#[test]
fn bool_vector_binops_treat_explicit_nil_dest_as_fresh() {
    crate::test_utils::init_test_tracing();
    let a = make_bv(&[true, true, false, false]);
    let b = make_bv(&[true, false, true, false]);

    let union = builtin_bool_vector_union(vec![a, b, Value::NIL]).unwrap();
    assert_bv_bits(&union, &[true, true, true, false]);

    let inter = builtin_bool_vector_intersection(vec![a, b, Value::NIL]).unwrap();
    assert_bv_bits(&inter, &[true, false, false, false]);

    let xor = builtin_bool_vector_exclusive_or(vec![a, b, Value::NIL]).unwrap();
    assert_bv_bits(&xor, &[false, true, true, false]);

    let diff = builtin_bool_vector_set_difference(vec![a, b, Value::NIL]).unwrap();
    assert_bv_bits(&diff, &[false, true, false, false]);
}

#[test]
fn bool_vector_not_treats_explicit_nil_dest_as_fresh() {
    crate::test_utils::init_test_tracing();
    let a = make_bv(&[true, false, true, false]);
    let result = builtin_bool_vector_not(vec![a, Value::NIL]).unwrap();
    assert_bv_bits(&result, &[false, true, false, true]);
}

#[test]
fn bool_vector_union_into_unchanged_dest_returns_nil() {
    crate::test_utils::init_test_tracing();
    let a = make_bv(&[true, false, true]);
    let b = make_bv(&[false, true, true]);
    let dest = make_bv(&[true, true, true]);
    let result = builtin_bool_vector_union(vec![a, b, dest]).unwrap();
    assert_eq!(result, Value::NIL);
    assert_bv_bits(&dest, &[true, true, true]);
}

#[test]
fn is_predicates_disjoint() {
    crate::test_utils::init_test_tracing();
    let ct = make_char_table_value(Value::symbol("test"), Value::NIL);
    let bv = builtin_make_bool_vector(vec![Value::fixnum(3), Value::NIL]).unwrap();
    let v = Value::vector(vec![Value::fixnum(1)]);
    assert!(is_char_table(&ct));
    assert!(!is_bool_vector(&ct));
    assert!(!is_char_table(&bv));
    assert!(is_bool_vector(&bv));
    assert!(!is_char_table(&v));
    assert!(!is_bool_vector(&v));
}

#[test]
fn bool_vector_wrong_arg_count() {
    crate::test_utils::init_test_tracing();
    assert!(builtin_make_bool_vector(vec![]).is_err());
    assert!(builtin_bool_vector_p(vec![]).is_err());
    assert!(builtin_bool_vector_subsetp(vec![Value::NIL]).is_err());
    assert!(builtin_bool_vector_not(vec![]).is_err());
    assert!(builtin_bool_vector_not(vec![Value::NIL, Value::NIL, Value::NIL]).is_err());
}

#[test]
fn char_table_range_invalid_range_type() {
    crate::test_utils::init_test_tracing();
    let ct = make_char_table_value(Value::symbol("test"), Value::NIL);
    let result =
        builtin_set_char_table_range(vec![ct, Value::string("invalid"), Value::fixnum(1)], None);
    match result {
        Err(Flow::Signal(signal)) => {
            assert_eq!(signal.symbol_name(), "error");
            assert!(
                signal
                    .data
                    .first()
                    .and_then(|v| v.as_utf8_str())
                    .is_some_and(|message| {
                        // GNU requotes C-level `error()` messages via `doprnt`
                        // (`text-quoting-style' = `curve' in batch), turning the
                        // grave accent/apostrophe into curly quotes.
                        message == "Invalid RANGE argument to \u{2018}set-char-table-range\u{2019}"
                    })
            );
        }
        other => panic!("expected invalid range error, got {other:?}"),
    }
}

#[test]
fn char_table_range_reverse_cons_returns_value_without_changing_entries() {
    crate::test_utils::init_test_tracing();
    let ct = make_char_table_value(Value::symbol("test"), Value::fixnum(0));
    let range = Value::cons(Value::fixnum(70), Value::fixnum(65)); // min > max
    assert_eq!(
        builtin_set_char_table_range(vec![ct, range, Value::fixnum(1)], None).unwrap(),
        Value::fixnum(1)
    );
    assert_eq!(
        builtin_char_table_range(vec![ct, Value::fixnum(65)], None).unwrap(),
        Value::fixnum(0)
    );
    assert_eq!(
        builtin_char_table_range(vec![ct, Value::fixnum(70)], None).unwrap(),
        Value::fixnum(0)
    );
}

#[test]
fn char_table_range_rejects_non_character_fixnum_atoms_like_gnu() {
    crate::test_utils::init_test_tracing();
    let ct = make_char_table_value(Value::symbol("test"), Value::NIL);

    for result in [
        builtin_char_table_range(vec![ct, Value::fixnum(-1)], None),
        builtin_set_char_table_range(vec![ct, Value::fixnum(-1), Value::symbol("x")], None),
    ] {
        match result {
            Err(Flow::Signal(signal)) => {
                assert_eq!(signal.symbol_name(), "error");
                assert!(
                    signal
                        .data
                        .first()
                        .and_then(|v| v.as_utf8_str())
                        .is_some_and(|message| {
                            // Curly quotes from GNU's `doprnt` requoting (curve style).
                            message.starts_with("Invalid RANGE argument to \u{2018}")
                        })
                );
            }
            other => panic!("expected invalid range error, got {other:?}"),
        }
    }
}

#[test]
fn fillarray_preserves_ascii_cache_while_rewriting_contents_like_gnu() {
    crate::test_utils::init_test_tracing();
    let ct = make_char_table_value(Value::symbol("case-table"), Value::symbol("base"));
    crate::emacs_core::builtins::builtin_fillarray(vec![ct, Value::symbol("x")]).unwrap();

    assert_eq!(
        builtin_char_table_range(vec![ct, Value::fixnum('a' as i64)], None).unwrap(),
        Value::symbol("base")
    );
    assert_eq!(
        builtin_char_table_range(vec![ct, Value::fixnum(999_999)], None).unwrap(),
        Value::symbol("x")
    );
    assert_eq!(
        builtin_char_table_range(vec![ct, Value::NIL], None).unwrap(),
        Value::symbol("x")
    );

    let slots = char_table_external_slots(&ct).unwrap();
    assert_eq!(slots[3], Value::symbol("base"));
    assert!(slots[4..68].iter().all(|slot| *slot == Value::symbol("x")));
}

#[test]
fn unicode_property_table_internal_returns_alist_char_table() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let prop = Value::symbol("neo-test-property");
    let table =
        make_char_table_with_extra_slots(Value::symbol("char-code-property-table"), Value::NIL, 5);
    builtin_set_char_table_extra_slot(vec![table, Value::fixnum(0), prop]).unwrap();
    builtin_set_char_table_extra_slot(vec![table, Value::fixnum(1), Value::fixnum(0)]).unwrap();
    builtin_set_char_table_extra_slot(vec![
        table,
        Value::fixnum(4),
        Value::vector(vec![Value::NIL, Value::symbol("letter")]),
    ])
    .unwrap();
    builtin_set_char_table_range(vec![table, Value::fixnum(65), Value::fixnum(1)], None).unwrap();
    eval.obarray.set_symbol_value(
        "char-code-property-alist",
        Value::list(vec![Value::cons(prop, table)]),
    );

    let returned = builtin_unicode_property_table_internal(&mut eval, vec![prop])
        .expect("unicode-property-table-internal should return the table");
    assert!(is_char_table(&returned));

    let decoded = builtin_get_unicode_property_internal(vec![returned, Value::fixnum(65)])
        .expect("run-length decoder should map through extra slot 4");
    assert!(decoded.is_symbol_named("letter"));
}

// -----------------------------------------------------------------------
// Test helpers
// -----------------------------------------------------------------------

/// Build a bool-vector from a slice of bools (test helper).
fn make_bv(bits: &[bool]) -> Value {
    bool_vector_from_bits(bits)
}

/// Assert that a bool-vector has the expected bits.
fn assert_bv_bits(bv: &Value, expected: &[bool]) {
    assert!(bv.is_vector(), "expected a vector");
    let vec = bv.as_vector_data().unwrap().clone();
    let len = bv_length(&vec) as usize;
    assert_eq!(len, expected.len(), "bool-vector length mismatch");
    let bits = bv_bits(&vec);
    assert_eq!(bits, expected);
}

#[test]
fn maybe_unify_char_fixnum_returns_unified() {
    let val = Value::fixnum(0x4E2D);
    assert_eq!(maybe_unify_char(0x110000, &val), 0x4E2D);
}

#[test]
fn maybe_unify_char_nil_is_identity() {
    assert_eq!(maybe_unify_char(0x110042, &Value::NIL), 0x110042);
}

#[test]
fn maybe_unify_char_out_of_range_fixnum_is_identity() {
    let val = Value::fixnum(MAX_CHAR + 1);
    assert_eq!(maybe_unify_char(0x110000, &val), 0x110000);
}

// -----------------------------------------------------------------------
// put-unicode-property-internal (bug fix9-chartab-category #2)
// -----------------------------------------------------------------------

/// Build a `general-category`-style run-length uniprop table: decoder (slot 1)
/// and encoder (slot 2) are index 0/1, and slot 4 holds the value vector
/// `[nil Lu Ll Lt]`.
fn make_run_length_uniprop_table() -> Value {
    let table =
        make_char_table_with_extra_slots(Value::symbol("char-code-property-table"), Value::NIL, 5);
    builtin_set_char_table_extra_slot(vec![
        table,
        Value::fixnum(0),
        Value::symbol("general-category"),
    ])
    .unwrap();
    builtin_set_char_table_extra_slot(vec![table, Value::fixnum(1), Value::fixnum(0)]).unwrap();
    builtin_set_char_table_extra_slot(vec![table, Value::fixnum(2), Value::fixnum(1)]).unwrap();
    builtin_set_char_table_extra_slot(vec![
        table,
        Value::fixnum(4),
        Value::vector(vec![
            Value::NIL,
            Value::symbol("Lu"),
            Value::symbol("Ll"),
            Value::symbol("Lt"),
        ]),
    ])
    .unwrap();
    table
}

#[test]
fn put_unicode_property_internal_run_length_round_trips() {
    crate::test_utils::init_test_tracing();
    let table = make_run_length_uniprop_table();

    // GNU `put-char-code-property` -> `put-unicode-property-internal` encodes
    // `Ll` to its index (2) in the value vector and stores it.
    assert!(
        builtin_put_unicode_property_internal(vec![
            table,
            Value::fixnum('B' as i64),
            Value::symbol("Ll"),
        ])
        .unwrap()
        .is_nil()
    );

    // The stored element is the raw fixnum index.
    assert_eq!(ct_lookup(&table, 'B' as i64).unwrap(), Value::fixnum(2));

    // get-unicode-property-internal decodes back to the symbol.
    assert_eq!(
        builtin_get_unicode_property_internal(vec![table, Value::fixnum('B' as i64)]).unwrap(),
        Value::symbol("Ll")
    );
}

#[test]
fn put_unicode_property_internal_run_length_rejects_unknown_value() {
    crate::test_utils::init_test_tracing();
    let table = make_run_length_uniprop_table();

    // GNU `uniprop_encode_value_run_length` signals
    // `(wrong-type-argument "Unicode property value" VALUE)` for a value that is
    // not present in the value vector.  Note the first datum is a *string*.
    let err = builtin_put_unicode_property_internal(vec![
        table,
        Value::fixnum('B' as i64),
        Value::symbol("ZZ"),
    ])
    .expect_err("unknown run-length value should signal");
    match err {
        Flow::Signal(signal) => {
            assert_eq!(signal.symbol_name(), "wrong-type-argument");
            assert_eq!(
                signal
                    .data
                    .first()
                    .and_then(|v| v.as_lisp_string())
                    .map(|s| crate::emacs_core::emacs_char::to_utf8_lossy(s.as_bytes())),
                Some("Unicode property value".to_string())
            );
            assert_eq!(signal.data.get(1).copied(), Some(Value::symbol("ZZ")));
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
}

#[test]
fn put_unicode_property_internal_numeric_round_trips_and_rejects_non_fixnum() {
    crate::test_utils::init_test_tracing();
    let table =
        make_char_table_with_extra_slots(Value::symbol("char-code-property-table"), Value::NIL, 5);
    builtin_set_char_table_extra_slot(vec![
        table,
        Value::fixnum(0),
        Value::symbol("numeric-value"),
    ])
    .unwrap();
    builtin_set_char_table_extra_slot(vec![table, Value::fixnum(1), Value::fixnum(0)]).unwrap();
    builtin_set_char_table_extra_slot(vec![table, Value::fixnum(2), Value::fixnum(2)]).unwrap();
    builtin_set_char_table_extra_slot(vec![table, Value::fixnum(4), Value::vector(vec![])])
        .unwrap();

    // First number is appended to the value vector at index 0 and stored as 0.
    builtin_put_unicode_property_internal(vec![
        table,
        Value::fixnum('B' as i64),
        Value::fixnum(42),
    ])
    .unwrap();
    assert_eq!(ct_lookup(&table, 'B' as i64).unwrap(), Value::fixnum(0));
    assert_eq!(
        char_table_extra_slot_value(&table, 4)
            .and_then(|v| v.as_vector_data())
            .map(|v| v.to_vec()),
        Some(vec![Value::fixnum(42)])
    );

    // GNU `uniprop_encode_value_numeric` runs `CHECK_FIXNUM`, so a non-fixnum
    // value signals `(wrong-type-argument fixnump VALUE)`.
    let err = builtin_put_unicode_property_internal(vec![
        table,
        Value::fixnum('B' as i64),
        Value::symbol("X"),
    ])
    .expect_err("non-fixnum numeric value should signal");
    match err {
        Flow::Signal(signal) => {
            assert_eq!(signal.symbol_name(), "wrong-type-argument");
            assert_eq!(
                signal.data.first().and_then(|v| v.as_symbol_name()),
                Some("fixnump")
            );
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
}

#[test]
fn put_unicode_property_internal_character_encoder_rejects_non_char() {
    crate::test_utils::init_test_tracing();
    let table =
        make_char_table_with_extra_slots(Value::symbol("char-code-property-table"), Value::NIL, 5);
    builtin_set_char_table_extra_slot(vec![table, Value::fixnum(0), Value::symbol("mirroring")])
        .unwrap();
    builtin_set_char_table_extra_slot(vec![table, Value::fixnum(2), Value::fixnum(0)]).unwrap();

    // A valid character is stored verbatim (no value-vector indirection).
    builtin_put_unicode_property_internal(vec![
        table,
        Value::fixnum('B' as i64),
        Value::fixnum('b' as i64),
    ])
    .unwrap();
    assert_eq!(
        ct_lookup(&table, 'B' as i64).unwrap(),
        Value::fixnum('b' as i64)
    );

    // GNU `uniprop_encode_value_character` signals
    // `(wrong-type-argument integerp VALUE)` for a non-character.
    assert_signal_symbol_and_predicate(
        builtin_put_unicode_property_internal(vec![
            table,
            Value::fixnum('B' as i64),
            Value::symbol("X"),
        ]),
        "wrong-type-argument",
        "integerp",
    );
}

#[test]
fn put_unicode_property_internal_rejects_nil_char_table() {
    crate::test_utils::init_test_tracing();
    // GNU runs `CHECK_CHAR_TABLE` first.
    assert_signal_symbol_and_predicate(
        builtin_put_unicode_property_internal(vec![Value::NIL, Value::fixnum(0), Value::fixnum(1)]),
        "wrong-type-argument",
        "char-table-p",
    );
}
