//! GNU Lisp type-keyword mapping, no bus required.

use dbus::arg::ArgType;

use crate::emacs_core::value::Value;

use super::super::types::object_to_arg_type;

#[test]
fn gnu_object_to_dbus_type_matches_dbusbind_c() {
    crate::test_utils::init_test_tracing();
    assert_eq!(object_to_arg_type(Value::T).unwrap(), ArgType::Boolean);
    assert_eq!(object_to_arg_type(Value::NIL).unwrap(), ArgType::Boolean);
    assert_eq!(object_to_arg_type(Value::fixnum(3)).unwrap(), ArgType::UInt32);
    assert_eq!(object_to_arg_type(Value::fixnum(-3)).unwrap(), ArgType::Int32);
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
