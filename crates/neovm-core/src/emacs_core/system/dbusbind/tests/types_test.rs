//! GNU Lisp type-keyword mapping, no bus required.

use dbus::arg::ArgType;

use crate::emacs_core::value::Value;

use super::super::types::object_to_arg_type;

#[test]
fn gnu_object_to_dbus_type_matches_dbusbind_c() {
    crate::test_utils::init_test_tracing();
    assert_eq!(object_to_arg_type(Value::T).unwrap(), ArgType::Boolean);
    assert_eq!(object_to_arg_type(Value::NIL).unwrap(), ArgType::Boolean);
    assert_eq!(
        object_to_arg_type(Value::fixnum(3)).unwrap(),
        ArgType::UInt32
    );
    assert_eq!(
        object_to_arg_type(Value::fixnum(-3)).unwrap(),
        ArgType::Int32
    );
    assert_eq!(
        object_to_arg_type(Value::make_float(1.5)).unwrap(),
        ArgType::Double
    );
    assert_eq!(
        object_to_arg_type(Value::string("hi")).unwrap(),
        ArgType::String
    );
    assert_eq!(
        object_to_arg_type(Value::keyword_by_name(":object-path")).unwrap(),
        ArgType::ObjectPath
    );
    assert_eq!(
        object_to_arg_type(Value::keyword_by_name(":array")).unwrap(),
        ArgType::Array
    );
}

fn keyword(name: &str) -> Value {
    Value::keyword_by_name(name)
}

fn marshal_argument(
    dtype: ArgType,
    value: Value,
) -> Result<dbus::Message, crate::emacs_core::error::Flow> {
    let mut message = dbus::Message::new_signal("/org/neomacs/Test", "org.neomacs.Test", "Types")
        .expect("literal test header is valid");
    super::super::types::append_arg(&mut dbus::arg::IterAppend::new(&mut message), dtype, value)?;
    Ok(message)
}

#[test]
fn dbus_marshalling_array_of_structs_has_complete_signature_and_body() {
    crate::test_utils::init_test_tracing();
    let value = Value::list(vec![
        keyword(":array"),
        Value::list(vec![
            keyword(":struct"),
            Value::fixnum(7),
            Value::string("seven"),
        ]),
        Value::list(vec![
            keyword(":struct"),
            Value::fixnum(9),
            Value::string("nine"),
        ]),
    ]);
    let message = marshal_argument(ArgType::Array, value).expect("valid struct array");
    assert_eq!(message.iter_init().signature().to_string(), "a(us)");
    assert_eq!(
        message.read1::<Vec<(u32, String)>>().expect("typed body"),
        vec![(7, "seven".to_owned()), (9, "nine".to_owned())]
    );
}

#[test]
fn dbus_marshalling_dictionary_has_basic_keys_and_variant_values() {
    crate::test_utils::init_test_tracing();
    let value = Value::list(vec![
        keyword(":array"),
        Value::list(vec![
            keyword(":dict-entry"),
            Value::string("urgency"),
            Value::list(vec![
                keyword(":variant"),
                keyword(":byte"),
                Value::fixnum(1),
            ]),
        ]),
        Value::list(vec![
            keyword(":dict-entry"),
            Value::string("category"),
            Value::list(vec![keyword(":variant"), Value::string("test")]),
        ]),
    ]);
    let message = marshal_argument(ArgType::Array, value).expect("valid dictionary");
    assert_eq!(message.iter_init().signature().to_string(), "a{sv}");
    let body = message
        .read1::<dbus::arg::PropMap>()
        .expect("typed dictionary body");
    assert_eq!(body.len(), 2);
    assert_eq!(body["urgency"].0.as_u64(), Some(1));
    assert_eq!(body["category"].0.as_str(), Some("test"));
}

#[test]
fn dbus_marshalling_variant_of_struct_has_complete_contained_type() {
    crate::test_utils::init_test_tracing();
    let value = Value::list(vec![
        keyword(":variant"),
        Value::list(vec![
            keyword(":struct"),
            Value::fixnum(7),
            Value::string("seven"),
        ]),
    ]);
    let message = marshal_argument(ArgType::Variant, value).expect("valid struct variant");
    assert_eq!(message.iter_init().signature().to_string(), "v");
    let body = message
        .read1::<dbus::arg::Variant<(u32, String)>>()
        .expect("typed variant body");
    assert_eq!(body.0, (7, "seven".to_owned()));
}

#[test]
fn dbus_marshalling_variant_of_array_has_complete_contained_type() {
    crate::test_utils::init_test_tracing();
    let value = Value::list(vec![
        keyword(":variant"),
        Value::list(vec![keyword(":array"), Value::fixnum(7), Value::fixnum(9)]),
    ]);
    let message = marshal_argument(ArgType::Variant, value).expect("valid array variant");
    let body = message
        .read1::<dbus::arg::Variant<Vec<u32>>>()
        .expect("typed variant body");
    assert_eq!(body.0, vec![7, 9]);
}

#[test]
fn dbus_marshalling_empty_array_uses_explicit_element_signature() {
    crate::test_utils::init_test_tracing();
    let value = Value::list(vec![
        keyword(":array"),
        keyword(":signature"),
        Value::string("(us)"),
    ]);
    let message = marshal_argument(ArgType::Array, value).expect("valid typed empty array");
    assert_eq!(message.iter_init().signature().to_string(), "a(us)");
    assert!(
        message
            .read1::<Vec<(u32, String)>>()
            .expect("typed empty body")
            .is_empty()
    );
}

#[test]
fn dbus_marshalling_invalid_signature_returns_lisp_error_without_panicking() {
    crate::test_utils::init_test_tracing();
    for signature in ["a", "r", "e", "a{vv}", "s\0u"] {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            marshal_argument(ArgType::Signature, Value::string(signature))
        }));
        assert!(
            result.is_ok(),
            "malformed signature {signature:?} must not panic"
        );
        let flow = result.unwrap().expect_err("malformed signature must fail");
        assert!(
            matches!(flow.kind(), crate::emacs_core::error::FlowRef::Signal(signal) if signal.symbol_name() == "dbus-error"),
            "malformed signature {signature:?}: {flow:?}"
        );
    }
}

#[test]
fn dbus_marshalling_invalid_nested_payload_is_rejected_before_message_mutation() {
    crate::test_utils::init_test_tracing();
    let value = Value::list(vec![
        keyword(":array"),
        keyword(":byte"),
        Value::fixnum(1),
        keyword(":byte"),
        Value::fixnum(300),
    ]);
    let mut message =
        dbus::Message::new_signal("/org/neomacs/Test", "org.neomacs.Test", "Types").unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        super::super::types::append_arg(
            &mut dbus::arg::IterAppend::new(&mut message),
            ArgType::Array,
            value,
        )
    }));
    assert!(
        result.is_ok(),
        "invalid payload must return Flow before container append"
    );
    assert!(result.unwrap().is_err(), "out-of-range byte must fail");
    assert_eq!(
        message.iter_init().arg_type(),
        ArgType::Invalid,
        "validation must leave the message untouched"
    );
}

#[cfg(unix)]
#[test]
fn dbus_marshalling_unix_fd_preserves_wire_type_and_caller_ownership() {
    use std::os::fd::AsRawFd;
    crate::test_utils::init_test_tracing();
    let original = std::fs::File::open("/dev/null").expect("read-only test descriptor");
    let message = marshal_argument(ArgType::UnixFd, Value::fixnum(original.as_raw_fd() as i64))
        .expect("valid Unix file descriptor");
    assert_eq!(message.iter_init().signature().to_string(), "h");
    let received = message
        .read1::<std::fs::File>()
        .expect("typed descriptor body");
    received.metadata().expect("received descriptor is live");
    drop(received);
    drop(message);
    original
        .metadata()
        .expect("marshalling must not close the caller's descriptor");
}

fn assert_structural_type_error(dtype: ArgType, value: Value, predicate: &str) {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        marshal_argument(dtype, value)
    }));
    let flow = result
        .expect("malformed container must not panic")
        .expect_err("malformed container must fail");
    match flow.kind() {
        crate::emacs_core::error::FlowRef::Signal(signal) => {
            assert_eq!(signal.symbol_name(), "wrong-type-argument");
            assert_eq!(signal.data.first().copied(), Some(Value::symbol(predicate)));
        }
        _ => panic!("expected Lisp type error: {flow:?}"),
    }
}

#[test]
fn dbus_marshalling_heterogeneous_struct_array_reports_gnu_type_error() {
    crate::test_utils::init_test_tracing();
    assert_structural_type_error(
        ArgType::Array,
        Value::list(vec![
            keyword(":array"),
            Value::list(vec![keyword(":struct"), Value::fixnum(7)]),
            Value::list(vec![keyword(":struct"), Value::string("seven")]),
        ]),
        "D-Bus",
    );
}

#[test]
fn dbus_marshalling_multi_value_variant_reports_gnu_type_error() {
    crate::test_utils::init_test_tracing();
    assert_structural_type_error(
        ArgType::Variant,
        Value::list(vec![
            keyword(":variant"),
            Value::fixnum(7),
            Value::fixnum(9),
        ]),
        "D-Bus",
    );
}

#[test]
fn dbus_marshalling_container_dictionary_key_reports_gnu_type_error() {
    crate::test_utils::init_test_tracing();
    assert_structural_type_error(
        ArgType::Array,
        Value::list(vec![
            keyword(":array"),
            Value::list(vec![
                keyword(":dict-entry"),
                Value::list(vec![keyword(":struct"), Value::fixnum(7)]),
                Value::string("seven"),
            ]),
        ]),
        "D-Bus",
    );
}

#[test]
fn dbus_marshalling_standalone_dictionary_entry_reports_gnu_type_error() {
    crate::test_utils::init_test_tracing();
    assert_structural_type_error(
        ArgType::DictEntry,
        Value::list(vec![
            keyword(":dict-entry"),
            Value::string("key"),
            Value::fixnum(7),
        ]),
        "D-Bus",
    );
}

#[test]
fn dbus_marshalling_empty_struct_and_variant_report_gnu_cons_type_error() {
    crate::test_utils::init_test_tracing();
    for (dtype, marker) in [(ArgType::Struct, ":struct"), (ArgType::Variant, ":variant")] {
        assert_structural_type_error(dtype, Value::list(vec![keyword(marker)]), "consp");
    }
}

#[test]
fn dbus_marshalling_total_container_depth_includes_empty_leaf() {
    crate::test_utils::init_test_tracing();
    let mut at_limit = Value::fixnum(7);
    for _ in 0..64 {
        at_limit = Value::list(vec![keyword(":variant"), at_limit]);
    }
    let message = marshal_argument(ArgType::Variant, at_limit)
        .expect("64 containers with a basic leaf are permitted");
    assert_eq!(message.iter_init().signature().to_string(), "v");
    // 64 variants plus an empty array are 65 containers, even though the
    // leaf's type signature itself is short and independently valid.
    let mut value = Value::list(vec![keyword(":array")]);
    for _ in 0..64 {
        value = Value::list(vec![keyword(":variant"), value]);
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        marshal_argument(ArgType::Variant, value)
    }));
    let flow = result
        .expect("depth overflow must not reach native append")
        .expect_err("65 containers exceed D-Bus depth limit");
    assert!(
        matches!(flow.kind(), crate::emacs_core::error::FlowRef::Signal(signal) if signal.symbol_name() == "dbus-error")
    );
}

#[test]
fn dbus_marshalling_short_dictionary_entry_reports_gnu_cons_type_error() {
    crate::test_utils::init_test_tracing();
    assert_structural_type_error(
        ArgType::Array,
        Value::list(vec![
            keyword(":array"),
            Value::list(vec![keyword(":dict-entry"), Value::string("key")]),
        ]),
        "consp",
    );
}
