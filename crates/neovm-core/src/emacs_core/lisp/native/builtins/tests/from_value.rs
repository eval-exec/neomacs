use super::*;
use crate::buffer::LispCharPos1;
use crate::emacs_core::eval::Context;

fn assert_wrong_type(flow: Flow, predicate: &str, offender: Value) {
    match flow {
        Flow::Signal(data) => {
            assert_eq!(data.symbol_name(), "wrong-type-argument");
            assert_eq!(data.data[0].as_symbol_name(), Some(predicate));
            assert_eq!(data.data[1], offender);
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
}

#[test]
fn value_identity_accepts_anything() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let v = Value::string("anything");
    let out = Value::from_value(&mut eval, v).expect("identity");
    assert!(out.bits() == v.bits());
}

#[test]
fn i64_extracts_fixnum_and_signals_integerp() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    assert_eq!(i64::from_value(&mut eval, Value::fixnum(42)).unwrap(), 42);
    let bad = Value::string("x");
    assert_wrong_type(
        i64::from_value(&mut eval, bad).unwrap_err(),
        "integerp",
        bad,
    );
}

#[test]
fn f64_extracts_numbers_and_signals_numberp() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    assert_eq!(f64::from_value(&mut eval, Value::fixnum(3)).unwrap(), 3.0);
    assert_eq!(
        f64::from_value(&mut eval, Value::make_float(2.5)).unwrap(),
        2.5
    );
    let bad = Value::symbol("nan");
    assert_wrong_type(f64::from_value(&mut eval, bad).unwrap_err(), "numberp", bad);
}

/// DIVERGENCES.md 163 deleted `impl FromValue for &'static LispString`: a
/// conversion that takes its `Value` by value cannot hand back a borrow that
/// outlives the call, and only `as_lisp_string`'s laundered `'static` let it
/// typecheck. `StringDesignator` is the shape that survives — 167 made it
/// carry the OPERAND rather than a borrow of it, so the accessor that
/// reborrows from `&self` is now the only lifetime available rather than a
/// convention — so this pins the borrowing conversion through it, plus the
/// lossy sibling.
#[test]
fn lisp_string_borrow_and_lossy_string_signal_stringp() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let s = Value::string("héllo");
    let borrowed = StringDesignator::from_value(&mut eval, s).unwrap();
    assert_eq!(borrowed.text().schars(), 5);
    assert_eq!(String::from_value(&mut eval, s).unwrap(), "héllo");
    let bad = Value::fixnum(7);
    assert_wrong_type(
        StringDesignator::from_value(&mut eval, bad).unwrap_err(),
        "stringp",
        bad,
    );
    assert_wrong_type(
        String::from_value(&mut eval, bad).unwrap_err(),
        "stringp",
        bad,
    );
}

#[test]
fn bool_is_nil_test_and_never_signals() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    assert!(!bool::from_value(&mut eval, Value::NIL).unwrap());
    assert!(bool::from_value(&mut eval, Value::T).unwrap());
    assert!(bool::from_value(&mut eval, Value::fixnum(0)).unwrap());
}

#[test]
fn sym_id_extracts_symbols_including_nil() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let sym = Value::symbol("from-value-test-symbol");
    let id = SymId::from_value(&mut eval, sym).unwrap();
    assert_eq!(resolve_sym(id), "from-value-test-symbol");
    assert!(SymId::from_value(&mut eval, Value::NIL).is_ok());
    let bad = Value::string("not-a-symbol");
    assert_wrong_type(
        SymId::from_value(&mut eval, bad).unwrap_err(),
        "symbolp",
        bad,
    );
}

#[test]
fn option_maps_nil_to_none_and_extracts_some() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    assert_eq!(
        Option::<i64>::from_value(&mut eval, Value::NIL).unwrap(),
        None
    );
    assert_eq!(
        Option::<i64>::from_value(&mut eval, Value::fixnum(5)).unwrap(),
        Some(5)
    );
    let bad = Value::string("x");
    assert_wrong_type(
        Option::<i64>::from_value(&mut eval, bad).unwrap_err(),
        "integerp",
        bad,
    );
}

#[test]
fn fixnum_wholenum_character_code_predicates() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    assert_eq!(
        Fixnum::from_value(&mut eval, Value::fixnum(-3)).unwrap(),
        Fixnum(-3)
    );
    let bad = Value::make_float(1.0);
    assert_wrong_type(
        Fixnum::from_value(&mut eval, bad).unwrap_err(),
        "fixnump",
        bad,
    );

    assert_eq!(
        Wholenum::from_value(&mut eval, Value::fixnum(9)).unwrap(),
        Wholenum(9)
    );
    let neg = Value::fixnum(-1);
    assert_wrong_type(
        Wholenum::from_value(&mut eval, neg).unwrap_err(),
        "wholenump",
        neg,
    );

    assert_eq!(
        CharacterCode::from_value(&mut eval, Value::fixnum(0x41)).unwrap(),
        CharacterCode(0x41)
    );
    let out_of_range = Value::fixnum(0x40_0000);
    assert_wrong_type(
        CharacterCode::from_value(&mut eval, out_of_range).unwrap_err(),
        "characterp",
        out_of_range,
    );
}

#[test]
fn string_designator_accepts_symbols() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let string = Value::string("abc");
    let string_storage = string.as_lisp_string().expect("string value has storage");
    let from_string = StringDesignator::from_value(&mut eval, string).unwrap();
    assert_eq!(from_string.text().as_bytes(), b"abc");
    assert!(
        std::ptr::eq(from_string.text(), string_storage),
        "GNU compares the original string object instead of cloning its storage"
    );

    let symbol = Value::symbol("abc");
    let symbol_name = crate::emacs_core::intern::resolve_lisp_visible_symbol_name(
        symbol.as_symbol_id().expect("symbol value has an id"),
    );
    let from_symbol = StringDesignator::from_value(&mut eval, symbol).unwrap();
    assert_eq!(from_symbol.text().as_bytes(), b"abc");
    assert!(
        std::ptr::eq(from_symbol.text(), symbol_name.text()),
        "GNU compares the symbol's existing name string instead of rebuilding it"
    );
    let bad = Value::fixnum(1);
    assert_wrong_type(
        StringDesignator::from_value(&mut eval, bad).unwrap_err(),
        "stringp",
        bad,
    );
}

#[test]
fn string_designator_honors_positioned_symbol_dynamic_view() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let positioned = eval
        .tagged_heap
        .alloc_symbol_with_pos(Value::symbol("alpha"), Value::fixnum(17));

    assert_wrong_type(
        StringDesignator::from_value(&mut eval, positioned).unwrap_err(),
        "stringp",
        positioned,
    );

    eval.set_variable("symbols-with-pos-enabled", Value::T);
    let designator = StringDesignator::from_value(&mut eval, positioned).unwrap();
    assert_eq!(designator.text().as_bytes(), b"alpha");
}

#[test]
fn number_or_marker_reads_live_marker_position() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.buffers
        .current_buffer_mut()
        .expect("current buffer")
        .insert("abcdef");
    let buffer_id = eval.buffers.current_buffer_id().expect("current buffer");
    let marker = crate::emacs_core::marker::make_registered_buffer_marker(
        &mut eval.buffers,
        buffer_id,
        LispCharPos1::new(4),
        false,
    );
    match NumberOrMarker::from_value(&mut eval, marker).unwrap() {
        NumberOrMarker::Int(n) => assert_eq!(n, 4),
        NumberOrMarker::Float(f) => panic!("expected int position, got float {f}"),
    }
    let bad = Value::symbol("m");
    assert_wrong_type(
        NumberOrMarker::from_value(&mut eval, bad).unwrap_err(),
        "number-or-marker-p",
        bad,
    );
}

#[test]
fn lisp_char_pos_accepts_fixnums_and_markers() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    assert_eq!(
        LispCharPos1::from_value(&mut eval, Value::fixnum(3)).unwrap(),
        LispCharPos1::new(3)
    );
    eval.buffers
        .current_buffer_mut()
        .expect("current buffer")
        .insert("abcdef");
    let buffer_id = eval.buffers.current_buffer_id().expect("current buffer");
    let marker = crate::emacs_core::marker::make_registered_buffer_marker(
        &mut eval.buffers,
        buffer_id,
        LispCharPos1::new(5),
        false,
    );
    assert_eq!(
        LispCharPos1::from_value(&mut eval, marker).unwrap(),
        LispCharPos1::new(5)
    );
    let bad = Value::string("5");
    assert_wrong_type(
        LispCharPos1::from_value(&mut eval, bad).unwrap_err(),
        "integer-or-marker-p",
        bad,
    );
}

typed_subr! {
    /// Repeat S N times joined by SEP (test-only sample builtin).
    fn sample_typed_repeat(_eval, s: String, n: Wholenum, sep: Option<String>) -> EvalResult {
        let sep = sep.unwrap_or_default();
        Ok(Value::string(vec![s; n.0 as usize].join(&sep)))
    }
}

#[test]
fn typed_subr_extracts_arguments_in_order_and_signals_typed_errors() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();

    let ok = sample_typed_repeat(&mut eval, Value::string("ab"), Value::fixnum(3), Value::NIL)
        .expect("typed call");
    assert_eq!(
        ok.as_lisp_string().map(|s| s.as_bytes().to_vec()),
        Some(b"ababab".to_vec())
    );

    let with_sep = sample_typed_repeat(
        &mut eval,
        Value::string("ab"),
        Value::fixnum(2),
        Value::string("-"),
    )
    .expect("typed call with separator");
    assert_eq!(
        with_sep.as_lisp_string().map(|s| s.as_bytes().to_vec()),
        Some(b"ab-ab".to_vec())
    );

    let bad_first = Value::fixnum(9);
    assert_wrong_type(
        sample_typed_repeat(&mut eval, bad_first, Value::fixnum(1), Value::NIL).unwrap_err(),
        "stringp",
        bad_first,
    );

    let bad_count = Value::fixnum(-1);
    assert_wrong_type(
        sample_typed_repeat(&mut eval, Value::string("x"), bad_count, Value::NIL).unwrap_err(),
        "wholenump",
        bad_count,
    );
}
