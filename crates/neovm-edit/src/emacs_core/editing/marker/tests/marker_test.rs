use super::*;

use crate::buffer::{EmacsByteRange, LispCharPos1};

use std::cell::RefCell;

#[test]
fn is_marker_rejects_non_markers() {
    crate::test_utils::init_test_tracing();
    assert!(!is_marker(&Value::NIL));
    assert!(!is_marker(&Value::fixnum(42)));
    assert!(!is_marker(&Value::vector(vec![Value::fixnum(1)])));
}
