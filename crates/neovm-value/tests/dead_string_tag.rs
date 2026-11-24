//! GNU's free-list poison is a string tag over a null pointer. Naming it
//! must not produce a live string borrow.

use neovm_value::value::Value;
use std::hash::{Hash, Hasher};

#[test]
fn dead_object_is_not_a_live_string() {
    assert!(Value::DEAD.is_string());
    assert!(
        Value::DEAD.as_string_ptr().is_none(),
        "DEAD handed out a string pointer"
    );
    assert!(Value::DEAD.as_utf8_str().is_none());
    assert!(Value::DEAD.as_lisp_string().is_none());
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    Value::DEAD.hash(&mut hasher);
    let _ = hasher.finish();
    assert_eq!(Value::DEAD, Value::DEAD);
}
