use super::*;

use neovm_value::boolvec::{bool_vector_from_bits, bool_vector_ref_value};

use neovm_value::error::{FlowKind, FlowResultExt as _};

use neovm_value::intern::{intern, intern_uninterned};

use neovm_value::symbol::CanonicalAccess;

use neovm_value::value::ValueKind;

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
