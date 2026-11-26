//! Free-list poison (`TaggedValue::DEAD`) lives in the unused non-Lisp `0b001`
//! tag space and stays crate-private so it never satisfies `is_string()`,
//! `is_heap_object()`, or any live Lisp type predicate.

use crate::value::{Value, ValueKind};
use std::hash::{Hash, Hasher};

#[test]
fn dead_object_is_not_a_live_string() {
    assert!(Value::DEAD.is_dead());
    assert!(!Value::DEAD.is_string());
    assert!(!Value::DEAD.is_heap_object());
    assert!(!Value::DEAD.is_cons());
    assert!(!Value::DEAD.is_veclike());
    assert!(!Value::DEAD.is_float());
    assert!(!Value::DEAD.is_fixnum());
    assert!(!Value::DEAD.is_bare_symbol());
    assert!(matches!(Value::DEAD.kind(), ValueKind::Unknown));
    assert!(
        Value::DEAD.as_string_ptr().is_none(),
        "DEAD handed out a string pointer"
    );
    assert!(
        Value::DEAD.heap_ptr().is_none(),
        "DEAD handed out a heap pointer"
    );
    assert!(Value::DEAD.as_utf8_str().is_none());
    assert!(Value::DEAD.as_lisp_string().is_none());
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    Value::DEAD.hash(&mut hasher);
    let _ = hasher.finish();
    assert_eq!(Value::DEAD, Value::DEAD);
}

#[test]
fn dead_object_clear_string_intervals_returns_false() {
    let _heap = crate::tagged::gc::TestHeap::new();
    assert!(
        !crate::tagged::mutate::clear_string_intervals(Value::DEAD),
        "DEAD must not be dereferenced or reported as cleared"
    );
}

#[test]
fn dead_object_set_string_byte_same_char_count_returns_false() {
    let _heap = crate::tagged::gc::TestHeap::new();
    assert!(
        !Value::DEAD.set_string_byte_same_char_count(0, b'a'),
        "DEAD must not be dereferenced as a mutable StringObj"
    );
}
