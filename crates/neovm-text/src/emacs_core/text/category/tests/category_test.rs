use super::*;

use neovm_value::error::FlowKind;

use neovm_value::heap_types::LispString;

#[test]
fn make_category_table_matches_gnu_shape() {
    crate::test_utils::init_test_tracing();
    reset_category_thread_locals();
    let table = builtin_make_category_table(vec![]).unwrap();
    assert!(builtin_category_table_p(vec![table]).unwrap().is_truthy());

    let default =
        super::super::chartable::builtin_char_table_range(vec![table, Value::NIL], None).unwrap();
    assert!(
        super::super::chartable::builtin_bool_vector_p(vec![default])
            .unwrap()
            .is_truthy()
    );
    let docs =
        super::super::chartable::builtin_char_table_extra_slot(vec![table, Value::fixnum(0)])
            .unwrap();
    if !docs.is_vector() {
        panic!("expected docstring vector");
    };
    assert_eq!(docs.as_vector_data().unwrap().len(), 95);
    assert!(
        super::super::chartable::builtin_char_table_extra_slot(vec![table, Value::fixnum(1)])
            .unwrap()
            .is_nil()
    );
}

#[test]
fn make_category_set_sets_bits_for_valid_categories() {
    crate::test_utils::init_test_tracing();
    let set = builtin_make_category_set(vec![Value::string("al")]).unwrap();
    assert!(category_set_bit(&set, 'a' as usize));
    assert!(category_set_bit(&set, 'l' as usize));
    assert!(!category_set_bit(&set, 'b' as usize));
}

#[test]
fn make_category_set_rejects_control_char() {
    crate::test_utils::init_test_tracing();
    // GNU's `Fmake_category_set` runs `CHECK_CATEGORY` on each byte; a control
    // character (0x01) is outside `0x20..=0x7E`, so it signals
    // `(wrong-type-argument categoryp 1)`.
    let set = Value::heap_string(LispString::from_unibyte(vec![0x01]));
    let err = builtin_make_category_set(vec![set]).unwrap_err();
    match err.into_kind() {
        FlowKind::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("categoryp"), Value::fixnum(1)]);
        }
        other => panic!("expected wrong-type-argument categoryp, got {other:?}"),
    }
}

#[test]
fn make_category_set_rejects_multibyte_string() {
    crate::test_utils::init_test_tracing();
    // GNU signals `(error "Multibyte string in `make-category-set'")`.
    let multibyte = Value::heap_string(LispString::from_utf8("中"));
    assert!(multibyte.as_lisp_string().unwrap().is_multibyte());
    let err = builtin_make_category_set(vec![multibyte]).unwrap_err();
    match err.into_kind() {
        FlowKind::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string("Multibyte string in ‘make-category-set’")]
            );
        }
        other => panic!("expected multibyte error signal, got {other:?}"),
    }
}

#[test]
fn make_category_set_rejects_non_string() {
    crate::test_utils::init_test_tracing();
    let err = builtin_make_category_set(vec![Value::fixnum(0x20)]).unwrap_err();
    match err.into_kind() {
        FlowKind::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data.first().and_then(|v| v.as_bare_symbol_name()),
                Some("stringp")
            );
        }
        other => panic!("expected wrong-type-argument stringp, got {other:?}"),
    }
}

fn category_set_bit(set: &Value, idx: usize) -> bool {
    neovm_value::boolvec::bool_vector_ref_value(set, idx).is_some_and(|bit| bit.is_truthy())
}
