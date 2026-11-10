//! Lisp-event half of `w32_request_parsing_and_lisp_event_shape_match_gnu`.
//! The codec assertions stay in the eval windows test. The event fields stay
//! private; this calls the authority forge and compares the same three fields.

use crate::test_utils::runtime_startup_context;
use neovm_eval::emacs_core::builtins::file_notify::{WatchId, WatchRegistration};
use neovm_eval::test_support::authority::w32_renamed_to_event_lisp_for_test;
use neovm_value::value::Value;
use std::path::Path;

#[test]
fn w32_request_parsing_and_lisp_event_shape_match_gnu() {
    let eval = runtime_startup_context();
    let fields = neovm_value::value::list_to_vec(&w32_renamed_to_event_lisp_for_test(
        &eval,
        WatchId::new(42, 0),
        Path::new(r"nested\new.txt"),
        WatchRegistration::new(Value::NIL, Value::NIL),
    ))
    .expect("w32 event is a proper list");
    assert_eq!(
        fields,
        [
            Value::fixnum(42),
            Value::symbol("renamed-to"),
            Value::string(r"nested\new.txt")
        ]
    );
}
