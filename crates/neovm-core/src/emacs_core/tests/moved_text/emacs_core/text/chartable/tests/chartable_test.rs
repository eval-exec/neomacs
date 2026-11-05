use crate::emacs_core::chartable_entries::builtin_unicode_property_table_internal;

use crate::emacs_core::eval::Context;

use crate::emacs_core::eval::VariableView;

use neovm_text::emacs_core::chartable::*;

use neovm_value::boolvec::{bool_vector_from_bits, bool_vector_ref_value};

use neovm_value::error::{FlowKind, FlowResultExt as _};

use neovm_value::intern::{intern, intern_uninterned};

use neovm_value::symbol::CanonicalAccess;

use neovm_value::value::ValueKind;

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
            purpose.as_bare_symbol_id().unwrap(),
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
            purpose.as_bare_symbol_id().unwrap(),
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
fn optimize_char_table_custom_test_is_noop_until_callbacks_are_supported() {
    crate::test_utils::init_test_tracing();
    let ct = make_char_table_value(Value::symbol("test"), Value::NIL);
    let first = Value::string("same");
    let second = Value::string("same");
    builtin_set_char_table_range(vec![ct, Value::fixnum('A' as i64), first], None).unwrap();
    builtin_set_char_table_range(vec![ct, Value::fixnum('B' as i64), second], None).unwrap();

    crate::emacs_core::builtins::symbols::builtin_optimize_char_table(
        vec![ct, Value::symbol("custom-test")],
        neovm_value::tagged::SymbolMode::Bare,
    )
    .unwrap();

    assert!(char_table_external_slots(&ct).is_some());
    let first_lookup = builtin_char_table_range(vec![ct, Value::fixnum('A' as i64)], None).unwrap();
    let second_lookup =
        builtin_char_table_range(vec![ct, Value::fixnum('B' as i64)], None).unwrap();
    assert!(neovm_value::value::eq_value(&first_lookup, &first));
    assert!(neovm_value::value::eq_value(&second_lookup, &second));
}

#[test]
fn map_char_table_shared_range_survives_callback_gc() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
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
        VariableView::canonical(CanonicalAccess::TEST, &neovm_value::symbol::Obarray::new()),
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

#[test]
fn unicode_property_table_read_from_generated_uni_category_decodes_ascii() {
    crate::test_utils::init_test_tracing();
    let path = crate::test_utils::workspace_root()
        .as_path()
        .join("lisp/international/uni-category.el");
    let bytes = std::fs::read(&path).expect("read generated Unicode category table");
    let decoded = crate::emacs_core::load::decode_emacs_utf8(&bytes);
    let forms = crate::emacs_core::value_reader::read_all_with_source_multibyte(
        &decoded,
        true,
        VariableView::canonical(CanonicalAccess::TEST, &neovm_value::symbol::Obarray::new()),
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
    let path = crate::test_utils::workspace_root()
        .as_path()
        .join("lisp/international/uni-bidi.el");
    let bytes = std::fs::read(&path).expect("read generated Unicode bidi table");
    let decoded = crate::emacs_core::load::decode_emacs_utf8(&bytes);
    let forms = crate::emacs_core::value_reader::read_all_with_source_multibyte(
        &decoded,
        true,
        VariableView::canonical(CanonicalAccess::TEST, &neovm_value::symbol::Obarray::new()),
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
fn char_table_wrong_arg_count() {
    crate::test_utils::init_test_tracing();
    // builtin_make_char_table arity is validated by the Context dispatch
    // layer; make_char_table_value doesn't validate, so skip that assertion.
    assert!(builtin_char_table_p(vec![]).is_err());
    assert!(builtin_char_table_range(vec![Value::NIL], None).is_err());
    assert!(builtin_set_char_table_range(vec![Value::NIL, Value::NIL], None).is_err());
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
    assert!(decoded.is_bare_symbol_named("letter"));
}

fn assert_signal_symbol_and_predicate(result: EvalResult, symbol: &str, predicate: &str) {
    match result.kinded() {
        Err(FlowKind::Signal(signal)) => {
            assert_eq!(signal.symbol_name(), symbol);
            assert_eq!(
                signal.data.first().and_then(|v| v.as_bare_symbol_name()),
                Some(predicate)
            );
        }
        other => panic!("expected signal {symbol} {predicate}, got {other:?}"),
    }
}

fn find_char_table(value: Value) -> Option<Value> {
    if neovm_text::emacs_core::chartable::is_char_table(&value) {
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
